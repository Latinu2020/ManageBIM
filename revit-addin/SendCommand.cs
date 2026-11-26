using System;
using System.Collections.Generic;
using System.Linq;
using Autodesk.Revit.Attributes;
using Autodesk.Revit.DB;
using Autodesk.Revit.UI;

namespace ManageBIM.Revit
{
    /// <summary>
    /// "Send to ManageBIM": exporta vederea 3D activa (sau {3D}) direct in ManageBIM, fara dialog.
    /// A doua trimitere actualizeaza live modelul deja deschis in ManageBIM (selectia si vederea raman).
    /// </summary>
    [Transaction(TransactionMode.ReadOnly)]
    [Regeneration(RegenerationOption.Manual)]
    public class SendCommand : IExternalCommand
    {
        public Result Execute(ExternalCommandData commandData, ref string message, ElementSet elements)
        {
            string msg = message;
            Result r = Safe.Command(GetType().Name.Replace("Command", ""), () => Run(commandData, ref msg, elements), out string error);
            message = error ?? msg;
            return r;
        }

        private Result Run(ExternalCommandData commandData, ref string message, ElementSet elements)
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

            string path = ManageBimLink.InboxPath("Revit - " + doc.Title);
            string tmp = ManageBimLink.TempPath(path);
            try
            {
                var context = new BimcExportContext(doc);
                var exporter = new CustomExporter(doc, context) { IncludeGeometricObjects = false, ShouldStopOnError = false };
                exporter.Export(view);
                var info = new List<KeyValuePair<string, string>>
                {
                    new KeyValuePair<string, string>("Source application", "Autodesk Revit " + commandData.Application.Application.VersionNumber),
                    new KeyValuePair<string, string>("Project", doc.ProjectInformation?.Name ?? doc.Title),
                    new KeyValuePair<string, string>("Revit file", doc.Title),
                    new KeyValuePair<string, string>("Exported view", view.Name),
                    new KeyValuePair<string, string>("Coordinates", "Shared coordinates, metres"),
                    new KeyValuePair<string, string>("Exporter", "ManageBIM Revit Exporter 0.10 (Send)"),
                    new KeyValuePair<string, string>("Exported", DateTime.Now.ToString("yyyy-MM-dd HH:mm")),
                };
                BimcWriter.Write(tmp, info, context.Warnings, context.Elements);
                ManageBimLink.Commit(tmp, path);
                ManageBimLink.EnsureRunning(path);
                return Result.Succeeded;
            }
            catch (Exception ex)
            {
                ManageBimLink.Log("Revit Send", ex.ToString());
                try { System.IO.File.Delete(tmp); } catch { }
                message = "Send to ManageBIM failed: " + ex.Message;
                return Result.Failed;
            }
        }
    }
}
