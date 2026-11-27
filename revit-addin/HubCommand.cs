using System;
using System.Drawing;
using System.Windows.Forms;
using Autodesk.Revit.Attributes;
using Autodesk.Revit.DB;
using Autodesk.Revit.UI;
using Form = System.Windows.Forms.Form;
using TaskDialog = Autodesk.Revit.UI.TaskDialog;
using Control = System.Windows.Forms.Control;

namespace ManageBIM.Revit
{
    /// <summary>
    /// "ManageBIM": a window with all the add-in commands, grouped (Send, Issues, Coordination,
    /// Reference, Live Link), each with a short description. The ribbon stays compact; the chosen command
    /// runs in the same context (the window is modal, so the Revit API is available).
    /// </summary>
    [Transaction(TransactionMode.Manual)]
    [Regeneration(RegenerationOption.Manual)]
    public class HubCommand : IExternalCommand
    {
        private static readonly (string Group, string Title, string Desc, Func<IExternalCommand> Make)[] Items =
        {
            ("Send", "Send to ManageBIM", "Send the whole model; send again to update it live", () => new SendCommand()),
            ("Send", "Send selection", "Only the selected elements (quick clash check of one area)", () => new SendSelectionCommand()),
            ("Send", "Export BIMC…", "Save the model as a .bimc file", () => new ExportCommand()),
            ("Coordination", "Coordination objects", "Show clearance zones, voids and placeholders from ManageBIM (display only)", () => new CoordinationCommand()),
            ("Coordination", "Issues", "Open a ManageBIM issue: 3D view with its camera, section box and elements", () => new IssuesCommand()),
            ("Reference", "ManageBIM reference", "Bring a ManageBIM model in as pinned reference geometry", () => new ReferenceCommand()),
            ("Live Link", "Sync selection", "Selection follows ManageBIM and Navisworks, both ways (click again to stop)", () => new SyncCommand()),
        };

        public Result Execute(ExternalCommandData commandData, ref string message, ElementSet elements)
        {
            string msg = message;
            Result r = Safe.Command(GetType().Name.Replace("Command", ""), () => Run(commandData, ref msg, elements), out string error);
            message = error ?? msg;
            return r;
        }

        private Result Run(ExternalCommandData commandData, ref string message, ElementSet elements)
        {
            int pick = Show();
            if (pick < 0) return Result.Cancelled;
            return Items[pick].Make().Execute(commandData, ref message, elements);
        }

        /// <summary>Compact window: one wide button per command, with title and description, by group.</summary>
        private static int Show()
        {
            int chosen = -1;
            var accent = System.Drawing.Color.FromArgb(37, 99, 235);
            using (var form = new Form
            {
                Text = "ManageBIM",
                Width = 460,
                StartPosition = FormStartPosition.CenterScreen,
                FormBorderStyle = FormBorderStyle.FixedDialog,
                MinimizeBox = false,
                MaximizeBox = false,
                ShowInTaskbar = false,
                Font = new Font("Segoe UI", 9f),
                BackColor = System.Drawing.Color.FromArgb(248, 249, 251),
                KeyPreview = true,
            })
            {
                var flow = new FlowLayoutPanel { Dock = DockStyle.Fill, FlowDirection = FlowDirection.TopDown, WrapContents = false, AutoScroll = true, Padding = new Padding(12, 10, 12, 10) };
                string group = null;
                for (int i = 0; i < Items.Length; i++)
                {
                    var it = Items[i];
                    if (it.Group != group)
                    {
                        group = it.Group;
                        flow.Controls.Add(new Label { Text = group.ToUpperInvariant(), AutoSize = true, ForeColor = accent, Font = new Font("Segoe UI", 8f, FontStyle.Bold), Margin = new Padding(2, group == Items[0].Group ? 0 : 10, 0, 3) });
                    }
                    int idx = i;
                    var b = new Button
                    {
                        Width = 410,
                        Height = 46,
                        FlatStyle = FlatStyle.Flat,
                        BackColor = System.Drawing.Color.White,
                        TextAlign = ContentAlignment.MiddleLeft,
                        Padding = new Padding(8, 0, 0, 0),
                        Margin = new Padding(0, 0, 0, 4),
                        Text = it.Title + "\n" + it.Desc,
                        Cursor = Cursors.Hand,
                    };
                    b.FlatAppearance.BorderColor = System.Drawing.Color.FromArgb(222, 226, 232);
                    b.FlatAppearance.MouseOverBackColor = System.Drawing.Color.FromArgb(235, 242, 255);
                    b.Click += (s, e) => { chosen = idx; form.Close(); };
                    flow.Controls.Add(b);
                }
                form.Controls.Add(flow);
                form.KeyDown += (s, e) => { if (e.KeyCode == Keys.Escape) form.Close(); };
                // height by content (no empty space), capped at the screen
                int h = 0;
                foreach (Control c in flow.Controls) h += c.Height + c.Margin.Vertical;
                form.ClientSize = new Size(form.ClientSize.Width, Math.Min(h + 30, Screen.PrimaryScreen.WorkingArea.Height - 80));
                form.ShowDialog();
            }
            return chosen;
        }
    }
}
