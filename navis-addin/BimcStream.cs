using System;
using System.Collections.Generic;
using System.IO;
using System.IO.Compression;
using System.Text;

namespace ManageBIM.Navis
{
    /// <summary>
    /// Scriere .bimc versiunea 2 "pe bucati": fiecare element e codat si scris imediat, apoi eliberat din memorie.
    /// Memoria ramane constanta oricat de mare e modelul si nu exista limita de 4 GB a versiunii 1.
    ///
    /// Format v2 (flags = 2):  "BIMC" u16 2  u16 2, apoi bucati:
    ///   u8 tip | u32 marime necomprimata | u32 marime comprimata | DEFLATE
    ///   tip 1 STRINGS : n, n x (lungime, octeti UTF-8)  - se adauga la tabela de siruri
    ///   tip 2 ELEMENTS: n, n x element (exact codarea elementului din v1)
    ///   tip 3 INFO    : info (k, v) + warnings (indici in tabela de siruri)
    ///   tip 0 END     : sfarsit
    /// Trebuie sa ramana compatibil cu src/bimc.rs (read_v2).
    /// </summary>
    public sealed class BimcStream : IDisposable
    {
        private const double Q = 10000.0; // 0.1 mm
        private const int FlushBytes = 8 << 20;

        private readonly string path, tmp;
        private FileStream fs;
        private readonly Dictionary<string, uint> map = new Dictionary<string, uint>(StringComparer.Ordinal);
        private readonly List<string> strings = new List<string>();
        private int stringsWritten;
        private readonly Buf body = new Buf();
        private int pending;
        // tampoane refolosite intre elemente
        private readonly Dictionary<(long, long, long), int> vmap = new Dictionary<(long, long, long), int>();
        private readonly List<(long, long, long)> verts = new List<(long, long, long)>();
        private readonly List<int> idx = new List<int>();

        public long Elements { get; private set; }
        public long Triangles { get; private set; }

        public BimcStream(string path)
        {
            this.path = path;
            tmp = path + ".tmp";
            fs = new FileStream(tmp, FileMode.Create, FileAccess.Write, FileShare.None, 1 << 20);
            fs.Write(Encoding.ASCII.GetBytes("BIMC"), 0, 4);
            fs.Write(BitConverter.GetBytes((ushort)2), 0, 2); // versiune
            fs.Write(BitConverter.GetBytes((ushort)2), 0, 2); // flags: bucati DEFLATE
        }

        private uint Id(string s)
        {
            s = s ?? "";
            if (map.TryGetValue(s, out uint i)) return i;
            i = (uint)strings.Count;
            map[s] = i;
            strings.Add(s);
            return i;
        }

        /// <summary>Codeaza elementul in bucata curenta; geometria lui poate fi eliberata dupa apel.</summary>
        public void Add(BimcElement e)
        {
            body.U(Id(e.Name)); body.U(Id(e.Type)); body.U(Id(e.Guid)); body.U(Id(e.Level));
            body.U(e.DefaultHidden ? 1UL : 0UL);
            body.U((ulong)e.Props.Count);
            foreach (var p in e.Props) { body.U(Id(p.Group)); body.U(Id(p.Name)); body.U(Id(p.Value)); }

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
            // culori pe secvente
            var runs = new List<(uint, int)>();
            foreach (uint c in e.Colors)
            {
                if (runs.Count > 0 && runs[runs.Count - 1].Item1 == c) runs[runs.Count - 1] = (c, runs[runs.Count - 1].Item2 + 1);
                else runs.Add((c, 1));
            }
            body.U((ulong)runs.Count);
            foreach (var r in runs) { body.U(r.Item1); body.U((ulong)r.Item2); }

            pending++;
            Elements++;
            Triangles += e.TriangleCount;
            if (body.Length >= FlushBytes) Flush();
        }

        private void Flush()
        {
            WriteNewStrings();
            if (pending == 0) return;
            var head = new Buf();
            head.U((ulong)pending);
            var chunk = new byte[head.Length + body.Length];
            Buffer.BlockCopy(head.Data, 0, chunk, 0, head.Length);
            Buffer.BlockCopy(body.Data, 0, chunk, head.Length, body.Length);
            WriteChunk(2, chunk, chunk.Length);
            body.Length = 0;
            pending = 0;
        }

        private void WriteNewStrings()
        {
            if (stringsWritten == strings.Count) return;
            var b = new Buf();
            b.U((ulong)(strings.Count - stringsWritten));
            for (int i = stringsWritten; i < strings.Count; i++)
            {
                byte[] s = Encoding.UTF8.GetBytes(strings[i]);
                b.U((ulong)s.Length);
                b.Bytes(s);
            }
            WriteChunk(1, b.Data, b.Length);
            stringsWritten = strings.Count;
        }

        private void WriteChunk(byte kind, byte[] data, int len)
        {
            byte[] comp;
            using (var ms = new MemoryStream(len / 3 + 64))
            {
                using (var ds = new DeflateStream(ms, CompressionLevel.Fastest, true)) ds.Write(data, 0, len);
                comp = ms.ToArray();
            }
            fs.WriteByte(kind);
            fs.Write(BitConverter.GetBytes((uint)len), 0, 4);
            fs.Write(BitConverter.GetBytes((uint)comp.Length), 0, 4);
            fs.Write(comp, 0, comp.Length);
        }

        /// <summary>Scrie informatiile modelului, inchide fisierul si il pune la locul final. Intoarce marimea.</summary>
        public long Finish(IList<KeyValuePair<string, string>> info, IList<string> warnings)
        {
            Flush();
            var b = new Buf();
            var ids = new List<uint>();
            foreach (var kv in info) { ids.Add(Id(kv.Key)); ids.Add(Id(kv.Value)); }
            var wids = new List<uint>();
            foreach (var w in warnings) wids.Add(Id(w));
            WriteNewStrings();
            b.U((ulong)info.Count);
            foreach (uint i in ids) b.U(i);
            b.U((ulong)wids.Count);
            foreach (uint i in wids) b.U(i);
            WriteChunk(3, b.Data, b.Length);
            WriteChunk(0, new byte[0], 0);
            fs.Flush();
            fs.Dispose();
            fs = null;
            if (File.Exists(path)) File.Delete(path);
            File.Move(tmp, path);
            return new FileInfo(path).Length;
        }

        public void Dispose()
        {
            if (fs != null)
            {
                fs.Dispose();
                fs = null;
                try { File.Delete(tmp); } catch { }
            }
        }

        private sealed class Buf
        {
            public byte[] Data = new byte[1 << 16];
            public int Length;
            private void Ensure(int n)
            {
                if (Length + n <= Data.Length) return;
                Array.Resize(ref Data, Math.Max(Data.Length * 2, Length + n));
            }
            public void Bytes(byte[] b) { Ensure(b.Length); Buffer.BlockCopy(b, 0, Data, Length, b.Length); Length += b.Length; }
            public void U(ulong v)
            {
                Ensure(10);
                while (true)
                {
                    byte x = (byte)(v & 0x7f);
                    v >>= 7;
                    if (v == 0) { Data[Length++] = x; return; }
                    Data[Length++] = (byte)(x | 0x80);
                }
            }
            public void I(long v) => U((ulong)((v << 1) ^ (v >> 63)));
        }
    }
}
