using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Reflection;

namespace ManageBIM.Navis.Cli
{
    /// <summary>
    /// nwd2bimc model.nwd [iesire.bimc]
    /// nwd2bimc [-o folder] [-f] fisier1.nwd fisier2.nwc folder\ ...
    ///
    /// Porneste Navisworks o singura data in fundal (Automation API oficial), deschide pe rand fiecare
    /// fisier si ruleaza add-in-ul ManageBIM "Export to .bimc". Pentru un folder: toate .nwd / .nwc / .nwf din el.
    /// Fisierele al caror .bimc e mai nou decat sursa sunt sarite (-f le reface).
    /// Necesita Navisworks Manage / Simulate instalat si licentiat. Folosit si de ManageBIM (File > Open .nwd).
    /// Coduri de iesire: 0 OK, 1 utilizare, 2 Navisworks lipsa, 3 cel putin un export esuat.
    /// </summary>
    internal static class Program
    {
        private static string nwDir;
        private static readonly string[] Exts = { ".nwd", ".nwc", ".nwf" };

        [STAThread]
        private static int Main(string[] args)
        {
            if (args.Length < 1 || args[0] == "-h" || args[0] == "--help" || args[0] == "/?")
            {
                Console.WriteLine("Usage: nwd2bimc <model.nwd|.nwc|.nwf|.rvt|...> [output.bimc]");
                Console.WriteLine("       nwd2bimc [-o <output folder>] [-f] <files or folders>...");
                Console.WriteLine("  -o  write the .bimc files into this folder (default: next to each model)");
                Console.WriteLine("  -f  convert again even if the .bimc is newer than the model");
                Console.WriteLine("  --year YYYY  use this Navisworks version (default: the newest supported one with the ManageBIM add-in)");
                return 1;
            }
            // --- argumente
            var jobs = new List<(string inp, string outp)>();
            string outDir = null;
            bool force = false;
            int year = 0;
            var inputs = new List<string>();
            for (int i = 0; i < args.Length; i++)
            {
                if (args[i] == "-o" && i + 1 < args.Length) { outDir = Path.GetFullPath(args[++i]); continue; }
                if (args[i] == "-f") { force = true; continue; }
                if (args[i] == "--year" && i + 1 < args.Length) { int.TryParse(args[++i], out year); continue; }
                inputs.Add(args[i]);
            }
            if (inputs.Count == 2 && inputs[1].EndsWith(".bimc", StringComparison.OrdinalIgnoreCase) && File.Exists(inputs[0]))
            {
                // forma clasica: intrare + iesire (folosita de ManageBIM) - convertim mereu
                jobs.Add((Path.GetFullPath(inputs[0]), Path.GetFullPath(inputs[1])));
                force = true;
            }
            else
            {
                foreach (string a in inputs)
                {
                    var files = Directory.Exists(a)
                        ? Directory.GetFiles(a).Where(f => Exts.Contains(Path.GetExtension(f).ToLowerInvariant())).OrderBy(f => f)
                        : (IEnumerable<string>)new[] { a };
                    foreach (string f in files)
                    {
                        string full = Path.GetFullPath(f);
                        if (!File.Exists(full)) { Console.Error.WriteLine("File not found: " + full); continue; }
                        string o = outDir != null ? Path.Combine(outDir, Path.GetFileNameWithoutExtension(full) + ".bimc") : Path.ChangeExtension(full, ".bimc");
                        jobs.Add((full, o));
                    }
                }
            }
            if (outDir != null) Directory.CreateDirectory(outDir);
            if (!force)
            {
                int before = jobs.Count;
                jobs = jobs.Where(j => !File.Exists(j.outp) || File.GetLastWriteTimeUtc(j.outp) < File.GetLastWriteTimeUtc(j.inp)).ToList();
                if (before > jobs.Count) Console.WriteLine($"{before - jobs.Count} file(s) already up to date (use -f to convert again).");
            }
            if (jobs.Count == 0) { Console.WriteLine("Nothing to convert."); return 0; }

            nwDir = FindNavisworks(year);
            if (nwDir == null) { Console.Error.WriteLine("Autodesk Navisworks Manage / Simulate is not installed."); return 2; }
            // DLL-urile Navisworks se incarca din folderul de instalare
            AppDomain.CurrentDomain.AssemblyResolve += (s, e) =>
            {
                string f = Path.Combine(nwDir, new AssemblyName(e.Name).Name + ".dll");
                return File.Exists(f) ? Assembly.LoadFrom(f) : null;
            };
            return Run(jobs);
        }

