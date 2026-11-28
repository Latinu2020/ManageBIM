#![cfg_attr(windows, windows_subsystem = "windows")]
use eframe::egui;

// the same drawing as the exe / installer icon (tools/icon_gen.rs)
include!("../tools/icon_gen.rs");

fn icon() -> egui::IconData {
    let n = 128u32;
    egui::IconData { rgba: icon_rgba(n), width: n, height: n }
}

fn run(files: Vec<String>, depth: u8, msaa: u16) -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("ManageBIM — BIM Coordination")
            .with_inner_size([1500.0, 920.0])
            .with_min_inner_size([900.0, 600.0])
            .with_drag_and_drop(true)
            .with_icon(icon()),
        depth_buffer: depth,
        multisampling: msaa,
        ..Default::default()
    };
    eframe::run_native(
        "ManageBIM",
        options,
        Box::new(move |cc| {
            bimrust::theme::install_fonts(&cc.egui_ctx);
            let mut app = bimrust::app::App::new(files);
            // autosave and recovery only in the graphical application (not in tests / converter)
            app.autosave = bimrust::app::Autosave::start();
            Ok(Box::new(app))
        }),
    )
}

/// Error message on screen (the application has no console).
fn error_box(text: &str) {
    #[cfg(windows)]
    {
        extern "system" {
            fn MessageBoxW(h: *mut std::ffi::c_void, text: *const u16, caption: *const u16, t: u32) -> i32;
        }
        let w = |s: &str| s.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
        unsafe {
            MessageBoxW(std::ptr::null_mut(), w(text).as_ptr(), w("ManageBIM").as_ptr(), 0x10);
        }
    }
    #[cfg(not(windows))]
    eprintln!("{text}");
}

/// On an internal error: the reason + the location in %APPDATA%\ManageBIM\crash.log and a message on screen
/// (otherwise a windowed application closes without any sign).
fn install_crash_log() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let bt = std::backtrace::Backtrace::force_capture();
        let msg = format!(
            "ManageBIM {} crashed at {:?}\n{}\nthread: {}\n\n{}\n",
            env!("CARGO_PKG_VERSION"),
            std::time::SystemTime::now(),
            info,
            std::thread::current().name().unwrap_or("?"),
            bt
        );
        let path = bimrust::formats::config_root().map(|a| a.join("ManageBIM").join("crash.log"));
        if let Some(p) = &path {
            if let Some(d) = p.parent() {
                let _ = std::fs::create_dir_all(d);
            }
            // the log does not grow forever: above 1 MB we keep only the last half
            if std::fs::metadata(p).map(|m| m.len() > 1_000_000).unwrap_or(false) {
                if let Ok(old) = std::fs::read(p) {
                    let tail = &old[old.len() / 2..];
                    let _ = std::fs::write(p, tail);
                }
            }
            use std::io::Write;
            if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(p) {
                let _ = writeln!(f, "{msg}");
            }
        }
        // the errors in the worker threads are caught (they become error messages); only the main thread closes the application
        if std::thread::current().name() == Some("main") {
            error_box(&format!("ManageBIM stopped because of an internal error:\n\n{info}\n\nDetails were saved to:\n{}", path.as_ref().map(|p| p.display().to_string()).unwrap_or_default()));
        }
        default(info);
    }));
}

