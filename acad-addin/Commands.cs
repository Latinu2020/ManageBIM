using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Linq;
using System.Reflection;
using Autodesk.AutoCAD.ApplicationServices;
using Autodesk.AutoCAD.Colors;
using Autodesk.AutoCAD.DatabaseServices;
using Autodesk.AutoCAD.EditorInput;
using Autodesk.AutoCAD.Geometry;
using Autodesk.AutoCAD.Runtime;
using AcApp = Autodesk.AutoCAD.ApplicationServices.Core.Application;

namespace ManageBIM.Acad
{
    public class Commands
    {
        /// <summary>BIMCEXPORT: writes the 3D content of model space (or a selection) to .bimc.</summary>
        [CommandMethod("BIMCEXPORT", CommandFlags.Modal | CommandFlags.NoBlockEditor)]
        public void Export() => Run(false);

        /// <summary>BIMCSEND: trimite tot modelul direct in ManageBIM (fara dialog); a doua oara actualizeaza live.</summary>
        [CommandMethod("BIMCSEND", CommandFlags.Modal | CommandFlags.NoBlockEditor)]
        public void Send() => Run(true);

        private static void Run(bool send)
        {
            Document doc = AcApp.DocumentManager.MdiActiveDocument;
            if (doc == null) return;
            Editor ed = doc.Editor;
            Database db = doc.Database;
            string dwg = db.Filename;
            string name = Path.GetFileNameWithoutExtension(doc.Name);
            ObjectId[] chosen = null;
            string path;
            if (send)
            {
                path = ManageBimLink.InboxPath("AutoCAD - " + name);
            }
            else
            {
                var kw = new PromptKeywordOptions("\nExport [All/Selection] <All>: ") { AllowNone = true };
                kw.Keywords.Add("All");
                kw.Keywords.Add("Selection");
                var kr = ed.GetKeywords(kw);
                if (kr.Status == PromptStatus.Cancel) return;
                if (kr.Status == PromptStatus.OK && kr.StringResult == "Selection")
                {
                    var sr = ed.GetSelection();
                    if (sr.Status != PromptStatus.OK) return;
                    chosen = sr.Value.GetObjectIds();
                }
                string folder = string.IsNullOrEmpty(dwg) || !File.Exists(dwg) ? Environment.GetFolderPath(Environment.SpecialFolder.MyDocuments) : Path.GetDirectoryName(dwg);
                var so = new PromptSaveFileOptions("Export ManageBIM model cache")
                {
                    Filter = "ManageBIM model cache (*.bimc)|*.bimc",
                    InitialDirectory = folder,
                    InitialFileName = name + ".bimc",
                    DialogCaption = "Export ManageBIM model cache",
                };
                var fr = ed.GetFileNameForSave(so);
                if (fr.Status != PromptStatus.OK) return;
                path = fr.StringResult;
                if (!path.EndsWith(".bimc", StringComparison.OrdinalIgnoreCase)) path += ".bimc";
            }

            var sw = Stopwatch.StartNew();
            try
            {
                var ex = new AcadExporter(db, ed);
                using (doc.LockDocument())
                using (Transaction tr = db.TransactionManager.StartOpenCloseTransaction())
                {
                    if (chosen == null)
                    {
                        var bt = (BlockTable)tr.GetObject(db.BlockTableId, OpenMode.ForRead);
                        var ms = (BlockTableRecord)tr.GetObject(bt[BlockTableRecord.ModelSpace], OpenMode.ForRead);
                        chosen = ms.Cast<ObjectId>().ToArray();
                    }
                    ex.Run(tr, chosen);
                    tr.Commit();
                }
                var info = new List<KeyValuePair<string, string>>
                {
                    new KeyValuePair<string, string>("Source application", "Autodesk AutoCAD " + AcApp.Version),
                    new KeyValuePair<string, string>("Project", name),
                    new KeyValuePair<string, string>("Drawing", string.IsNullOrEmpty(dwg) ? doc.Name : dwg),
                    new KeyValuePair<string, string>("Units", ex.UnitsLabel + " -> metres (x" + ex.Scale.ToString("0.####") + ")"),
                    new KeyValuePair<string, string>("Exporter", "ManageBIM AutoCAD Exporter 0.9"),
                    new KeyValuePair<string, string>("Exported", DateTime.Now.ToString("yyyy-MM-dd HH:mm")),
                };
                long bytes;
                if (send)
                {
                    string tmp = ManageBimLink.TempPath(path);
                    bytes = BimcWriter.Write(tmp, info, ex.Warnings, ex.Elements);
                    ManageBimLink.Commit(tmp, path);
                    bool running = ManageBimLink.EnsureRunning(path);
                    ed.WriteMessage(running ? "\nManageBIM: model sent - the open ManageBIM updates it now.\n" : "\nManageBIM: starting ManageBIM with the model...\n");
                }
                else
                {
                    bytes = BimcWriter.Write(path, info, ex.Warnings, ex.Elements);
                }
                sw.Stop();
                int tris = ex.Elements.Sum(e => e.TriangleCount);
                ed.WriteMessage($"\nManageBIM: {ex.Elements.Count:N0} objects, {tris:N0} triangles, {bytes / 1048576.0:0.00} MB in {sw.Elapsed.TotalSeconds:0.0} s\n  {path}\n");
                if (ex.Skipped > 0) ed.WriteMessage($"  {ex.Skipped:N0} objects without 3D geometry were skipped (lines, text, hatches...).\n");
                if (ex.Elements.Count == 0) ed.WriteMessage("  Nothing exported: the drawing has no 3D solids, surfaces, meshes, 3D faces or blocks containing them.\n");
            }
            catch (System.Exception e)
            {
                ed.WriteMessage("\nManageBIM export failed: " + e.Message + "\n");
            }
        }
    }

