// The properties from the AutoCAD Properties palette for application-defined objects (Plant 3D: beams,
// columns, plates, gratings, stairs, railings, foundations; Civil 3D: surfaces, corridors, pipes, structures;
// Architecture / MEP). They are NOT in the Plant 3D database (DataLinks) nor in .NET as properties -
// we read them through the object's COM interface (IDispatch + ITypeInfo), exactly what the Properties palette shows.
// Read-only, simple values only (text, numbers, bool, points); nested COM objects are skipped.
using System;
using System.Collections.Generic;
using System.Globalization;
using System.Runtime.InteropServices;
using ITypeInfo = System.Runtime.InteropServices.ComTypes.ITypeInfo;
using TYPEATTR = System.Runtime.InteropServices.ComTypes.TYPEATTR;
using FUNCDESC = System.Runtime.InteropServices.ComTypes.FUNCDESC;
using INVOKEKIND = System.Runtime.InteropServices.ComTypes.INVOKEKIND;
using FUNCFLAGS = System.Runtime.InteropServices.ComTypes.FUNCFLAGS;

namespace ManageBIM.Acad
{
    [ComImport, Guid("00020400-0000-0000-C000-000000000046"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    internal interface IDispatchRaw
    {
        [PreserveSig] int GetTypeInfoCount(out uint count);
        [PreserveSig] int GetTypeInfo(uint index, int lcid, out ITypeInfo info);
    }

    internal static class ComProps
    {
        // the properties common to all entities (we already write them in the "Item" group) or internal ones
        private static readonly HashSet<string> Skip = new HashSet<string>(StringComparer.OrdinalIgnoreCase)
        {
            "Application", "Document", "Database", "Handle", "ObjectID", "ObjectID32", "OwnerID", "OwnerID32", "ObjectName",
            "HasExtensionDictionary", "Layer", "Linetype", "LinetypeScale", "Lineweight", "Color", "TrueColor", "PlotStyleName",
            "Visible", "Hyperlinks", "Material", "EntityTransparency", "EntityName", "EntityType", "ShadowDisplay",
            "EnableShadowDisplay", "ReceiveShadows", "CastShadows", "Transparency"
        };

        /// <summary>Reads the COM properties of the object into the given group (max <paramref name="max"/>).</summary>
        public static int Read(object com, BimcElement el, string group, double scale, int max = 120)
        {
            if (com == null || !Marshal.IsComObject(com)) return 0;
            ITypeInfo ti;
            try
            {
                var disp = (IDispatchRaw)com;
                if (disp.GetTypeInfoCount(out uint n) != 0 || n == 0) return 0;
                if (disp.GetTypeInfo(0, 0, out ti) != 0 || ti == null) return 0;
            }
            catch { return 0; }
            var names = new List<string>();
            var seen = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
            CollectGetters(ti, names, seen, 0);
            int added = 0;
            Type t = com.GetType();
            foreach (string name in names)
            {
                if (added >= max) break;
                if (Skip.Contains(name)) continue;
                object v;
                try { v = t.InvokeMember(name, System.Reflection.BindingFlags.GetProperty, null, com, null, CultureInfo.InvariantCulture); }
                catch { continue; }
                string s = Format(name, v, scale);
                if (string.IsNullOrWhiteSpace(s)) continue;
                el.Add(group, Pretty(name), s);
                added++;
            }
            return added;
        }

        /// <summary>The names of the properties readable without parameters (also from the inherited interfaces).</summary>
        private static void CollectGetters(ITypeInfo ti, List<string> names, HashSet<string> seen, int depth)
        {
            if (depth > 6) return;
            IntPtr pAttr = IntPtr.Zero;
            try
            {
                ti.GetTypeAttr(out pAttr);
                var attr = (TYPEATTR)Marshal.PtrToStructure(pAttr, typeof(TYPEATTR));
                for (int i = 0; i < attr.cFuncs; i++)
                {
                    IntPtr pf = IntPtr.Zero;
                    try
                    {
                        ti.GetFuncDesc(i, out pf);
                        var fd = (FUNCDESC)Marshal.PtrToStructure(pf, typeof(FUNCDESC));
                        bool getter = (fd.invkind & INVOKEKIND.INVOKE_PROPERTYGET) != 0 && fd.cParams == 0;
                        // hidden / restricted: skipped
                        bool hidden = (fd.wFuncFlags & (short)(FUNCFLAGS.FUNCFLAG_FHIDDEN | FUNCFLAGS.FUNCFLAG_FRESTRICTED)) != 0;
                        if (getter && !hidden)
                        {
                            var arr = new string[1];
                            ti.GetNames(fd.memid, arr, 1, out int cn);
                            if (cn > 0 && !string.IsNullOrEmpty(arr[0]) && seen.Add(arr[0])) names.Add(arr[0]);
                        }
                    }
                    catch { }
                    finally { if (pf != IntPtr.Zero) ti.ReleaseFuncDesc(pf); }
                }
                // the base interfaces (IAcadEntity -> IAcadObject...) after the object's own
                for (int k = 0; k < attr.cImplTypes; k++)
                {
                    try
                    {
                        ti.GetRefTypeOfImplType(k, out int href);
                        ti.GetRefTypeInfo(href, out ITypeInfo bti);
                        CollectGetters(bti, names, seen, depth + 1);
                    }
                    catch { }
                }
            }
            catch { }
            finally { if (pAttr != IntPtr.Zero) ti.ReleaseTypeAttr(pAttr); }
        }

        private static string Format(string name, object v, double scale)
        {
            switch (v)
            {
                case null: return null;
                case string s: return s.Trim();
                case bool b: return b ? "Yes" : "No";
                case double d:
                    if (double.IsNaN(d) || double.IsInfinity(d)) return null;
                    return d.ToString("0.######", CultureInfo.InvariantCulture);
                case float f: return ((double)f).ToString("0.######", CultureInfo.InvariantCulture);
                case int _:
                case short _:
                case long _:
                case uint _:
                case ushort _:
                case byte _:
                    return Convert.ToString(v, CultureInfo.InvariantCulture);
                case Enum e: return e.ToString();
                case double[] p when p.Length == 3:
                    return $"{p[0] * scale:0.###}, {p[1] * scale:0.###}, {p[2] * scale:0.###}";
                case double[] p2 when p2.Length > 0 && p2.Length <= 6:
                    return string.Join(", ", Array.ConvertAll(p2, x => x.ToString("0.###", CultureInfo.InvariantCulture)));
                default:
                    // nested COM objects (styles, collections): their name, if they have one
                    if (Marshal.IsComObject(v))
                    {
                        try
                        {
                            object n = v.GetType().InvokeMember("Name", System.Reflection.BindingFlags.GetProperty, null, v, null, CultureInfo.InvariantCulture);
                            return n as string;
                        }
                        catch { return null; }
                        finally { try { Marshal.ReleaseComObject(v); } catch { } }
                    }
                    return null;
            }
        }

        /// <summary>"MaterialCode" -> "Material Code"</summary>
        private static string Pretty(string n)
        {
            var sb = new System.Text.StringBuilder();
            for (int i = 0; i < n.Length; i++)
            {
                char c = n[i];
                if (i > 0 && char.IsUpper(c) && (char.IsLower(n[i - 1]) || (i + 1 < n.Length && char.IsLower(n[i + 1]) && char.IsUpper(n[i - 1])))) sb.Append(' ');
                sb.Append(c == '_' ? ' ' : c);
            }
            return sb.ToString();
        }
    }
}
