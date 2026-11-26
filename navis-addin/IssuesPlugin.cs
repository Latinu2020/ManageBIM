using System;
using System.Linq;
using System.Collections.Generic;
using System.Globalization;
using System.Windows.Forms;
using Autodesk.Navisworks.Api;
using Autodesk.Navisworks.Api.Plugins;
using NwApp = Autodesk.Navisworks.Api.Application;

namespace ManageBIM.Navis
{
    /// <summary>
    /// "ManageBIM Issues": aduce viewpoints / issue-urile din ManageBIM (Outbox\issues.txt) ca Saved Viewpoints:
    /// camera, section box si comentariile (descriere, stare, comentarii). Fara fisier XML intermediar.
    /// </summary>
    [Plugin("ManageBIMIssues", "MBIM", DisplayName = "ManageBIM Issues", ToolTip = "Bring the ManageBIM viewpoints / issues in as Saved Viewpoints (camera, section box, comments)")]
    [AddInPlugin(AddInLocation.None)]
    public class IssuesPlugin : AddInPlugin
    {
        public override int Execute(params string[] parameters) => Safe.Plugin(GetType().Name.Replace("Plugin", ""), parameters, () => Run(parameters));

        private int Run(string[] parameters)
        {
            // "quiet" (Automation / tests): no windows, every issue is added; "file=<path>" reads another issues file
            bool quiet = parameters != null && parameters.Contains("quiet");
            string file = parameters?.FirstOrDefault(p => p.StartsWith("file="))?.Substring(5);
            void Say(string m) { if (!quiet) MessageBox.Show(m, "ManageBIM"); }
            Document doc = NwApp.ActiveDocument;
            if (doc == null || doc.Models.Count == 0)
            {
                Say("Open the model first (the same model as in ManageBIM).");
                return 1;
            }
            var issues = ManageBimIssues.Read(out string err, file);
            if (issues.Count == 0)
            {
                Say(err);
                return 1;
            }
            var pick = quiet ? Enumerable.Range(0, issues.Count).ToList() : ManageBimIssues.Pick(issues, "ManageBIM issues -> Saved Viewpoints", "Add as viewpoints", true);
            if (pick == null || pick.Count == 0) return 0;
            // metri -> unitatile documentului
            double s = UnitConversion.ScaleFactor(Units.Meters, doc.Units);
            Viewpoint original = doc.CurrentViewpoint.CreateCopy();
            int ok = 0, boxes = 0, marked = 0, wanted = 0;
            foreach (int k in pick) if (issues[k].Lines.Count + issues[k].Texts.Count > 0) wanted++;
            var failed = new List<string>();
            foreach (int k in pick)
            {
                MbIssue it = issues[k];
                try
                {
                    Viewpoint vp = doc.CurrentViewpoint.CreateCopy();
                    vp.Position = new Point3D(it.Eye[0] * s, it.Eye[1] * s, it.Eye[2] * s);
                    double[] q = ManageBimIssues.Quaternion(it.Fwd, it.Up);
                    vp.Rotation = new Rotation3D(q[0], q[1], q[2], q[3]);
                    if (it.OrthoHeight > 0)
                    {
                        vp.Projection = ViewpointProjection.Orthographic;
                        vp.HeightField = it.OrthoHeight * s;
                    }
                    else
                    {
                        vp.Projection = ViewpointProjection.Perspective;
                        vp.HeightField = it.FovDeg * Math.PI / 180.0;
                    }
                    doc.CurrentViewpoint.CopyFrom(vp);
                    // section box (API 2019+: View.SetClippingPlanes, apelat prin reflexie ca sa mearga si fara el)
                    bool box = it.BoxMin != null && SetBox(doc, it, s);
                    if (box) boxes++;
                    var sv = new SavedViewpoint(doc.CurrentViewpoint.CreateCopy()) { DisplayName = it.Name };
                    string body = $"Status: {it.Status}" + (string.IsNullOrEmpty(it.Priority) ? "" : $" | Priority: {it.Priority}") + (string.IsNullOrEmpty(it.Assigned) ? "" : $" | Assigned to: {it.Assigned}") + (string.IsNullOrEmpty(it.Text) ? "" : "\n" + it.Text);
                    sv.Comments.Add(new Comment(body, StatusOf(it.Status)));
                    // folderul din ManageBIM (si subfolderele "A/B") devine folder de Saved Viewpoints
                    GroupItem folder = EnsureFolder(doc, it.Folder);
                    if (folder != null) doc.SavedViewpoints.InsertCopy(folder, folder.Children.Count, sv);
                    else doc.SavedViewpoints.AddCopy(sv);
                    if (it.Lines.Count + it.Texts.Count > 0 && AddRedlines(doc, it, folder)) marked++;
                    if (box) ClearBox(doc);
                    ok++;
                }
                catch (Exception ex)
                {
                    failed.Add(it.Name + ": " + ex.Message);
                }
            }
            try { doc.CurrentViewpoint.CopyFrom(original); } catch { }
            string marks = wanted == 0 ? "" : marked == wanted ? $"\n{marked} with markups (redlines)." :
                $"\nMarkups added on {marked} of {wanted} viewpoints. For the others use Output > Navisworks (XML) > Viewpoints in ManageBIM, then Saved Viewpoints > right-click > Import Viewpoints.";
            if (quiet) return ok == pick.Count ? 0 : 3;
            MessageBox.Show($"{ok} viewpoints added to Saved Viewpoints (folder list), {boxes} with a section box." + marks + (failed.Count > 0 ? "\n\nNot added:\n" + string.Join("\n", failed) : ""), "ManageBIM");
            return 0;
        }

