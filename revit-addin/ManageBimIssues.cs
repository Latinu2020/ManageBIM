using System;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.Windows.Forms;

namespace ManageBIM
{
    /// <summary>
    /// Issue-urile (viewpoints) scrise de ManageBIM in %LOCALAPPDATA%\ManageBIM\Outbox\issues.txt.
    /// Coordonate WORLD in metri (aceleasi cu modelele exportate de add-in-uri). Acelasi fisier in Revit / Navisworks.
    /// </summary>
    internal sealed class MbIssue
    {
        public string Name = "", Status = "", Guid = "", Priority = "", Assigned = "", Text = "";
        public double[] Eye = new double[3], Fwd = { 1, 0, 0 }, Up = { 0, 0, 1 };
        public double FovDeg = 50, OrthoHeight;
        public double[] BoxMin, BoxMax;
        public List<string> Selected = new List<string>();
        /// <summary>Markup-uri: polilinii si texte in coordonate de vedere (x fata de centru / inaltime, y in jos).</summary>
        public List<MbMarkLine> Lines = new List<MbMarkLine>();
        public List<MbMarkText> Texts = new List<MbMarkText>();
        public override string ToString() => $"{Name}   [{Status}{(string.IsNullOrEmpty(Priority) ? "" : " · " + Priority)}]";
    }

    internal sealed class MbMarkLine { public int R = 230, G = 40, B = 40; public double Width = 3; public List<double[]> Pts = new List<double[]>(); }
    internal sealed class MbMarkText { public int R = 230, G = 40, B = 40; public double Size = 3; public double X, Y; public string Text = ""; }

    internal static class ManageBimIssues
    {
        public static string OutboxFile() => Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "ManageBIM", "Outbox", "issues.txt");

        private static double[] V(string s)
        {
            var p = s.Split(new[] { ' ' }, StringSplitOptions.RemoveEmptyEntries);
            var r = new double[3];
            for (int i = 0; i < 3 && i < p.Length; i++) double.TryParse(p[i], NumberStyles.Float, CultureInfo.InvariantCulture, out r[i]);
            return r;
        }

        private static void Rgb(string s, out int r, out int g, out int b)
        {
            var v = V(s);
            r = (int)Math.Max(0, Math.Min(255, v[0])); g = (int)Math.Max(0, Math.Min(255, v[1])); b = (int)Math.Max(0, Math.Min(255, v[2]));
            if (r == 0 && g == 0 && b == 0 && s.Trim().Length == 0) { r = 230; g = 40; b = 40; }
        }

