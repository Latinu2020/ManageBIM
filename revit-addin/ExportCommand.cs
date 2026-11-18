using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Linq;
using Autodesk.Revit.Attributes;
using Autodesk.Revit.DB;
using Autodesk.Revit.UI;

namespace ManageBIM.Revit
{
    /// <summary>Exports the active 3D view to .bimc (read-only, no model changes).</summary>
    [Transaction(TransactionMode.ReadOnly)]
    [Regeneration(RegenerationOption.Manual)]
    public class ExportCommand : IExternalCommand
    {
        public Result Execute(ExternalCommandData commandData, ref string message, ElementSet elements)
        {
            UIDocument uidoc = commandData.Application.ActiveUIDocument;
            if (uidoc == null) { message = "Open a project first."; return Result.Failed; }
            Document doc = uidoc.Document;

            View3D view = uidoc.ActiveView as View3D;
            if (view == null || view.IsTemplate)
            {
                view = new FilteredElementCollector(doc).OfClass(typeof(View3D)).Cast<View3D>()
                    .Where(v => !v.IsTemplate)
                    .OrderByDescending(v => v.Name == "{3D}")
                    .FirstOrDefault();
            }
            if (view == null) { message = "The project has no 3D view. Create one and try again."; return Result.Failed; }

            string folder = string.IsNullOrEmpty(doc.PathName) ? Environment.GetFolderPath(Environment.SpecialFolder.MyDocuments) : Path.GetDirectoryName(doc.PathName);
            string path;
            var dlg = new FileSaveDialog("ManageBIM model cache (*.bimc)|*.bimc")
            {
                Title = "Export ManageBIM model cache",
                InitialFileName = Path.Combine(folder, Sanitize(doc.Title) + ".bimc"),
            };
            if (dlg.Show() != ItemSelectionDialogResult.Confirmed) return Result.Cancelled;
            path = ModelPathUtils.ConvertModelPathToUserVisiblePath(dlg.GetSelectedModelPath());
            if (!path.EndsWith(".bimc", StringComparison.OrdinalIgnoreCase)) path += ".bimc";

            var sw = Stopwatch.StartNew();
            try
            {
                var context = new BimcExportContext(doc);
                var exporter = new CustomExporter(doc, context)
                {
                    IncludeGeometricObjects = false,
                    ShouldStopOnError = false,
                };
                exporter.Export(view);

                var info = new List<KeyValuePair<string, string>>
                {
                    new KeyValuePair<string, string>("Source application", "Autodesk Revit " + commandData.Application.Application.VersionNumber),
                    new KeyValuePair<string, string>("Project", doc.ProjectInformation?.Name ?? doc.Title),
                    new KeyValuePair<string, string>("Revit file", doc.Title),
                    new KeyValuePair<string, string>("Exported view", view.Name),
                    new KeyValuePair<string, string>("Coordinates", "Shared coordinates, metres"),
                    new KeyValuePair<string, string>("Exporter", "ManageBIM Revit Exporter 0.9"),
                    new KeyValuePair<string, string>("Exported", DateTime.Now.ToString("yyyy-MM-dd HH:mm")),
                };
                long bytes = BimcWriter.Write(path, info, context.Warnings, context.Elements);
                sw.Stop();

                int tris = context.Elements.Sum(e => e.TriangleCount);
                var td = new TaskDialog("ManageBIM")
                {
                    MainInstruction = "Export finished",
                    MainContent = $"{context.Elements.Count:N0} elements, {tris:N0} triangles\n" +
                                  $"{bytes / 1048576.0:0.00} MB written in {sw.Elapsed.TotalSeconds:0.0} s\n\n{path}",
                };
                td.AddCommandLink(TaskDialogCommandLinkId.CommandLink1, "Open the file", "Opens the .bimc with the program associated with it (ManageBIM).");
                td.AddCommandLink(TaskDialogCommandLinkId.CommandLink2, "Open the folder");
                td.CommonButtons = TaskDialogCommonButtons.Close;
                var r = td.Show();
                if (r == TaskDialogResult.CommandLink1) TryStart(path);
                else if (r == TaskDialogResult.CommandLink2) TryStart("explorer.exe", "/select,\"" + path + "\"");
                return Result.Succeeded;
            }
            catch (Exception ex)
            {
                message = "ManageBIM export failed: " + ex.Message;
                return Result.Failed;
            }
        }

        private static string Sanitize(string s)
        {
            foreach (char c in Path.GetInvalidFileNameChars()) s = s.Replace(c, '_');
            return s;
        }

        private static void TryStart(string file, string args = null)
        {
            try { Process.Start(new ProcessStartInfo(file, args ?? "") { UseShellExecute = true }); }
            catch (Exception ex) { TaskDialog.Show("ManageBIM", "Could not open: " + ex.Message); }
        }
    }
}