        private static CommentStatus StatusOf(string s)
        {
            switch ((s ?? "").ToLowerInvariant())
            {
                case "in progress": return CommentStatus.Active;
                case "resolved": return CommentStatus.Resolved;
                case "closed": return CommentStatus.Approved;
                default: return CommentStatus.New;
            }
        }

        private static string F(double v) => v.ToString("R", CultureInfo.InvariantCulture);

        private static bool SetBox(Document doc, MbIssue it, double s)
        {
            try
            {
                object view = doc.ActiveView;
                var m = view?.GetType().GetMethod("SetClippingPlanes", new[] { typeof(string) });
                if (m == null) return false;
                string json = "{\"Type\":\"ClipPlaneSet\",\"Version\":1,\"OrientedBox\":{\"Type\":\"OrientedBox3D\",\"Version\":1,\"Box\":[[" +
                              F(it.BoxMin[0] * s) + "," + F(it.BoxMin[1] * s) + "," + F(it.BoxMin[2] * s) + "],[" +
                              F(it.BoxMax[0] * s) + "," + F(it.BoxMax[1] * s) + "," + F(it.BoxMax[2] * s) + "]],\"Rotation\":[0,0,0]},\"Enabled\":true}";
                m.Invoke(view, new object[] { json });
                return true;
            }
            catch { return false; }
        }

        /// <summary>
        /// Markup-urile ca redlines pe viewpoint-ul salvat adineauri: il facem curent si trimitem colectia JSON
        /// (View.TrySetRedlines, API 2019+). Formatul JSON nu e documentat: incercam cateva variante si o pastram
        /// pe prima acceptata de Navisworks.
        /// </summary>
        private static int jsonVariant = -1;

        /// <summary>Folderul (si subfolderele, separate prin /) din Saved Viewpoints; creat daca lipseste.</summary>
        private static GroupItem EnsureFolder(Document doc, string path)
        {
            if (string.IsNullOrWhiteSpace(path)) return null;
            GroupItem parent = doc.SavedViewpoints.RootItem;
            foreach (string seg in path.Split('/'))
            {
                string name = seg.Trim();
                if (name.Length == 0) continue;
                GroupItem found = Find(parent, name);
                if (found == null)
                {
                    doc.SavedViewpoints.InsertCopy(parent, parent.Children.Count, new FolderItem { DisplayName = name });
                    found = Find(parent, name);
                }
                if (found == null) return null;
                parent = found;
            }
            return parent;
        }

