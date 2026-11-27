using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Windows.Forms;
using Autodesk.Revit.Attributes;
using Autodesk.Revit.DB;
using Autodesk.Revit.UI;
using TaskDialog = Autodesk.Revit.UI.TaskDialog;

namespace ManageBIM.Revit
{
    /// <summary>
    /// "ManageBIM Reference": the models exported from ManageBIM (Output › Reference, .mbref file) as reference
    /// geometry: one DirectShape (Generic Models) per element, with its color, in a group pinned on the reference.
    /// The window shows the loaded references: Load… / Reload (the same file, updated) / Unload.
    /// The coordinates are the shared ones, as in the export.
    /// </summary>
    [Transaction(TransactionMode.Manual)]
    [Regeneration(RegenerationOption.Manual)]
    public class ReferenceCommand : IExternalCommand
    {
        private const double FeetToMetres = 0.3048;
        /// <summary>The element comment: "MBREF|name|path|type guid" - this is how the references are found on Reload / Unload.</summary>
        private const string Tag = "MBREF|";

        private sealed class LoadedRef
        {
            public string Name = "", Path = "";
            public List<ElementId> Ids = new List<ElementId>();
            public override string ToString() => $"{Name}   ({Ids.Count} items){(File.Exists(Path) ? "" : "   - file not found")}";
        }

        public Result Execute(ExternalCommandData commandData, ref string message, ElementSet elements)
        {
            string msg = message;
            Result r = Safe.Command(GetType().Name.Replace("Command", ""), () => Run(commandData, ref msg, elements), out string error);
            message = error ?? msg;
            return r;
        }

        private Result Run(ExternalCommandData commandData, ref string message, ElementSet elements)
        {
            UIDocument uidoc = commandData.Application.ActiveUIDocument;
            if (uidoc == null) { message = "Open a project first."; return Result.Failed; }
            Document doc = uidoc.Document;
            var refs = Loaded(doc);
            // the window: the list of references + Load… / Reload / Unload
            using (var form = new System.Windows.Forms.Form { Text = "ManageBIM references", Width = 560, Height = 360, StartPosition = FormStartPosition.CenterScreen, MinimizeBox = false, MaximizeBox = false, ShowInTaskbar = false })
            {
                var info = new Label { Dock = DockStyle.Top, Height = 34, Padding = new Padding(8, 8, 8, 0), Text = "Models from ManageBIM (Output › Reference, .mbref) loaded in this project:" };
                var lst = new ListBox { Dock = DockStyle.Fill, IntegralHeight = false };
                foreach (var r in refs) lst.Items.Add(r);
                if (lst.Items.Count > 0) lst.SelectedIndex = 0;
                var bar = new FlowLayoutPanel { Dock = DockStyle.Bottom, Height = 42, FlowDirection = FlowDirection.LeftToRight, Padding = new Padding(6) };
                string action = null;
                Button B(string t, string a) { var b = new Button { Text = t, Width = 110 }; b.Click += (s, e) => { action = a; form.Close(); }; bar.Controls.Add(b); return b; }
                B("Load…", "load");
                var rl = B("Reload", "reload");
                var ul = B("Unload", "unload");
                B("Close", null);
                rl.Enabled = ul.Enabled = lst.Items.Count > 0;
                form.Controls.Add(lst);
                form.Controls.Add(bar);
                form.Controls.Add(info);
                form.ShowDialog();
                var sel = lst.SelectedItem as LoadedRef;
                switch (action)
                {
                    case "load":
                        using (var dlg = new OpenFileDialog { Title = "ManageBIM reference", Filter = "ManageBIM reference (*.mbref)|*.mbref", InitialDirectory = Directory.Exists(MbRef.Folder()) ? MbRef.Folder() : "" })
                        {
                            if (dlg.ShowDialog() != DialogResult.OK) return Result.Cancelled;
                            // the same reference already loaded: it is replaced (like Reload)
                            var same = refs.FirstOrDefault(r => string.Equals(r.Path, dlg.FileName, StringComparison.OrdinalIgnoreCase));
                            return Load(doc, dlg.FileName, same);
                        }
                    case "reload":
                        if (sel == null) return Result.Cancelled;
                        if (!File.Exists(sel.Path)) { TaskDialog.Show("ManageBIM", "The reference file was not found:\n" + sel.Path + "\n\nUse Load… to choose it again."); return Result.Cancelled; }
                        return Load(doc, sel.Path, sel);
                    case "unload":
                        if (sel == null) return Result.Cancelled;
                        using (var tr = new Transaction(doc, "ManageBIM reference unload"))
                        {
                            tr.Start();
                            Unload(doc, sel);
                            tr.Commit();
                        }
                        TaskDialog.Show("ManageBIM reference", $"{sel.Name} unloaded ({sel.Ids.Count} items removed).");
                        return Result.Succeeded;
                    default:
                        return Result.Cancelled;
                }
            }
        }