/// Headless model checks (no window): `ManageBIM --check model.ifc [more models] [--ids spec.ids] [--out report.json]`.
/// Exit code: 0 = every check passed, 1 = failures, 2 = usage / load / IDS error. The JSON report is written
/// to `--out` (or printed); this is what a CI job or a scheduler reads.
fn check_cli(args: &[String]) -> i32 {
    // A windowed executable has no console of its own: show the progress in the console that started it.
    #[cfg(windows)]
    {
        extern "system" {
            fn AttachConsole(pid: u32) -> i32;
        }
        // ATTACH_PARENT_PROCESS
        unsafe {
            AttachConsole(0xFFFF_FFFF);
        }
    }
    let (mut ids_path, mut out) = (None::<String>, None::<String>);
    let mut models: Vec<String> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--check" => {}
            "--ids" => {
                i += 1;
                ids_path = args.get(i).cloned();
            }
            "--out" => {
                i += 1;
                out = args.get(i).cloned();
            }
            a if a.starts_with("--") => {
                eprintln!("ManageBIM --check: unknown option {a}");
                return 2;
            }
            a => models.push(a.to_string()),
        }
        i += 1;
    }
    if models.is_empty() {
        eprintln!("usage: ManageBIM --check <model> [more models] [--ids spec.ids] [--out report.json]");
        return 2;
    }
    let mut loaded = Vec::new();
    for (slot, path) in models.iter().enumerate() {
        let t = std::time::Instant::now();
        match bimrust::load_any(path, &|_, _| {}).map(|raw| bimrust::build_model(raw, path, glam::DVec3::ZERO)) {
            Ok(m) => {
                eprintln!("{path}: {} elements ({:.1} s)", m.elements.len(), t.elapsed().as_secs_f32());
                loaded.push((slot, m));
            }
            Err(e) => {
                eprintln!("{path}: {e}");
                return 2;
            }
        }
    }
    let refs: Vec<(usize, &bimrust::model::Model)> = loaded.iter().map(|(i, m)| (*i, m)).collect();
    let failure_json = |id: bimrust::model::ElemId, why: &str| {
        let e = loaded.get(id.m as usize).and_then(|(_, m)| m.elements.get(id.e as usize));
        serde_json::json!({
            "model": id.m,
            "element": id.e,
            "name": e.map(|e| e.name.clone()).unwrap_or_default(),
            "type": e.map(|e| e.ifc_type.clone()).unwrap_or_default(),
            "level": e.map(|e| e.storey.clone()).unwrap_or_default(),
            "guid": e.map(|e| e.guid.clone()).unwrap_or_default(),
            "reason": why,
        })
    };
    let quick = bimrust::check::quick_checks(&refs);
    let quick_json: Vec<serde_json::Value> = quick
        .iter()
        .map(|c| {
            serde_json::json!({
                "check": c.name,
                "about": c.about,
                "checked": c.result.checked,
                "failed": c.result.failed.len(),
                "failures": c.result.failed.iter().map(|(id, why)| failure_json(*id, why)).collect::<Vec<_>>(),
            })
        })
        .collect();
    let mut rules_json: Vec<serde_json::Value> = Vec::new();
    if let Some(p) = &ids_path {
        let txt = match std::fs::read_to_string(p) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("{p}: {e}");
                return 2;
            }
        };
        match bimrust::check::parse_ids(&txt, &bimrust::file_name(p)) {
            Ok((rules, warn)) => {
                if !warn.is_empty() {
                    eprintln!("IDS warnings: {}", warn.join("; "));
                }
                let res = bimrust::check::run(&rules, &refs);
                for (r, x) in rules.iter().zip(res.iter()) {
                    rules_json.push(serde_json::json!({
                        "rule": r.name,
                        "source": r.source,
                        "checked": x.checked,
                        "failed": x.failed.len(),
                        "failures": x.failed.iter().map(|(id, why)| failure_json(*id, why)).collect::<Vec<_>>(),
                    }));
                }
            }
            Err(e) => {
                eprintln!("{p}: {e}");
                return 2;
            }
        }
    }
    let failed: usize = quick.iter().map(|c| c.result.failed.len()).sum::<usize>()
        + rules_json.iter().map(|r| r["failed"].as_u64().unwrap_or(0) as usize).sum::<usize>();
    let report = serde_json::json!({
        "version": env!("CARGO_PKG_VERSION"),
        "ok": failed == 0,
        "failed": failed,
        "models": loaded.iter().map(|(_, m)| serde_json::json!({
            "name": m.name, "file": m.path, "elements": m.elements.len(),
            "triangles": m.pos.len() / 3, "warnings": m.warnings,
        })).collect::<Vec<_>>(),
        "quick": quick_json,
        "rules": rules_json,
    });
    let text = serde_json::to_string_pretty(&report).unwrap_or_default();
    match out {
        Some(p) => match std::fs::write(&p, text) {
            Ok(()) => eprintln!("Report: {p}"),
            Err(e) => {
                eprintln!("{p}: {e}");
                return 2;
            }
        },
        None => println!("{text}"),
    }
    eprintln!("Model check: {} failure(s) -> {}", failed, if failed == 0 { "PASS" } else { "FAIL" });
    if failed == 0 { 0 } else { 1 }
}

fn main() -> eframe::Result<()> {
    // MCP stdio bridge for Claude Desktop / OpenCode (no window): talks to the already running ManageBIM
    if std::env::args().nth(1).as_deref() == Some("--mcp-stdio") {
        std::process::exit(bimrust::mcp::stdio_bridge());
    }
    // headless model checks for CI / a scheduler (no window, no graphics card needed)
    let all: Vec<String> = std::env::args().skip(1).collect();
    if all.iter().any(|a| a == "--check") {
        std::process::exit(check_cli(&all));
    }
    install_crash_log();
    let files: Vec<String> = std::env::args().skip(1).collect();
    // software OpenGL (Mesa) when asked: the automatic restart below, or Settings › Software rendering
    if bimrust::softgl::requested() || bimrust::theme::Settings::load().software_gl {
        bimrust::softgl::enable();
    }
    // tries simpler and simpler GL configurations (old video cards, VM, Remote Desktop)
    let mut last = Ok(());
    for (depth, msaa) in [(24u8, 4u16), (24, 0), (16, 0)] {
        match run(files.clone(), depth, msaa) {
            Ok(()) => return Ok(()),
            Err(e) => {
                let s = e.to_string();
                eprintln!("GL configuration (depth {depth}, MSAA {msaa}) unavailable: {s}");
                last = Err(e);
                if !(s.contains("Glutin") || s.contains("Config") || s.contains("config")) {
                    break;
                }
            }
        }
    }
    // no configuration worked on the graphics card: start again once with software OpenGL (Mesa)
    if last.is_err() && !bimrust::softgl::active() && bimrust::softgl::available() && bimrust::softgl::restart_with_software().is_ok() {
        return Ok(());
    }
    // no configuration worked: an explanation instead of closing without any sign
    if let Err(e) = &last {
        error_box(&format!(
            "ManageBIM could not open its 3D window.\n\n\
             ManageBIM needs a graphics card / driver with OpenGL 3.3 or newer.\n\
             - Update the graphics driver (Intel / AMD / NVIDIA website).\n\
             - On a laptop with two graphics cards, run ManageBIM on the dedicated one.\n\
             - Over Remote Desktop or in a virtual machine, enable 3D acceleration.\n\n\
             Details: {e}"
        ));
    }
    last
}