    /// <summary>Turns model-space entities into ManageBIM elements (one per top-level entity / block).</summary>
    internal class AcadExporter
    {
        private const uint DefaultColor = 190u | (190u << 8) | (190u << 16) | (255u << 24);
        private readonly Database _db;
        private readonly Editor _ed;
        private readonly Dictionary<ObjectId, Color> _layerColors = new Dictionary<ObjectId, Color>();
        public readonly List<BimcElement> Elements = new List<BimcElement>();
        public readonly List<string> Warnings = new List<string>();
        public double Scale = 1.0;
        public string UnitsLabel = "";
        public int Skipped;

        private readonly PlantData _plant;
        // diagnostic: erorile Plant 3D si obiectele fara geometrie, grupate pe clasa (rezumat in jurnalul .bimc)
        private readonly Dictionary<string, int> _plantErrors = new Dictionary<string, int>();
        private readonly Dictionary<string, (int count, string reason)> _noGeometry = new Dictionary<string, (int, string)>();
        private string _reason;
        private readonly Dictionary<string, int> _skipped = new Dictionary<string, int>();
        private void Skip(string cls, string why)
        {
            string k = cls + " (" + why + ")";
            _skipped[k] = _skipped.TryGetValue(k, out int c) ? c + 1 : 1;
        }

        public AcadExporter(Database db, Editor ed)
        {
            _db = db;
            _ed = ed;
            _plant = PlantData.TryCreate(db, Warnings);
            if (_plant != null) ed.WriteMessage("\nManageBIM: Plant 3D project data found - exporting Plant 3D properties.\n");
            UnitsLabel = db.Insunits.ToString();
            switch (db.Insunits)
            {
                case UnitsValue.Millimeters: Scale = 0.001; break;
                case UnitsValue.Centimeters: Scale = 0.01; break;
                case UnitsValue.Decimeters: Scale = 0.1; break;
                case UnitsValue.Meters: Scale = 1.0; break;
                case UnitsValue.Inches: Scale = 0.0254; break;
                case UnitsValue.Feet: Scale = 0.3048; break;
                default:
                    // unitless drawing: a building larger than 2 km is almost surely in mm
                    try
                    {
                        Extents3d ext = new Extents3d(db.Extmin, db.Extmax);
                        double size = ext.MinPoint.DistanceTo(ext.MaxPoint);
                        Scale = size > 2000 ? 0.001 : 1.0;
                        UnitsLabel = "Unitless (assumed " + (Scale == 1 ? "metres" : "millimetres") + ")";
                    }
                    catch { Scale = 1.0; }
                    break;
            }
        }

