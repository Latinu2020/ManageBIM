using System;
using System.Collections.Generic;
using System.IO;
using System.Text;

namespace ManageBIM
{
    /// <summary>An element of the ManageBIM reference (.mbref): name, GUID, type, level, color and triangles (meters, relative to Origin).</summary>
    internal sealed class MbRefElement
    {
        public string Name = "", Guid = "", Type = "", Level = "";
        public byte R = 200, G = 200, B = 200, A = 255;
        /// <summary>9 values per triangle (x0 y0 z0 x1 ... z2), meters relative to the origin</summary>
        public float[] Tris = new float[0];
    }

    /// <summary>
    /// The .mbref file written by ManageBIM (Output › Reference): "MBREF1", the origin (3 x f64, real meters), the number of
    /// elements, then per element: 4 texts (u16 length + UTF-8), RGBA, the triangle count and the coordinates (f32).
    /// The same file in Revit and AutoCAD / Plant 3D.
    /// </summary>
    internal static class MbRef
    {
        public static string Folder() => Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "ManageBIM", "Outbox");

        public static List<MbRefElement> Read(string path, out double[] origin)
        {
            var list = new List<MbRefElement>();
            using (var br = new BinaryReader(File.OpenRead(path)))
            {
                var magic = Encoding.ASCII.GetString(br.ReadBytes(6));
                if (magic != "MBREF1") throw new InvalidDataException("Not a ManageBIM reference file (.mbref).");
                origin = new[] { br.ReadDouble(), br.ReadDouble(), br.ReadDouble() };
                uint n = br.ReadUInt32();
                string S() => Encoding.UTF8.GetString(br.ReadBytes(br.ReadUInt16()));
                for (uint i = 0; i < n; i++)
                {
                    var e = new MbRefElement { Name = S(), Guid = S(), Type = S(), Level = S() };
                    e.R = br.ReadByte(); e.G = br.ReadByte(); e.B = br.ReadByte(); e.A = br.ReadByte();
                    uint nt = br.ReadUInt32();
                    if (nt > 50_000_000) throw new InvalidDataException("Damaged reference file.");
                    e.Tris = new float[nt * 9];
                    var bytes = br.ReadBytes((int)(nt * 9 * 4));
                    Buffer.BlockCopy(bytes, 0, e.Tris, 0, bytes.Length);
                    list.Add(e);
                }
            }
            return list;
        }
    }
}
