using System;
using System.Linq;
using System.Windows.Forms;

namespace ManageBIM.Navis
{
    /// <summary>
    /// Guard around every ManageBIM plugin: an unexpected error never reaches Navisworks as an unhandled
    /// exception (which can close Navisworks). The full error goes to %LOCALAPPDATA%\ManageBIM\addin.log;
    /// a short message is shown, except in "quiet" runs (Automation / tests). Returns 1 on error.
    /// </summary>
    internal static class Safe
    {
        public static int Plugin(string name, string[] parameters, Func<int> run)
        {
            try
            {
                return run();
            }
            catch (Exception ex)
            {
                ManageBimLink.Log("Navisworks", name + " failed: " + ex);
                bool quiet = parameters != null && parameters.Contains("quiet");
                if (!quiet)
                {
                    try
                    {
                        MessageBox.Show("ManageBIM: " + name + " stopped because of an error:\n" + ex.Message +
                                        "\n\nDetails were saved to %LOCALAPPDATA%\\ManageBIM\\addin.log. Navisworks and your model are not affected.",
                                        "ManageBIM", MessageBoxButtons.OK, MessageBoxIcon.Warning);
                    }
                    catch { }
                }
                return 1;
            }
        }

        /// <summary>For window events (paint, click, list changes): log the error instead of letting it close Navisworks.</summary>
        public static void Ui(string name, Action a)
        {
            try { a(); }
            catch (Exception ex) { ManageBimLink.Log("Navisworks", name + " (window) failed: " + ex); }
        }
    }
}
