using System;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.Linq;
using System.Reflection;
using System.Windows.Forms;
using Autodesk.Navisworks.Api;
using Autodesk.Navisworks.Api.Plugins;
using NwApp = Autodesk.Navisworks.Api.Application;

namespace ManageBIM.Navis
{
    /// <summary>
    /// "ManageBIM Walk path": traseul de mers desenat in ManageBIM (Walk Path › To Navisworks, fisierul
    /// %LOCALAPPDATA%\ManageBIM\Outbox\walkpath.txt) devine o animatie de viewpoint-uri in Saved Viewpoints:
    /// cadre cheie la distante egale (viteza constanta), la inaltimea ochilor, privind inainte pe traseu.
    /// Animatia se face prin COM API (InwOpAnimView); daca nu merge, viewpoint-urile raman intr-un folder.
    /// Parametrul "quiet" (Automation / teste): fara ferestre; codul intors: 0 animatie, 4 folder, 1 eroare.
    /// </summary>
    [Plugin("ManageBIMWalk", "MBIM", DisplayName = "ManageBIM Walk path", ToolTip = "Turn the walk path drawn in ManageBIM (Walk Path › To Navisworks) into a viewpoint animation")]
    [AddInPlugin(AddInLocation.None)]
    public class WalkPlugin : AddInPlugin
    {
        public override int Execute(params string[] parameters) => Safe.Plugin(GetType().Name.Replace("Plugin", ""), parameters, () => Run(parameters));

