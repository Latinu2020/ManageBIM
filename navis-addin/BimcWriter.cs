using System;
using System.Collections.Generic;
using System.IO;
using System.IO.Compression;
using System.Text;
// same writer as the AutoCAD / Revit exporters (keep byte-compatible with src/bimc.rs)

namespace ManageBIM.Navis
{
    public struct V3
    {
        public double X, Y, Z;
        public V3(double x, double y, double z) { X = x; Y = y; Z = z; }
    }

    public struct BimcProp
    {
        public string Group, Name, Value;
        public BimcProp(string g, string n, string v) { Group = g; Name = n; Value = v; }
    }

    public class BimcElement
    {
        public string Name = "", Type = "", Guid = "", Level = "";
        public bool DefaultHidden;
        public readonly List<BimcProp> Props = new List<BimcProp>();
        /// <summary>3 vertices per triangle, metres.</summary>
        public readonly List<V3> Vertices = new List<V3>();
        public readonly List<uint> Colors = new List<uint>();
        public int TriangleCount => Colors.Count;

        public void Add(string group, string name, string value)
        {
            if (!string.IsNullOrEmpty(value)) Props.Add(new BimcProp(group, name, value));
        }

        public void AddTriangle(V3 a, V3 b, V3 c, uint color)
        {
            Vertices.Add(a); Vertices.Add(b); Vertices.Add(c);
            Colors.Add(color);
        }
    }

    /// <summary>
    /// Writes the ManageBIM cache format (.bimc), version 1, DEFLATE variant (flags = 1).
    /// Must stay byte-compatible with src/bimc.rs in ManageBIM.
    /// </summary>
    public static class BimcWriter
    {
        private const double Q = 10000.0; // 0.1 mm

        private sealed class Buf
        {
            public byte[] Data = new byte[1 << 20];
            public int Length;
            private void Ensure(int n)
            {
                if (Length + n <= Data.Length) return;
                Array.Resize(ref Data, Math.Max(Data.Length * 2, Length + n));
            }
            public void Byte(byte b) { Ensure(1); Data[Length++] = b; }
            public void Bytes(byte[] b) { Ensure(b.Length); Buffer.BlockCopy(b, 0, Data, Length, b.Length); Length += b.Length; }
            public void U(ulong v)
            {
                Ensure(10);
                while (true)
                {
                    byte b = (byte)(v & 0x7f);
                    v >>= 7;
                    if (v == 0) { Data[Length++] = b; return; }
                    Data[Length++] = (byte)(b | 0x80);
                }
            }
            public void I(long v) => U((ulong)((v << 1) ^ (v >> 63)));
        }

        private sealed class Strings
        {
            public readonly Dictionary<string, uint> Map = new Dictionary<string, uint>(StringComparer.Ordinal);
            public readonly List<string> List = new List<string>();
            public uint Id(string s)
            {
                s = s ?? "";
                if (Map.TryGetValue(s, out uint i)) return i;
                i = (uint)List.Count;
                Map[s] = i;
                List.Add(s);
                return i;
            }
        }

        public static long Write(string path, IList<KeyValuePair<string, string>> info, IList<string> warnings, IList<BimcElement> elements)
        {
            var st = new Strings();
            var body = new Buf();

            body.U((ulong)info.Count);
            foreach (var kv in info) { body.U(st.Id(kv.Key)); body.U(st.Id(kv.Value)); }
            body.U((ulong)warnings.Count);
            foreach (var w in warnings) body.U(st.Id(w));

            body.U((ulong)elements.Count);
            var vmap = new Dictionary<(long, long, long), int>();
            var verts = new List<(long, long, long)>();
            var idx = new List<int>();
            foreach (var e in elements)
            {
                body.U(st.Id(e.Name)); body.U(st.Id(e.Type)); body.U(st.Id(e.Guid)); body.U(st.Id(e.Level));
                body.U(e.DefaultHidden ? 1UL : 0UL);
                body.U((ulong)e.Props.Count);
                foreach (var p in e.Props) { body.U(st.Id(p.Group)); body.U(st.Id(p.Name)); body.U(st.Id(p.Value)); }

                vmap.Clear(); verts.Clear(); idx.Clear();
                foreach (V3 v in e.Vertices)
                {
                    var q = ((long)Math.Round(v.X * Q, MidpointRounding.AwayFromZero),
                             (long)Math.Round(v.Y * Q, MidpointRounding.AwayFromZero),
                             (long)Math.Round(v.Z * Q, MidpointRounding.AwayFromZero));
                    if (!vmap.TryGetValue(q, out int k)) { k = verts.Count; vmap[q] = k; verts.Add(q); }
                    idx.Add(k);
                }
                body.U((ulong)verts.Count);
                body.U((ulong)e.TriangleCount);
                long px = 0, py = 0, pz = 0;
                foreach (var v in verts)
                {
                    body.I(v.Item1 - px); body.I(v.Item2 - py); body.I(v.Item3 - pz);
                    px = v.Item1; py = v.Item2; pz = v.Item3;
                }
                long pa = 0;
                for (int t = 0; t + 2 < idx.Count; t += 3)
                {
                    long a = idx[t];
                    body.I(a - pa); body.I(idx[t + 1] - a); body.I(idx[t + 2] - a);
                    pa = a;
                }
                // colour runs
                var runs = new List<(uint, int)>();
                foreach (uint c in e.Colors)
                {
                    if (runs.Count > 0 && runs[runs.Count - 1].Item1 == c) runs[runs.Count - 1] = (c, runs[runs.Count - 1].Item2 + 1);
                    else runs.Add((c, 1));
                }
                body.U((ulong)runs.Count);
                foreach (var r in runs) { body.U(r.Item1); body.U((ulong)r.Item2); }
            }

            var payload = new Buf();
            payload.U((ulong)st.List.Count);
            foreach (string s in st.List)
            {
                byte[] b = Encoding.UTF8.GetBytes(s);
                payload.U((ulong)b.Length);
                payload.Bytes(b);
            }
            var bodyBytes = new byte[body.Length];
            Buffer.BlockCopy(body.Data, 0, bodyBytes, 0, body.Length);
            payload.Bytes(bodyBytes);

            string tmp = path + ".tmp";
            using (var fs = new FileStream(tmp, FileMode.Create, FileAccess.Write))
            {
                fs.Write(Encoding.ASCII.GetBytes("BIMC"), 0, 4);
                fs.Write(BitConverter.GetBytes((ushort)1), 0, 2); // version
                fs.Write(BitConverter.GetBytes((ushort)1), 0, 2); // flags: deflate
                fs.Write(BitConverter.GetBytes((uint)payload.Length), 0, 4);
                using (var ds = new DeflateStream(fs, CompressionMode.Compress, true))
                    ds.Write(payload.Data, 0, payload.Length);
            }
            if (File.Exists(path)) File.Delete(path);
            File.Move(tmp, path);
            return new FileInfo(path).Length;
        }
    }
}
