using System;
using System.Collections.Generic;
using System.Drawing;
using System.Globalization;
using System.IO;
using System.Linq;
using System.Text;
using System.Windows.Forms;
using Autodesk.Navisworks.Api;
using Autodesk.Navisworks.Api.Plugins;
using NwApp = Autodesk.Navisworks.Api.Application;
using NwColor = Autodesk.Navisworks.Api.Color;
using Graphics = System.Drawing.Graphics;
using View = System.Windows.Forms.View;
using Control = System.Windows.Forms.Control;

namespace ManageBIM.Navis
{
    /// <summary>A coordination object sent from ManageBIM (shared coordinates, meters).</summary>
    internal sealed class MbCoord
    {
        public string Kind = "", Name = "", Guid = "", Status = "";
        public double[] Centre = new double[3], Size = new double[3];
        public double RotDeg;
        public int R = 150, G = 150, B = 150, A = 120;
        public string Label => Kind == "void" ? "Void" : Kind == "clearance" ? "Clearance" : "Placeholder";
    }

    /// <summary>
    /// "ManageBIM Coordination": shows in Navisworks the maintenance zones, proposed openings and reserved
    /// volumes sent from ManageBIM (Outbox\coordination.txt) - display ONLY. The Navisworks API does not create
    /// geometry directly, so the objects are written to a DXF (3D faces, one layer per object) added with
    /// Append, then colored by type and made transparent. Previous sends are hidden.
    /// </summary>
    [Plugin("ManageBIMCoordination", "MBIM", DisplayName = "ManageBIM Coordination", ToolTip = "Show the clearance zones, voids and placeholders sent from ManageBIM (display only)")]
    [AddInPlugin(AddInLocation.None)]
    public class CoordinationPlugin : AddInPlugin
    {
        private const string FilePrefix = "ManageBIM-coordination-";

        internal static string OutboxFile() => Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "ManageBIM", "Outbox", "coordination.txt");

        private static double[] V(string s)
        {
            var p = s.Split(new[] { ' ' }, StringSplitOptions.RemoveEmptyEntries);
            var r = new double[Math.Max(3, p.Length)];
            for (int i = 0; i < p.Length; i++) double.TryParse(p[i], NumberStyles.Float, CultureInfo.InvariantCulture, out r[i]);
            return r;
        }

        internal static List<MbCoord> Read(out string err)
        {
            err = "";
            var list = new List<MbCoord>();
            string f = OutboxFile();
            if (!File.Exists(f)) { err = "Nothing sent from ManageBIM yet.\n\nIn ManageBIM: Coordination › Objects › Send to Revit / Navisworks."; return list; }
            foreach (var line in File.ReadAllLines(f))
            {
                if (line.StartsWith("#") || line.Trim().Length == 0) continue;
                var c = line.Split('\t');
                if (c.Length < 6) continue;
                var col = V(c[5]);
                double.TryParse(c[4], NumberStyles.Float, CultureInfo.InvariantCulture, out double rot);
                list.Add(new MbCoord
                {
                    Kind = c[0].Trim(), Name = c[1], Centre = V(c[2]), Size = V(c[3]), RotDeg = rot,
                    R = (int)col[0], G = (int)col[1], B = (int)col[2], A = col.Length > 3 ? (int)col[3] : 120,
                    Guid = c.Length > 6 ? c[6].Trim() : "", Status = c.Length > 7 ? c[7].Trim() : "",
                });
            }
            if (list.Count == 0) err = "The file from ManageBIM has no objects.";
            return list;
        }

        public override int Execute(params string[] parameters) => Safe.Plugin(GetType().Name.Replace("Plugin", ""), parameters, () => Run(parameters));

