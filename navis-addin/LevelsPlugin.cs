using System;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.Linq;
using System.Windows.Forms;
using Autodesk.Navisworks.Api;
using Autodesk.Navisworks.Api.Plugins;
using NwApp = Autodesk.Navisworks.Api.Application;

namespace ManageBIM.Navis
{
    /// <summary>
    /// "ManageBIM Levels": the levels sent from ManageBIM (View › Grids &amp; Levels › Send Levels, the file
    /// %LOCALAPPDATA%\ManageBIM\Outbox\levels.txt) - Navisworks has no levels for DWG / Plant 3D models:
    ///  - one selection set per level (the "ManageBIM levels" folder in Sets): the elements by their bottom elevation;
    ///    used for hiding, filtering and as Selection A / B in Clash Detective;
    ///  - one plan viewpoint per level (Saved Viewpoints › "ManageBIM levels"): top view, orthographic,
    ///    with a section box between the level and the next one.
    /// The "quiet" and "file=&lt;path&gt;" parameters for Automation / tests; code 0 = done, 1 = error.
    /// </summary>
    [Plugin("ManageBIMLevels", "MBIM", DisplayName = "ManageBIM Levels", ToolTip = "Levels from ManageBIM as selection sets and plan viewpoints")]
    [AddInPlugin(AddInLocation.None)]
    public class LevelsPlugin : AddInPlugin
    {
        private const string Folder = "ManageBIM levels";

        public override int Execute(params string[] parameters) => Safe.Plugin(GetType().Name.Replace("Plugin", ""), parameters, () => Run(parameters));

