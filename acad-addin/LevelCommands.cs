using System;
using System.Collections.Generic;
using System.Globalization;
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
    /// MBLEVELS: the levels sent from ManageBIM (View › Grids &amp; Levels › Send Levels, the file
    /// %LOCALAPPDATA%\ManageBIM\Outbox\levels.txt) brought into the drawing - Plant 3D and AutoCAD have no levels:
    ///  - a level line (rectangle around the model) and a label at each elevation, on the MB-Levels layer;
    ///  - a named UCS "Level - &lt;name&gt;" at each elevation (UCS › Named: working directly on the level);
    ///  - the level on each object (XData MANAGEBIM_LEVEL), by its bottom elevation - ManageBIM reads it as a property.
    /// </summary>
    public class LevelCommands
    {
        private const string Layer = "MB-Levels";
        private const string App = "MANAGEBIM_LEVEL";

        [CommandMethod("MBLEVELS", CommandFlags.Modal | CommandFlags.NoBlockEditor)]
        public void Levels() => Safe.Command("MBLEVELS", LevelsBody);

        private void LevelsBody()
        {
            Document doc = AcApp.DocumentManager.MdiActiveDocument;
            if (doc == null) return;
            Database db = doc.Database;
            Editor ed = doc.Editor;
            string file = Environment.GetEnvironmentVariable("MB_LEVELS_FILE");
            if (string.IsNullOrEmpty(file))
                file = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "ManageBIM", "Outbox", "levels.txt");
            List<(string Name, double Z)> levels;
            try { levels = Read(file); }
            catch (System.Exception ex)
            {
                ed.WriteMessage("\nManageBIM: no levels yet - in ManageBIM: View › Grids & Levels › Send Levels. (" + ex.Message + ")\n");
                return;
            }
            // meters -> drawing units
            double s = 1.0 / UnitScale(db);
            Extents3d ext;
            try { ext = new Extents3d(db.Extmin, db.Extmax); if (ext.MinPoint.X > ext.MaxPoint.X) throw new InvalidOperationException(); }
            catch { ext = new Extents3d(new Point3d(-10 * s, -10 * s, 0), new Point3d(10 * s, 10 * s, 0)); }
            double dx = ext.MaxPoint.X - ext.MinPoint.X, dy = ext.MaxPoint.Y - ext.MinPoint.Y;
            double margin = Math.Max(dx, dy) * 0.05 + 1.0 * s;
            double x0 = ext.MinPoint.X - margin, y0 = ext.MinPoint.Y - margin, x1 = ext.MaxPoint.X + margin, y1 = ext.MaxPoint.Y + margin;
            double th = Math.Max(Math.Max(dx, dy) * 0.012, 0.15 * s);
            int lines = 0, ucs = 0, tagged = 0;
            using (doc.LockDocument())
            using (Transaction tr = db.TransactionManager.StartTransaction())
            {
                ObjectId layer = EnsureLayer(tr, db);
                EnsureRegApp(tr, db);
                var ms = (BlockTableRecord)tr.GetObject(SymbolUtilityServices.GetBlockModelSpaceId(db), OpenMode.ForWrite);
                // the levels sent last time are replaced (not duplicated)
                foreach (ObjectId id in ms)
                {
                    var ent = tr.GetObject(id, OpenMode.ForRead) as Entity;
                    if (ent != null && ent.LayerId == layer)
                    {
                        ent.UpgradeOpen();
                        ent.Erase();
                    }
                }
                var ucsTable = (UcsTable)tr.GetObject(db.UcsTableId, OpenMode.ForWrite);
                foreach (var (name, zm) in levels)
                {
                    double z = zm * s;
                    var pl = new Polyline(4) { Closed = true, Elevation = z, LayerId = layer };
                    pl.AddVertexAt(0, new Point2d(x0, y0), 0, 0, 0);
                    pl.AddVertexAt(1, new Point2d(x1, y0), 0, 0, 0);
                    pl.AddVertexAt(2, new Point2d(x1, y1), 0, 0, 0);
                    pl.AddVertexAt(3, new Point2d(x0, y1), 0, 0, 0);
                    ms.AppendEntity(pl);
                    tr.AddNewlyCreatedDBObject(pl, true);
                    var txt = new DBText
                    {
                        TextString = $"{name}  {zm:+0.00;-0.00} m",
                        Height = th,
                        Position = new Point3d(x0, y0 - th * 1.6, z),
                        LayerId = layer,
                    };
                    ms.AppendEntity(txt);
                    tr.AddNewlyCreatedDBObject(txt, true);
                    lines++;
                    // named UCS at the level elevation
                    string un = SafeName("Level - " + name);
                    UcsTableRecord rec;
                    if (ucsTable.Has(un)) rec = (UcsTableRecord)tr.GetObject(ucsTable[un], OpenMode.ForWrite);
                    else
                    {
                        rec = new UcsTableRecord { Name = un };
                        ucsTable.Add(rec);
                        tr.AddNewlyCreatedDBObject(rec, true);
                    }
                    rec.Origin = new Point3d(0, 0, z);
                    rec.XAxis = Vector3d.XAxis;
                    rec.YAxis = Vector3d.YAxis;
                    ucs++;
                }
                // the level of each object: the highest level at or below the object base (+30 cm tolerance)
                double tol = 0.3 * s;
                foreach (ObjectId id in ms)
                {
                    try
                    {
                        var ent = tr.GetObject(id, OpenMode.ForRead) as Entity;
                        if (ent == null || ent.LayerId == layer || ent.IsErased) continue;
                        Extents3d e;
                        try { e = ent.GeometricExtents; } catch { continue; }
                        double bottom = e.MinPoint.Z;
                        var lv = levels.Where(l => l.Z * s <= bottom + tol).Select(l => l.Name).LastOrDefault() ?? levels[0].Name;
                        ent.UpgradeOpen();
                        ent.XData = new ResultBuffer(new TypedValue((int)DxfCode.ExtendedDataRegAppName, App), new TypedValue((int)DxfCode.ExtendedDataAsciiString, lv));
                        tagged++;
                    }
                    catch { /* obiecte blocate / proxy: sarite */ }
                }
                tr.Commit();
            }
            ed.WriteMessage($"\nManageBIM: {lines} level lines on layer {Layer}, {ucs} named UCS (UCS › Named › \"Level - ...\"), level written on {tagged} objects (XData {App}).\n");
        }

        internal static List<(string Name, double Z)> Read(string file)
        {
            if (!File.Exists(file)) throw new FileNotFoundException("levels.txt not found", file);
            var lines = File.ReadAllLines(file);
            if (lines.Length == 0 || !lines[0].StartsWith("MBLEVELS")) throw new InvalidDataException("Not a ManageBIM levels file.");
            var outl = new List<(string, double)>();
            foreach (string l in lines.Skip(1))
            {
                string[] t = l.Split('\t');
                if (t.Length >= 3 && t[0] == "L" && double.TryParse(t[2], NumberStyles.Float, CultureInfo.InvariantCulture, out double z)) outl.Add((t[1], z));
            }
            if (outl.Count == 0) throw new InvalidDataException("The file has no levels.");
            return outl.OrderBy(x => x.Item2).ToList();
        }

        /// <summary>meters per drawing unit (as in the export)</summary>
        internal static double UnitScale(Database db)
        {
            switch (db.Insunits)
            {
                case UnitsValue.Millimeters: return 0.001;
                case UnitsValue.Centimeters: return 0.01;
                case UnitsValue.Decimeters: return 0.1;
                case UnitsValue.Meters: return 1.0;
                case UnitsValue.Inches: return 0.0254;
                case UnitsValue.Feet: return 0.3048;
                default:
                    try
                    {
                        double size = db.Extmin.DistanceTo(db.Extmax);
                        return size > 2000 ? 0.001 : 1.0;
                    }
                    catch { return 1.0; }
            }
        }

        private static string SafeName(string n)
        {
            var bad = "<>/\\\":;?*|,=`".ToCharArray();
            string r = new string(n.Select(c => bad.Contains(c) ? '_' : c).ToArray()).Trim();
            return r.Length > 200 ? r.Substring(0, 200) : r;
        }

        private static ObjectId EnsureLayer(Transaction tr, Database db)
        {
            var lt = (LayerTable)tr.GetObject(db.LayerTableId, OpenMode.ForRead);
            if (lt.Has(Layer)) return lt[Layer];
            lt.UpgradeOpen();
            var rec = new LayerTableRecord { Name = Layer, Color = Color.FromColorIndex(ColorMethod.ByAci, 4) };
            ObjectId id = lt.Add(rec);
            tr.AddNewlyCreatedDBObject(rec, true);
            return id;
        }

        private static void EnsureRegApp(Transaction tr, Database db)
        {
            var rt = (RegAppTable)tr.GetObject(db.RegAppTableId, OpenMode.ForRead);
            if (rt.Has(App)) return;
            rt.UpgradeOpen();
            var r = new RegAppTableRecord { Name = App };
            rt.Add(r);
            tr.AddNewlyCreatedDBObject(r, true);
        }
    }
}
