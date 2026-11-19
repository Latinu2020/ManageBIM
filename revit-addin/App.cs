using System;
using System.IO;
using System.Reflection;
using System.Windows.Media.Imaging;
using Autodesk.Revit.UI;

namespace ManageBIM.Revit
{
    /// <summary>Adds a "ManageBIM" ribbon tab with the export button.</summary>
    public class App : IExternalApplication
    {
        public Result OnStartup(UIControlledApplication app)
        {
            const string tab = "ManageBIM";
            try { app.CreateRibbonTab(tab); } catch { /* tab already exists */ }
            RibbonPanel panel = app.CreateRibbonPanel(tab, "Export");

            string asm = Assembly.GetExecutingAssembly().Location;
            var data = new PushButtonData("ManageBIMExportBimc", "Export\nBIMC", asm, typeof(ExportCommand).FullName)
            {
                ToolTip = "Export the 3D view to a ManageBIM model cache (.bimc)",
                LongDescription = "Writes every visible element of the active 3D view (or the default {3D} view) " +
                                  "with geometry, material colours, level and all instance and type parameters " +
                                  "into a compact .bimc file that opens directly in ManageBIM - similar to a Navisworks NWC export.",
            };
            string dir = Path.GetDirectoryName(asm);
            if (panel.AddItem(data) is PushButton button)
            {
                button.LargeImage = LoadImage(Path.Combine(dir, "Resources", "managebim32.png"));
                button.Image = LoadImage(Path.Combine(dir, "Resources", "managebim16.png"));
            }
            var send = new PushButtonData("ManageBIMSend", "Send to\nManageBIM", asm, typeof(SendCommand).FullName)
            {
                ToolTip = "Send the 3D view straight into ManageBIM (opens it if needed). Send again to update the model live.",
                LongDescription = "Exports the active 3D view (or {3D}) without any dialog and opens it in ManageBIM. " +
                                  "If ManageBIM is already open with this project, the model is replaced in place, keeping the selection, hidden elements and the camera.",
            };
            if (panel.AddItem(send) is PushButton sb)
            {
                sb.LargeImage = LoadImage(Path.Combine(dir, "Resources", "managebim32.png"));
                sb.Image = LoadImage(Path.Combine(dir, "Resources", "managebim16.png"));
            }
            var issues = new PushButtonData("ManageBIMIssues", "ManageBIM\nIssues", asm, typeof(IssuesCommand).FullName)
            {
                ToolTip = "Open a ManageBIM issue / viewpoint: 3D view with the same camera and section box, its elements selected",
                LongDescription = "Reads the viewpoints saved in ManageBIM (they are shared automatically) and creates a 3D view 'ManageBIM - <issue>' with the camera, the section box and the issue's elements selected.",
            };
            if (panel.AddItem(issues) is PushButton ib)
            {
                ib.LargeImage = LoadImage(Path.Combine(dir, "Resources", "managebim32.png"));
                ib.Image = LoadImage(Path.Combine(dir, "Resources", "managebim16.png"));
            }
            var reference = new PushButtonData("ManageBIMReference", "ManageBIM\nReference", asm, typeof(ReferenceCommand).FullName)
            {
                ToolTip = "Bring a model from ManageBIM in as reference geometry (Generic Models, pinned group)",
                LongDescription = "In ManageBIM: Output › Reference (Revit / CAD) saves a .mbref file (the visible or the selected items). " +
                                  "This command reads it and creates one DirectShape per item, with its colour, in shared coordinates.",
            };
            if (panel.AddItem(reference) is PushButton rb)
            {
                rb.LargeImage = LoadImage(Path.Combine(dir, "Resources", "managebim32.png"));
                rb.Image = LoadImage(Path.Combine(dir, "Resources", "managebim16.png"));
            }
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
