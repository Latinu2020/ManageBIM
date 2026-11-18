using System;
using System.Collections.Generic;
using System.Linq;
using Autodesk.Revit.Attributes;
using Autodesk.Revit.DB;
using Autodesk.Revit.UI;

namespace ManageBIM.Revit
{
    /// <summary>
    /// "ManageBIM Issues": alegi un issue din ManageBIM -> vedere 3D cu aceeasi camera si acelasi section box,
    /// cu elementele issue-ului selectate. Coordonatele ManageBIM sunt cele partajate (shared), in metri.
    /// </summary>
    [Transaction(TransactionMode.Manual)]
    [Regeneration(RegenerationOption.Manual)]
    public class IssuesCommand : IExternalCommand
    {
        private const double FeetToMetres = 0.3048;

        public Result Execute(ExternalCommandData commandData, ref string message, ElementSet elements)
        {
            UIDocument uidoc = commandData.Application.ActiveUIDocument;
            if (uidoc == null) { message = "Open a project first."; return Result.Failed; }
            Document doc = uidoc.Document;
            var issues = ManageBimIssues.Read(out string err);
            if (issues.Count == 0) { TaskDialog.Show("ManageBIM", err); return Result.Cancelled; }
            var pick = ManageBimIssues.Pick(issues, "ManageBIM issues", "Open in a 3D view", false);
            if (pick == null || pick.Count == 0) return Result.Cancelled;
            MbIssue it = issues[pick[0]];

            // shared (metri) -> coordonate interne Revit (picioare); exportul face invers: toShared = total.Inverse
            Transform total = doc.ActiveProjectLocation?.GetTotalTransform() ?? Transform.Identity;
            XYZ P(double[] v) => total.OfPoint(new XYZ(v[0] / FeetToMetres, v[1] / FeetToMetres, v[2] / FeetToMetres));
            XYZ D(double[] v) => total.OfVector(new XYZ(v[0], v[1], v[2])).Normalize();

            View3D view;
            using (var tr = new Transaction(doc, "ManageBIM issue view"))
            {
                tr.Start();
                string name = ViewName(doc, "ManageBIM - " + it.Name);
                var vft = new FilteredElementCollector(doc).OfClass(typeof(ViewFamilyType)).Cast<ViewFamilyType>().First(x => x.ViewFamily == ViewFamily.ThreeDimensional);
                bool persp = it.OrthoHeight <= 0;
                view = persp ? View3D.CreatePerspective(doc, vft.Id) : View3D.CreateIsometric(doc, vft.Id);
                view.Name = name;
                XYZ fwd = D(it.Fwd);
                XYZ up = D(it.Up);
                // sus perpendicular pe directie (cerinta Revit)
                up = (up - fwd.Multiply(up.DotProduct(fwd))).Normalize();
                view.SetOrientation(new ViewOrientation3D(P(it.Eye), up, fwd));
                if (it.BoxMin != null && it.BoxMax != null)
                {
                    // cutia e aliniata la axele partajate: transformarea cutiei = shared -> intern
                    var bb = new BoundingBoxXYZ
                    {
                        Transform = total,
                        Min = new XYZ(Math.Min(it.BoxMin[0], it.BoxMax[0]) / FeetToMetres, Math.Min(it.BoxMin[1], it.BoxMax[1]) / FeetToMetres, Math.Min(it.BoxMin[2], it.BoxMax[2]) / FeetToMetres),
                        Max = new XYZ(Math.Max(it.BoxMin[0], it.BoxMax[0]) / FeetToMetres, Math.Max(it.BoxMin[1], it.BoxMax[1]) / FeetToMetres, Math.Max(it.BoxMin[2], it.BoxMax[2]) / FeetToMetres),
                    };
                    try { view.SetSectionBox(bb); view.IsSectionBoxActive = true; } catch { }
                }
                tr.Commit();
            }
            uidoc.ActiveView = view;
            string markInfo = it.Lines.Count + it.Texts.Count > 0 ? AddMarkups(doc, view, it, P, D) : "";

            // elementele issue-ului: UniqueId (export Revit) sau parametrul IFC GUID
            var ids = new List<ElementId>();
            var want = new HashSet<string>(it.Selected.Select(g => g.Contains(":") ? g.Substring(g.LastIndexOf(':') + 1) : g));
            foreach (string g in want.ToList())
            {
                Element e = null;
                try { e = doc.GetElement(g); } catch { }
                if (e != null) { ids.Add(e.Id); want.Remove(g); }
            }
            if (want.Count > 0)
            {
                foreach (Element e in new FilteredElementCollector(doc).WhereElementIsNotElementType())
                {
                    Parameter p = e.get_Parameter(BuiltInParameter.IFC_GUID);
                    string v = p?.AsString();
                    if (!string.IsNullOrEmpty(v) && want.Remove(v)) ids.Add(e.Id);
                    if (want.Count == 0) break;
                }
            }
            if (ids.Count > 0) uidoc.Selection.SetElementIds(ids);

            var td = new TaskDialog("ManageBIM issue")
            {
                MainInstruction = it.Name,
                MainContent = $"Status: {it.Status}   Priority: {it.Priority}" + (string.IsNullOrEmpty(it.Assigned) ? "" : $"   Assigned to: {it.Assigned}") +
                              (string.IsNullOrEmpty(it.Text) ? "" : "\n\n" + it.Text) +
                              (it.Selected.Count > 0 ? $"\n\n{ids.Count} of {it.Selected.Count} elements found and selected." : "") + markInfo,
            };
            td.Show();
            return Result.Succeeded;
        }

