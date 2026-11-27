using System;
using System.Windows.Input;
using Autodesk.AutoCAD.ApplicationServices;
using Autodesk.AutoCAD.Runtime;
using Autodesk.Windows;
using AcApp = Autodesk.AutoCAD.ApplicationServices.Core.Application;

[assembly: ExtensionApplication(typeof(ManageBIM.Acad.Plugin))]
[assembly: CommandClass(typeof(ManageBIM.Acad.Commands))]
// the reference commands (MBREFERENCE / MBREFRELOAD / MBREFUNLOAD): without this line AutoCAD / Plant 3D does not see them
[assembly: CommandClass(typeof(ManageBIM.Acad.RefCommands))]
[assembly: CommandClass(typeof(ManageBIM.Acad.LevelCommands))]

namespace ManageBIM.Acad
{
    /// <summary>Adds a "ManageBIM" ribbon tab with the export button when AutoCAD starts.</summary>
    public class Plugin : IExtensionApplication
    {
        public void Initialize()
        {
            try
            {
                if (ComponentManager.Ribbon != null) AddRibbon();
                else ComponentManager.ItemInitialized += OnItemInitialized;
            }
            catch { /* console-only AutoCAD (accoreconsole): no ribbon */ }
            try
            {
                AcApp.DocumentManager.MdiActiveDocument?.Editor.WriteMessage("\nManageBIM exporter loaded - command BIMCEXPORT (ribbon tab ManageBIM).\n");
            }
            catch { }
        }

        public void Terminate() { }

        private void OnItemInitialized(object sender, RibbonItemEventArgs e)
        {
            if (ComponentManager.Ribbon == null) return;
            ComponentManager.ItemInitialized -= OnItemInitialized;
            AddRibbon();
        }

        private static void AddRibbon()
        {
            RibbonControl ribbon = ComponentManager.Ribbon;
            foreach (RibbonTab t in ribbon.Tabs) if (t.Id == "MANAGEBIM_TAB") return;
            var tab = new RibbonTab { Title = "ManageBIM", Id = "MANAGEBIM_TAB" };
            var src = new RibbonPanelSource { Title = "Export" };
            var btn = new RibbonButton
            {
                Text = "Export\nBIMC",
                ShowText = true,
                ShowImage = true,
                Size = RibbonItemSize.Large,
                Orientation = System.Windows.Controls.Orientation.Vertical,
                CommandParameter = "_.BIMCEXPORT ",
                CommandHandler = new SendCommand(),
                ToolTip = "Export the 3D model to a ManageBIM model cache (.bimc) with layers, colours, blocks and attributes",
                LargeImage = Icons.Large(),
                Image = Icons.Small(),
            };
            src.Items.Add(btn);
            src.Items.Add(new RibbonButton
            {
                Text = "Send to\nManageBIM",
                ShowText = true,
                ShowImage = true,
                Size = RibbonItemSize.Large,
                Orientation = System.Windows.Controls.Orientation.Vertical,
                CommandParameter = "_.BIMCSEND ",
                CommandHandler = new SendCommand(),
                ToolTip = "Send the whole 3D model straight into ManageBIM (opens it if needed). Send again to update the model live in ManageBIM.",
                LargeImage = Icons.Large(),
                Image = Icons.Small(),
            });
            tab.Panels.Add(new RibbonPanel { Source = src });
            // references from ManageBIM (.mbref): attach / reload / detach, as with XREF
            var refs = new RibbonPanelSource { Title = "Reference" };
            foreach (var (text, cmd, tip) in new[]
            {
                ("Attach\nReference", "_.MBREFERENCE ", "Attach a model from ManageBIM (Output › Reference, .mbref) as 3D meshes on a locked layer"),
                ("Reload\nReference", "_.MBREFRELOAD ", "Reload a ManageBIM reference after updating the .mbref file in ManageBIM"),
                ("Detach\nReference", "_.MBREFUNLOAD ", "Remove a ManageBIM reference (its meshes and layer)"),
            })
            {
                refs.Items.Add(new RibbonButton
                {
                    Text = text,
                    ShowText = true,
                    ShowImage = true,
                    Size = RibbonItemSize.Large,
                    Orientation = System.Windows.Controls.Orientation.Vertical,
                    CommandParameter = cmd,
                    CommandHandler = new SendCommand(),
                    ToolTip = tip,
                    LargeImage = Icons.Large(),
                    Image = Icons.Small(),
                });
            }
            tab.Panels.Add(new RibbonPanel { Source = refs });
            ribbon.Tabs.Add(tab);
        }
    }

