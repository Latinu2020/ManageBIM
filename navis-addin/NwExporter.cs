using System;
using System.Collections.Generic;
using System.Linq;
using System.Runtime.InteropServices;
using Autodesk.Navisworks.Api;

namespace ManageBIM.Navis
{
    /// <summary>
    /// Exportul documentului Navisworks deschis (NWD / NWC / NWF / orice format incarcat) in .bimc:
    /// un element ManageBIM pentru fiecare obiect Navisworks (ex. un element Revit), cu toata
    /// geometria lui triangulata, culoarea afisata, toate proprietatile, GUID si nivel.
    /// </summary>
    public static class NwExporter
    {
        public sealed class Result
        {
            public int Elements;
            public long Triangles;
            public long Bytes;
            public List<string> Warnings = new List<string>();
            /// timpii pe etape (pentru jurnal)
            public string Timing = "";
        }

        /// <param name="progress">(fractie 0..1, text) -> false = anulare</param>
        public static Result Export(Document doc, string path, Func<double, string, bool> progress)
        {
            double toM = UnitConversion.ScaleFactor(doc.Units, Units.Meters);
            var res = new Result();
            var sw = System.Diagnostics.Stopwatch.StartNew();
            long tCollect, tMeta = 0, tGeo = 0, tWrite = 0;
            progress?.Invoke(0.0, "Reading the model structure");

            // 1) elementele cu geometrie, grupate pe "obiect" (nivelul pe care il selecteaza Navisworks)
            // o singura parcurgere de sus in jos: elementul = primul nod de sus care NU e fisier, layer sau
            // grup (collection) - pentru Revit, elementul Revit. Mai fin decat FindFirstObjectAncestor (care
            // lipea de ex. toate tubulaturile unei categorii intr-un singur obiect) si fara sa urcam prin
            // ierarhie pentru fiecare nod (la milioane de noduri asta dura zeci de minute).
            // Fara dictionar cu cheie ModelItem: hash-ul lui e slab si la zeci de mii de obiecte cautarile
            // deveneau tot mai lente (~1 000 noduri / s). Fiecare nod poarta direct indicele obiectului sau.
            var order = new List<ModelItem>();          // obiectele, in ordinea din Selection Tree
            var members = new List<List<ModelItem>>();  // nodurile de geometrie ale fiecarui obiect
            var stack = new Stack<(ModelItem item, int obj, bool root)>();
            var modelRoots = new List<ModelItem>();
            foreach (Model mdl in doc.Models) modelRoots.Add(mdl.RootItem);
            for (int m = modelRoots.Count - 1; m >= 0; m--) stack.Push((modelRoots[m], -1, true));
            long visited = 0, lastScan = 0;
            var kids = new List<ModelItem>();
            while (stack.Count > 0)
            {
                var (it, cur, root) = stack.Pop();
                visited++;
                if (cur < 0 && !root && !it.IsLayer && !it.IsCollection && !IsFileNode(it))
                {
                    order.Add(it);
                    members.Add(new List<ModelItem>());
                    cur = order.Count - 1;
                }
                if (it.HasGeometry)
                {
                    if (cur < 0)
                    {
                        // geometrie direct sub fisier / layer: element de sine statator
                        order.Add(it);
                        members.Add(new List<ModelItem> { it });
                    }
                    else members[cur].Add(it);
                }
                // copiii in ordine inversa pe stiva -> parcurgere in ordinea din Selection Tree
                kids.Clear();
                foreach (ModelItem c in it.Children) kids.Add(c);
                for (int k = kids.Count - 1; k >= 0; k--) stack.Push((kids[k], cur, false));
                if (progress != null && sw.ElapsedMilliseconds - lastScan > 500)
                {
                    lastScan = sw.ElapsedMilliseconds;
                    if (!progress(0.0, $"Reading the model structure: {visited:N0} nodes, {order.Count:N0} objects"))
                        throw new OperationCanceledException();
                }
            }

            // "obiecte" care sunt de fapt grupuri mari (modele din 3ds / FBX / DWG fara blocuri, ansambluri):
            // peste SplitAbove noduri de geometrie -> fiecare nod devine un element (cu proprietatile obiectului)
            const int SplitAbove = 50;
            var units = new List<(ModelItem item, List<ModelItem> geo, ModelItem parent)>(order.Count);
            int split = 0, geoNodes = 0, objects = 0;
            for (int i = 0; i < order.Count; i++)
            {
                ModelItem obj = order[i];
                var list = members[i];
                if (list.Count == 0) continue; // obiect fara geometrie (ex. doar adnotari)
                objects++;
                geoNodes += list.Count;
                if (list.Count > SplitAbove)
                {
                    split++;
                    foreach (ModelItem g in list) units.Add((g, new List<ModelItem> { g }, obj));
                }
                else units.Add((obj, list, null));
            }
            members = null;
            tCollect = sw.ElapsedMilliseconds;

            // 2) geometrie + proprietati pentru fiecare element, scrise imediat (memorie constanta)
            long nprops = 0;
            using (var outp = new BimcStream(path))
            {
                int done = 0, total = Math.Max(1, units.Count);
                long lastReport = 0;
                foreach (var (obj, geo, parent) in units)
                {
                    done++;
                    if (progress != null && sw.ElapsedMilliseconds - lastReport > 500)
                    {
                        lastReport = sw.ElapsedMilliseconds;
                        if (!progress(0.98 * done / total, $"Element {done:N0} / {total:N0}  ({outp.Triangles:N0} triangles)"))
                            throw new OperationCanceledException();
                    }
                    var e = new BimcElement();
                    long t0 = sw.ElapsedMilliseconds;
                    // element dintr-un grup impartit: proprietatile nodului + ale obiectului parinte
                    FillMetadata(obj, parent != null ? new List<ModelItem> { parent } : geo, e);
                    long t1 = sw.ElapsedMilliseconds;
                    tMeta += t1 - t0;
                    foreach (ModelItem g in geo)
                    {
                        uint col = ColorOf(g);
                        try { AddGeometry(g, e, col, toM); }
                        catch (Exception ex) { res.Warnings.Add($"{e.Name}: geometry skipped ({ex.Message})"); }
                        if (g.IsHidden) e.DefaultHidden = true;
                    }
                    long t2 = sw.ElapsedMilliseconds;
                    tGeo += t2 - t1;
                    if (e.TriangleCount == 0) continue;
                    nprops += e.Props.Count;
                    outp.Add(e); // codat si scris; geometria elementului se elibereaza aici
                    tWrite += sw.ElapsedMilliseconds - t2;
                }
                res.Elements = (int)outp.Elements;
                res.Triangles = outp.Triangles;
                if (outp.Elements == 0) throw new InvalidOperationException("The model has no geometry that can be exported.");

                var info = new List<KeyValuePair<string, string>>
                {
                    new KeyValuePair<string, string>("Source", string.IsNullOrEmpty(doc.FileName) ? doc.Title : doc.FileName),
                    new KeyValuePair<string, string>("Exported from", Autodesk.Navisworks.Api.Application.Version.RuntimeProductName + " (ManageBIM add-in)"),
                    new KeyValuePair<string, string>("Source units", doc.Units.ToString()),
                    new KeyValuePair<string, string>("Exported", DateTime.Now.ToString("yyyy-MM-dd HH:mm")),
                    new KeyValuePair<string, string>("Models", string.Join(", ", doc.Models.Select(m => System.IO.Path.GetFileName(m.FileName)))),
                };
                progress?.Invoke(0.99, "Writing");
                long tw0 = sw.ElapsedMilliseconds;
                res.Bytes = outp.Finish(info, res.Warnings.Distinct().Take(200).ToList());
                tWrite += sw.ElapsedMilliseconds - tw0;
            }
            res.Timing = $"collect {tCollect} ms, properties {tMeta} ms ({nprops} values), geometry {tGeo} ms, encode+write {tWrite} ms; {geoNodes} geometry nodes, {objects} objects, {split} large groups split into their parts";
            return res;
        }

