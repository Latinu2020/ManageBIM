using System;
using System.Linq;
using System.Reflection;
using System.Runtime.InteropServices;
using Autodesk.Navisworks.Api;

namespace ManageBIM.Navis
{
    // The Navisworks COM interfaces used for extracting the geometry, declared here instead of
    // Autodesk.Navisworks.Interop.ComApi.dll (which has a different version number every year: 20.x in 2023,
    // 22.0 in 2025...). The COM contract (GUIDs, method order) is the same in all versions, so
    // the same code works in Navisworks 2022 - 2026. The definitions are copied from the 2025 interop.

    /// <summary>Our callback for GenerateSimplePrimitives (Navisworks calls it through the vtable).</summary>
    [ComVisible(true)]
    [Guid("1f4067d7-48fb-4759-bb8e-3e4a281fc841")]
    [InterfaceType(ComInterfaceType.InterfaceIsDual)]
    public interface INwSimplePrimitivesCB
    {
        [DispId(1610743808)] void Triangle([MarshalAs(UnmanagedType.Interface)] object v1, [MarshalAs(UnmanagedType.Interface)] object v2, [MarshalAs(UnmanagedType.Interface)] object v3);
        [DispId(1610743809)] void Line([MarshalAs(UnmanagedType.Interface)] object v1, [MarshalAs(UnmanagedType.Interface)] object v2);
        [DispId(1610743810)] void Point([MarshalAs(UnmanagedType.Interface)] object v1);
        [DispId(1610743811)] void SnapPoint([MarshalAs(UnmanagedType.Interface)] object v1);
    }

    /// <summary>
    /// InwSimpleVertex (derived from InwBase): called millions of times, so bound directly through the vtable.
    /// The first 15 slots are the InwBase members (unused here) - they only keep the positions.
    /// </summary>
    [ComImport]
    [Guid("3ef67071-eeb7-4e23-bacd-6a7251492a37")]
    [InterfaceType(ComInterfaceType.InterfaceIsDual)]
    public interface INwSimpleVertex
    {
        void _nwReadOnly(); void _nwInvalidated(); void _nwHandle(); void _nwID(); void _Xtension();
        void _nwProcess(); void _nwThread(); void _nwInit(); void _nwInvalidate(); void _set_nwLock();
        void _get_nwLock(); void _set_nwOwn(); void _get_nwOwn(); void _ObjectName(); void _Copy();
        [DispId(1610809344)] [return: MarshalAs(UnmanagedType.Struct)] object get_coord();
    }

    /// <summary>Access to the COM API without a reference to the interop: ComApiBridge found at runtime, the rest through IDispatch.</summary>
    internal static class NwCom
    {
        private static MethodInfo toPath;

        /// ModelItem -> InwOaPath (COM object), through ComApiBridge from the running Navisworks version
        public static object ToPath(ModelItem item)
        {
            if (toPath == null)
            {
                Assembly asm = AppDomain.CurrentDomain.GetAssemblies().FirstOrDefault(a => a.GetName().Name == "Autodesk.Navisworks.ComApi");
                if (asm == null)
                {
                    // next to Autodesk.Navisworks.Api.dll in the Navisworks folder
                    string dir = System.IO.Path.GetDirectoryName(typeof(ModelItem).Assembly.Location);
                    asm = Assembly.LoadFrom(System.IO.Path.Combine(dir, "Autodesk.Navisworks.ComApi.dll"));
                }
                Type bridge = asm.GetType("Autodesk.Navisworks.Api.ComApi.ComApiBridge", true);
                toPath = bridge.GetMethod("ToInwOaPath", new[] { typeof(ModelItem) }) ?? throw new MissingMethodException("ComApiBridge.ToInwOaPath");
            }
            return toPath.Invoke(null, new object[] { item });
        }

        public static int[] Ints(object safeArray)
        {
            var arr = (Array)safeArray;
            var key = new int[arr.Length];
            int k = 0;
            foreach (object o in arr) key[k++] = Convert.ToInt32(o);
            return key;
        }

        public static double[] Doubles(object safeArray, int n)
        {
            var arr = (Array)safeArray;
            var m = new double[n];
            int k = 0;
            foreach (object o in arr) { if (k < n) m[k++] = Convert.ToDouble(o); }
            return m;
        }
    }
}
