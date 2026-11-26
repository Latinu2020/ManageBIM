using System;
using Autodesk.Revit.UI;

namespace ManageBIM.Revit
{
    /// <summary>
    /// Guard around every ManageBIM command: an unexpected error never reaches Revit as an unhandled
    /// exception. The full error goes to %LOCALAPPDATA%\ManageBIM\addin.log and Revit shows a short message.
    /// Esc during a pick (OperationCanceledException) stays a normal cancel.
    /// </summary>
    internal static class Safe
    {
        public static Result Command(string name, Func<Result> run, out string error)
        {
            error = null;
            try
            {
                return run();
            }
            catch (Autodesk.Revit.Exceptions.OperationCanceledException)
            {
                return Result.Cancelled;
            }
            catch (Exception ex)
            {
                ManageBimLink.Log("Revit", name + " failed: " + ex);
                error = "ManageBIM: " + name + " stopped because of an error:\n" + ex.Message +
                        "\n\nDetails were saved to %LOCALAPPDATA%\\ManageBIM\\addin.log. Revit and your model are not affected.";
                return Result.Failed;
            }
        }

        /// <summary>For window events (paint, click, list changes): log the error instead of letting it close Revit.</summary>
        public static void Ui(string name, Action a)
        {
            try { a(); }
            catch (Exception ex) { ManageBimLink.Log("Revit", name + " (window) failed: " + ex); }
        }
    }
}