        public void Run(Transaction tr, IList<ObjectId> ids)
        {
            int n = 0;
            foreach (ObjectId id in ids)
            {
                if (++n % 500 == 0) { _ed.WriteMessage($"\rManageBIM: {n}/{ids.Count}"); }
                Entity ent;
                try { ent = tr.GetObject(id, OpenMode.ForRead) as Entity; }
                catch (System.Exception e) { Skip(id.ObjectClass?.DxfName ?? "?", "cannot open: " + e.Message); continue; }
                if (ent == null) { Skip(id.ObjectClass?.DxfName ?? "?", "not an entity"); continue; }
                string ecls = ent.GetRXClass().DxfName ?? ent.GetType().Name;
                if (!ent.Visible) { Skip(ecls, "invisible"); continue; }
                var layer = (LayerTableRecord)tr.GetObject(ent.LayerId, OpenMode.ForRead);
                if (layer.IsOff || layer.IsFrozen) { Skip(ecls, $"layer '{layer.Name}' is {(layer.IsFrozen ? "frozen" : "off")}"); continue; }
                var el = Describe(tr, ent, layer);
                _reason = null;
                uint col0 = Resolve(tr, ent, DefaultColor);
                try
                {
                    Collect(tr, ent, Matrix3d.Identity, el, col0, 0);
                }
                catch (System.Exception e)
                {
                    _reason = "error: " + e.Message;
                }
                // obiectele custom (Plant 3D: grinzi, placi, scari, balustrade...; Architecture, MEP...) pot deriva
                // din Curve / BlockReference - daca ramura lor nu a dat geometrie, incercam faceter + explode
                if (el.TriangleCount == 0 && IsCustom(ent))
                {
                    string first = _reason;
                    try { Custom(tr, ent, Matrix3d.Identity, el, col0, 0); }
                    catch (System.Exception e) { _reason = "error: " + e.Message; }
                    _reason = $"[{Who(ent)}] " + (_reason ?? first ?? "no geometry");
                }
                // diagnostic pentru orice obiect fara geometrie care nu e evident 2D
                if (el.TriangleCount == 0 && _reason == null && !(ent is Curve c2 && Thickness(c2) == 0) && !(ent is DBText || ent is MText || ent is Dimension || ent is Hatch || ent is DBPoint || ent is MLeader))
                    _reason = $"[{Who(ent)}] no geometry produced";
                if (el.TriangleCount > 0) Elements.Add(el);
                else
                {
                    Skipped++;
                    string cls = ent.GetRXClass().DxfName ?? ent.GetType().Name;
                    // 2D (linii, text, cote) nu e o problema; raportam doar obiectele 3D / custom fara geometrie
                    if (_reason != null)
                        _noGeometry[cls] = _noGeometry.TryGetValue(cls, out var v) ? (v.count + 1, v.reason) : (1, _reason);
                }
            }
            // rezumatul diagnosticului la inceputul listei de avertismente (vizibil in ManageBIM > Project > Log)
            var head = new List<string>();
            // ce contine model space-ul, pe clase (diagnostic pentru desene "goale")
            var hist = new Dictionary<string, int>();
            foreach (ObjectId id in ids) { string c = id.ObjectClass?.DxfName ?? "?"; hist[c] = hist.TryGetValue(c, out int k) ? k + 1 : 1; }
            head.Add("Model space: " + string.Join(", ", hist.OrderByDescending(h => h.Value).Select(h => h.Value + " " + h.Key)));
            foreach (var kv in _skipped.OrderByDescending(k => k.Value))
                head.Add($"Skipped {kv.Value} x {kv.Key}");
            foreach (var kv in _noGeometry.OrderByDescending(k => k.Value.count))
                head.Add($"No geometry for {kv.Value.count} x {kv.Key}: {kv.Value.reason}");
            foreach (var kv in _plantErrors.OrderByDescending(k => k.Value))
                head.Add($"Plant 3D data not read for {kv.Value} x {kv.Key}");
            Warnings.InsertRange(0, head);
        }