        /// nod de tip fisier (si fisierele atasate / xref din NWF, NWD compuse)
        private static bool IsFileNode(ModelItem it)
        {
            string c = it.ClassName ?? "";
            return c == "LcOaPartition" || c.IndexOf("Partition", StringComparison.OrdinalIgnoreCase) >= 0;
        }

        // ------------------------------------------------------------------ metadate
        /// <summary>
        /// Toate proprietatile: categoriile obiectului + categoriile care exista doar pe nodurile lui de
        /// geometrie (ex. Material, Geometry) + calea din Selection Tree.
        /// </summary>
        private static void FillMetadata(ModelItem obj, List<ModelItem> geo, BimcElement e)
        {
            e.Name = !string.IsNullOrEmpty(obj.DisplayName) ? obj.DisplayName : obj.ClassDisplayName;
            string guid = null, level = null, category = null, layer = null;
            var seen = new HashSet<string>(StringComparer.Ordinal);
            var sources = new List<ModelItem> { obj };
            foreach (ModelItem g in geo) if (!g.Equals(obj)) sources.Add(g);
            foreach (ModelItem src in sources)
            foreach (PropertyCategory cat in src.PropertyCategories)
            {
                string g = cat.DisplayName;
                // aceeasi categorie pe obiect si pe geometrie: o pastram o singura data (cea a obiectului)
                if (!seen.Add(g)) continue;
                foreach (DataProperty p in cat.Properties)
                {
                    string n = p.DisplayName;
                    string v;
                    try { v = p.Value == null ? "" : p.Value.ToDisplayString(); }
                    catch { continue; }
                    if (string.IsNullOrEmpty(v)) continue;
                    e.Add(g, n, v);
                    // GUID stabil intre versiuni: IFC GlobalId (din Revit / IFC), altfel GUID-ul Navisworks
                    if (guid == null && (n == "IfcGUID" || n == "IFC GUID" || n == "GlobalId" || n == "IfcGlobalId" || n == "Ifc GUID")) guid = v;
                    if (level == null && (n == "Level" || n == "Base Constraint" || n == "Reference Level" || n == "Building Storey")) level = v;
                    if (category == null && n == "Category" && g != "Item") category = v;
                    if (layer == null && g == "Item" && n == "Layer") layer = v;
                }
            }
            // calea din Selection Tree (fisier > nivel / strat > ... > obiect)
            var chain = new List<string>();
            for (ModelItem p = obj.Parent; p != null; p = p.Parent)
                if (!string.IsNullOrEmpty(p.DisplayName)) chain.Add(p.DisplayName);
            chain.Reverse();
            if (chain.Count > 0) e.Add("Navisworks", "Tree path", string.Join(" > ", chain));
            e.Add("Navisworks", "Class", obj.ClassDisplayName);
            if (obj.InstanceGuid != Guid.Empty) e.Add("Navisworks", "Instance GUID", obj.InstanceGuid.ToString());
            if (guid == null && obj.InstanceGuid != Guid.Empty) guid = obj.InstanceGuid.ToString();
            e.Guid = guid ?? "";
            e.Level = level ?? layer ?? LayerOf(obj) ?? "";
            e.Type = category ?? obj.ClassDisplayName ?? "";
        }

