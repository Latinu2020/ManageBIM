using System;
using System.Drawing;
using System.Windows.Forms;
using Autodesk.Navisworks.Api.Plugins;
using Control = System.Windows.Forms.Control;

namespace ManageBIM.Navis
{
    /// <summary>
    /// "ManageBIM": o fereastra cu toate comenzile add-in-ului (ca in Revit). In tab-ul Tool add-ins raman
    /// doar "Send to ManageBIM" si acesta; celelalte comenzi se ruleaza din fereastra.
    /// </summary>
    [Plugin("ManageBIMHub", "MBIM", DisplayName = "ManageBIM", ToolTip = "All ManageBIM commands in one window")]
    [AddInPlugin(AddInLocation.AddIn)]
    public class HubPlugin : AddInPlugin
    {
        private static readonly (string Group, string Title, string Desc, Func<AddInPlugin> Make)[] Items =
        {
            ("Send", "Send to ManageBIM", "Send the open model; send again to update it live", () => new SendPlugin()),
            ("Send", "Export BIMC…", "Save the model as a .bimc file (geometry, colours, all properties)", () => new ExportPlugin()),
            ("Coordination", "Coordination objects", "Show clearance zones, voids and placeholders from ManageBIM (display only)", () => new CoordinationPlugin()),
            ("Coordination", "Issues", "Bring ManageBIM viewpoints / issues in as Saved Viewpoints", () => new IssuesPlugin()),
            ("Coordination", "Walk path", "Turn the walk path drawn in ManageBIM into a viewpoint animation (Saved Viewpoints)", () => new WalkPlugin()),
            ("Coordination", "Levels", "Levels from ManageBIM as selection sets and plan viewpoints", () => new LevelsPlugin()),
            ("Live Link", "Sync selection", "Selection follows ManageBIM and Revit, both ways (click again to stop)", () => new SyncPlugin()),
        };

        public override int Execute(params string[] parameters)
        {
            int pick = Show();
            return pick < 0 ? 0 : Items[pick].Make().Execute(parameters);
        }

        private static int Show()
        {
            int chosen = -1;
            var accent = Color.FromArgb(37, 99, 235);
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
                BackColor = Color.FromArgb(248, 249, 251),
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
                        flow.Controls.Add(new Label { Text = group.ToUpperInvariant(), AutoSize = true, ForeColor = accent, Font = new Font("Segoe UI", 8f, FontStyle.Bold), Margin = new Padding(2, i == 0 ? 0 : 10, 0, 3) });
                    }
                    int idx = i;
                    var b = new Button
                    {
                        Width = 410,
                        Height = 46,
                        FlatStyle = FlatStyle.Flat,
                        BackColor = Color.White,
                        TextAlign = ContentAlignment.MiddleLeft,
                        Padding = new Padding(8, 0, 0, 0),
                        Margin = new Padding(0, 0, 0, 4),
                        Text = it.Title + "\n" + it.Desc,
                        Cursor = Cursors.Hand,
                    };
                    b.FlatAppearance.BorderColor = Color.FromArgb(222, 226, 232);
                    b.FlatAppearance.MouseOverBackColor = Color.FromArgb(235, 242, 255);
                    b.Click += (s, e) => { chosen = idx; form.Close(); };
                    flow.Controls.Add(b);
                }
                form.Controls.Add(flow);
                form.KeyDown += (s, e) => { if (e.KeyCode == Keys.Escape) form.Close(); };
                int h = 0;
                foreach (Control c in flow.Controls) h += c.Height + c.Margin.Vertical;
                form.ClientSize = new Size(form.ClientSize.Width, Math.Min(h + 30, Screen.PrimaryScreen.WorkingArea.Height - 80));
                form.ShowDialog();
            }
            return chosen;
        }
    }
}
