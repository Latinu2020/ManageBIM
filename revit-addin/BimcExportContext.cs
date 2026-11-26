using System;
using System.Collections.Generic;
using Autodesk.Revit.DB;

namespace ManageBIM.Revit
{
    /// <summary>
    /// Collects the visible geometry of a 3D view element by element (Revit's own tessellation,
    /// including linked models), with material colours and all instance / type parameters.
    /// </summary>
    public class BimcExportContext : IExportContext
    {
        private const double FeetToMetres = 0.3048;
        private const uint DefaultColor = 190u | (190u << 8) | (190u << 16) | (255u << 24);

        private readonly Stack<Document> _docs = new Stack<Document>();
        private readonly Stack<Transform> _xf = new Stack<Transform>();
        private readonly Transform _toShared;
        private readonly Dictionary<string, List<BimcProp>> _typeProps = new Dictionary<string, List<BimcProp>>();
        private BimcElement _cur;
        private uint _color = DefaultColor;

        public List<BimcElement> Elements { get; } = new List<BimcElement>();
        public List<string> Warnings { get; } = new List<string>();

        /// <summary>Send selection: only these elements of the host document are exported (null = everything visible).</summary>
        public HashSet<ElementId> Only { get; set; }

        public BimcExportContext(Document doc)
        {
            _docs.Push(doc);
            _xf.Push(Transform.Identity);
            // internal -> shared coordinates, so the export lines up with IFC / other disciplines
            Transform total = doc.ActiveProjectLocation?.GetTotalTransform() ?? Transform.Identity;
            _toShared = total.Inverse;
        }

        public bool Start() => true;
        public void Finish() { }
        public bool IsCanceled() => false;

        public RenderNodeAction OnViewBegin(ViewNode node) => RenderNodeAction.Proceed;
        public void OnViewEnd(ElementId elementId) { }

        public RenderNodeAction OnElementBegin(ElementId elementId)
        {
            Document doc = _docs.Peek();
            Element e = doc.GetElement(elementId);
            if (e == null) { _cur = null; return RenderNodeAction.Skip; }
            // Send selection: elementele neselectate ale modelului principal sunt sarite
            if (Only != null && _docs.Count == 1 && !Only.Contains(elementId)) { _cur = null; return RenderNodeAction.Skip; }
            try { _cur = Describe(doc, e); }
            catch (Exception ex)
            {
                Warnings.Add($"Element {elementId}: {ex.Message}");
                _cur = new BimcElement { Name = e.Name, Type = "Revit Element", Guid = e.UniqueId };
            }
            _color = DefaultColor;
            return RenderNodeAction.Proceed;
        }

        public void OnElementEnd(ElementId elementId)
        {
            if (_cur != null && _cur.TriangleCount > 0) Elements.Add(_cur);
            _cur = null;
        }

        public RenderNodeAction OnInstanceBegin(InstanceNode node)
        {
            _xf.Push(_xf.Peek().Multiply(node.GetTransform()));
            return RenderNodeAction.Proceed;
        }

        public void OnInstanceEnd(InstanceNode node) => _xf.Pop();

        public RenderNodeAction OnLinkBegin(LinkNode node)
        {
            Document linked = node.GetDocument();
            if (linked == null) return RenderNodeAction.Skip;
            _xf.Push(_xf.Peek().Multiply(node.GetTransform()));
            _docs.Push(linked);
            return RenderNodeAction.Proceed;
        }

        public void OnLinkEnd(LinkNode node)
        {
            _xf.Pop();
            _docs.Pop();
        }

        public RenderNodeAction OnFaceBegin(FaceNode node) => RenderNodeAction.Proceed;
        public void OnFaceEnd(FaceNode node) { }

        public void OnMaterial(MaterialNode node)
        {
            Color c = node.Color;
            if (c == null || !c.IsValid) { _color = DefaultColor; return; }
            double alpha = Math.Max(0.15, 1.0 - node.Transparency);
            _color = c.Red | ((uint)c.Green << 8) | ((uint)c.Blue << 16) | ((uint)Math.Round(alpha * 255) << 24);
        }

        public void OnPolymesh(PolymeshTopology mesh)
        {
            if (_cur == null) return;
            Transform t = _xf.Peek();
            IList<XYZ> pts = mesh.GetPoints();
            var world = new XYZ[pts.Count];
            for (int i = 0; i < pts.Count; i++)
                world[i] = _toShared.OfPoint(t.OfPoint(pts[i])) * FeetToMetres;
            foreach (PolymeshFacet f in mesh.GetFacets())
                _cur.AddTriangle(world[f.V1], world[f.V2], world[f.V3], _color);
        }