        private int Run(string[] parameters)
        {
            bool quiet = parameters != null && parameters.Contains("quiet");
            string log = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "ManageBIM", "Outbox", "levels.log");
            void Say(string m) { if (!quiet) MessageBox.Show(m, "ManageBIM"); else try { File.WriteAllText(log, m); } catch { } }
            Document doc = NwApp.ActiveDocument;
            if (doc == null || doc.Models.Count == 0)
            {
                Say("Open the model first.");
                return 1;
            }
            string file = parameters?.FirstOrDefault(p => p.StartsWith("file="))?.Substring(5)
                          ?? Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "ManageBIM", "Outbox", "levels.txt");
            List<(string Name, double Z)> levels;
            try { levels = Read(file); }
            catch (Exception ex)
            {
                Say("No levels from ManageBIM yet.\nIn ManageBIM: View › Grids & Levels › Send Levels.\n\n" + ex.Message);
                return 1;
            }
            double s = UnitConversion.ScaleFactor(Units.Meters, doc.Units);
            // the elements with geometry, on the level below their base (+30 cm)
            var groups = levels.ToDictionary(l => l.Name, l => new ModelItemCollection());
            double tol = 0.3 * s;
            BoundingBox3D all = null;
            foreach (ModelItem it in doc.Models.RootItemDescendantsAndSelf)
            {
                if (!it.HasGeometry) continue;
                BoundingBox3D bb;
                try { bb = it.BoundingBox(); } catch { continue; }
                if (bb == null || bb.IsEmpty) continue;
                all = all == null ? bb : all.Extend(bb);
                string lv = levels.Where(l => l.Z * s <= bb.Min.Z + tol).Select(l => l.Name).LastOrDefault() ?? levels[0].Name;
                groups[lv].Add(it);
            }
            // the folder from last time is replaced
            RemoveFolder(doc.SelectionSets.Value, f => doc.SelectionSets.Remove(f));
            RemoveFolder(doc.SavedViewpoints.Value, f => doc.SavedViewpoints.Remove(f));
            doc.SelectionSets.AddCopy(new FolderItem { DisplayName = Folder });
            GroupItem setFolder = doc.SelectionSets.Value.OfType<GroupItem>().LastOrDefault(g => g.DisplayName == Folder);
            int sets = 0;
            foreach (var (name, _) in levels)
            {
                var set = new SelectionSet(groups[name]) { DisplayName = $"{name} ({groups[name].Count})" };
                if (setFolder != null) doc.SelectionSets.InsertCopy(setFolder, setFolder.Children.Count, set);
                else doc.SelectionSets.AddCopy(set);
                sets++;
            }
            int views = 0;
            if (all != null)
            {
                doc.SavedViewpoints.AddCopy(new FolderItem { DisplayName = Folder });
                GroupItem vpFolder = doc.SavedViewpoints.Value.OfType<GroupItem>().LastOrDefault(g => g.DisplayName == Folder);
                Viewpoint original = doc.CurrentViewpoint.CreateCopy();
                double cx = (all.Min.X + all.Max.X) / 2, cy = (all.Min.Y + all.Max.Y) / 2;
                double size = Math.Max(all.Max.X - all.Min.X, all.Max.Y - all.Min.Y) * 1.1;
                for (int i = 0; i < levels.Count; i++)
                {
                    double z0 = levels[i].Z * s;
                    double z1 = i + 1 < levels.Count ? levels[i + 1].Z * s : Math.Max(all.Max.Z, z0 + 3.0 * s);
                    try
                    {
                        Viewpoint vp = doc.CurrentViewpoint.CreateCopy();
                        vp.Position = new Point3D(cx, cy, all.Max.Z + size);
                        double[] q = ManageBimIssues.Quaternion(new[] { 0.0, 0.0, -1.0 }, new[] { 0.0, 1.0, 0.0 });
                        vp.Rotation = new Rotation3D(q[0], q[1], q[2], q[3]);
                        vp.Projection = ViewpointProjection.Orthographic;
                        vp.HeightField = size;
                        doc.CurrentViewpoint.CopyFrom(vp);
                        // section box between the level and the next one (slightly below the floor), API 2019+ through reflection
                        SetBox(doc, all.Min.X, all.Min.Y, z0 - 0.2 * s, all.Max.X, all.Max.Y, z1 - 0.05 * s);
                        var sv = new SavedViewpoint(doc.CurrentViewpoint.CreateCopy()) { DisplayName = $"Plan - {levels[i].Name}" };
                        if (vpFolder != null) doc.SavedViewpoints.InsertCopy(vpFolder, vpFolder.Children.Count, sv);
                        else doc.SavedViewpoints.AddCopy(sv);
                        views++;
                    }
                    catch { }
                }
                ClearBox(doc);
                try { doc.CurrentViewpoint.CopyFrom(original); } catch { }
            }
            int placed = groups.Values.Sum(g => g.Count);
            Say($"{sets} level selection sets ({placed} items) in Sets › \"{Folder}\" and {views} plan viewpoints in Saved Viewpoints › \"{Folder}\".");
            return 0;
        }

        private static void RemoveFolder(IEnumerable<SavedItem> root, Action<SavedItem> remove)
        {
            foreach (var f in root.OfType<GroupItem>().Where(g => g.DisplayName == Folder).ToList())
            {
                try { remove(f); } catch { }
            }
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

        private static string F(double v) => v.ToString("R", CultureInfo.InvariantCulture);

        private static void SetBox(Document doc, double x0, double y0, double z0, double x1, double y1, double z1)
        {
            try
            {
                object view = doc.ActiveView;
                var m = view?.GetType().GetMethod("SetClippingPlanes", new[] { typeof(string) });
                if (m == null) return;
                string json = "{\"Type\":\"ClipPlaneSet\",\"Version\":1,\"OrientedBox\":{\"Type\":\"OrientedBox3D\",\"Version\":1,\"Box\":[[" +
                              F(x0) + "," + F(y0) + "," + F(z0) + "],[" + F(x1) + "," + F(y1) + "," + F(z1) + "]],\"Rotation\":[0,0,0]},\"Enabled\":true}";
                m.Invoke(view, new object[] { json });
            }
            catch { }
        }

        private static void ClearBox(Document doc)
        {
            try
            {
                object view = doc.ActiveView;
                var m = view?.GetType().GetMethod("SetClippingPlanes", new[] { typeof(string) });
                m?.Invoke(view, new object[] { "{\"Type\":\"ClipPlaneSet\",\"Version\":1,\"Enabled\":false}" });
            }
            catch { }
        }
    }
}