        private int Run(string[] parameters)
        {
            bool quiet = parameters != null && parameters.Contains("quiet");
            string log = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "ManageBIM", "Outbox", "walkpath.log");
            void Say(string m) { if (!quiet) MessageBox.Show(m, "ManageBIM"); else try { File.WriteAllText(log, m); } catch { } }
            Document doc = NwApp.ActiveDocument;
            if (doc == null || doc.Models.Count == 0)
            {
                Say("Open the model first (the same model as in ManageBIM).");
                return 1;
            }
            // implicit fisierul scris de ManageBIM; din Automation se poate da altul ("file=<cale>")
            string file = parameters?.FirstOrDefault(p => p.StartsWith("file="))?.Substring(5)
                          ?? Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "ManageBIM", "Outbox", "walkpath.txt");
            WalkPath w;
            try { w = Read(file); }
            catch (Exception ex)
            {
                string seen;
                try { seen = string.Join(", ", Directory.GetFiles(Path.GetDirectoryName(file) ?? "").Select(Path.GetFileName)); }
                catch (Exception e2) { seen = e2.Message; }
                Say("No walk path from ManageBIM yet.\nIn ManageBIM: Viewpoint › Walk Path › Draw, then To Navisworks.\n\n" + ex.GetType().Name + ": " + ex.Message + "\nIn the folder: " + seen);
                return 1;
            }
            // metri -> unitatile documentului
            double s = UnitConversion.ScaleFactor(Units.Meters, doc.Units);
            List<double[]> keys = Resample(w.Points, Math.Max(0.3, w.Speed * 0.5));
            Viewpoint original = doc.CurrentViewpoint.CreateCopy();
            var views = new List<Viewpoint>();
            for (int i = 0; i < keys.Count; i++)
            {
                double[] p = keys[i];
                double[] q = keys[Math.Min(i + 1, keys.Count - 1)];
                double[] r = keys[Math.Max(i - 1, 0)];
                // privirea: orizontal, spre urmatorul cadru (la ultimul: in continuarea directiei)
                double[] fwd = i + 1 < keys.Count ? new[] { q[0] - p[0], q[1] - p[1], 0.0 } : new[] { p[0] - r[0], p[1] - r[1], 0.0 };
                if (Math.Abs(fwd[0]) + Math.Abs(fwd[1]) < 1e-9) fwd = new[] { 1.0, 0.0, 0.0 };
                Viewpoint vp = doc.CurrentViewpoint.CreateCopy();
                vp.Position = new Point3D(p[0] * s, p[1] * s, (p[2] + w.Eye) * s);
                double[] rot = ManageBimIssues.Quaternion(fwd, new[] { 0.0, 0.0, 1.0 });
                vp.Rotation = new Rotation3D(rot[0], rot[1], rot[2], rot[3]);
                vp.Projection = ViewpointProjection.Perspective;
                vp.HeightField = w.Fov * Math.PI / 180.0;
                views.Add(vp);
            }
            int code;
            string how;
            try
            {
                AddAnimation(doc, w.Name, views);
                code = 0;
                how = $"Viewpoint animation \"{w.Name}\" added to Saved Viewpoints ({views.Count} key frames, {Length(w.Points):0.0} m at {w.Speed:0.0} m/s).\nSelect it and press Play (Viewpoint › Saved Viewpoints, or Animation › Playback).";
            }
            catch (Exception ex)
            {
                AddFolder(doc, w.Name, views);
                code = 4;
                how = $"{views.Count} viewpoints added to the folder \"{w.Name}\" in Saved Viewpoints.\nThe animation could not be created automatically ({ex.GetBaseException().Message}): right-click in Saved Viewpoints › Add Animation and drag the viewpoints into it.";
            }
            try { doc.CurrentViewpoint.CopyFrom(original); } catch { }
            Say(how);
            return code;
        }

        private sealed class WalkPath
        {
            public string Name = "Walk path";
            public double Speed = 1.4, Eye = 1.7, Fov = 60;
            public List<double[]> Points = new List<double[]>();
        }

        private static WalkPath Read(string file)
        {
            if (!File.Exists(file)) throw new FileNotFoundException(file);
            var w = new WalkPath();
            var lines = File.ReadAllLines(file);
            if (lines.Length == 0 || !lines[0].StartsWith("MBWALK")) throw new InvalidDataException("Not a ManageBIM walk path file.");
            double D(string v) => double.Parse(v, CultureInfo.InvariantCulture);
            foreach (string l in lines.Skip(1))
            {
                string[] t = l.Split('\t');
                if (t.Length < 2) continue;
                switch (t[0])
                {
                    case "NAME": w.Name = t[1]; break;
                    case "SPEED": w.Speed = Math.Max(0.1, D(t[1])); break;
                    case "EYE": w.Eye = D(t[1]); break;
                    case "FOV": w.Fov = D(t[1]); break;
                    case "P":
                        var c = t[1].Split(' ').Select(D).ToArray();
                        if (c.Length == 3) w.Points.Add(c);
                        break;
                }
            }
            if (w.Points.Count < 2) throw new InvalidDataException("The walk path has less than two points.");
            return w;
        }

        private static double Dist(double[] a, double[] b) => Math.Sqrt((a[0] - b[0]) * (a[0] - b[0]) + (a[1] - b[1]) * (a[1] - b[1]) + (a[2] - b[2]) * (a[2] - b[2]));

        private static double Length(List<double[]> p)
        {
            double l = 0;
            for (int i = 1; i < p.Count; i++) l += Dist(p[i - 1], p[i]);
            return l;
        }

        /// <summary>Puncte la distante egale pe traseu (colturile incluse): cadre egale = viteza constanta.</summary>
        internal static List<double[]> Resample(List<double[]> pts, double step)
        {
            var outp = new List<double[]> { pts[0] };
            for (int i = 1; i < pts.Count; i++)
            {
                double[] a = pts[i - 1], b = pts[i];
                double len = Dist(a, b);
                int n = Math.Max(1, (int)Math.Round(len / step));
                for (int k = 1; k <= n; k++)
                {
                    double t = (double)k / n;
                    outp.Add(new[] { a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t });
                }
            }
            return outp;
        }

        /// <summary>InwOpAnimView prin COM (legare tarzie: aceeasi cod pentru Navisworks 2022 - 2026).</summary>
        private static void AddAnimation(Document doc, string name, List<Viewpoint> views)
        {
            object state = ComState();
            int animType = EnumValue("eObjectType_nwOpAnimView", 6);
            int viewType = EnumValue("eObjectType_nwOpView", 11);
            object anim = Call(state, "ObjectFactory", animType, Missing.Value, Missing.Value);
            Set(anim, "name", name);
            object animViews = Call(anim, "SavedViews");
            for (int i = 0; i < views.Count; i++)
            {
                doc.CurrentViewpoint.CopyFrom(views[i]);
                object view = Call(state, "ObjectFactory", viewType, Missing.Value, Missing.Value);
                Set(view, "name", $"{name} {i + 1:000}");
                object anon = Get(view, "anonview");
                object cur = Get(Get(state, "CurrentView"), "ViewPoint");
                Set(anon, "ViewPoint", Call(cur, "Copy"));
                Call(animViews, "Add", view);
            }
            Call(Call(state, "SavedViews"), "Add", anim);
        }

        /// <summary>Varianta de rezerva: viewpoint-urile intr-un folder (API .NET).</summary>
        private static void AddFolder(Document doc, string name, List<Viewpoint> views)
        {
            var folder = new FolderItem { DisplayName = name };
            doc.SavedViewpoints.AddCopy(folder);
            GroupItem added = doc.SavedViewpoints.Value.OfType<GroupItem>().LastOrDefault(g => g.DisplayName == name);
            for (int i = 0; i < views.Count; i++)
            {
                var sv = new SavedViewpoint(views[i]) { DisplayName = $"{name} {i + 1:000}" };
                if (added != null) doc.SavedViewpoints.InsertCopy(added, added.Children.Count, sv);
                else doc.SavedViewpoints.AddCopy(sv);
            }
        }

        private static object ComState()
        {
            Assembly asm = AppDomain.CurrentDomain.GetAssemblies().FirstOrDefault(a => a.GetName().Name == "Autodesk.Navisworks.ComApi")
                           ?? Assembly.LoadFrom(Path.Combine(Path.GetDirectoryName(typeof(Document).Assembly.Location) ?? "", "Autodesk.Navisworks.ComApi.dll"));
            Type bridge = asm.GetType("Autodesk.Navisworks.Api.ComApi.ComApiBridge", true);
            return bridge.GetProperty("State").GetValue(null);
        }

        /// <summary>valoarea enum-ului COM din interop-ul versiunii care ruleaza (altfel valoarea cunoscuta)</summary>
        private static int EnumValue(string name, int fallback)
        {
            try
            {
                string dir = Path.GetDirectoryName(typeof(Document).Assembly.Location) ?? "";
                Assembly asm = AppDomain.CurrentDomain.GetAssemblies().FirstOrDefault(a => a.GetName().Name == "Autodesk.Navisworks.Interop.ComApi")
                               ?? Assembly.LoadFrom(Path.Combine(dir, "Autodesk.Navisworks.Interop.ComApi.dll"));
                Type t = asm.GetType("Autodesk.Navisworks.Api.Interop.ComApi.nwEObjectType", true);
                return (int)Enum.Parse(t, name);
            }
            catch { return fallback; }
        }

        private static object Call(object o, string m, params object[] a) => o.GetType().InvokeMember(m, BindingFlags.InvokeMethod, null, o, a);
        private static object Get(object o, string p) => o.GetType().InvokeMember(p, BindingFlags.GetProperty, null, o, null);
        private static void Set(object o, string p, object v) => o.GetType().InvokeMember(p, BindingFlags.SetProperty, null, o, new[] { v });
    }
}
