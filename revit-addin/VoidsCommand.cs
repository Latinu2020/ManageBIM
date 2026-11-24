using System;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.Linq;
using Autodesk.Revit.Attributes;
using Autodesk.Revit.DB;
using Autodesk.Revit.UI;

namespace ManageBIM.Revit
{
    /// <summary>
    /// "ManageBIM Voids": citeste golurile APROBATE trimise din ManageBIM (Coordination objects ›
    /// Send voids to Revit, fisierul %LOCALAPPDATA%\ManageBIM\Outbox\voids.txt) si le taie in model:
    /// in pereti un gol rectangular (Opening pe perete), in placi / acoperisuri un gol vertical (Shaft
    /// opening intre cotele golului). Gazda se cauta dupa IFC GUID / UniqueId, altfel elementul care
    /// contine centrul golului. Coordonatele ManageBIM sunt cele partajate (shared), in metri.
    /// </summary>
    [Transaction(TransactionMode.Manual)]
    [Regeneration(RegenerationOption.Manual)]
    public class VoidsCommand : IExternalCommand
    {
        private const double FeetToMetres = 0.3048;

        private sealed class MbVoid
        {
            public string Name = "", HostGuid = "", Guid = "";
            public double[] Min = new double[3], Max = new double[3];
        }

        private static string OutboxFile() => Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "ManageBIM", "Outbox", "voids.txt");

        private static double[] V(string s)
        {
            var p = s.Split(new[] { ' ' }, StringSplitOptions.RemoveEmptyEntries);
            var r = new double[3];
            for (int i = 0; i < 3 && i < p.Length; i++) double.TryParse(p[i], NumberStyles.Float, CultureInfo.InvariantCulture, out r[i]);
            return r;
        }

        private static List<MbVoid> Read(out string err)
        {
            err = "";
            var list = new List<MbVoid>();
            string f = OutboxFile();
            if (!File.Exists(f)) { err = "No voids from ManageBIM yet.\n\nIn ManageBIM: Coordination › Objects, set the voids to Approved and press 'Send voids to Revit'."; return list; }
            foreach (var line in File.ReadAllLines(f))
            {
                if (!line.StartsWith("void\t")) continue;
                var c = line.Split('\t');
                if (c.Length < 4) continue;
                list.Add(new MbVoid { Name = c[1], Min = V(c[2]), Max = V(c[3]), HostGuid = c.Length > 4 ? c[4].Trim() : "", Guid = c.Length > 5 ? c[5].Trim() : "" });
            }
            if (list.Count == 0) err = "The voids file from ManageBIM is empty.";
            return list;
        }

