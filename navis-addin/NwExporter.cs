using System;
using System.Collections.Generic;
using System.Linq;
using System.Runtime.InteropServices;
using Autodesk.Navisworks.Api;

namespace ManageBIM.Navis
{
    /// <summary>
    /// Export of the open Navisworks document (NWD / NWC / NWF / any loaded format) to .bimc:
    /// one ManageBIM element for each Navisworks object (e.g. a Revit element), with all its
    /// triangulated geometry, the displayed color, all properties, GUID and level.
    /// </summary>
    public static class NwExporter
    {
        public sealed class Result
        {
            public int Elements;
            public long Triangles;
            public long Bytes;
            public List<string> Warnings = new List<string>();
            /// the times per stage (for the log)
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

            // 1) the elements with geometry, grouped by "object" (the level that Navisworks selects)
            // a single top-down pass: the element = the first node from the top that is NOT a file, layer or
            // group (collection) - for Revit, the Revit element. Finer than FindFirstObjectAncestor (which
            // glued e.g. all the ducts of a category into a single object) and without climbing the
            // hierarchy for each node (with millions of nodes that took tens of minutes).
            // No dictionary keyed by ModelItem: its hash is weak and with tens of thousands of objects the lookups
            // became slower and slower (~1 000 nodes / s). Each node carries its object index directly.
            var order = new List<ModelItem>();          // the objects, in the Selection Tree order
            var members = new List<List<ModelItem>>();  // the geometry nodes of each object
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
                        // geometry directly under file / layer: a standalone element
                        order.Add(it);
                        members.Add(new List<ModelItem> { it });
                    }
                    else members[cur].Add(it);
                }
                // the children in reverse order on the stack -> traversal in the Selection Tree order
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

            // "objects" that are actually large groups (models from 3ds / FBX / DWG without blocks, assemblies):
            // above SplitAbove geometry nodes -> each node becomes an element (with the object properties)
            const int SplitAbove = 50;
            var units = new List<(ModelItem item, List<ModelItem> geo, ModelItem parent)>(order.Count);
            int split = 0, geoNodes = 0, objects = 0;
            for (int i = 0; i < order.Count; i++)
            {
                ModelItem obj = order[i];
                var list = members[i];
                if (list.Count == 0) continue; // object without geometry (e.g. only annotations)
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

            // 2) geometry + properties for each element, written immediately (constant memory)
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
                    // element of a split group: the node properties + those of the parent object
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
                    outp.Add(e); // encoded and written; the element geometry is released here
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

        /// file-type node (and the attached files / xref from NWF, composite NWD)
        private static bool IsFileNode(ModelItem it)
        {
            string c = it.ClassName ?? "";
            return c == "LcOaPartition" || c.IndexOf("Partition", StringComparison.OrdinalIgnoreCase) >= 0;
        }

        // ------------------------------------------------------------------ metadata
        /// <summary>
        /// All properties: the object categories + the categories that exist only on its
        /// geometry nodes (e.g. Material, Geometry) + the Selection Tree path.
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
                // the same category on the object and on the geometry: we keep it only once (the object's)
                if (!seen.Add(g)) continue;
                foreach (DataProperty p in cat.Properties)
                {
                    string n = p.DisplayName;
                    string v;
                    try { v = p.Value == null ? "" : p.Value.ToDisplayString(); }
                    catch { continue; }
                    if (string.IsNullOrEmpty(v)) continue;
                    e.Add(g, n, v);
                    // GUID stable between versions: IFC GlobalId (from Revit / IFC), otherwise the Navisworks GUID
                    if (guid == null && (n == "IfcGUID" || n == "IFC GUID" || n == "GlobalId" || n == "IfcGlobalId" || n == "Ifc GUID")) guid = v;
                    if (level == null && (n == "Level" || n == "Base Constraint" || n == "Reference Level" || n == "Building Storey")) level = v;
                    if (category == null && n == "Category" && g != "Item") category = v;
                    if (layer == null && g == "Item" && n == "Layer") layer = v;
                }
            }
            // the Selection Tree path (file > level / layer > ... > object)
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

        /// the layer from the hierarchy - for models without a "Level" property (DWG, NWC from AutoCAD)
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

        // ------------------------------------------------------------------ geometry (COM API)
        // Note: Navisworks returns a new COM object on every access to frag.Geometry, so the instanced
        // geometry cannot be recognized / reused through the API (tested: 0 matches) - we triangulate it per fragment.
        // Without Autodesk.Navisworks.Interop.ComApi (a different version every year): the path and fragments through
        // IDispatch (a few calls / fragment), the vertices through the vtable (NwCom.cs) - the same DLL in 2022-2026.
        private static void AddGeometry(ModelItem item, BimcElement e, uint color, double toM)
        {
            dynamic path = NwCom.ToPath(item);
            int[] own = NwCom.Ints((object)path.ArrayData);
            foreach (dynamic frag in path.Fragments())
            {
                // the instanced geometry is shared between several paths: we keep only the fragments of this element
                int[] fp = NwCom.Ints((object)frag.path.ArrayData);
                if (!own.SequenceEqual(fp)) continue;
                double[] m = NwCom.Doubles((object)frag.GetLocalToWorldMatrix().Matrix, 16);
                var sink = new TriangleSink(e, color, m, toM);
                // 0 = eNONE: no normals (ManageBIM recomputes them) - less data through COM per vertex
                frag.GenerateSimplePrimitives(0, sink);
            }
        }
    }

    /// receives the triangles from Navisworks (fragment local coordinates) and transforms them to meters
    [ComVisible(true)]
    [ClassInterface(ClassInterfaceType.None)]
    public sealed class TriangleSink : INwSimplePrimitivesCB
    {
            private readonly BimcElement e;
            private readonly uint color;
            private readonly double[] m;
            private readonly double s;
            // reused buffer: the coordinates come as float[*] (indexed from 1) - direct copy, no boxing
            private readonly float[] c = new float[3];

            internal TriangleSink(BimcElement e, uint color, double[] m, double toM) { this.e = e; this.color = color; this.m = m; s = toM; }

            private V3 P(object v)
            {
                var a = (Array)((INwSimpleVertex)v).get_coord();
                Array.Copy(a, a.GetLowerBound(0), c, 0, 3);
                // column matrix (the translation in elements 12..14)
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
