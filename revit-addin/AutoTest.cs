using System;
using System.IO;
using Autodesk.Revit.UI;
using Autodesk.Revit.UI.Events;

namespace ManageBIM.Revit
{
    /// <summary>
    /// Self-test for tools\test-addins.ps1 (Revit has no headless mode). Only when Revit is started with the
    /// environment variable MANAGEBIM_AUTOTEST=&lt;folder&gt; and a model: once the model is open and Revit is
    /// idle, the 3D view is exported exactly like "Send to ManageBIM" into &lt;folder&gt;\revit.bimc, the result
    /// goes to &lt;folder&gt;\revit.result ("OK &lt;elements&gt;" or "FAIL &lt;error&gt;") and Revit closes.
    /// Without the variable nothing is registered.
    /// </summary>
    internal static class AutoTest
    {
        private static string _dir;
        private static bool _done;

        public static void Register(UIControlledApplication app)
        {
            _dir = Environment.GetEnvironmentVariable("MANAGEBIM_AUTOTEST");
            if (string.IsNullOrEmpty(_dir)) return;
            app.Idling += OnIdling;
        }

        private static void OnIdling(object sender, IdlingEventArgs e)
        {
            if (_done || !(sender is UIApplication uiapp)) return;
            UIDocument uidoc = uiapp.ActiveUIDocument;
            if (uidoc == null) return; // the model is still opening
            _done = true;
            string result;
            try
            {
                Directory.CreateDirectory(_dir);
                var view = SendCommand.View3DToSend(uidoc);
                if (view == null) result = "FAIL the model has no 3D view";
                else
                {
                    int n = SendCommand.Export(uidoc.Document, view, Path.Combine(_dir, "revit.bimc"), uiapp.Application.VersionNumber);
                    result = "OK " + n + " elements from view '" + view.Name + "'";
                }
            }
            catch (Exception ex)
            {
                result = "FAIL " + ex.Message;
                ManageBimLink.Log("Revit", "AutoTest failed: " + ex);
            }
            try { File.WriteAllText(Path.Combine(_dir, "revit.result"), result); } catch { }
            // close Revit without saving (the model was only read)
            try { uiapp.PostCommand(RevitCommandId.LookupPostableCommandId(PostableCommand.ExitRevit)); } catch { }
        }
    }
}