        /// <summary>Punct de markup (coordonate de vedere) -> punct 3D pe planul aflat la distanta d in fata camerei.</summary>
        public static double[] OnPlane(MbIssue it, double x, double y, double d)
        {
            double[] N(double[] v) { double l = Math.Sqrt(v[0] * v[0] + v[1] * v[1] + v[2] * v[2]); if (l < 1e-12) l = 1; return new[] { v[0] / l, v[1] / l, v[2] / l }; }
            double[] X(double[] a, double[] b) => new[] { a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0] };
            var f = N(it.Fwd);
            var right = N(X(f, it.Up));
            var up = X(right, f);
            // inaltimea vederii la distanta d (perspectiva) sau inaltimea ortografica
            double h = it.OrthoHeight > 0 ? it.OrthoHeight : 2 * d * Math.Tan(it.FovDeg * Math.PI / 360.0);
            var r = new double[3];
            for (int i = 0; i < 3; i++) r[i] = it.Eye[i] + f[i] * d + right[i] * x * h - up[i] * y * h;
            return r;
        }

        private static string Un(string s) => s.Replace("\\n", "\n").Replace("\\\\", "\\");

        public static List<MbIssue> Read(out string error, string file = null)
        {
            error = null;
            var list = new List<MbIssue>();
            string f = string.IsNullOrEmpty(file) ? OutboxFile() : file;
            if (!File.Exists(f)) { error = "No issues from ManageBIM yet.\nOpen ManageBIM, open the model and save at least one viewpoint (Review > Save View)."; return list; }
            MbIssue cur = null;
            foreach (string line in File.ReadAllLines(f))
            {
                var t = line.Split('\t');
                switch (t[0])
                {
                    case "VIEW":
                        cur = new MbIssue { Name = t.Length > 1 ? Un(t[1]) : "", Status = t.Length > 2 ? Un(t[2]) : "", Guid = t.Length > 3 ? t[3] : "", Priority = t.Length > 4 ? Un(t[4]) : "", Assigned = t.Length > 5 ? Un(t[5]) : "" };
                        break;
                    case "CAM":
                        if (cur != null && t.Length >= 6)
                        {
                            cur.Eye = V(t[1]); cur.Fwd = V(t[2]); cur.Up = V(t[3]);
                            double.TryParse(t[4], NumberStyles.Float, CultureInfo.InvariantCulture, out cur.FovDeg);
                            double.TryParse(t[5], NumberStyles.Float, CultureInfo.InvariantCulture, out cur.OrthoHeight);
                        }
                        break;
                    case "BOX":
                        if (cur != null && t.Length >= 3) { cur.BoxMin = V(t[1]); cur.BoxMax = V(t[2]); }
                        break;
                    case "SEL":
                        if (cur != null) for (int i = 1; i < t.Length; i++) if (t[i].Length > 0) cur.Selected.Add(Un(t[i]));
                        break;
                    case "TXT":
                        if (cur != null && t.Length > 1) cur.Text = Un(t[1]);
                        break;
                    case "RED":
                        if (cur != null && t.Length >= 4)
                        {
                            var ml = new MbMarkLine();
                            Rgb(t[1], out ml.R, out ml.G, out ml.B);
                            double.TryParse(t[2], NumberStyles.Float, CultureInfo.InvariantCulture, out ml.Width);
                            var n = t[3].Split(new[] { ' ' }, StringSplitOptions.RemoveEmptyEntries);
                            for (int i = 0; i + 1 < n.Length; i += 2)
                            {
                                double.TryParse(n[i], NumberStyles.Float, CultureInfo.InvariantCulture, out double x);
                                double.TryParse(n[i + 1], NumberStyles.Float, CultureInfo.InvariantCulture, out double y);
                                ml.Pts.Add(new[] { x, y });
                            }
                            if (ml.Pts.Count >= 2) cur.Lines.Add(ml);
                        }
                        break;
                    case "RTX":
                        if (cur != null && t.Length >= 5)
                        {
                            var mt = new MbMarkText { Text = Un(t[4]) };
                            Rgb(t[1], out mt.R, out mt.G, out mt.B);
                            double.TryParse(t[2], NumberStyles.Float, CultureInfo.InvariantCulture, out mt.Size);
                            var p = V(t[3] + " 0");
                            mt.X = p[0]; mt.Y = p[1];
                            if (mt.Text.Trim().Length > 0) cur.Texts.Add(mt);
                        }
                        break;                    case "END":
                        if (cur != null) list.Add(cur);
                        cur = null;
                        break;
                }
            }
            if (list.Count == 0) error = "ManageBIM has no viewpoints / issues for the open project.";
            return list;
        }

        /// <summary>Fereastra cu issue-urile: intoarce indicii alesi (bifati), sau null la Cancel.</summary>
        public static List<int> Pick(List<MbIssue> issues, string title, string okText, bool multi)
        {
            using (var form = new Form { Text = title, Width = 620, Height = 480, StartPosition = FormStartPosition.CenterScreen, MinimizeBox = false, MaximizeBox = false, ShowInTaskbar = false })
            {
                var info = new Label { Dock = DockStyle.Top, Height = 34, Padding = new Padding(8, 8, 8, 0), Text = multi ? "Tick the issues to bring in (camera, section box, comments):" : "Choose an issue - a 3D view with its camera and section box is created and its elements are selected:" };
                var lst = new CheckedListBox { Dock = DockStyle.Fill, CheckOnClick = true, IntegralHeight = false };
                foreach (var i in issues) lst.Items.Add(i, multi);
                var details = new TextBox { Dock = DockStyle.Bottom, Height = 90, Multiline = true, ReadOnly = true, ScrollBars = ScrollBars.Vertical };
                lst.SelectedIndexChanged += (s, e) =>
                {
                    try
                    {
                        if (lst.SelectedItem is MbIssue it)
                        {
                            details.Text = (string.IsNullOrEmpty(it.Assigned) ? "" : "Assigned to: " + it.Assigned + "\r\n") + it.Text.Replace("\n", "\r\n") + (it.Selected.Count > 0 ? $"\r\n{it.Selected.Count} selected elements" : "") + (it.Lines.Count + it.Texts.Count > 0 ? $"\r\n{it.Lines.Count} markup lines, {it.Texts.Count} texts" : "");
                            if (!multi) for (int k = 0; k < lst.Items.Count; k++) lst.SetItemChecked(k, k == lst.SelectedIndex);
                        }
                    }
                    catch (Exception ex) { ManageBimLink.Log("Revit", "Issue list: " + ex); }
                };
                var bar = new FlowLayoutPanel { Dock = DockStyle.Bottom, Height = 40, FlowDirection = FlowDirection.RightToLeft, Padding = new Padding(6) };
                var ok = new Button { Text = okText, DialogResult = DialogResult.OK, Width = 150 };
                var cancel = new Button { Text = "Cancel", DialogResult = DialogResult.Cancel, Width = 90 };
                bar.Controls.Add(ok);
                bar.Controls.Add(cancel);
                form.Controls.Add(lst);
                form.Controls.Add(details);
                form.Controls.Add(bar);
                form.Controls.Add(info);
                form.AcceptButton = ok;
                form.CancelButton = cancel;
                if (lst.Items.Count > 0) lst.SelectedIndex = 0;
                if (form.ShowDialog() != DialogResult.OK) return null;
                var r = new List<int>();
                foreach (int k in lst.CheckedIndices) r.Add(k);
                if (r.Count == 0 && lst.SelectedIndex >= 0) r.Add(lst.SelectedIndex);
                return r;
            }
        }

        /// <summary>Rotatia camerei Navisworks (priveste spre -Z, sus +Y) ca quaternion (a, b, c, d) = (x, y, z, w).</summary>
        public static double[] Quaternion(double[] fwd, double[] up)
        {
            double[] N(double[] v) { double l = Math.Sqrt(v[0] * v[0] + v[1] * v[1] + v[2] * v[2]); if (l < 1e-12) l = 1; return new[] { v[0] / l, v[1] / l, v[2] / l }; }
            double[] X(double[] a, double[] b) => new[] { a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0] };
            var z = N(new[] { -fwd[0], -fwd[1], -fwd[2] });
            var x = N(X(up, z));
            var y = X(z, x);
            double tr = x[0] + y[1] + z[2], qx, qy, qz, qw;
            if (tr > 0) { double s = Math.Sqrt(tr + 1) * 2; qw = 0.25 * s; qx = (y[2] - z[1]) / s; qy = (z[0] - x[2]) / s; qz = (x[1] - y[0]) / s; }
            else if (x[0] > y[1] && x[0] > z[2]) { double s = Math.Sqrt(1 + x[0] - y[1] - z[2]) * 2; qw = (y[2] - z[1]) / s; qx = 0.25 * s; qy = (y[0] + x[1]) / s; qz = (z[0] + x[2]) / s; }
            else if (y[1] > z[2]) { double s = Math.Sqrt(1 + y[1] - x[0] - z[2]) * 2; qw = (z[0] - x[2]) / s; qx = (y[0] + x[1]) / s; qy = 0.25 * s; qz = (z[1] + y[2]) / s; }
            else { double s = Math.Sqrt(1 + z[2] - x[0] - y[1]) * 2; qw = (x[1] - y[0]) / s; qx = (z[0] + x[2]) / s; qy = (z[1] + y[2]) / s; qz = 0.25 * s; }
            return new[] { qx, qy, qz, qw };
        }
    }
}