        // separat de Main: tipurile Navisworks se rezolva abia dupa ce AssemblyResolve e activ
        [System.Runtime.CompilerServices.MethodImpl(System.Runtime.CompilerServices.MethodImplOptions.NoInlining)]
        private static int Run(List<(string inp, string outp)> jobs)
        {
            int ok = 0, failed = 0;
            var t0 = DateTime.Now;
            try
            {
                Console.WriteLine($"Starting Navisworks ({jobs.Count} file(s))...");
                using (var app = new Autodesk.Navisworks.Api.Automation.NavisworksApplication())
                {
                    app.DisableProgress();
                    foreach (var (input, output) in jobs)
                    {
                        var t = DateTime.Now;
                        try
                        {
                            if (File.Exists(output + ".log")) File.Delete(output + ".log");
                            Console.WriteLine("Opening " + Path.GetFileName(input) + "...");
                            Console.WriteLine("PROGRESS 0.000\tOpening " + Path.GetFileName(input) + " in Navisworks");
                            app.OpenFile(input);
                            Console.WriteLine("PROGRESS 0.000\tReading the model structure");
                            // progresul exportului: add-in-ul scrie <iesire>.progress, noi il afisam (citit de ManageBIM)
                            using (var stop = new System.Threading.ManualResetEventSlim(false))
                            {
                                var watcher = new System.Threading.Thread(() =>
                                {
                                    string last = null;
                                    while (!stop.Wait(500))
                                    {
                                        try
                                        {
                                            string p = File.Exists(output + ".progress") ? File.ReadAllText(output + ".progress") : null;
                                            if (p != null && p != last) { Console.WriteLine("PROGRESS " + p); last = p; }
                                        }
                                        catch { }
                                    }
                                }) { IsBackground = true };
                                watcher.Start();
                                try { app.ExecuteAddInPlugin("ExportBimc.MBIM", output); }
                                finally { stop.Set(); watcher.Join(1000); }
                            }
                            string log = File.Exists(output + ".log") ? File.ReadAllText(output + ".log") : "";
                            if (File.Exists(output) && log.StartsWith("OK"))
                            {
                                Console.WriteLine($"  {log.Split('\n')[0].Trim()} -> {output}  ({(DateTime.Now - t).TotalSeconds:0} s)");
                                ok++;
                            }
                            else
                            {
                                Console.Error.WriteLine("  " + (string.IsNullOrEmpty(log) ? "Export failed: is the ManageBIM add-in installed in Navisworks? Run AddIns\\ManageBIM-Navisworks-AddIn.exe." : log.Trim()));
                                failed++;
                            }
                        }
                        catch (Exception ex)
                        {
                            Console.Error.WriteLine($"  {Path.GetFileName(input)}: {ex.Message}");
                            failed++;
                        }
                    }
                }
            }
            catch (Exception ex)
            {
                Console.Error.WriteLine("Navisworks could not be started: " + ex.Message);
                return 3;
            }
            if (jobs.Count > 1) Console.WriteLine($"Done: {ok} converted, {failed} failed, {(DateTime.Now - t0).TotalSeconds:0} s in total.");
            return failed == 0 ? 0 : 3;
        }

        /// <summary>
        /// Versiunea de Navisworks folosita: cea ceruta (--year), altfel cea mai noua SUPORTATA (<= 2026) in care add-in-ul ManageBIM e instalat (fiecare versiune
        /// are propriul DLL); daca nu e instalat nicaieri, cea mai noua versiune (mesajul de eroare spune ce lipseste).
        /// </summary>
        private static string FindNavisworks(int year)
        {
            string pf = Environment.GetFolderPath(Environment.SpecialFolder.ProgramFiles);
            string appData = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData);
            string newest = null;
            // ordinea: versiunea ceruta; apoi cele suportate (2026 -> 2022); apoi cele mai noi (2027+, inca nesuportate de API)
            var order = new List<int>();
            if (year > 0) order.Add(year);
            for (int v = 2026; v >= 2020; v--) if (!order.Contains(v)) order.Add(v);
            for (int v = 2027; v <= 2035; v++) if (!order.Contains(v)) order.Add(v);
            foreach (int v in order)
                foreach (string ed in new[] { "Manage", "Simulate" })
                {
                    string d = Path.Combine(pf, "Autodesk", $"Navisworks {ed} {v}");
                    if (!File.Exists(Path.Combine(d, "Autodesk.Navisworks.Automation.dll"))) continue;
                    newest = newest ?? d;
                    foreach (string root in new[] { Path.Combine(appData, "Autodesk", $"Navisworks {ed} {v}"), Path.Combine(appData, $"Autodesk Navisworks {ed} {v}") })
                        if (File.Exists(Path.Combine(root, "Plugins", "ManageBIM.NwExporter", "ManageBIM.NwExporter.dll")))
                        {
                            Console.WriteLine($"Using Navisworks {ed} {v}");
                            return d;
                        }
                }
            if (newest != null) Console.Error.WriteLine("The ManageBIM add-in is not installed in any Navisworks version - run AddIns\\ManageBIM-Navisworks-AddIn.exe.");
            return newest;
        }
    }
}
