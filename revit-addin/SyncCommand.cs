using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Text;
using Autodesk.Revit.Attributes;
using Autodesk.Revit.DB;
using Autodesk.Revit.UI;
using TaskDialog = Autodesk.Revit.UI.TaskDialog;

namespace ManageBIM.Revit
{
    /// <summary>
    /// Live Link: selectia sincronizata Revit &lt;-&gt; ManageBIM (si Navisworks), prin fisiere mici in
    /// %LOCALAPPDATA%\ManageBIM\Sync. Butonul porneste / opreste legatura pentru sesiunea curenta.
    /// </summary>
    [Transaction(TransactionMode.ReadOnly)]
    public class SyncCommand : IExternalCommand
    {
        public Result Execute(ExternalCommandData data, ref string message, ElementSet elements)
        {
            bool on = SelectionSync.Toggle();
            TaskDialog.Show("ManageBIM",
                on ? "Selection sync is ON.\n\nSelect elements in Revit and they are selected in ManageBIM (and Navisworks); select in ManageBIM and they are selected here.\n\nIn ManageBIM: Select › Live Link › Sync Selection.\nClick this button again to stop."
                   : "Selection sync is OFF.");
            return Result.Succeeded;
        }
    }

    internal static class SelectionSync
    {
        static bool _on;
        static ExternalEvent _event;
        static System.Threading.Timer _timer;
        static DateTime _seen = DateTime.MinValue;
        static string _lastKey = "";
        static Dictionary<string, ElementId> _index;
        static string _indexDoc;
        static DateTime _indexBuilt = DateTime.MinValue;

        static string Dir => Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "ManageBIM", "Sync");
        static string Incoming => Path.Combine(Dir, "managebim.sel");
        static string Outgoing => Path.Combine(Dir, "revit.sel");

        public static bool Toggle()
        {
            _on = !_on;
            if (_on)
            {
                // selectia veche (de dinainte de pornire) nu se aplica
                _seen = File.Exists(Incoming) ? File.GetLastWriteTimeUtc(Incoming) : DateTime.MinValue;
                _lastKey = "";
                if (_event == null) _event = ExternalEvent.Create(new Handler());
                _timer = new System.Threading.Timer(_ => { try { _event.Raise(); } catch { } }, null, 500, 500);
            }
            else
            {
                _timer?.Dispose();
                _timer = null;
            }
            return _on;
        }

        class Handler : IExternalEventHandler
        {
            public void Execute(UIApplication app)
            {
                if (!_on) return;
                try { Poll(app); } catch { /* o eroare de sincronizare nu are voie sa deranjeze lucrul in Revit */ }
            }
            public string GetName() => "ManageBIM selection sync";
        }

        static void Poll(UIApplication app)
        {
            UIDocument uidoc = app.ActiveUIDocument;
            if (uidoc == null) return;
            Document doc = uidoc.Document;

            // ---- primit din ManageBIM
            if (File.Exists(Incoming))
            {
                DateTime t = File.GetLastWriteTimeUtc(Incoming);
                if (t > _seen)
                {
                    _seen = t;
                    var ids = ReadIds(Incoming);
                    var found = new List<ElementId>();
                    if (ids.Count > 0)
                    {
                        var index = Index(doc, false);
                        foreach (string s in ids) if (index.TryGetValue(s, out ElementId id) && !found.Contains(id)) found.Add(id);
                        if (found.Count == 0 && (DateTime.UtcNow - _indexBuilt).TotalSeconds > 20)
                        {
                            index = Index(doc, true);
                            foreach (string s in ids) if (index.TryGetValue(s, out ElementId id) && !found.Contains(id)) found.Add(id);
                        }
                    }
                    uidoc.Selection.SetElementIds(found);
                    if (found.Count > 0)
                    {
                        try { uidoc.ShowElements(found); } catch { }
                    }
                    _lastKey = Key(found);
                    return;
                }
            }

            // ---- trimis catre ManageBIM
            ICollection<ElementId> sel = uidoc.Selection.GetElementIds();
            string key = Key(sel);
            if (key == _lastKey) return;
            _lastKey = key;
            var sb = new StringBuilder("MBSEL1\n");
            foreach (ElementId id in sel.Take(20000))
            {
                Element e = doc.GetElement(id);
                if (e == null) continue;
                var parts = new List<string> { e.UniqueId };
                string ifc = e.get_Parameter(BuiltInParameter.IFC_GUID)?.AsString();
                if (!string.IsNullOrEmpty(ifc)) parts.Add(ifc);
                try { parts.Add(ExportUtils.GetExportId(doc, id).ToString()); } catch { }
                sb.Append(string.Join("|", parts)).Append('\n');
            }
            Directory.CreateDirectory(Dir);
            string tmp = Outgoing + ".tmp";
            File.WriteAllText(tmp, sb.ToString());
            if (File.Exists(Outgoing)) File.Delete(Outgoing);
            File.Move(tmp, Outgoing);
        }

        static string Key(IEnumerable<ElementId> ids) => string.Join(",", ids.Select(i => i.ToString()).OrderBy(s => s, StringComparer.Ordinal));

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

        /// <summary>UniqueId / IfcGUID / GUID de export -> element (pentru documentul activ, refacut la nevoie).</summary>
        static Dictionary<string, ElementId> Index(Document doc, bool rebuild)
        {
            string key = doc.PathName + "|" + doc.Title;
            if (!rebuild && _index != null && _indexDoc == key) return _index;
            var idx = new Dictionary<string, ElementId>();
            foreach (Element e in new FilteredElementCollector(doc).WhereElementIsNotElementType())
            {
                if (e.Category == null) continue;
                idx[e.UniqueId] = e.Id;
                string ifc = e.get_Parameter(BuiltInParameter.IFC_GUID)?.AsString();
                if (!string.IsNullOrEmpty(ifc)) idx[ifc] = e.Id;
                try { idx[ExportUtils.GetExportId(doc, e.Id).ToString().ToLowerInvariant()] = e.Id; } catch { }
            }
            _index = idx;
            _indexDoc = key;
            _indexBuilt = DateTime.UtcNow;
            return idx;
        }
    }
}
