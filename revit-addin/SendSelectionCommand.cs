using System;
using System.Collections.Generic;
using System.Linq;
using Autodesk.Revit.Attributes;
using Autodesk.Revit.DB;
using Autodesk.Revit.UI;

namespace ManageBIM.Revit
{
    /// <summary>
    /// "Send selection to ManageBIM": exports only the elements currently selected in Revit,
    /// without touching the source model. A second send updates the model live in ManageBIM.
    /// </summary>
    [Transaction(TransactionMode.ReadOnly)]
    [Regeneration(RegenerationOption.Manual)]
    public class SendSelectionCommand : IExternalCommand
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
            var ids = uidoc.Selection.GetElementIds().ToList();
            if (ids.Count == 0)
            {
                message = "Select at least one element in Revit, then run 'Send selection' again.";
                return Result.Failed;
            }

            // CustomExporter.Export primeste VEDERI, nu elemente: exportam vederea 3D (cea activa, altfel {3D})
            // si contextul pastreaza doar elementele selectate
            View3D view = uidoc.ActiveView as View3D;
            if (view == null || view.IsTemplate)
            {
                view = new FilteredElementCollector(doc).OfClass(typeof(View3D)).Cast<View3D>()
                    .Where(v => !v.IsTemplate)
                    .OrderByDescending(v => v.Name == "{3D}")
                    .FirstOrDefault();
            }
            if (view == null) { message = "The project has no 3D view. Create one and try again."; return Result.Failed; }

            string path = ManageBimLink.InboxPath("Revit selection - " + doc.Title);
            string tmp = ManageBimLink.TempPath(path);
            try
            {
                var context = new BimcExportContext(doc) { Only = new HashSet<ElementId>(ids) };
                var exporter = new CustomExporter(doc, context) { IncludeGeometricObjects = false, ShouldStopOnError = false };
                exporter.Export(view);
                if (context.Elements.Count == 0)
                {
                    message = "The selected elements have no geometry visible in the 3D view \"" + view.Name + "\" (hidden, outside the section box, or not model elements).";
                    return Result.Failed;
                }
                var info = new List<KeyValuePair<string, string>>
                {
                    new KeyValuePair<string, string>("Source application", "Autodesk Revit " + commandData.Application.Application.VersionNumber),
                    new KeyValuePair<string, string>("Project", doc.ProjectInformation?.Name ?? doc.Title),
                    new KeyValuePair<string, string>("Revit file", doc.Title),
                    new KeyValuePair<string, string>("Export mode", "Selection only (" + context.Elements.Count + " elements, from view " + view.Name + ")"),
                    new KeyValuePair<string, string>("Coordinates", "Shared coordinates, metres"),
                    new KeyValuePair<string, string>("Exporter", "ManageBIM Revit Exporter 0.11 (Send selection)"),
                    new KeyValuePair<string, string>("Exported", DateTime.Now.ToString("yyyy-MM-dd HH:mm")),
                };
                BimcWriter.Write(tmp, info, context.Warnings, context.Elements);
                ManageBimLink.Commit(tmp, path);
                ManageBimLink.EnsureRunning(path);
                return Result.Succeeded;
            }
            catch (Exception ex)
            {
                ManageBimLink.Log("Revit SendSelection", ex.ToString());
                try { System.IO.File.Delete(tmp); } catch { }
                message = "Send selection failed: " + ex.Message;
                return Result.Failed;
            }
        }
    }
}