        /// <summary>The references in the project, by the element comment ("MBREF|name|path|...").</summary>
        private static List<LoadedRef> Loaded(Document doc)
        {
            var by = new Dictionary<string, LoadedRef>();
            foreach (DirectShape ds in new FilteredElementCollector(doc).OfClass(typeof(DirectShape)))
            {
                if (ds.ApplicationId != "ManageBIM") continue;
                string c = ds.get_Parameter(BuiltInParameter.ALL_MODEL_INSTANCE_COMMENTS)?.AsString() ?? "";
                if (!c.StartsWith(Tag)) continue;
                var parts = c.Split('|');
                if (parts.Length < 3) continue;
                if (!by.TryGetValue(parts[1], out var r)) by[parts[1]] = r = new LoadedRef { Name = parts[1], Path = parts[2] };
                r.Ids.Add(ds.Id);
            }
            return by.Values.OrderBy(r => r.Name).ToList();
        }

        /// <summary>Removes the reference: its group (with the group type) and the elements.</summary>
        private static void Unload(Document doc, LoadedRef r)
        {
            var groups = new HashSet<ElementId>();
            foreach (var id in r.Ids)
            {
                var e = doc.GetElement(id);
                if (e != null && e.GroupId != ElementId.InvalidElementId) groups.Add(e.GroupId);
            }
            foreach (var g in groups)
            {
                var grp = doc.GetElement(g) as Group;
                if (grp == null) continue;
                var gt = grp.GroupType?.Id;
                grp.Pinned = false;
                // the group type deletes both the instances and the elements in them
                try { if (gt != null) doc.Delete(gt); else doc.Delete(g); } catch { try { doc.Delete(g); } catch { } }
            }
            var left = r.Ids.Where(id => doc.GetElement(id) != null).ToList();
            if (left.Count > 0) try { doc.Delete(left); } catch { }
        }

