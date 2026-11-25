using System;
using System.Collections.Generic;
using System.Drawing;
using System.Globalization;
using System.IO;
using System.Linq;
using System.Windows.Forms;
using Autodesk.Revit.Attributes;
using Autodesk.Revit.DB;
using Autodesk.Revit.UI;
using Color = Autodesk.Revit.DB.Color;
using Form = System.Windows.Forms.Form;
using TaskDialog = Autodesk.Revit.UI.TaskDialog;
using Control = System.Windows.Forms.Control;
using View = System.Windows.Forms.View;
using Point = System.Drawing.Point;
using Rectangle = System.Drawing.Rectangle;

namespace ManageBIM.Revit
{
    /// <summary>Un obiect de coordonare trimis din ManageBIM (coordonate partajate, metri).</summary>
    internal sealed class MbCoord
    {
        public string Kind = "", Name = "", Guid = "", Status = "";
        public double[] Centre = new double[3], Size = new double[3];
        public double RotDeg;
        public int R = 150, G = 150, B = 150, A = 120;
        public string Label => Kind == "void" ? "Void" : Kind == "clearance" ? "Clearance" : "Placeholder";
    }

    /// <summary>
    /// "ManageBIM Coordination": arata in Revit zonele de mentenanta (clearance), golurile propuse (voids)
    /// si volumele rezervate (placeholders) trimise din ManageBIM - DOAR afisare: cutii transparente
    /// DirectShape (Generic Models) cu culoarea fiecarui tip. Nu taie si nu modifica elementele modelului.
    /// La o noua trimitere, formele vechi se inlocuiesc. Inainte de creare, o fereastra de previzualizare
    /// (lista cu bife + vedere de sus) arata ce vine.
    /// </summary>
    [Transaction(TransactionMode.Manual)]
    [Regeneration(RegenerationOption.Manual)]
    public class CoordinationCommand : IExternalCommand
    {
        private const double FeetToMetres = 0.3048;
        internal const string AppDataPrefix = "coord|";

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
            if (!File.Exists(f)) { err = "Nothing sent from ManageBIM yet.\n\nIn ManageBIM: Coordination › Objects › Send to Revit."; return list; }
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

        public Result Execute(ExternalCommandData commandData, ref string message, ElementSet elements)
        {
            UIDocument uidoc = commandData.Application.ActiveUIDocument;
            if (uidoc == null) { message = "Open a project first."; return Result.Failed; }
            Document doc = uidoc.Document;
            var objs = Read(out string err);
            if (objs.Count == 0) { TaskDialog.Show("ManageBIM", err); return Result.Cancelled; }

            var chosen = Preview(objs, out bool removeOld);
            if (chosen == null) return Result.Cancelled;

            Transform total = doc.ActiveProjectLocation?.GetTotalTransform() ?? Transform.Identity;
            var made = new List<ElementId>();
            int removed = 0;
            using (var tr = new Transaction(doc, "ManageBIM coordination objects"))
            {
                tr.Start();
                if (removeOld)
                {
                    var old = new FilteredElementCollector(doc).OfClass(typeof(DirectShape)).Cast<DirectShape>()
                        .Where(d => d.ApplicationId == "ManageBIM" && (d.ApplicationDataId ?? "").StartsWith(AppDataPrefix)).Select(d => d.Id).ToList();
                    if (old.Count > 0) { doc.Delete(old); removed = old.Count; }
                }
                var mats = new Dictionary<string, ElementId>();
                foreach (var o in chosen)
                {
                    try
                    {
                        // cutia in coordonate partajate (metri), rotita in jurul verticalei, apoi in coordonate interne
                        double rad = o.RotDeg * Math.PI / 180.0, co = Math.Cos(rad), si = Math.Sin(rad);
                        double hx = o.Size[0] / 2, hy = o.Size[1] / 2;
                        XYZ P(double lx, double ly, double z)
                        {
                            double x = o.Centre[0] + lx * co - ly * si, y = o.Centre[1] + lx * si + ly * co;
                            return total.OfPoint(new XYZ(x / FeetToMetres, y / FeetToMetres, z / FeetToMetres));
                        }
                        double z0 = o.Centre[2] - o.Size[2] / 2;
                        var loop = new CurveLoop();
                        var pts = new[] { P(-hx, -hy, z0), P(hx, -hy, z0), P(hx, hy, z0), P(-hx, hy, z0) };
                        for (int i = 0; i < 4; i++) loop.Append(Line.CreateBound(pts[i], pts[(i + 1) % 4]));
                        double h = Math.Max(o.Size[2], 0.01) / FeetToMetres;
                        Solid solid = GeometryCreationUtilities.CreateExtrusionGeometry(new[] { loop }, XYZ.BasisZ, h, new SolidOptions(Material(doc, mats, o), ElementId.InvalidElementId));
                        var ds = DirectShape.CreateElement(doc, new ElementId(BuiltInCategory.OST_GenericModel));
                        ds.ApplicationId = "ManageBIM";
                        ds.ApplicationDataId = AppDataPrefix + o.Guid;
                        ds.SetShape(new GeometryObject[] { solid });
                        try { ds.SetName($"{o.Label} - {o.Name}"); } catch { }
                        try { ds.get_Parameter(BuiltInParameter.ALL_MODEL_INSTANCE_COMMENTS)?.Set($"ManageBIM {o.Label} ({o.Status})"); } catch { }
                        try { ds.get_Parameter(BuiltInParameter.ALL_MODEL_MARK)?.Set(o.Name); } catch { }
                        made.Add(ds.Id);
                    }
                    catch (Exception e) { TaskDialog.Show("ManageBIM", $"{o.Name}: {e.Message}"); }
                }
                tr.Commit();
            }
            if (made.Count > 0) uidoc.Selection.SetElementIds(made);
            TaskDialog.Show("ManageBIM coordination", $"{made.Count} object(s) shown as transparent Generic Models (selected now)." + (removed > 0 ? $"\n{removed} older ManageBIM object(s) replaced." : "") + "\n\nDisplay only: walls, floors and other elements are not changed.");
            return Result.Succeeded;
        }