        private BimcElement Describe(Transaction tr, Entity ent, LayerTableRecord layer)
        {
            string dxf = ent.GetRXClass().DxfName ?? ent.GetType().Name;
            string kind = ent.GetType().Name;
            var el = new BimcElement
            {
                Type = string.IsNullOrEmpty(dxf) ? kind : dxf,
                Guid = "acad:" + ent.Handle.ToString(),
                Level = layer.Name,
                Name = kind + " [" + ent.Handle + "]",
            };
            el.Add("Item", "Handle", ent.Handle.ToString());
            el.Add("Item", "Layer", ent.Layer);
            el.Add("Item", "Object class", kind);
            el.Add("Item", "Color", ent.Color.IsByLayer ? "ByLayer" : ent.Color.ToString());
            el.Add("Item", "Linetype", ent.Linetype);
            el.Add("Item", "Material", ent.Material);
            if (!ent.Transparency.IsByLayer && ent.Transparency.IsByAlpha) el.Add("Item", "Transparency", (100 - ent.Transparency.Alpha * 100 / 255) + " %");
            try
            {
                Extents3d e = ent.GeometricExtents;
                el.Add("Geometry", "Min", Fmt(e.MinPoint));
                el.Add("Geometry", "Max", Fmt(e.MaxPoint));
            }
            catch { }
            if (ent is Solid3d sol)
            {
                try
                {
                    el.Add("Geometry", "Volume", (sol.MassProperties.Volume * Scale * Scale * Scale).ToString("0.###") + " m³");
                }
                catch { }
            }
            if (ent is BlockReference br)
            {
                string bname = BlockName(tr, br);
                el.Name = bname;
                el.Add("Block", "Name", bname);
                el.Add("Block", "Insertion point", Fmt(br.Position));
                el.Add("Block", "Rotation", (br.Rotation * 180 / Math.PI).ToString("0.##") + "°");
                el.Add("Block", "Scale", $"{br.ScaleFactors.X:0.###} {br.ScaleFactors.Y:0.###} {br.ScaleFactors.Z:0.###}");
                foreach (ObjectId aid in br.AttributeCollection)
                {
                    if (tr.GetObject(aid, OpenMode.ForRead) is AttributeReference ar && !string.IsNullOrEmpty(ar.Tag))
                        el.Add("Attributes", ar.Tag, ar.TextString);
                }
                if (br.IsDynamicBlock)
                {
                    try
                    {
                        foreach (DynamicBlockReferenceProperty p in br.DynamicBlockReferencePropertyCollection)
                            if (p.Value != null && p.PropertyName != "Origin") el.Add("Dynamic block", p.PropertyName, Convert.ToString(p.Value, System.Globalization.CultureInfo.InvariantCulture));
                    }
                    catch { }
                }
            }
            // Plant 3D: class (Pipe, Elbow, Valve, Equipment...) and all project properties
            if (_plant != null)
            {
                try { _plant.Read(ent.ObjectId, el); }
                catch (System.Exception e)
                {
                    // the real error is inside TargetInvocationException (reflection call)
                    System.Exception x = e;
                    while (x is TargetInvocationException && x.InnerException != null) x = x.InnerException;
                    // ObjectDoesNotHaveLink = entitate AutoCAD obisnuita (nu obiect Plant) - normal, nu e eroare
                    if (!x.Message.Contains("ObjectDoesNotHaveLink"))
                    {
                        string key = dxf + ": " + x.GetType().Name + ": " + x.Message;
                        _plantErrors[key] = _plantErrors.TryGetValue(key, out int c) ? c + 1 : 1;
                    }
                }
            }
            // obiectele aplicatiilor (Plant 3D structuri / echipamente, Civil 3D, Architecture, MEP): proprietatile
            // din paleta Properties (marime profil, material, lungime, cota, stil, descriere...) - nu sunt in DataLinks
            if (IsCustom(ent))
            {
                try
                {
                    object com = ent.AcadObject;
                    string grp = dxf.StartsWith("AECC", StringComparison.Ordinal) ? "Civil 3D"
                        : dxf.StartsWith("ACPP", StringComparison.Ordinal) ? "Plant 3D object"
                        : dxf.StartsWith("AEC", StringComparison.Ordinal) ? "Architecture"
                        : "Object";
                    ComProps.Read(com, el, grp, Scale);
                }
                catch { }
            }
            // XDATA application names (e.g. data attached by other plug-ins)
            try
            {
                ResultBuffer rb = ent.XData;
                if (rb != null)
                {
                    string app = null;
                    int i = 0;
                    foreach (TypedValue tv in rb)
                    {
                        if (tv.TypeCode == (short)DxfCode.ExtendedDataRegAppName) { app = tv.Value as string; i = 0; continue; }
                        if (app != null && tv.Value != null && i < 20) el.Add("XData " + app, "Value " + (++i), Convert.ToString(tv.Value, System.Globalization.CultureInfo.InvariantCulture));
                    }
                }
            }
            catch { }
            return el;
        }

        private static string BlockName(Transaction tr, BlockReference br)
        {
            ObjectId bid = br.IsDynamicBlock ? br.DynamicBlockTableRecord : br.BlockTableRecord;
            return ((BlockTableRecord)tr.GetObject(bid, OpenMode.ForRead)).Name;
        }

        private string Fmt(Point3d p) => $"{p.X * Scale:0.###}, {p.Y * Scale:0.###}, {p.Z * Scale:0.###}";

        /// <summary>RGBA of an entity (ByLayer / ByBlock resolved).</summary>
        private uint Resolve(Transaction tr, Entity ent, uint byBlock)
        {
            Color c = ent.Color;
            uint rgb;
            if (c.IsByBlock) rgb = byBlock;
            else
            {
                if (c.IsByLayer)
                {
                    if (!_layerColors.TryGetValue(ent.LayerId, out c))
                    {
                        c = ((LayerTableRecord)tr.GetObject(ent.LayerId, OpenMode.ForRead)).Color;
                        _layerColors[ent.LayerId] = c;
                    }
                }
                try
                {
                    System.Drawing.Color d = c.ColorValue;
                    rgb = (uint)d.R | ((uint)d.G << 8) | ((uint)d.B << 16) | (255u << 24);
                }
                catch { rgb = DefaultColor; }
            }
            try
            {
                if (ent.Transparency.IsByAlpha && !ent.Transparency.IsByLayer)
                    rgb = (rgb & 0x00FFFFFFu) | ((uint)Math.Max((int)ent.Transparency.Alpha, 40) << 24);
            }
            catch { }
            return rgb;
        }