        private int Run(string[] parameters)
        {
            Document doc = NwApp.ActiveDocument;
            if (doc == null || doc.Models.Count == 0) { MessageBox.Show("Open the model first (the same model as in ManageBIM).", "ManageBIM"); return 1; }
            var objs = Read(out string err);
            if (objs.Count == 0) { MessageBox.Show(err, "ManageBIM"); return 1; }
            var chosen = CoordPreview.Show(objs, "Show in Navisworks", out bool replace);
            if (chosen == null || chosen.Count == 0) return 0;

            // DXF in meters, shared coordinates (the same as the models exported from Revit / ManageBIM)
            string dir = Path.Combine(Path.GetTempPath(), "ManageBIM");
            Directory.CreateDirectory(dir);
            string path = Path.Combine(dir, FilePrefix + DateTime.Now.ToString("yyyyMMdd-HHmmss") + ".dxf");
            File.WriteAllText(path, Dxf(chosen), Encoding.ASCII);

            if (replace)
            {
                // previous sends: hidden (the API does not allow removing an appended model)
                var old = new ModelItemCollection();
                foreach (Model m in doc.Models)
                    if (Path.GetFileName(m.FileName ?? "").StartsWith(FilePrefix, StringComparison.OrdinalIgnoreCase)) old.Add(m.RootItem);
                if (old.Count > 0) doc.Models.SetHidden(old, true);
            }
            int before = doc.Models.Count;
            try { doc.AppendFile(path); }
            catch (Exception e) { MessageBox.Show("Navisworks could not append the coordination objects:\n" + path + "\n\n" + e.Message, "ManageBIM"); return 1; }
            if (doc.Models.Count <= before) { MessageBox.Show("Navisworks did not add the coordination objects:\n" + path, "ManageBIM"); return 1; }

            // color by type + transparency, on each object's layer
            Model added = doc.Models.Cast<Model>().LastOrDefault(m => string.Equals(m.FileName, path, StringComparison.OrdinalIgnoreCase)) ?? doc.Models.Last();
            foreach (var kind in chosen.Select(o => o.Kind).Distinct())
            {
                var first = chosen.First(o => o.Kind == kind);
                var items = new ModelItemCollection();
                foreach (ModelItem it in added.RootItem.Descendants)
                    if ((it.DisplayName ?? "").StartsWith(first.Label + " ", StringComparison.OrdinalIgnoreCase)) items.Add(it);
                if (items.Count == 0) continue;
                doc.Models.OverridePermanentColor(items, new NwColor(first.R / 255.0, first.G / 255.0, first.B / 255.0));
                doc.Models.OverridePermanentTransparency(items, 0.55);
            }
            MessageBox.Show($"{chosen.Count} coordination object(s) appended as '{Path.GetFileName(path)}' (transparent, coloured by type).\n\nDisplay only: the model is not changed.", "ManageBIM");
            return 0;
        }

