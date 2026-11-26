using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Text;
using System.Windows.Forms;
using Autodesk.Navisworks.Api;
using Autodesk.Navisworks.Api.Plugins;
using NwApp = Autodesk.Navisworks.Api.Application;

namespace ManageBIM.Navis
{
    /// <summary>
    /// Live Link: selectia sincronizata Navisworks &lt;-&gt; ManageBIM (si Revit), prin fisiere mici in
    /// %LOCALAPPDATA%\ManageBIM\Sync. Butonul porneste / opreste legatura pentru sesiunea curenta.
    /// </summary>
    [Plugin("ManageBIMSync", "MBIM", DisplayName = "Sync Selection", ToolTip = "Live Link: the selection follows ManageBIM and Revit, both ways (click again to stop)")]
    [AddInPlugin(AddInLocation.None)]
    public class SyncPlugin : AddInPlugin
    {
        public override int Execute(params string[] parameters) => Safe.Plugin(GetType().Name.Replace("Plugin", ""), parameters, () => Run(parameters));

        private int Run(string[] parameters)
        {
            bool on = NavisSync.Toggle();
            MessageBox.Show(on
                ? "Selection sync is ON.\n\nSelect items in Navisworks and they are selected in ManageBIM (and Revit); select in ManageBIM and they are selected here.\n\nIn ManageBIM: Select › Live Link › Sync Selection.\nRun this command again to stop."
                : "Selection sync is OFF.", "ManageBIM");
            return 0;
        }
    }

    internal static class NavisSync
    {
        static bool _on;
        static Timer _timer;
        static DateTime _seen = DateTime.MinValue;
        static string _lastKey = "";
        static Dictionary<string, ModelItem> _index;
        static Document _indexDoc;
        static int _indexModels = -1;
        static DateTime _indexBuilt = DateTime.MinValue;

        static readonly HashSet<string> IdProps = new HashSet<string> { "IfcGUID", "IFC GUID", "Ifc GUID", "GlobalId", "IfcGlobalId", "UniqueId", "Unique Id" };

        static string Dir => Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "ManageBIM", "Sync");
        static string Incoming => Path.Combine(Dir, "managebim.sel");
        static string Outgoing => Path.Combine(Dir, "navis.sel");

        public static bool Toggle()
        {
            _on = !_on;
            if (_on)
            {
                _seen = File.Exists(Incoming) ? File.GetLastWriteTimeUtc(Incoming) : DateTime.MinValue;
                _lastKey = "";
                // timer pe firul interfetei: API-ul Navisworks se poate folosi direct
                _timer = new Timer { Interval = 500 };
                _timer.Tick += (s, e) => { try { Poll(); } catch { /* nu deranjeaza lucrul in Navisworks */ } };
                _timer.Start();
            }
            else
            {
                _timer?.Stop();
                _timer?.Dispose();
                _timer = null;
            }
            return _on;
        }

        static void Poll()
        {
            Document doc = NwApp.ActiveDocument;
            if (doc == null || doc.Models.Count == 0) return;

            // ---- primit din ManageBIM
            if (File.Exists(Incoming))
            {
                DateTime t = File.GetLastWriteTimeUtc(Incoming);
                if (t > _seen)
                {
                    _seen = t;
                    var ids = ReadIds(Incoming);
                    var found = new ModelItemCollection();
                    var index = Index(doc, false);
                    foreach (string s in ids) if (index.TryGetValue(s, out ModelItem it) && !found.Contains(it)) found.Add(it);
                    if (found.Count == 0 && ids.Count > 0 && (DateTime.UtcNow - _indexBuilt).TotalSeconds > 20)
                    {
                        index = Index(doc, true);
                        foreach (string s in ids) if (index.TryGetValue(s, out ModelItem it) && !found.Contains(it)) found.Add(it);
                    }
                    doc.CurrentSelection.CopyFrom(found);
                    _lastKey = Key(found);
                    return;
                }
            }

            // ---- trimis catre ManageBIM
            ModelItemCollection sel = doc.CurrentSelection.SelectedItems;
            string key = Key(sel);
            if (key == _lastKey) return;
            _lastKey = key;
            var sb = new StringBuilder("MBSEL1\n");
            int n = 0;
            foreach (ModelItem it in sel)
            {
                if (++n > 20000) break;
                var parts = Ids(it);
                if (parts.Count > 0) sb.Append(string.Join("|", parts)).Append('\n');
            }
            Directory.CreateDirectory(Dir);
            string tmp = Outgoing + ".tmp";
            File.WriteAllText(tmp, sb.ToString());
            if (File.Exists(Outgoing)) File.Delete(Outgoing);
            File.Move(tmp, Outgoing);
        }

        /// <summary>GUID-ul Navisworks + IfcGUID / GlobalId / UniqueId din proprietati.</summary>
        static List<string> Ids(ModelItem it)
        {
            var parts = new List<string>();
            if (it.InstanceGuid != Guid.Empty) parts.Add(it.InstanceGuid.ToString().ToLowerInvariant());
            try
            {
                foreach (PropertyCategory cat in it.PropertyCategories)
                    foreach (DataProperty p in cat.Properties)
                    {
                        if (!IdProps.Contains(p.DisplayName)) continue;
                        string v;
                        try { v = p.Value == null ? "" : p.Value.ToDisplayString(); } catch { continue; }
                        v = v.Trim();
                        if (v.Length == 0) continue;
                        if (v.Length == 36 && v.Count(c => c == '-') == 4) v = v.ToLowerInvariant();
                        if (!parts.Contains(v)) parts.Add(v);
                    }
            }
            catch { }
            return parts;
        }

        static string Key(ModelItemCollection items)
        {
            var sb = new StringBuilder();
            sb.Append(items.Count).Append(':');
            foreach (ModelItem it in items.Take(5000)) sb.Append(it.GetHashCode()).Append(',');
            return sb.ToString();
        }

        static List<string> ReadIds(string path)
        {
            var list = new List<string>();
            string[] lines;
            try { lines = File.ReadAllLines(path); } catch { return list; }
            if (lines.Length == 0 || lines[0].Trim() != "MBSEL1") return list;
            foreach (string l in lines.Skip(1))
                foreach (string t in l.Split('|'))
                {
                    string s = t.Trim();
                    if (s.Length == 0) continue;
                    list.Add(s.Length == 36 && s.Count(c => c == '-') == 4 ? s.ToLowerInvariant() : s);
                }
            return list;
        }

        /// <summary>identificator -> obiect (doar obiectele cu geometrie si parintii lor directi), refacut la nevoie.</summary>
        static Dictionary<string, ModelItem> Index(Document doc, bool rebuild)
        {
            if (!rebuild && _index != null && ReferenceEquals(_indexDoc, doc) && _indexModels == doc.Models.Count) return _index;
            var idx = new Dictionary<string, ModelItem>();
            foreach (ModelItem it in doc.Models.RootItemDescendantsAndSelf)
            {
                if (!it.HasGeometry && it.InstanceGuid == Guid.Empty) continue;
                foreach (string s in Ids(it))
                    if (!idx.ContainsKey(s)) idx[s] = it;
            }
            _index = idx;
            _indexDoc = doc;
            _indexModels = doc.Models.Count;
            _indexBuilt = DateTime.UtcNow;
            return idx;
        }
    }
}