        /// <summary>Materialul "ManageBIM <tip>" cu culoarea si transparenta tipului.</summary>
        private static ElementId Material(Document doc, Dictionary<string, ElementId> cache, MbCoord o)
        {
            string name = "ManageBIM " + o.Label;
            if (cache.TryGetValue(name, out var id)) return id;
            var m = new FilteredElementCollector(doc).OfClass(typeof(Material)).Cast<Material>().FirstOrDefault(x => x.Name == name);
            if (m == null)
            {
                id = Autodesk.Revit.DB.Material.Create(doc, name);
                m = (Material)doc.GetElement(id);
            }
            m.Color = new Color((byte)o.R, (byte)o.G, (byte)o.B);
            m.Transparency = Math.Max(30, Math.Min(85, (int)Math.Round((1 - o.A / 255.0) * 100)));
            cache[name] = m.Id;
            return m.Id;
        }

        /// <summary>Fereastra de previzualizare: lista cu bife (tip, nume, dimensiuni, stare) si o vedere de sus.
        /// Intoarce obiectele bifate, sau null la Cancel.</summary>
        private static List<MbCoord> Preview(List<MbCoord> objs, out bool removeOld)
        {
            removeOld = true;
            using (var form = new Form { Text = "ManageBIM - coordination objects", Width = 860, Height = 520, StartPosition = FormStartPosition.CenterScreen, MinimizeBox = false, ShowInTaskbar = false, Font = new Font("Segoe UI", 9f) })
            {
                var info = new Label { Dock = DockStyle.Top, Height = 30, Padding = new Padding(8, 8, 8, 0), Text = $"{objs.Count} object(s) from ManageBIM. They are shown as transparent boxes only - nothing in the model is cut or changed." };
                var split = new SplitContainer { Dock = DockStyle.Fill, SplitterDistance = 430 };
                var lst = new ListView { Dock = DockStyle.Fill, View = View.Details, CheckBoxes = true, FullRowSelect = true, GridLines = false, HideSelection = false };
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
                plan.Paint += (s, e) => DrawPlan(e.Graphics, plan.ClientRectangle, objs, lst);
                lst.ItemChecked += (s, e) => plan.Invalidate();
                lst.SelectedIndexChanged += (s, e) => plan.Invalidate();
                split.Panel1.Controls.Add(lst);
                split.Panel2.Controls.Add(plan);
                var bar = new FlowLayoutPanel { Dock = DockStyle.Bottom, Height = 42, FlowDirection = FlowDirection.RightToLeft, Padding = new Padding(6) };
                var ok = new Button { Text = "Show in Revit", DialogResult = DialogResult.OK, Width = 130, Height = 28 };
                var cancel = new Button { Text = "Cancel", DialogResult = DialogResult.Cancel, Width = 90, Height = 28 };
                var replace = new CheckBox { Text = "Replace ManageBIM objects shown before", Checked = true, AutoSize = true, Margin = new Padding(12, 7, 12, 0) };
                var all = new Button { Text = "All", Width = 60, Height = 28 };
                var none = new Button { Text = "None", Width = 60, Height = 28 };
                all.Click += (s, e) => { foreach (ListViewItem i in lst.Items) i.Checked = true; };
                none.Click += (s, e) => { foreach (ListViewItem i in lst.Items) i.Checked = false; };
                bar.Controls.AddRange(new Control[] { ok, cancel, replace, none, all });
                form.Controls.Add(split);
                form.Controls.Add(info);
                form.Controls.Add(bar);
                form.AcceptButton = ok;
                form.CancelButton = cancel;
                if (form.ShowDialog() != DialogResult.OK) return null;
                removeOld = replace.Checked;
                return lst.Items.Cast<ListViewItem>().Where(i => i.Checked).Select(i => (MbCoord)i.Tag).ToList();
            }
        }

        /// <summary>Vederea de sus: conturul fiecarei cutii (rotita), colorat dupa tip; cele debifate sunt sterse,
        /// cea selectata in lista e ingrosata.</summary>
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
                var col = System.Drawing.Color.FromArgb(on ? 110 : 25, o.R, o.G, o.B);
                using (var br = new SolidBrush(col)) g.FillPolygon(br, pts);
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