        public void OnRPC(RPCNode node) { }
        public void OnLight(LightNode node) { }

        // ------------------------------------------------------------------ metadata
        private BimcElement Describe(Document doc, Element e)
        {
            var el = new BimcElement();
            ElementType type = doc.GetElement(e.GetTypeId()) as ElementType;
            string category = e.Category?.Name ?? "Generic";
            string family = type?.FamilyName ?? "";
            string typeName = type?.Name ?? "";

            el.Name = type != null ? $"{family}: {typeName}" : (string.IsNullOrEmpty(e.Name) ? $"{category} {e.Id}" : e.Name);
            el.Type = category;
            el.Guid = ParamString(e, BuiltInParameter.IFC_GUID) ?? e.UniqueId;
            el.Level = LevelName(doc, e);

            el.Add("Element", "Name", el.Name);
            el.Add("Element", "Category", category);
            el.Add("Element", "Family", family);
            el.Add("Element", "Type", typeName);
            el.Add("Element", "Element Id", e.Id.ToString());
            el.Add("Element", "UniqueId", e.UniqueId);
            el.Add("Element", "Level", el.Level);
            el.Add("Element", "Revit file", doc.Title);

            foreach (Parameter p in e.Parameters) AddParam(el.Props, p, "");
            if (type != null)
            {
                string key = doc.Title + "|" + type.UniqueId;
                if (!_typeProps.TryGetValue(key, out var tp))
                {
                    tp = new List<BimcProp>();
                    foreach (Parameter p in type.Parameters) AddParam(tp, p, " (Type)");
                    _typeProps[key] = tp;
                }
                el.Props.AddRange(tp);
            }
            if (e.Category != null && e.Category.Id.IntegerValueCompat() == (int)BuiltInCategory.OST_Rooms)
                el.DefaultHidden = true;
            return el;
        }

        private static void AddParam(List<BimcProp> list, Parameter p, string suffix)
        {
            if (p == null || !p.HasValue || p.Definition == null) return;
            string value;
            switch (p.StorageType)
            {
                case StorageType.String: value = p.AsString(); break;
                case StorageType.Integer: value = p.AsValueString() ?? p.AsInteger().ToString(); break;
                case StorageType.Double: value = p.AsValueString() ?? p.AsDouble().ToString("0.###"); break;
                case StorageType.ElementId: value = p.AsValueString(); break;
                default: value = null; break;
            }
            if (string.IsNullOrWhiteSpace(value)) return;
            string group;
            try
            {
                ForgeTypeId g = p.Definition.GetGroupTypeId();
                group = (g == null || string.IsNullOrEmpty(g.TypeId)) ? "Other" : LabelUtils.GetLabelForGroup(g);
            }
            catch { group = "Other"; }
            list.Add(new BimcProp(group + suffix, p.Definition.Name, value));
        }

        private static string ParamString(Element e, BuiltInParameter bip)
        {
            try
            {
                string s = e.get_Parameter(bip)?.AsString();
                return string.IsNullOrWhiteSpace(s) ? null : s;
            }
            catch { return null; }
        }

        private static string LevelName(Document doc, Element e)
        {
            if (e.LevelId != null && e.LevelId != ElementId.InvalidElementId)
                return doc.GetElement(e.LevelId)?.Name ?? "";
            foreach (var bip in new[] { BuiltInParameter.SCHEDULE_LEVEL_PARAM, BuiltInParameter.FAMILY_LEVEL_PARAM,
                                         BuiltInParameter.INSTANCE_REFERENCE_LEVEL_PARAM, BuiltInParameter.WALL_BASE_CONSTRAINT,
                                         BuiltInParameter.RBS_START_LEVEL_PARAM })
            {
                Parameter p = e.get_Parameter(bip);
                if (p != null && p.StorageType == StorageType.ElementId)
                {
                    Element lvl = doc.GetElement(p.AsElementId());
                    if (lvl != null) return lvl.Name;
                }
            }
            return "";
        }
    }

    internal static class ElementIdCompat
    {
        /// <summary>ElementId.IntegerValue was replaced by ElementId.Value (Int64) in Revit 2024+.</summary>
        public static long IntegerValueCompat(this ElementId id)
        {
#if NET8_0_OR_GREATER
            return id.Value;
#else
            return long.Parse(id.ToString());
#endif
        }
    }
}