        /// <summary>
        /// Markup-urile issue-ului in vederea 3D: liniile ca Model Lines colorate (stil de linie "ManageBIM Markup RRGGBB")
        /// pe un plan perpendicular pe directia camerei, imediat in interiorul section box-ului, grupate intr-un grup
        /// "ManageBIM markup - ..." (se sterge usor); textele ca Text Notes in vederea blocata.
        /// </summary>
        private static string AddMarkups(Document doc, View3D view, MbIssue it, Func<double[], XYZ> P, Func<double[], XYZ> D)
        {
            // distanta planului (metri): la intrarea in section box (+10 cm), altfel 2 m in fata camerei
            double d = 2.0;
            if (it.BoxMin != null && it.BoxMax != null)
            {
                double along = 0, half = 0;
                double fl = Math.Sqrt(it.Fwd[0] * it.Fwd[0] + it.Fwd[1] * it.Fwd[1] + it.Fwd[2] * it.Fwd[2]);
                for (int i = 0; i < 3; i++)
                {
                    double f = it.Fwd[i] / (fl > 0 ? fl : 1);
                    double c = (it.BoxMin[i] + it.BoxMax[i]) * 0.5;
                    along += (c - it.Eye[i]) * f;
                    half += Math.Abs(f) * Math.Abs(it.BoxMax[i] - it.BoxMin[i]) * 0.5;
                }
                d = Math.Max(0.5, along - half + 0.1);
            }
            int lines = 0, texts = 0, textFailed = 0;
            var curveIds = new List<ElementId>();
            using (var tr = new Transaction(doc, "ManageBIM markups"))
            {
                tr.Start();
                XYZ origin = P(ManageBimIssues.OnPlane(it, 0, 0, d));
                SketchPlane sp = SketchPlane.Create(doc, Plane.CreateByNormalAndOrigin(D(it.Fwd), origin));
                double tol = doc.Application.ShortCurveTolerance;
                foreach (var l in it.Lines)
                {
                    GraphicsStyle gs = MarkupStyle(doc, l.R, l.G, l.B);
                    XYZ prev = null;
                    foreach (var p in l.Pts)
                    {
                        XYZ q = P(ManageBimIssues.OnPlane(it, p[0], p[1], d));
                        if (prev != null && prev.DistanceTo(q) > tol * 1.5)
                        {
                            try
                            {
                                ModelCurve mc = doc.Create.NewModelCurve(Line.CreateBound(prev, q), sp);
                                if (gs != null) mc.LineStyle = gs;
                                curveIds.Add(mc.Id);
                                lines++;
                            }
                            catch { }
                            prev = q;
                        }
                        else if (prev == null) prev = q;
                    }
                }
                if (curveIds.Count > 0)
                {
                    try
                    {
                        Group g = doc.Create.NewGroup(curveIds);
                        string gname = "ManageBIM markup - " + it.Name;
                        foreach (char c in "{}[]|;<>?`~:\\") gname = gname.Replace(c, '-');
                        var used = new HashSet<string>(new FilteredElementCollector(doc).OfClass(typeof(GroupType)).Select(x => x.Name));
                        string n = gname;
                        for (int i = 2; used.Contains(n); i++) n = $"{gname} ({i})";
                        g.GroupType.Name = n;
                    }
                    catch { }
                }
                if (it.Texts.Count > 0)
                {
                    // textele cer o vedere 3D blocata
                    try { if (!view.IsLocked) view.SaveOrientationAndLock(); } catch { }
                    // inaltimea textului: ~3.5% din inaltimea vederii la planul markup-urilor (marime pe hartie = model / scara)
                    double viewH = it.OrthoHeight > 0 ? it.OrthoHeight : 2 * d * Math.Tan(it.FovDeg * Math.PI / 360.0);
                    double paperFt = Math.Max(0.0066, Math.Min(0.1, viewH * 0.035 / FeetToMetres / Math.Max(1, view.Scale)));
                    foreach (var t in it.Texts)
                    {
                        try
                        {
                            ElementId tt = MarkupTextType(doc, t.R, t.G, t.B, paperFt);
                            TextNote.Create(doc, view.Id, P(ManageBimIssues.OnPlane(it, t.X, t.Y, d)), t.Text, tt);
                            texts++;
                        }
                        catch { textFailed++; }
                    }
                }
                tr.Commit();
            }
            string s = $"\n\nMarkups: {lines} lines" + (it.Texts.Count > 0 ? $", {texts} texts" : "") + " (Model Lines in the group \"ManageBIM markup\" - delete the group to remove them).";
            if (textFailed > 0) s += $"\n{textFailed} texts could not be placed in this 3D view (Revit allows text only in locked 3D views):\n" + string.Join("\n", it.Texts.Select(x => "• " + x.Text));
            return s;
        }