        private void Collect(Transaction tr, Entity ent, Matrix3d xf, BimcElement el, uint color, int depth)
        {
            if (depth > 12) return;
            switch (ent)
            {
                case BlockReference br:
                {
                    Matrix3d m = xf * br.BlockTransform;
                    var btr = (BlockTableRecord)tr.GetObject(br.BlockTableRecord, OpenMode.ForRead);
                    foreach (ObjectId cid in btr)
                    {
                        if (!(tr.GetObject(cid, OpenMode.ForRead) is Entity child) || !child.Visible) continue;
                        if (child is AttributeDefinition) continue;
                        var lay = (LayerTableRecord)tr.GetObject(child.LayerId, OpenMode.ForRead);
                        if (lay.IsOff || lay.IsFrozen) continue;
                        Collect(tr, child, m, el, Resolve(tr, child, color), depth + 1);
                    }
                    return;
                }
                case Face f:
                {
                    var p = new Point3d[4];
                    for (short i = 0; i < 4; i++) p[i] = f.GetVertexAt(i).TransformBy(xf);
                    Tri(el, p[0], p[1], p[2], color);
                    if (p[2].DistanceTo(p[3]) > 1e-9) Tri(el, p[0], p[2], p[3], color);
                    return;
                }
                case PolyFaceMesh pfm:
                {
                    var verts = new List<Point3d>();
                    foreach (ObjectId vid in pfm)
                    {
                        DBObject o = tr.GetObject(vid, OpenMode.ForRead);
                        if (o is PolyFaceMeshVertex v) verts.Add(v.Position.TransformBy(xf));
                        else if (o is FaceRecord fr)
                        {
                            var idx = new List<int>();
                            for (short i = 0; i < 4; i++)
                            {
                                int k = Math.Abs((int)fr.GetVertexAt(i));
                                if (k > 0 && k <= verts.Count) idx.Add(k - 1);
                            }
                            for (int i = 1; i + 1 < idx.Count; i++) Tri(el, verts[idx[0]], verts[idx[i]], verts[idx[i + 1]], color);
                        }
                    }
                    return;
                }
                case SubDMesh sm:
                    AddMesh(el, sm.Vertices, sm.FaceArray, xf, color);
                    return;
                case Solid3d _:
                case Autodesk.AutoCAD.DatabaseServices.Surface _:
                case Region _:
                case PolygonMesh _:
                    if (Facet(ent, xf, el, color, out _)) return;
                    break;
                case Hatch h when depth > 0:
                    // hasura dintr-un obiect custom descompus (ex. gratarele Plant 3D): suprafata plana
                    HatchSurface(h, xf, el, color);
                    return;
                case Curve _:
                case DBText _:
                case MText _:
                case Hatch _:
                case Dimension _:
                case DBPoint _:
                case MLeader _:
                    // curves with thickness become walls, like in ManageBIM's DXF import
                    if (ent is Curve cu && Thickness(cu) != 0) { Extrude(cu, xf, el, color); }
                    return;
            }
            // custom / proxy objects (AutoCAD Architecture, MEP, Civil 3D, Plant 3D...): try the faceter, then explode
            Custom(tr, ent, xf, el, color, depth);
        }

        /// <summary>
        /// Obiect definit de o aplicatie (Plant 3D: ACPPSTRUCTUREBEAM, ACPPEQUIPMENT...; Architecture, MEP...).
        /// Plant 3D nu inregistreaza tipuri .NET proprii pentru toate clasele - obiectul apare ca tip de baza
        /// AutoCAD (ex. Curve) - deci decidem dupa clasa ObjectARX: cele de baza vin din "ObjectDBX Classes".
        /// </summary>
        private static bool IsCustom(Entity ent)
        {
            try
            {
                RXClass c = ent.GetRXClass();
                string app = c.AppName ?? "";
                if (!app.StartsWith("ObjectDBX", StringComparison.OrdinalIgnoreCase)) return true;
                string dxf = c.DxfName ?? "";
                if (dxf.StartsWith("ACPP", StringComparison.Ordinal) || dxf.StartsWith("AEC", StringComparison.Ordinal)) return true;
            }
            catch { }
            string ns = ent.GetType().Namespace ?? "";
            return !ns.StartsWith("Autodesk.AutoCAD.DatabaseServices", StringComparison.Ordinal);
        }

        private static string Who(Entity ent)
        {
            string app = "";
            try { app = ent.GetRXClass().AppName; } catch { }
            return $"{ent.GetType().FullName}, app '{app}'";
        }

        private void Custom(Transaction tr, Entity ent, Matrix3d xf, BimcElement el, uint color, int depth)
        {
            if (Facet(ent, xf, el, color, out string facetErr)) return;
            DBObjectCollection parts = new DBObjectCollection();
            try { ent.Explode(parts); }
            catch (System.Exception e)
            {
                if (depth == 0) _reason = $"faceter: {facetErr}; explode: {e.Message}";
                return;
            }
            if (depth == 0)
            {
                var kinds = parts.Cast<DBObject>().GroupBy(o => o.GetRXClass().DxfName ?? o.GetType().Name).Select(g => g.Count() + " " + g.Key);
                _reason = $"faceter: {facetErr}; explode: {parts.Count} parts ({string.Join(", ", kinds)})";
            }
            foreach (DBObject o in parts)
            {
                if (o is Entity pe)
                {
                    try { Collect(tr, pe, xf, el, pe.Color.IsByBlock ? color : Resolve(tr, pe, color), depth + 1); }
                    catch { }
                }
                o.Dispose();
            }
        }