        public Result Execute(ExternalCommandData commandData, ref string message, ElementSet elements)
        {
            UIDocument uidoc = commandData.Application.ActiveUIDocument;
            if (uidoc == null) { message = "Open a project first."; return Result.Failed; }
            Document doc = uidoc.Document;
            var voids = Read(out string err);
            if (voids.Count == 0) { TaskDialog.Show("ManageBIM", err); return Result.Cancelled; }

            var ask = new TaskDialog("ManageBIM voids")
            {
                MainInstruction = $"Create {voids.Count} approved void(s) from ManageBIM?",
                MainContent = "Walls get a rectangular opening, floors and roofs a vertical shaft opening. Each void is a separate Revit element; you can undo the whole operation with Ctrl+Z.",
                CommonButtons = TaskDialogCommonButtons.Yes | TaskDialogCommonButtons.No,
                DefaultButton = TaskDialogResult.Yes,
            };
            if (ask.Show() != TaskDialogResult.Yes) return Result.Cancelled;

            // shared (metri) -> coordonate interne Revit (picioare)
            Transform total = doc.ActiveProjectLocation?.GetTotalTransform() ?? Transform.Identity;
            XYZ P(double x, double y, double z) => total.OfPoint(new XYZ(x / FeetToMetres, y / FeetToMetres, z / FeetToMetres));

            // gazdele posibile: pereti, placi, acoperisuri - indexate dupa IFC GUID si UniqueId
            var hosts = new List<Element>();
            hosts.AddRange(new FilteredElementCollector(doc).OfClass(typeof(Wall)).WhereElementIsNotElementType());
            hosts.AddRange(new FilteredElementCollector(doc).OfClass(typeof(Floor)).WhereElementIsNotElementType());
            hosts.AddRange(new FilteredElementCollector(doc).OfClass(typeof(RoofBase)).WhereElementIsNotElementType());
            var byGuid = new Dictionary<string, Element>(StringComparer.OrdinalIgnoreCase);
            foreach (var h in hosts)
            {
                byGuid[h.UniqueId] = h;
                string g = h.get_Parameter(BuiltInParameter.IFC_GUID)?.AsString();
                if (!string.IsNullOrEmpty(g)) byGuid[g] = h;
            }

            int made = 0;
            var failed = new List<string>();
            using (var tr = new Transaction(doc, "ManageBIM voids"))
            {
                tr.Start();
                foreach (var v in voids)
                {
                    try
                    {
                        XYZ a = P(v.Min[0], v.Min[1], v.Min[2]);
                        XYZ b = P(v.Max[0], v.Max[1], v.Max[2]);
                        XYZ lo = new XYZ(Math.Min(a.X, b.X), Math.Min(a.Y, b.Y), Math.Min(a.Z, b.Z));
                        XYZ hi = new XYZ(Math.Max(a.X, b.X), Math.Max(a.Y, b.Y), Math.Max(a.Z, b.Z));
                        XYZ mid = (lo + hi) * 0.5;
                        Element host = null;
                        if (!string.IsNullOrEmpty(v.HostGuid)) byGuid.TryGetValue(v.HostGuid, out host);
                        if (host == null)
                        {
                            // gazda: primul perete / placa a carui cutie contine centrul golului
                            host = hosts.FirstOrDefault(h =>
                            {
                                var bb = h.get_BoundingBox(null);
                                return bb != null && mid.X >= bb.Min.X && mid.X <= bb.Max.X && mid.Y >= bb.Min.Y && mid.Y <= bb.Max.Y && mid.Z >= bb.Min.Z && mid.Z <= bb.Max.Z;
                            });
                        }
                        if (host == null) { failed.Add(v.Name + ": no wall or floor found at its position"); continue; }

                        Element op;
                        if (host is Wall wall)
                        {
                            // gol rectangular pe perete: doua colturi opuse in planul peretelui (Revit le proiecteaza)
                            op = doc.Create.NewOpening(wall, lo, hi);
                        }
                        else
                        {
                            // placa / acoperis: gol vertical (shaft) cu conturul golului, intre cotele lui
                            Level lvl = new FilteredElementCollector(doc).OfClass(typeof(Level)).Cast<Level>()
                                .OrderBy(l => Math.Abs(l.Elevation - lo.Z)).FirstOrDefault(l => l.Elevation <= lo.Z + 1e-6)
                                ?? new FilteredElementCollector(doc).OfClass(typeof(Level)).Cast<Level>().OrderBy(l => l.Elevation).First();
                            var arr = new CurveArray();
                            var p0 = new XYZ(lo.X, lo.Y, lvl.Elevation);
                            var p1 = new XYZ(hi.X, lo.Y, lvl.Elevation);
                            var p2 = new XYZ(hi.X, hi.Y, lvl.Elevation);
                            var p3 = new XYZ(lo.X, hi.Y, lvl.Elevation);
                            arr.Append(Line.CreateBound(p0, p1));
                            arr.Append(Line.CreateBound(p1, p2));
                            arr.Append(Line.CreateBound(p2, p3));
                            arr.Append(Line.CreateBound(p3, p0));
                            Opening shaft = doc.Create.NewOpening(lvl, null, arr);
                            // cotele golului; un parametru read-only nu anuleaza golul deja creat
                            void Try(BuiltInParameter bip, Action<Parameter> set) { try { var prm = shaft.get_Parameter(bip); if (prm != null && !prm.IsReadOnly) set(prm); } catch { } }
                            Try(BuiltInParameter.WALL_BASE_OFFSET, prm => prm.Set(lo.Z - lvl.Elevation));
                            Try(BuiltInParameter.WALL_HEIGHT_TYPE, prm => prm.Set(ElementId.InvalidElementId));
                            Try(BuiltInParameter.WALL_USER_HEIGHT_PARAM, prm => prm.Set(Math.Max(hi.Z - lo.Z, 0.01)));
                            op = shaft;
                        }
                        try { op.get_Parameter(BuiltInParameter.ALL_MODEL_INSTANCE_COMMENTS)?.Set("ManageBIM: " + v.Name); } catch { }
                        try { if (!string.IsNullOrEmpty(v.Guid)) op.get_Parameter(BuiltInParameter.IFC_GUID)?.Set(v.Guid); } catch { }
                        made++;
                    }
                    catch (Exception e)
                    {
                        failed.Add(v.Name + ": " + e.Message);
                    }
                }
                tr.Commit();
            }
            string report = $"{made} void(s) created.";
            if (failed.Count > 0) report += $"\n\n{failed.Count} could not be created:\n" + string.Join("\n", failed.Take(15));
            TaskDialog.Show("ManageBIM voids", report);
            return made > 0 ? Result.Succeeded : Result.Failed;
        }
    }
}
