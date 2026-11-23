using System;
using System.Diagnostics;
using System.IO;

namespace ManageBIM
{
    /// <summary>
    /// "Send to ManageBIM": modelul se scrie in %LOCALAPPDATA%\ManageBIM\Inbox\&lt;sursa&gt;.bimc.
    /// ManageBIM urmareste folderul: un fisier nou se deschide singur, unul rescris se actualizeaza live
    /// (pastrand selectia si vederea). Daca ManageBIM nu ruleaza, il pornim cu fisierul.
    /// Acelasi fisier e copiat in toate add-in-urile (Revit / AutoCAD / Navisworks).
    /// </summary>
    internal static class ManageBimLink
    {
        public static string InboxDir()
        {
            // MANAGEBIM_INBOX: alt folder (teste automate)
            string d = Environment.GetEnvironmentVariable("MANAGEBIM_INBOX");
            if (string.IsNullOrEmpty(d)) d = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "ManageBIM", "Inbox");
            Directory.CreateDirectory(d);
            return d;
        }

        /// <summary>Calea finala din Inbox pentru o sursa (ex. "Revit - Spital").</summary>
        public static string InboxPath(string name)
        {
            foreach (char c in Path.GetInvalidFileNameChars()) name = name.Replace(c, '_');
            if (string.IsNullOrWhiteSpace(name)) name = "Model";
            return Path.Combine(InboxDir(), name.Trim() + ".bimc");
        }

        /// <summary>Fisierul temporar in care se scrie (extensia .tmp e ignorata de ManageBIM).</summary>
        public static string TempPath(string final) => final + ".tmp";

        /// <summary>Log local (fara server) pentru diagnosticarea exporturilor add-in-urilor.</summary>
        public static void Log(string app, string msg)
        {
            try
            {
                string dir = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "ManageBIM");
                Directory.CreateDirectory(dir);
                File.AppendAllText(Path.Combine(dir, "addin.log"),
                    DateTime.Now.ToString("yyyy-MM-dd HH:mm:ss") + " [" + app + "] " + msg + Environment.NewLine);
            }
            catch { }
        }

        /// <summary>Inlocuieste atomic fisierul final cu cel temporar, ca ManageBIM sa nu citeasca un fisier pe jumatate.</summary>
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
                    // ManageBIM poate citi chiar acum versiunea anterioara
                    System.Threading.Thread.Sleep(250);
                }
            }
        }

        /// <summary>true daca ManageBIM rula deja (va prelua singur fisierul din Inbox).</summary>
        public static bool EnsureRunning(string file)
        {
            if (Process.GetProcessesByName("ManageBIM").Length > 0) return true;
            if (!string.IsNullOrEmpty(Environment.GetEnvironmentVariable("MANAGEBIM_NO_LAUNCH"))) return false;
            string exe = FindExe();
            if (exe != null) Process.Start(new ProcessStartInfo(exe, "\"" + file + "\"") { UseShellExecute = false, WorkingDirectory = Path.GetDirectoryName(exe) });
            else Process.Start(new ProcessStartInfo(file) { UseShellExecute = true }); // asocierea .bimc
            return false;
        }

        private static string FindExe()
        {
            string local = Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData);
            string[] candidates =
            {
                Path.Combine(local, "Programs", "ManageBIM", "ManageBIM.exe"),
                // add-in-ul instalat din pachetul ManageBIM: ...\ManageBIM\AddIns\... -> ...\ManageBIM\ManageBIM.exe
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