        /// <summary>Conturul exterior al unei hasuri ca suprafata (triunghiuri in planul hasurii).</summary>
        private void HatchSurface(Hatch h, Matrix3d xf, BimcElement el, uint color)
        {
            Matrix3d toWorld = Matrix3d.PlaneToWorld(h.Normal) * Matrix3d.Displacement(h.Normal * h.Elevation);
            for (int i = 0; i < h.NumberOfLoops; i++)
            {
                HatchLoop loop;
                try { loop = h.GetLoopAt(i); } catch { continue; }
                // doar conturul exterior (golurile interioare ar cere triangulare cu gauri)
                if ((loop.LoopType & HatchLoopTypes.External) == 0 && h.NumberOfLoops > 1 && i > 0) continue;
                var pts = new List<Point3d>();
                if (loop.IsPolyline)
                {
                    foreach (BulgeVertex bv in loop.Polyline) pts.Add(new Point3d(bv.Vertex.X, bv.Vertex.Y, 0));
                }
                else
                {
                    foreach (Curve2d c in loop.Curves)
                    {
                        try
                        {
                            var sp = c.StartPoint;
                            pts.Add(new Point3d(sp.X, sp.Y, 0));
                        }
                        catch { }
                    }
                }
                if (pts.Count < 3) continue;
                var w = pts.Select(p => p.TransformBy(toWorld).TransformBy(xf)).ToList();
                // conturul gratarelor e convex (dreptunghiuri / poligoane simple): triangulare in evantai
                for (int k = 1; k + 1 < w.Count; k++) Tri(el, w[0], w[k], w[k + 1], color);
            }
        }

        private static double Thickness(Curve c)
        {
            switch (c)
            {
                case Line l: return l.Thickness;
                case Circle ci: return ci.Thickness;
                case Arc a: return a.Thickness;
                case Polyline p: return p.Thickness;
                case Polyline2d p2: return p2.Thickness;
                default: return 0;
            }
        }

        private void Extrude(Curve c, Matrix3d xf, BimcElement el, uint color)
        {
            double t = Thickness(c);
            Vector3d n = (c is Line l ? l.Normal : c is Circle ci ? ci.Normal : c is Arc a ? a.Normal : c is Polyline p ? p.Normal : Vector3d.ZAxis) * t;
            var pts = new List<Point3d>();
            try
            {
                double s0 = c.StartParam, s1 = c.EndParam;
                int seg = c is Line ? 1 : 48;
                if (c is Polyline pl && !pl.HasBulges) { for (int i = 0; i < pl.NumberOfVertices; i++) pts.Add(pl.GetPoint3dAt(i)); if (pl.Closed) pts.Add(pl.GetPoint3dAt(0)); }
                else for (int i = 0; i <= seg; i++) pts.Add(c.GetPointAtParameter(s0 + (s1 - s0) * i / seg));
            }
            catch { return; }
            for (int i = 0; i + 1 < pts.Count; i++)
            {
                Point3d a0 = pts[i].TransformBy(xf), b0 = pts[i + 1].TransformBy(xf);
                Point3d a1 = (pts[i] + n).TransformBy(xf), b1 = (pts[i + 1] + n).TransformBy(xf);
                Tri(el, a0, b0, b1, color);
                Tri(el, a0, b1, a1, color);
            }
        }

        private bool Facet(Entity ent, Matrix3d xf, BimcElement el, uint color, out string error)
        {
            error = null;
            try
            {
                // abaterea coardei: 2 mm .. 5 cm, ~1/1000 din diagonala - conducte, rezervoare si coturi raman
                // rotunde (inainte: diagonala/400 si 30° - fatete vizibile pe echipamentele mari)
                double dev = 0.002 / Scale;
                try
                {
                    Extents3d e = ent.GeometricExtents;
                    dev = Math.Min(Math.Max(dev, e.MinPoint.DistanceTo(e.MaxPoint) / 1000.0), 0.05 / Scale);
                    // terenuri / suprafete foarte mari (Civil 3D): pana la 1/2000 din diagonala, ca sa nu explodeze numarul de triunghiuri
                    double diag = e.MinPoint.DistanceTo(e.MaxPoint) * Scale;
                    if (diag > 200) dev = Math.Max(dev, e.MinPoint.DistanceTo(e.MaxPoint) / 2000.0);
                }
                catch { }
                var fd = new MeshFaceterData
                {
                    FaceterMeshType = 2, // triangles
                    FaceterDevSurface = dev,
                    FaceterDevNormal = 15,
                    FaceterGridRatio = 0,
                    FaceterMaxEdgeLength = 0,
                    FaceterMaxGrid = 256,
                    FaceterMinUGrid = 0,
                    FaceterMinVGrid = 0,
                };
                MeshDataCollection md = SubDMesh.GetObjectMesh(ent, fd);
                if (md.VertexArray == null || md.VertexArray.Count == 0) { error = "empty mesh"; return false; }
                AddMesh(el, md.VertexArray, md.FaceArray, xf, color);
                return true;
            }
            catch (System.Exception e) { error = e.Message; return false; }
        }