        private static Result Load(Document doc, string path, LoadedRef replace)
        {
            List<MbRefElement> items;
            double[] origin;
            try { items = MbRef.Read(path, out origin); }
            catch (Exception ex) { TaskDialog.Show("ManageBIM", ex.Message); return Result.Failed; }

            Transform total = doc.ActiveProjectLocation?.GetTotalTransform() ?? Transform.Identity;
            XYZ P(float[] t, int i) => total.OfPoint(new XYZ((origin[0] + t[i]) / FeetToMetres, (origin[1] + t[i + 1]) / FeetToMetres, (origin[2] + t[i + 2]) / FeetToMetres));
            double tol = doc.Application.ShortCurveTolerance;
            var mats = new Dictionary<string, ElementId>();
            var ids = new List<ElementId>();
            string refName = Path.GetFileNameWithoutExtension(path);
            var sw = System.Diagnostics.Stopwatch.StartNew();
            // compact and fast: all triangles of the same color in a single smooth mesh (Mesh, without the triangle
            // edges), split into chunks of at most 150 000 triangles - a few shapes instead of thousands
            var byColour = items.GroupBy(e => (e.R, e.G, e.B, e.A)).ToList();
            int tris = 0;
            using (var tr = new Transaction(doc, replace != null ? "ManageBIM reference reload" : "ManageBIM reference"))
            {
                tr.Start();
                if (replace != null) Unload(doc, replace);
                foreach (var grp in byColour)
                {
                    ElementId mat = Material(doc, mats, grp.First());
                    TessellatedShapeBuilder b = null;
                    int inShape = 0;
                    void Flush()
                    {
                        if (b == null) return;
                        b.CloseConnectedFaceSet();
                        if (inShape > 0)
                        {
                            b.Target = TessellatedShapeBuilderTarget.Mesh;
                            b.Fallback = TessellatedShapeBuilderFallback.Salvage;
                            try
                            {
                                b.Build();
                                var ds = DirectShape.CreateElement(doc, new ElementId(BuiltInCategory.OST_GenericModel));
                                ds.ApplicationId = "ManageBIM";
                                ds.ApplicationDataId = refName;
                                ds.SetShape(b.GetBuildResult().GetGeometricalObjects());
                                try { ds.SetName($"ManageBIM {refName}"); } catch { }
                                ds.get_Parameter(BuiltInParameter.ALL_MODEL_INSTANCE_COMMENTS)?.Set($"{Tag}{refName}|{path}|");
                                ds.Pinned = true;
                                ids.Add(ds.Id);
                            }
                            catch { }
                        }
                        b = null;
                        inShape = 0;
                    }
                    foreach (var e in grp)
                    {
                        for (int i = 0; i + 8 < e.Tris.Length; i += 9)
                        {
                            XYZ a = P(e.Tris, i), c = P(e.Tris, i + 3), d = P(e.Tris, i + 6);
                            if (a.DistanceTo(c) < tol || c.DistanceTo(d) < tol || d.DistanceTo(a) < tol) continue;
                            if (b == null)
                            {
                                b = new TessellatedShapeBuilder();
                                b.OpenConnectedFaceSet(false);
                            }
                            b.AddFace(new TessellatedFace(new List<XYZ> { a, c, d }, mat));
                            inShape++;
                            tris++;
                            if (inShape >= 150000) Flush();
                        }
                    }
                    Flush();
                }
                tr.Commit();
            }
            TaskDialog.Show("ManageBIM reference", $"{refName}: {items.Count} items {(replace != null ? "reloaded" : "loaded")} in {sw.Elapsed.TotalSeconds:0.0} s " +
                $"as {ids.Count} pinned reference shapes (one per colour, {tris:N0} triangles, Generic Models).\n\n" +
                "Run ManageBIM Reference again to reload (after updating the file in ManageBIM) or unload it.");
            return Result.Succeeded;
        }

        /// <summary>The "ManageBIM ref RRGGBB" material (the element color and transparency), created when needed.</summary>
        private static ElementId Material(Document doc, Dictionary<string, ElementId> cache, MbRefElement e)
        {
            string name = $"ManageBIM ref {e.R:X2}{e.G:X2}{e.B:X2}{(e.A < 255 ? "T" : "")}";
            if (cache.TryGetValue(name, out var id)) return id;
            var m = new FilteredElementCollector(doc).OfClass(typeof(Material)).Cast<Material>().FirstOrDefault(x => x.Name == name);
            if (m == null)
            {
                try
                {
                    id = Autodesk.Revit.DB.Material.Create(doc, name);
                    var mm = (Material)doc.GetElement(id);
                    mm.Color = new Color(e.R, e.G, e.B);
                    mm.Transparency = (int)Math.Round((1 - e.A / 255.0) * 100);
                }
                catch { id = ElementId.InvalidElementId; }
            }
            else id = m.Id;
            cache[name] = id;
            return id;
        }
    }
}
