using System;
using System.IO;
using System.Reflection;
using System.Windows.Media.Imaging;
using Autodesk.Revit.UI;

namespace ManageBIM.Revit
{
    /// <summary>
    /// The "ManageBIM" tab, compact (0.28): two large buttons - "Send" (the most used) and "ManageBIM"
    /// (the window with all commands) - and the rest as small buttons, stacked three at a time.
    /// </summary>
    public class App : IExternalApplication
    {
        public Result OnStartup(UIControlledApplication app)
        {
            const string tab = "ManageBIM";
            try { app.CreateRibbonTab(tab); } catch { /* tab already exists */ }
            RibbonPanel panel = app.CreateRibbonPanel(tab, "ManageBIM");
            string asm = Assembly.GetExecutingAssembly().Location;
            string dir = Path.GetDirectoryName(asm);
            var large = LoadImage(Path.Combine(dir, "Resources", "managebim32.png"));
            var small = LoadImage(Path.Combine(dir, "Resources", "managebim16.png"));

            PushButtonData B(string id, string text, Type cmd, string tip, string desc) =>
                new PushButtonData(id, text, asm, cmd.FullName) { ToolTip = tip, LongDescription = desc, LargeImage = large, Image = small };

            // ---- large
            panel.AddItem(B("ManageBIMSend", "Send to\nManageBIM", typeof(SendCommand),
                "Send the 3D view straight into ManageBIM (opens it if needed). Send again to update the model live.",
                "Exports the active 3D view (or {3D}) without any dialog and opens it in ManageBIM. If ManageBIM is already open with this project, the model is replaced in place, keeping the selection, hidden elements and the camera."));
            panel.AddItem(B("ManageBIMHub", "ManageBIM", typeof(HubCommand),
                "All ManageBIM commands in one window",
                "Send, export, issues, coordination objects, reference models and the live selection link - each with a short description."));
            panel.AddSeparator();

            // ---- small, stacked: send / coordination / links
            panel.AddStackedItems(
                B("ManageBIMSendSelection", "Send selection", typeof(SendSelectionCommand), "Send only the selected Revit elements to ManageBIM (no model change)", "Exports the elements currently selected in Revit to ManageBIM without modifying the source model."),
                B("ManageBIMExportBimc", "Export BIMC", typeof(ExportCommand), "Export the 3D view to a ManageBIM model cache (.bimc)", "Writes every visible element of the active 3D view with geometry, colours, level and all parameters into a .bimc file - like a Navisworks NWC export."),
                B("ManageBIMReference", "Reference", typeof(ReferenceCommand), "Bring a model from ManageBIM in as reference geometry (Generic Models, pinned group)", "In ManageBIM: Output › Reference saves a .mbref file; this command creates one DirectShape per item, with its colour, in shared coordinates."));
            panel.AddStackedItems(
                B("ManageBIMCoordination", "Coordination", typeof(CoordinationCommand), "Show the clearance zones, voids and placeholders sent from ManageBIM (display only)", "In ManageBIM: Coordination › Objects › Send to Revit. A preview window lists them with a top view; they are shown as transparent Generic Models and nothing in the model is cut or changed."),
                B("ManageBIMIssues", "Issues", typeof(IssuesCommand), "Open a ManageBIM issue / viewpoint: 3D view with the same camera and section box, its elements selected", "Reads the viewpoints saved in ManageBIM and creates a 3D view 'ManageBIM - <issue>' with the camera, the section box and the issue's elements selected."),
                B("ManageBIMSync", "Sync selection", typeof(SyncCommand), "Live Link: the selection follows ManageBIM and Navisworks, both ways (click again to stop)", "Elements are matched by UniqueId / IFC GUID, so the ManageBIM model must come from this Revit model. In ManageBIM turn it on in Select › Live Link › Sync Selection."));
            // tools\test-addins.ps1: only active with MANAGEBIM_AUTOTEST set
            AutoTest.Register(app);
            return Result.Succeeded;
        }

        public Result OnShutdown(UIControlledApplication app) => Result.Succeeded;

        private static BitmapImage LoadImage(string path)
        {
            if (!File.Exists(path)) return null;
            var img = new BitmapImage();
            img.BeginInit();
            img.UriSource = new Uri(path);
            img.CacheOption = BitmapCacheOption.OnLoad;
            img.EndInit();
            return img;
        }
    }
}
