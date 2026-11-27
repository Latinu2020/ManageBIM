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

fn main() -> eframe::Result<()> {
    // MCP stdio bridge for Claude Desktop / OpenCode (no window): talks to the already running ManageBIM
    if std::env::args().nth(1).as_deref() == Some("--mcp-stdio") {
        std::process::exit(bimrust::mcp::stdio_bridge());
    }
    install_crash_log();
    let files: Vec<String> = std::env::args().skip(1).collect();
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
