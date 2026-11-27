using System;
using System.Diagnostics;
using System.IO;

namespace ManageBIM
{
    /// <summary>
    /// "Send to ManageBIM": the model is written to %LOCALAPPDATA%\ManageBIM\Inbox\&lt;source&gt;.bimc.
    /// ManageBIM watches the folder: a new file opens on its own, a rewritten one is updated live
    /// (keeping the selection and the view). If ManageBIM is not running, we start it with the file.
    /// The same file is copied into all add-ins (Revit / AutoCAD / Navisworks).
    /// </summary>
    internal static class ManageBimLink
    {
        public static string InboxDir()
        {
            // MANAGEBIM_INBOX: another folder (automated tests)
            string d = Environment.GetEnvironmentVariable("MANAGEBIM_INBOX");
            if (string.IsNullOrEmpty(d)) d = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "ManageBIM", "Inbox");
            Directory.CreateDirectory(d);
            return d;
        }

        /// <summary>The final path in Inbox for a source (e.g. "Revit - Spital").</summary>
        public static string InboxPath(string name)
        {
            foreach (char c in Path.GetInvalidFileNameChars()) name = name.Replace(c, '_');
            if (string.IsNullOrWhiteSpace(name)) name = "Model";
            return Path.Combine(InboxDir(), name.Trim() + ".bimc");
        }

        /// <summary>The temporary file being written (the .tmp extension is ignored by ManageBIM).</summary>
        public static string TempPath(string final) => final + ".tmp";

        /// <summary>Local log (no server) for diagnosing the add-in exports.</summary>
        public static void Log(string app, string msg)
        {
            try
            {
                string dir = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "ManageBIM");
                Directory.CreateDirectory(dir);
                File.AppendAllText(Path.Combine(dir, "addin.log"),
                    DateTime.Now.ToString("yyyy-MM-dd HH:mm:ss") + " [" + app + "] " + msg + Environment.NewLine);
            }
            catch { /* the log must not stop the export */ }
        }

        /// <summary>Atomically replaces the final file with the temporary one, so ManageBIM does not read a half-written file.</summary>
        public static void Commit(string tmp, string final)
        {
            for (int i = 0; ; i++)
            {
                try
                {
                    if (File.Exists(final)) File.Replace(tmp, final, null, true);
                    else File.Move(tmp, final);
                    return;
                }
                catch (IOException) when (i < 20)
                {
                    // ManageBIM may be reading the previous version right now
                    System.Threading.Thread.Sleep(250);
                }
            }
        }

        /// <summary>true if ManageBIM was already running (it will pick up the file from Inbox by itself).</summary>
        public static bool EnsureRunning(string file)
        {
            if (Process.GetProcessesByName("ManageBIM").Length > 0) return true;
            if (!string.IsNullOrEmpty(Environment.GetEnvironmentVariable("MANAGEBIM_NO_LAUNCH"))) return false;
            string exe = FindExe();
            if (exe != null) Process.Start(new ProcessStartInfo(exe, "\"" + file + "\"") { UseShellExecute = false, WorkingDirectory = Path.GetDirectoryName(exe) });
            else Process.Start(new ProcessStartInfo(file) { UseShellExecute = true }); // the .bimc association
            return false;
        }

        private static string FindExe()
        {
            string local = Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData);
            string[] candidates =
            {
                Path.Combine(local, "Programs", "ManageBIM", "ManageBIM.exe"),
                // the add-in installed from the ManageBIM package: ...\ManageBIM\AddIns\... -> ...\ManageBIM\ManageBIM.exe
                Path.Combine(Path.GetDirectoryName(typeof(ManageBimLink).Assembly.Location) ?? "", "..", "..", "ManageBIM.exe"),
            };
            foreach (string c in candidates)
            {
                try { if (File.Exists(c)) return Path.GetFullPath(c); } catch { }
            }
            return null;
        }
    }
}
