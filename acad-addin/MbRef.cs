using System;
using System.Collections.Generic;
using System.IO;
using System.Text;

namespace ManageBIM
{
    /// <summary>Un element din referinta ManageBIM (.mbref): nume, GUID, tip, nivel, culoare si triunghiuri (metri, fata de Origin).</summary>
    internal sealed class MbRefElement
    {
        public string Name = "", Guid = "", Type = "", Level = "";
        public byte R = 200, G = 200, B = 200, A = 255;
        /// <summary>9 valori pe triunghi (x0 y0 z0 x1 ... z2), metri fata de origine</summary>
        public float[] Tris = new float[0];
    }

    /// <summary>
    /// Fisierul .mbref scris de ManageBIM (Output › Reference): "MBREF1", originea (3 x f64, metri reali), numarul de
    /// elemente, apoi pe element: 4 texte (u16 lungime + UTF-8), RGBA, numarul de triunghiuri si coordonatele (f32).
    /// Acelasi fisier in Revit si AutoCAD / Plant 3D.
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
