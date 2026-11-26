#![cfg_attr(windows, windows_subsystem = "windows")]
use eframe::egui;

// acelasi desen ca iconita exe-ului / installerului (tools/icon_gen.rs)
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
            // salvarea automata si recuperarea doar in aplicatia grafica (nu in teste / convertor)
            app.autosave = bimrust::app::Autosave::start();
            Ok(Box::new(app))
        }),
    )
}

/// La o eroare interna: motivul + locul in %APPDATA%\ManageBIM\crash.log si un mesaj pe ecran
/// (altfel o aplicatie cu fereastra se inchide fara niciun semn).
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
            // jurnalul nu creste la infinit: peste 1 MB pastram doar ultima jumatate
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
        // erorile din firele de lucru sunt prinse (devin mesaje de eroare); doar firul principal inchide aplicatia
        if std::thread::current().name() == Some("main") {
            #[cfg(windows)]
            {
                extern "system" {
                    fn MessageBoxW(h: *mut std::ffi::c_void, text: *const u16, caption: *const u16, t: u32) -> i32;
                }
                let text = format!("ManageBIM stopped because of an internal error:\n\n{info}\n\nDetails were saved to:\n{}", path.as_ref().map(|p| p.display().to_string()).unwrap_or_default());
                let w = |s: &str| s.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
                unsafe {
                    MessageBoxW(std::ptr::null_mut(), w(&text).as_ptr(), w("ManageBIM").as_ptr(), 0x10);
                }
            }
        }
        default(info);
    }));
}

fn main() -> eframe::Result<()> {
    // punte MCP stdio pentru Claude Desktop / OpenCode (fara fereastra): vorbeste cu ManageBIM deja pornit
    if std::env::args().nth(1).as_deref() == Some("--mcp-stdio") {
        std::process::exit(bimrust::mcp::stdio_bridge());
    }
    install_crash_log();
    let files: Vec<String> = std::env::args().skip(1).collect();
    // incearca configuratii GL din ce in ce mai simple (placi video vechi, VM, Remote Desktop)
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
    last
}
