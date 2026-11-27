using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using Autodesk.AutoCAD.ApplicationServices;
using Autodesk.AutoCAD.Colors;
using Autodesk.AutoCAD.DatabaseServices;
using Autodesk.AutoCAD.EditorInput;
using Autodesk.AutoCAD.Geometry;
using Autodesk.AutoCAD.Runtime;
using AcApp = Autodesk.AutoCAD.ApplicationServices.Core.Application;

namespace ManageBIM.Acad
{
    /// <summary>
    /// Models from ManageBIM as a reference in AutoCAD / Plant 3D (.mbref file, from ManageBIM: Output › Reference):
    /// one 3D mesh (SubDMesh) per element, with its colour, on the layer "MBREF-nume" (locked). The file path
    /// is in the layer description, so that the reference can be reloaded.
    ///   MBREFERENCE  - Load (you pick the file)
    ///   MBREFRELOAD  - Reload (the same file, updated in ManageBIM)
    ///   MBREFUNLOAD  - Unload (deletes the meshes and the layer)
    /// </summary>
    public class RefCommands
    {
        private const string Prefix = "MBREF-";

        [CommandMethod("MBREFERENCE", CommandFlags.Modal | CommandFlags.NoBlockEditor)]
        public void Load() => Safe.Command("MBREFERENCE", LoadBody);

        private void LoadBody()
        {
            Document doc = AcApp.DocumentManager.MdiActiveDocument;
            if (doc == null) return;
            string dir = Directory.Exists(MbRef.Folder()) ? MbRef.Folder() : Environment.GetFolderPath(Environment.SpecialFolder.MyDocuments);
            var po = new PromptOpenFileOptions("ManageBIM reference") { Filter = "ManageBIM reference (*.mbref)|*.mbref", InitialDirectory = dir, DialogCaption = "ManageBIM reference" };
            var pr = doc.Editor.GetFileNameForOpen(po);
            if (pr.Status != PromptStatus.OK) return;
            LoadFile(doc, pr.StringResult);
        }

        [CommandMethod("MBREFRELOAD", CommandFlags.Modal | CommandFlags.NoBlockEditor)]
        public void Reload() => Safe.Command("MBREFRELOAD", ReloadBody);

        private void ReloadBody()
        {
            Document doc = AcApp.DocumentManager.MdiActiveDocument;
            if (doc == null) return;
            var r = Choose(doc, "Reload");
            if (r == null) return;
            if (!File.Exists(r.Item2))
            {
                doc.Editor.WriteMessage($"\nManageBIM: the reference file was not found: {r.Item2}. Use MBREFERENCE to choose it again.");
                return;
            }
            LoadFile(doc, r.Item2);
        }

        [CommandMethod("MBREFUNLOAD", CommandFlags.Modal | CommandFlags.NoBlockEditor)]
        public void Unload() => Safe.Command("MBREFUNLOAD", UnloadBody);

        private void UnloadBody()
        {
            Document doc = AcApp.DocumentManager.MdiActiveDocument;
            if (doc == null) return;
            var r = Choose(doc, "Unload");
            if (r == null) return;
            int n = Erase(doc.Database, r.Item1, true);
            doc.Editor.WriteMessage($"\nManageBIM: {r.Item1.Substring(Prefix.Length)} unloaded ({n} meshes removed).");
            doc.Editor.Regen();
        }

        /// <summary>The references in the drawing (the MBREF-* layers) and picking one: (layer, path).</summary>
        private static Tuple<string, string> Choose(Document doc, string verb)
        {
            var refs = new List<Tuple<string, string>>();
            using (var tr = doc.Database.TransactionManager.StartTransaction())
            {
                var lt = (LayerTable)tr.GetObject(doc.Database.LayerTableId, OpenMode.ForRead);
                foreach (ObjectId id in lt)
                {
                    var l = (LayerTableRecord)tr.GetObject(id, OpenMode.ForRead);
                    if (l.Name.StartsWith(Prefix, StringComparison.OrdinalIgnoreCase)) refs.Add(Tuple.Create(l.Name, l.Description ?? ""));
                }
                tr.Commit();
            }
            if (refs.Count == 0)
            {
                doc.Editor.WriteMessage("\nManageBIM: no reference loaded in this drawing (MBREFERENCE loads one).");
                return null;
            }
            if (refs.Count == 1) return refs[0];
            doc.Editor.WriteMessage("\nManageBIM references:");
            for (int i = 0; i < refs.Count; i++) doc.Editor.WriteMessage($"\n  {i + 1}. {refs[i].Item1.Substring(Prefix.Length)}");
            var pi = new PromptIntegerOptions($"\n{verb} which reference [1-{refs.Count}] <1>: ") { AllowNone = true, LowerLimit = 1, UpperLimit = refs.Count, DefaultValue = 1, UseDefaultValue = true };
            var res = doc.Editor.GetInteger(pi);
            if (res.Status != PromptStatus.OK) return null;
            return refs[res.Value - 1];
        }