        private void AddMesh(BimcElement el, Point3dCollection v, Int32Collection faces, Matrix3d xf, uint color)
        {
            var p = new Point3d[v.Count];
            for (int i = 0; i < v.Count; i++) p[i] = v[i].TransformBy(xf);
            int k = 0;
            while (k < faces.Count)
            {
                int n = faces[k];
                if (n < 3 || k + n >= faces.Count) { k += Math.Max(n, 0) + 1; continue; }
                int a = faces[k + 1];
                for (int i = 2; i < n; i++)
                {
                    int b = faces[k + i], c = faces[k + i + 1];
                    if (a < p.Length && b < p.Length && c < p.Length) Tri(el, p[a], p[b], p[c], color);
                }
                k += n + 1;
            }
        }

        private void Tri(BimcElement el, Point3d a, Point3d b, Point3d c, uint color)
        {
            el.AddTriangle(new V3(a.X * Scale, a.Y * Scale, a.Z * Scale), new V3(b.X * Scale, b.Y * Scale, b.Z * Scale), new V3(c.X * Scale, c.Y * Scale, c.Z * Scale), color);
        }
    }
}

namespace ManageBIM.Acad
{
    /// <summary>
    /// Plant 3D project data through the DataLinksManager, accessed by reflection so the same DLL
    /// also loads in plain AutoCAD (where Plant 3D is not present).
    /// </summary>
    internal sealed class PlantData
    {
        private object _mgr;
        private MethodInfo _findRow, _props, _cls;

        public static PlantData TryCreate(Database db, List<string> warnings)
        {
            try
            {
                Assembly asm = AppDomain.CurrentDomain.GetAssemblies().FirstOrDefault(a => string.Equals(a.GetName().Name, "PnPDataLinks", StringComparison.OrdinalIgnoreCase));
                if (asm == null) return null;
                Type t = asm.GetType("Autodesk.ProcessPower.DataLinks.DataLinksManager");
                if (t == null) return null;
                var p = new PlantData();
                p._findRow = t.GetMethod("FindAcPpRowId", new[] { typeof(ObjectId) });
                p._props = t.GetMethods(BindingFlags.Public | BindingFlags.Instance)
                    .Where(m => m.Name == "GetAllProperties" && m.GetParameters().Length >= 1 && m.GetParameters()[0].ParameterType == typeof(int))
                    .OrderByDescending(m => m.GetParameters().Length).FirstOrDefault();
                p._cls = t.GetMethod("GetObjectClassname", new[] { typeof(int) });
                if (p._findRow == null || p._props == null) return null;

                // 1. the manager of this drawing
                try
                {
                    MethodInfo get = t.GetMethod("GetManager", BindingFlags.Public | BindingFlags.Static, null, new[] { typeof(Database) }, null);
                    p._mgr = get?.Invoke(null, new object[] { db });
                }
                catch { }
                // 2. the Piping part of the current Plant project
                if (p._mgr == null) p._mgr = FromProject();
                return p._mgr == null ? null : p;
            }
            catch (System.Exception e)
            {
                warnings.Add("Plant 3D data not available: " + e.Message);
                return null;
            }
        }

        private static object FromProject()
        {
            try
            {
                Assembly pm = AppDomain.CurrentDomain.GetAssemblies().FirstOrDefault(a => a.GetType("Autodesk.ProcessPower.PlantInstance.PlantApplication") != null);
                if (pm == null) return null;
                Type app = pm.GetType("Autodesk.ProcessPower.PlantInstance.PlantApplication");
                object project = app.GetProperty("CurrentProject", BindingFlags.Public | BindingFlags.Static)?.GetValue(null);
                if (project == null) return null;
                foreach (string part in new[] { "Piping", "PnId" })
                {
                    try
                    {
                        MethodInfo pp = project.GetType().GetMethods().FirstOrDefault(m => m.Name == "ProjectParts" || (m.Name == "get_Item" && m.GetParameters().Length == 1 && m.GetParameters()[0].ParameterType == typeof(string)));
                        object parts = project.GetType().GetProperty("ProjectParts")?.GetValue(project);
                        object prj = null;
                        if (parts != null)
                        {
                            PropertyInfo idx = parts.GetType().GetProperties().FirstOrDefault(x => x.GetIndexParameters().Length == 1 && x.GetIndexParameters()[0].ParameterType == typeof(string));
                            prj = idx?.GetValue(parts, new object[] { part });
                        }
                        if (prj == null && pp != null && pp.Name == "get_Item") prj = pp.Invoke(project, new object[] { part });
                        object mgr = prj?.GetType().GetProperty("DataLinksManager")?.GetValue(prj);
                        if (mgr != null) return mgr;
                    }
                    catch { }
                }
            }
            catch { }
            return null;
        }