        /// <summary>
        /// Tip de text "ManageBIM Markup RRGGBB h.hmm": copie a tipului implicit, cu culoarea markup-ului, bold,
        /// fara chenar, inaltimea data (picioare, pe hartie). Refolosit daca exista deja.
        /// </summary>
        private static ElementId MarkupTextType(Document doc, int r, int g, int b, double paperFt)
        {
            ElementId def = doc.GetDefaultElementTypeId(ElementTypeGroup.TextNoteType);
            try
            {
                string name = $"ManageBIM Markup {r:X2}{g:X2}{b:X2} {paperFt * 304.8:0.0}mm";
                var found = new FilteredElementCollector(doc).OfClass(typeof(TextNoteType)).Cast<TextNoteType>().FirstOrDefault(x => x.Name == name);
                if (found != null) return found.Id;
                var baseType = doc.GetElement(def) as TextNoteType;
                if (baseType == null) return def;
                var nt = baseType.Duplicate(name) as TextNoteType;
                if (nt == null) return def;
                nt.get_Parameter(BuiltInParameter.TEXT_SIZE)?.Set(paperFt);
                nt.get_Parameter(BuiltInParameter.LINE_COLOR)?.Set(r + (g << 8) + (b << 16));
                nt.get_Parameter(BuiltInParameter.TEXT_STYLE_BOLD)?.Set(1);
                nt.get_Parameter(BuiltInParameter.TEXT_BACKGROUND)?.Set(1); // transparent
                nt.get_Parameter(BuiltInParameter.TEXT_BOX_VISIBILITY)?.Set(0);
                return nt.Id;
            }
            catch { return def; }
        }

        /// <summary>Stil de linie "ManageBIM Markup RRGGBB" (subcategorie a Lines), creat la nevoie.</summary>
        private static GraphicsStyle MarkupStyle(Document doc, int r, int g, int b)
        {
            try
            {
                Category lines = doc.Settings.Categories.get_Item(BuiltInCategory.OST_Lines);
                string nm = $"ManageBIM Markup {r:X2}{g:X2}{b:X2}";
                Category sub = lines.SubCategories.Contains(nm) ? lines.SubCategories.get_Item(nm) : doc.Settings.Categories.NewSubcategory(lines, nm);
                sub.LineColor = new Color((byte)r, (byte)g, (byte)b);
                sub.SetLineWeight(6, GraphicsStyleType.Projection);
                return sub.GetGraphicsStyle(GraphicsStyleType.Projection);
            }
            catch { return null; }
        }

        /// <summary>Nume de vedere valid si unic (Revit refuza unele caractere si numele duplicate).</summary>
        private static string ViewName(Document doc, string name)
        {
            foreach (char c in "{}[]|;<>?`~:\\") name = name.Replace(c, '-');
            var used = new HashSet<string>(new FilteredElementCollector(doc).OfClass(typeof(View)).Cast<View>().Select(v => v.Name));
            string n = name;
            for (int i = 2; used.Contains(n); i++) n = $"{name} ({i})";
            return n;
        }
    }
}