    internal class SendCommand : ICommand
    {
#pragma warning disable 67
        public event EventHandler CanExecuteChanged;
#pragma warning restore 67
        public bool CanExecute(object parameter) => true;
        public void Execute(object parameter)
        {
            Document doc = AcApp.DocumentManager.MdiActiveDocument;
            if (doc == null) return;
            string cmd = (parameter as RibbonButton)?.CommandParameter as string ?? "_.BIMCEXPORT ";
            doc.SendStringToExecute(cmd, true, false, false);
        }
    }

    /// <summary>Ribbon icons drawn in code (no resource files to lose).</summary>
    internal static class Icons
    {
        public static System.Windows.Media.ImageSource Large() => Draw(32);
        public static System.Windows.Media.ImageSource Small() => Draw(16);

        private static System.Windows.Media.ImageSource Draw(int n)
        {
            var dv = new System.Windows.Media.DrawingVisual();
            using (var dc = dv.RenderOpen())
            {
                var blue = new System.Windows.Media.SolidColorBrush(System.Windows.Media.Color.FromRgb(59, 130, 246));
                var dark = new System.Windows.Media.SolidColorBrush(System.Windows.Media.Color.FromRgb(30, 64, 140));
                double s = n / 32.0;
                dc.DrawRoundedRectangle(blue, null, new System.Windows.Rect(2 * s, 2 * s, 28 * s, 28 * s), 5 * s, 5 * s);
                var pen = new System.Windows.Media.Pen(System.Windows.Media.Brushes.White, 2.2 * s);
                // small isometric box
                var p = new System.Windows.Media.PointCollection();
                dc.DrawLine(pen, new System.Windows.Point(16 * s, 7 * s), new System.Windows.Point(25 * s, 12 * s));
                dc.DrawLine(pen, new System.Windows.Point(16 * s, 7 * s), new System.Windows.Point(7 * s, 12 * s));
                dc.DrawLine(pen, new System.Windows.Point(7 * s, 12 * s), new System.Windows.Point(16 * s, 17 * s));
                dc.DrawLine(pen, new System.Windows.Point(25 * s, 12 * s), new System.Windows.Point(16 * s, 17 * s));
                dc.DrawLine(pen, new System.Windows.Point(7 * s, 12 * s), new System.Windows.Point(7 * s, 21 * s));
                dc.DrawLine(pen, new System.Windows.Point(25 * s, 12 * s), new System.Windows.Point(25 * s, 21 * s));
                dc.DrawLine(pen, new System.Windows.Point(16 * s, 17 * s), new System.Windows.Point(16 * s, 26 * s));
                dc.DrawLine(pen, new System.Windows.Point(7 * s, 21 * s), new System.Windows.Point(16 * s, 26 * s));
                dc.DrawLine(pen, new System.Windows.Point(25 * s, 21 * s), new System.Windows.Point(16 * s, 26 * s));
                _ = dark; _ = p;
            }
            var bmp = new System.Windows.Media.Imaging.RenderTargetBitmap(n, n, 96, 96, System.Windows.Media.PixelFormats.Pbgra32);
            bmp.Render(dv);
            bmp.Freeze();
            return bmp;
        }
    }
}