        /// <summary>Toate proprietatile unui rand (nume, valoare), inclusiv cele goale.</summary>
        private List<(string, string)> Props(int row)
        {
            var res = new List<(string, string)>();
            var args = _props.GetParameters().Select((pi, i) => i == 0 ? (object)row : pi.ParameterType == typeof(bool) ? (object)true : (pi.HasDefaultValue ? pi.DefaultValue : null)).ToArray();
            if (!(_props.Invoke(_mgr, args) is System.Collections.IEnumerable list)) return res;
            foreach (object kv in list)
            {
                if (kv == null) continue;
                Type kt = kv.GetType();
                string k = Convert.ToString(kt.GetProperty("Key")?.GetValue(kv), System.Globalization.CultureInfo.InvariantCulture);
                string v = Convert.ToString(kt.GetProperty("Value")?.GetValue(kv), System.Globalization.CultureInfo.InvariantCulture);
                if (!string.IsNullOrEmpty(k)) res.Add((k, v ?? ""));
            }
            return res;
        }

        /// <summary>Randurile legate printr-o relatie Plant 3D (ex. piesa -> Line Group), prin reflectie
        /// (DataLinksManager.GetRelatedRowIds(relatie, rol1, rand, rol2)); lista goala daca nu exista.</summary>
        private List<int> Related(int row, string rel, string role1, string role2)
        {
            var res = new List<int>();
            try
            {
                MethodInfo m = _mgr.GetType().GetMethods(BindingFlags.Public | BindingFlags.Instance).FirstOrDefault(x =>
                {
                    var p = x.GetParameters();
                    return x.Name == "GetRelatedRowIds" && p.Length == 4 && p[0].ParameterType == typeof(string) && p[1].ParameterType == typeof(string) && p[2].ParameterType == typeof(int) && p[3].ParameterType == typeof(string);
                });
                if (m == null) return res;
                if (m.Invoke(_mgr, new object[] { rel, role1, row, role2 }) is System.Collections.IEnumerable ids)
                    foreach (object o in ids)
                        if (o != null) res.Add(Convert.ToInt32(o, System.Globalization.CultureInfo.InvariantCulture));
            }
            catch { }
            return res;
        }

        public void Read(ObjectId id, BimcElement el)
        {
            int row = Convert.ToInt32(_findRow.Invoke(_mgr, new object[] { id }));
            if (row <= 0) return;
            if (_cls != null)
            {
                try
                {
                    string cls = _cls.Invoke(_mgr, new object[] { row }) as string;
                    if (!string.IsNullOrEmpty(cls))
                    {
                        el.Add("Item", "AutoCAD type", el.Type);
                        el.Type = cls;
                        el.Add("Plant 3D", "Class", cls);
                    }
                }
                catch { }
            }
            string tag = null, desc = null;
            foreach (var (k, v) in Props(row))
            {
                // si valorile goale: in ManageBIM se vad TOATE proprietatile clasei (inainte lipseau cele necompletate)
                el.AddKeepEmpty("Plant 3D", k, v);
                if (string.IsNullOrEmpty(v)) continue;
                if (k == "Tag" || k == "PartTag") tag = tag ?? v;
                if (k == "PartFamilyLongDesc" || k == "Description") desc = desc ?? v;
            }
            // linia (Line Number, Service, Spec...) nu e pe piesa, ci pe randul Line Group legat de ea
            foreach (int lg in Related(row, "P3dLineGroupPartRelationship", "Part", "LineGroup"))
            {
                foreach (var (k, v) in Props(lg))
                {
                    if (string.IsNullOrEmpty(v)) continue;
                    el.Add("Plant 3D Line", k, v);
                    if (k == "Tag" || k == "LineNumberTag") el.Add("Plant 3D", "Line Number", v);
                }
                break;
            }
            // echipamentul unui stut (nozzle) si invers: legatura cu echipamentul gazda
            foreach (int eq in Related(row, "P3dEquipmentNozzleRelationship", "Nozzle", "Equipment"))
            {
                foreach (var (k, v) in Props(eq))
                    if (!string.IsNullOrEmpty(v) && (k == "Tag" || k == "Description" || k == "ClassName")) el.Add("Plant 3D Equipment", k, v);
                break;
            }
            // "?-?" / "?" = Tag neatribuit in Plant 3D: numele devine descrierea piesei (ex. "Gate Valve, 150 LB, RF...")
            bool realTag = !string.IsNullOrEmpty(tag) && tag.IndexOf('?') < 0;
            if (realTag) el.Name = tag;
            else if (!string.IsNullOrEmpty(desc)) el.Name = desc;
        }
    }
}
