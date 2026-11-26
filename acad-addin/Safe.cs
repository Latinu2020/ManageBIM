using System;
using Autodesk.AutoCAD.ApplicationServices;
using AcApp = Autodesk.AutoCAD.ApplicationServices.Core.Application;

namespace ManageBIM.Acad
{
    /// <summary>
    /// Guard around every ManageBIM command: an unexpected error never reaches AutoCAD / Plant 3D as an
    /// unhandled exception (which can close AutoCAD). The full error goes to %LOCALAPPDATA%\ManageBIM\addin.log
    /// and a short message is written on the command line.
    /// </summary>
    internal static class Safe
    {
        public static void Command(string name, Action run)
        {
            try
            {
                run();
            }
            catch (Exception ex)
            {
                ManageBimLink.Log("AutoCAD", name + " failed: " + ex);
                try
                {
                    Document doc = AcApp.DocumentManager.MdiActiveDocument;
                    doc?.Editor.WriteMessage("\nManageBIM: " + name + " stopped because of an error: " + ex.Message +
                                             "\n  Details: %LOCALAPPDATA%\\ManageBIM\\addin.log. The drawing is not affected.\n");
                }
                catch { /* no editor (closing / console): the log is enough */ }
            }
        }
    }
}
