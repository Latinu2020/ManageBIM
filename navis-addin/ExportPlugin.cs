using System;
using System.IO;
using System.Windows.Forms;
using Autodesk.Navisworks.Api;
using Autodesk.Navisworks.Api.Plugins;
using NwApp = Autodesk.Navisworks.Api.Application;

namespace ManageBIM.Navis
{
    /// <summary>
    /// "Send to ManageBIM": exporta modelul deschis direct in ManageBIM (Inbox), fara dialog.
    /// A doua trimitere actualizeaza live modelul deja deschis in ManageBIM.
    /// </summary>
    [Plugin("SendBimc", "MBIM", DisplayName = "Send to ManageBIM", ToolTip = "Send the open model straight into ManageBIM (opens it if needed); send again to update it live")]
    [AddInPlugin(AddInLocation.AddIn)]
    public class SendPlugin : AddInPlugin
    {
        public override int Execute(params string[] parameters)
        {
            Document doc = NwApp.ActiveDocument;
            if (doc == null || doc.Models.Count == 0)
            {
                MessageBox.Show("Open a model first (NWD, NWC, NWF, RVT, DWG, IFC...).", "ManageBIM");
                return 1;
            }
            string src = string.IsNullOrEmpty(doc.FileName) ? doc.Models[0].FileName : doc.FileName;
            string path = ManageBimLink.InboxPath("Navisworks - " + Path.GetFileNameWithoutExtension(src));
            string tmp = ManageBimLink.TempPath(path);
            Progress pr = NwApp.BeginProgress("ManageBIM", "Sending to ManageBIM");
            try
            {
                NwExporter.Export(doc, tmp, (f, msg) => pr.Update(f));
                NwApp.EndProgress();
                pr = null;
                ManageBimLink.Commit(tmp, path);
                ManageBimLink.EnsureRunning(path);
                return 0;
            }
            catch (OperationCanceledException)
            {
                return 2;
            }
            catch (Exception ex)
            {
                MessageBox.Show("Send to ManageBIM failed:\n" + ex.Message, "ManageBIM");
                return 3;
            }
            finally
            {
                if (pr != null) NwApp.EndProgress();
                try { File.Delete(tmp); } catch { }
            }
        }
    }

    /// <summary>
    /// Butonul "Export to ManageBIM (.bimc)" din tab-ul Tool add-ins.
    /// Din Automation (nwd2bimc.exe): parameters[0] = calea .bimc, fara dialoguri.
    /// </summary>
    [Plugin("ExportBimc", "MBIM", DisplayName = "Export to ManageBIM (.bimc)", ToolTip = "Export the open model (geometry, colours, all properties) to a ManageBIM .bimc file")]
    [AddInPlugin(AddInLocation.AddIn)]
    public class ExportPlugin : AddInPlugin
    {
        public override int Execute(params string[] parameters)
        {
            Document doc = NwApp.ActiveDocument;
            bool batch = parameters != null && parameters.Length > 0 && !string.IsNullOrWhiteSpace(parameters[0]);
            if (doc == null || doc.Models.Count == 0)
            {
                if (!batch) MessageBox.Show("Open a model first (NWD, NWC, NWF, RVT, DWG, IFC...).", "ManageBIM");
                return 1;
            }
            string path;
            if (batch)
            {
                path = parameters[0];
            }
            else
            {
                string src = string.IsNullOrEmpty(doc.FileName) ? (doc.Models.Count > 0 ? doc.Models[0].FileName : "") : doc.FileName;
                using (var dlg = new SaveFileDialog { Title = "Export to ManageBIM", Filter = "ManageBIM model (*.bimc)|*.bimc", FileName = Path.GetFileNameWithoutExtension(src) + ".bimc" })
                {
                    if (!string.IsNullOrEmpty(src) && Directory.Exists(Path.GetDirectoryName(src))) dlg.InitialDirectory = Path.GetDirectoryName(src);
                    if (dlg.ShowDialog() != DialogResult.OK) return 0;
                    path = dlg.FileName;
                }
            }

            Progress pr = batch ? null : NwApp.BeginProgress("ManageBIM", "Exporting to .bimc");
            try
            {
                var t0 = DateTime.Now;
                string progFile = path + ".progress";
                // in Navisworks: bara de progres; din nwd2bimc: un fisier mic citit de nwd2bimc / ManageBIM
                Func<double, string, bool> report = (f, msg) =>
                {
                    if (pr != null) return pr.Update(f);
                    try { File.WriteAllText(progFile, f.ToString("0.000", System.Globalization.CultureInfo.InvariantCulture) + "\t" + msg); } catch { }
                    return true;
                };
                NwExporter.Result r;
                try { r = NwExporter.Export(doc, path, report); }
                finally { try { File.Delete(progFile); } catch { } }
                File.WriteAllText(path + ".log", $"OK {r.Elements} elements, {r.Triangles} triangles, {r.Bytes} bytes, {(DateTime.Now - t0).TotalSeconds:0.0} s\r\n" + r.Timing + "\r\n" + string.Join("\r\n", r.Warnings));
                if (!batch)
                {
                    NwApp.EndProgress();
                    pr = null;
                    MessageBox.Show($"Exported {r.Elements:N0} elements ({r.Triangles:N0} triangles) to\n{path}\n\n{r.Bytes / 1e6:0.0} MB in {(DateTime.Now - t0).TotalSeconds:0.0} s." + (r.Warnings.Count > 0 ? $"\n\n{r.Warnings.Count} warnings (see {Path.GetFileName(path)}.log)." : ""), "ManageBIM");
                }
                return 0;
            }
            catch (OperationCanceledException)
            {
                return 2;
            }
            catch (Exception ex)
            {
                try { File.WriteAllText(path + ".log", "ERROR " + ex); } catch { }
                if (!batch) MessageBox.Show("Export failed:\n" + ex.Message, "ManageBIM");
                return 3;
            }
            finally
            {
                if (pr != null) NwApp.EndProgress();
            }
        }
    }
}