        /// stratul (layer) din ierarhie - pentru modele fara proprietate "Level" (DWG, NWC din AutoCAD)
        private static string LayerOf(ModelItem obj)
        {
            for (ModelItem p = obj.Parent; p != null; p = p.Parent)
                if (p.IsLayer) return p.DisplayName;
            return null;
        }

        private static uint ColorOf(ModelItem g)
        {
            var geo = g.Geometry;
            if (geo == null) return 0xFFC8C8C8;
            var c = geo.ActiveColor;
            double t = geo.ActiveTransparency;
            byte r = ToByte(c.R), gg = ToByte(c.G), b = ToByte(c.B), a = ToByte(1.0 - t);
            return (uint)(r | (gg << 8) | (b << 16) | (a << 24));
        }

        private static byte ToByte(double v) => (byte)Math.Max(0, Math.Min(255, Math.Round(v * 255.0)));

        // ------------------------------------------------------------------ geometrie (COM API)
        // Nota: Navisworks intoarce un obiect COM nou la fiecare acces la frag.Geometry, deci geometria
        // instantiata nu poate fi recunoscuta / refolosita prin API (testat: 0 potriviri) - o triangulam per fragment.
        // Fara Autodesk.Navisworks.Interop.ComApi (versiune diferita in fiecare an): calea si fragmentele prin
        // IDispatch (cateva apeluri / fragment), varfurile prin vtable (NwCom.cs) - acelasi DLL in 2022-2026.
        private static void AddGeometry(ModelItem item, BimcElement e, uint color, double toM)
        {
            dynamic path = NwCom.ToPath(item);
            int[] own = NwCom.Ints((object)path.ArrayData);
            foreach (dynamic frag in path.Fragments())
            {
                // geometria instantiata e impartita intre mai multe cai: pastram doar fragmentele acestui element
                int[] fp = NwCom.Ints((object)frag.path.ArrayData);
                if (!own.SequenceEqual(fp)) continue;
                double[] m = NwCom.Doubles((object)frag.GetLocalToWorldMatrix().Matrix, 16);
                var sink = new TriangleSink(e, color, m, toM);
                // 0 = eNONE: fara normale (ManageBIM le recalculeaza) - mai putine date prin COM pe fiecare varf
                frag.GenerateSimplePrimitives(0, sink);
            }
        }
    }

    /// primeste triunghiurile de la Navisworks (coordonate locale fragmentului) si le transforma in metri
    [ComVisible(true)]
    [ClassInterface(ClassInterfaceType.None)]
    public sealed class TriangleSink : INwSimplePrimitivesCB
    {
            private readonly BimcElement e;
            private readonly uint color;
            private readonly double[] m;
            private readonly double s;
            // tampon refolosit: coordonatele vin ca float[*] (indexat de la 1) - copiere directa, fara boxing
            private readonly float[] c = new float[3];

            internal TriangleSink(BimcElement e, uint color, double[] m, double toM) { this.e = e; this.color = color; this.m = m; s = toM; }

            private V3 P(object v)
            {
                var a = (Array)((INwSimpleVertex)v).get_coord();
                Array.Copy(a, a.GetLowerBound(0), c, 0, 3);
                // matrice coloana (translatia in elementele 12..14)
                double x = c[0], y = c[1], z = c[2];
                return new V3((m[0] * x + m[4] * y + m[8] * z + m[12]) * s, (m[1] * x + m[5] * y + m[9] * z + m[13]) * s, (m[2] * x + m[6] * y + m[10] * z + m[14]) * s);
            }

            public void Triangle(object v1, object v2, object v3)
            {
                e.AddTriangle(P(v1), P(v2), P(v3), color);
            }

            public void Line(object v1, object v2) { }
            public void Point(object v1) { }
            public void SnapPoint(object v1) { }
    }
}