        /// <summary>Deletes the meshes on the layer (and the layer, if `layerToo`). Returns how many were deleted.</summary>
        private static int Erase(Database db, string layer, bool layerToo)
        {
            int n = 0;
            using (var tr = db.TransactionManager.StartTransaction())
            {
                var lt = (LayerTable)tr.GetObject(db.LayerTableId, OpenMode.ForRead);
                if (!lt.Has(layer)) { tr.Commit(); return 0; }
                var lid = lt[layer];
                var ltr = (LayerTableRecord)tr.GetObject(lid, OpenMode.ForWrite);
                ltr.IsLocked = false;
                var ms = (BlockTableRecord)tr.GetObject(SymbolUtilityServices.GetBlockModelSpaceId(db), OpenMode.ForRead);
                foreach (ObjectId id in ms)
                {
                    var ent = tr.GetObject(id, OpenMode.ForRead) as Entity;
                    if (ent != null && ent.LayerId == lid)
                    {
                        ent.UpgradeOpen();
                        ent.Erase();
                        n++;
                    }
                }
                if (layerToo && db.Clayer != lid)
                {
                    try { ltr.Erase(); } catch { }
                }
                tr.Commit();
            }
            return n;
        }

        private static void LoadFile(Document doc, string path)
        {
            Database db = doc.Database;
            Editor ed = doc.Editor;
            List<MbRefElement> items;
            double[] origin;
            try { items = MbRef.Read(path, out origin); }
            catch (System.Exception ex) { ed.WriteMessage("\nManageBIM: " + ex.Message); return; }
            string name = Path.GetFileNameWithoutExtension(path);
            foreach (char c in "<>/\\\":;?*|,=`") name = name.Replace(c, '_');
            string layer = Prefix + name;
            // metres -> drawing units
            double s;
            switch (db.Insunits)
            {
                case UnitsValue.Millimeters: s = 1000; break;
                case UnitsValue.Centimeters: s = 100; break;
                case UnitsValue.Decimeters: s = 10; break;
                case UnitsValue.Inches: s = 1 / 0.0254; break;
                case UnitsValue.Feet: s = 1 / 0.3048; break;
                case UnitsValue.Meters: s = 1; break;
                default: s = 1000; break;
            }
            // reload: the old meshes of the same reference are deleted, the layer stays
            int old = Erase(db, layer, false);
            int made = 0;
            using (var tr = db.TransactionManager.StartTransaction())
            {
                var lt = (LayerTable)tr.GetObject(db.LayerTableId, OpenMode.ForWrite);
                LayerTableRecord ltr;
                if (lt.Has(layer)) ltr = (LayerTableRecord)tr.GetObject(lt[layer], OpenMode.ForWrite);
                else
                {
                    ltr = new LayerTableRecord { Name = layer };
                    lt.Add(ltr);
                    tr.AddNewlyCreatedDBObject(ltr, true);
                }
                ltr.Description = path;
                ltr.IsLocked = false;
                var ms = (BlockTableRecord)tr.GetObject(SymbolUtilityServices.GetBlockModelSpaceId(db), OpenMode.ForWrite);
                foreach (var e in items)
                {
                    if (e.Tris.Length < 9) continue;
                    // welded vertices (smaller mesh), triangular faces
                    var map = new Dictionary<(long, long, long), int>();
                    var verts = new Point3dCollection();
                    var faces = new Int32Collection();
                    for (int i = 0; i + 8 < e.Tris.Length; i += 9)
                    {
                        var idx = new int[3];
                        for (int k = 0; k < 3; k++)
                        {
                            double x = (origin[0] + e.Tris[i + k * 3]) * s, y = (origin[1] + e.Tris[i + k * 3 + 1]) * s, z = (origin[2] + e.Tris[i + k * 3 + 2]) * s;
                            var key = ((long)Math.Round(x * 1000), (long)Math.Round(y * 1000), (long)Math.Round(z * 1000));
                            if (!map.TryGetValue(key, out int vi))
                            {
                                vi = verts.Count;
                                verts.Add(new Point3d(x, y, z));
                                map[key] = vi;
                            }
                            idx[k] = vi;
                        }
                        if (idx[0] == idx[1] || idx[1] == idx[2] || idx[0] == idx[2]) continue;
                        faces.Add(3);
                        faces.Add(idx[0]);
                        faces.Add(idx[1]);
                        faces.Add(idx[2]);
                    }
                    if (faces.Count == 0) continue;
                    try
                    {
                        var mesh = new SubDMesh();
                        mesh.SetDatabaseDefaults();
                        mesh.SetSubDMesh(verts, faces, 0);
                        mesh.Layer = layer;
                        mesh.Color = Color.FromRgb(e.R, e.G, e.B);
                        if (e.A < 255) mesh.Transparency = new Transparency((byte)e.A);
                        ms.AppendEntity(mesh);
                        tr.AddNewlyCreatedDBObject(mesh, true);
                        made++;
                    }
                    catch { }
                }
                ltr.IsLocked = true;
                tr.Commit();
            }
            ed.WriteMessage($"\nManageBIM: {name} {(old > 0 ? "reloaded" : "loaded")} - {made} meshes on the locked layer {layer}. MBREFRELOAD updates it, MBREFUNLOAD removes it.");
            ed.Regen();
        }
    }
}