        /// <summary>DXF R12 with 3DFACE: 6 faces per box (rotated around the vertical), one layer per object
        /// ("Void - name"), the real color on the entity.</summary>
        private static string Dxf(List<MbCoord> objs)
        {
            // AC1018 (AutoCAD 2004): the true color (code 420) is read; units: meters
            var sb = new StringBuilder("0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1018\n9\n$INSUNITS\n70\n6\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n");
            string F(double v) => v.ToString("0.0000", CultureInfo.InvariantCulture);
            var used = new HashSet<string>();
            foreach (var o in objs)
            {
                string baseName = new string((o.Label + " " + o.Name).Select(ch => char.IsLetterOrDigit(ch) || ch == ' ' || ch == '-' || ch == '_' ? ch : '_').ToArray()).Trim();
                string layer = baseName;
                for (int k = 2; !used.Add(layer); k++) layer = baseName + " " + k;
                double rad = o.RotDeg * Math.PI / 180.0, co = Math.Cos(rad), si = Math.Sin(rad);
                double hx = o.Size[0] / 2, hy = o.Size[1] / 2, z0 = o.Centre[2] - o.Size[2] / 2, z1 = o.Centre[2] + o.Size[2] / 2;
                double[] P(double lx, double ly, double z) => new[] { o.Centre[0] + lx * co - ly * si, o.Centre[1] + lx * si + ly * co, z };
                var b = new[] { P(-hx, -hy, z0), P(hx, -hy, z0), P(hx, hy, z0), P(-hx, hy, z0) };
                var t = new[] { P(-hx, -hy, z1), P(hx, -hy, z1), P(hx, hy, z1), P(-hx, hy, z1) };
                var faces = new List<double[][]> { new[] { b[0], b[3], b[2], b[1] }, new[] { t[0], t[1], t[2], t[3] } };
                for (int i = 0; i < 4; i++) faces.Add(new[] { b[i], b[(i + 1) % 4], t[(i + 1) % 4], t[i] });
                int rgb = (o.R << 16) | (o.G << 8) | o.B;
                foreach (var f in faces)
                {
                    sb.Append($"0\n3DFACE\n8\n{layer}\n420\n{rgb}\n");
                    for (int i = 0; i < 4; i++) sb.Append($"1{i}\n{F(f[i][0])}\n2{i}\n{F(f[i][1])}\n3{i}\n{F(f[i][2])}\n");
                }
            }
            sb.Append("0\nENDSEC\n0\nEOF\n");
            return sb.ToString();
        }
    }

    /// <summary>Preview window: the list with checkboxes and the top view (the same as in Revit).</summary>
    internal static class CoordPreview
    {
        public static List<MbCoord> Show(List<MbCoord> objs, string okText, out bool replace)
        {
            replace = true;
            using (var form = new Form { Text = "ManageBIM - coordination objects", Width = 860, Height = 520, StartPosition = FormStartPosition.CenterScreen, MinimizeBox = false, ShowInTaskbar = false, Font = new Font("Segoe UI", 9f) })
            {
                var info = new Label { Dock = DockStyle.Top, Height = 30, Padding = new Padding(8, 8, 8, 0), Text = $"{objs.Count} object(s) from ManageBIM. They are shown as transparent boxes only - nothing in the model is changed." };
                var split = new SplitContainer { Dock = DockStyle.Fill, SplitterDistance = 430 };
                var lst = new ListView { Dock = DockStyle.Fill, View = View.Details, CheckBoxes = true, FullRowSelect = true, HideSelection = false };
                lst.Columns.Add("Type", 90);
                lst.Columns.Add("Name", 170);
                lst.Columns.Add("Size (mm)", 110);
                lst.Columns.Add("Status", 70);
                foreach (var o in objs)
                {
                    var it = new ListViewItem(o.Label) { Checked = true, Tag = o };
                    it.SubItems.Add(o.Name);
                    it.SubItems.Add($"{o.Size[0] * 1000:0}×{o.Size[1] * 1000:0}×{o.Size[2] * 1000:0}");
                    it.SubItems.Add(o.Status);
                    it.ForeColor = System.Drawing.Color.FromArgb(Math.Max(0, o.R - 60), Math.Max(0, o.G - 60), Math.Max(0, o.B - 60));
                    lst.Items.Add(it);
                }
                var plan = new PictureBox { Dock = DockStyle.Fill, BackColor = System.Drawing.Color.FromArgb(246, 243, 232) };
                plan.Paint += (s, e) => Safe.Ui("Coordination plan", () => DrawPlan(e.Graphics, plan.ClientRectangle, objs, lst));
                lst.ItemChecked += (s, e) => plan.Invalidate();
                lst.SelectedIndexChanged += (s, e) => plan.Invalidate();
                split.Panel1.Controls.Add(lst);
                split.Panel2.Controls.Add(plan);
                var bar = new FlowLayoutPanel { Dock = DockStyle.Bottom, Height = 42, FlowDirection = FlowDirection.RightToLeft, Padding = new Padding(6) };
                var ok = new Button { Text = okText, DialogResult = DialogResult.OK, Width = 140, Height = 28 };
                var cancel = new Button { Text = "Cancel", DialogResult = DialogResult.Cancel, Width = 90, Height = 28 };
                var rep = new CheckBox { Text = "Hide ManageBIM objects shown before", Checked = true, AutoSize = true, Margin = new Padding(12, 7, 12, 0) };
                var all = new Button { Text = "All", Width = 60, Height = 28 };
                var none = new Button { Text = "None", Width = 60, Height = 28 };
                all.Click += (s, e) => { foreach (ListViewItem i in lst.Items) i.Checked = true; };
                none.Click += (s, e) => { foreach (ListViewItem i in lst.Items) i.Checked = false; };
                bar.Controls.AddRange(new Control[] { ok, cancel, rep, none, all });
                form.Controls.Add(split);
                form.Controls.Add(info);
                form.Controls.Add(bar);
                form.AcceptButton = ok;
                form.CancelButton = cancel;
                if (form.ShowDialog() != DialogResult.OK) return null;
                replace = rep.Checked;
                return lst.Items.Cast<ListViewItem>().Where(i => i.Checked).Select(i => (MbCoord)i.Tag).ToList();
            }
        }

        private static void DrawPlan(Graphics g, Rectangle r, List<MbCoord> objs, ListView lst)
        {
            g.SmoothingMode = System.Drawing.Drawing2D.SmoothingMode.AntiAlias;
            var corners = objs.Select(Corners).ToList();
            if (corners.Count == 0) return;
            double x0 = corners.SelectMany(c => c).Min(p => p.X), x1 = corners.SelectMany(c => c).Max(p => p.X);
            double y0 = corners.SelectMany(c => c).Min(p => p.Y), y1 = corners.SelectMany(c => c).Max(p => p.Y);
            double w = Math.Max(x1 - x0, 0.5), h = Math.Max(y1 - y0, 0.5);
            double sc = Math.Min((r.Width - 40) / w, (r.Height - 50) / h);
            PointF Map(PointF p) => new PointF((float)(r.Left + 20 + (p.X - x0) * sc + ((r.Width - 40) - w * sc) / 2), (float)(r.Bottom - 30 - (p.Y - y0) * sc - ((r.Height - 50) - h * sc) / 2));
            for (int i = 0; i < objs.Count; i++)
            {
                var o = objs[i];
                var item = i < lst.Items.Count ? lst.Items[i] : null;
                bool on = item?.Checked ?? true, sel = item?.Selected ?? false;
                var pts = corners[i].Select(Map).ToArray();
                using (var br = new SolidBrush(System.Drawing.Color.FromArgb(on ? 110 : 25, o.R, o.G, o.B))) g.FillPolygon(br, pts);
                using (var pen = new Pen(System.Drawing.Color.FromArgb(on ? 255 : 70, Math.Max(0, o.R - 50), Math.Max(0, o.G - 50), Math.Max(0, o.B - 50)), sel ? 3f : 1.2f)) g.DrawPolygon(pen, pts);
            }
            using (var f = new Font("Segoe UI", 8.5f))
            using (var b = new SolidBrush(System.Drawing.Color.FromArgb(90, 90, 90)))
            {
                g.DrawString("Top view (green clearance · orange void · violet placeholder)", f, b, r.Left + 8, r.Bottom - 20);
                g.DrawString($"{w:0.0} × {h:0.0} m", f, b, r.Left + 8, r.Top + 6);
            }
        }

        private static PointF[] Corners(MbCoord o)
        {
            double rad = o.RotDeg * Math.PI / 180.0, co = Math.Cos(rad), si = Math.Sin(rad);
            double hx = o.Size[0] / 2, hy = o.Size[1] / 2;
            return new[] { (-hx, -hy), (hx, -hy), (hx, hy), (-hx, hy) }
                .Select(t => new PointF((float)(o.Centre[0] + t.Item1 * co - t.Item2 * si), (float)(o.Centre[1] + t.Item1 * si + t.Item2 * co))).ToArray();
        }
    }
}