        private static GroupItem Find(GroupItem parent, string name)
        {
            foreach (SavedItem c in parent.Children)
                if (c is GroupItem g && !(c is SavedViewpoint) && c.DisplayName == name) return g;
            return null;
        }

        private static bool AddRedlines(Document doc, MbIssue it, GroupItem folder)
        {
            try
            {
                object view = doc.ActiveView;
                var trySet = view?.GetType().GetMethod("TrySetRedlines", new[] { typeof(string) });
                if (trySet == null) return false;
                // viewpoint-ul adaugat ultimul (in folderul lui) devine cel curent (redline-urile se salveaza in el)
                var items = folder != null ? folder.Children : doc.SavedViewpoints.Value;
                if (items.Count == 0) return false;
                var last = items[items.Count - 1];
                var cur = doc.SavedViewpoints.GetType().GetProperty("CurrentSavedViewpoint");
                if (cur != null && cur.CanWrite) cur.SetValue(doc.SavedViewpoints, last, null);
                double aspect = 1.6;
                try
                {
                    var w = view.GetType().GetProperty("Width")?.GetValue(view, null);
                    var h = view.GetType().GetProperty("Height")?.GetValue(view, null);
                    if (w != null && h != null && Convert.ToDouble(h) > 0) aspect = Convert.ToDouble(w) / Convert.ToDouble(h);
                }
                catch { }
                int first = jsonVariant >= 0 ? jsonVariant : 0, lastV = jsonVariant >= 0 ? jsonVariant : 5;
                for (int v = first; v <= lastV; v++)
                {
                    string json = RedlineJson(it, aspect, v);
                    if ((bool)trySet.Invoke(view, new object[] { json }))
                    {
                        jsonVariant = v;
                        return true;
                    }
                }
            }
            catch { }
            return false;
        }

        /// <summary>Colectia de redlines in JSON. variant: 0-2 forma punctelor ([x,y] / {X,Y} / [x,y,0]), +3 fara culoare.</summary>
        private static string RedlineJson(MbIssue it, double aspect, int variant)
        {
            int pf = variant % 3;
            bool colour = variant < 3;
            string P(double x, double y)
            {
                // ManageBIM: fata de centru / inaltime, y in jos -> Navisworks: -1..1, y in sus
                double nx = Math.Max(-1, Math.Min(1, 2 * x / Math.Max(aspect, 0.01))), ny = Math.Max(-1, Math.Min(1, -2 * y));
                switch (pf)
                {
                    case 1: return "{\"X\":" + F(nx) + ",\"Y\":" + F(ny) + "}";
                    case 2: return "[" + F(nx) + "," + F(ny) + ",0]";
                    default: return "[" + F(nx) + "," + F(ny) + "]";
                }
            }
            string C(int r, int g, int b) => colour ? ",\"Color\":[" + F(r / 255.0) + "," + F(g / 255.0) + "," + F(b / 255.0) + "]" : "";
            var vals = new List<string>();
            foreach (var l in it.Lines)
            {
                int th = Math.Max(1, (int)Math.Round(l.Width));
                for (int i = 0; i + 1 < l.Pts.Count && vals.Count < 4000; i++)
                    vals.Add("{\"Type\":\"RedlineLine\",\"Version\":1,\"Thickness\":" + th + C(l.R, l.G, l.B) + ",\"Start\":" + P(l.Pts[i][0], l.Pts[i][1]) + ",\"End\":" + P(l.Pts[i + 1][0], l.Pts[i + 1][1]) + "}");
            }
            foreach (var t in it.Texts)
            {
                string txt = t.Text.Replace("\\", "\\\\").Replace("\"", "\\\"").Replace("\r", "").Replace("\n", "\\n");
                vals.Add("{\"Type\":\"RedlineText\",\"Version\":1,\"Thickness\":" + Math.Max(1, (int)Math.Round(t.Size)) + C(t.R, t.G, t.B) + ",\"Origin\":" + P(t.X, t.Y) + ",\"Text\":\"" + txt + "\"}");
            }
            return "{\"Type\":\"RedlineCollection\",\"Version\":1,\"Values\":[" + string.Join(",", vals) + "]}";
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
