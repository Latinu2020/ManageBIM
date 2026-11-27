//! Software OpenGL (Mesa llvmpipe) for PCs whose graphics card / driver has no OpenGL 3.3: virtual
//! machines, old Remote Desktop sessions, very old drivers. Mesa's opengl32.dll lives in <exe>\mesa
//! (tools\get-mesa.ps1, copied by make-setup.ps1); when software rendering is on, that folder is put in
//! the DLL search path before the window is created, so Windows loads Mesa instead of the system OpenGL.
//! Slower than a graphics card (the processor draws everything), but the whole application works.
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

static ACTIVE: AtomicBool = AtomicBool::new(false);

/// Environment variable that asks for software rendering (set by the automatic restart).
pub const ENV: &str = "MANAGEBIM_SOFTWARE_GL";

/// The folder with Mesa's opengl32.dll: next to the exe, or third_party\mesa in a development build.
pub fn mesa_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    let candidates = [dir.join("mesa"), dir.join("..").join("..").join("third_party").join("mesa")];
    candidates.into_iter().find(|d| d.join("opengl32.dll").is_file())
}

/// Mesa is available on this installation.
pub fn available() -> bool {
    cfg!(windows) && mesa_dir().is_some()
}

/// Software rendering is in use in this session.
pub fn active() -> bool {
    ACTIVE.load(Ordering::Relaxed)
}

/// Software rendering was asked for this start (automatic restart or Settings).
pub fn requested() -> bool {
    std::env::var_os(ENV).is_some_and(|v| v == "1")
}

/// Uses Mesa for this process. Must run before the window (and so OpenGL) is created; true if it worked.
pub fn enable() -> bool {
    #[cfg(windows)]
    {
        let Some(dir) = mesa_dir() else { return false };
        extern "system" {
            fn SetDllDirectoryW(path: *const u16) -> i32;
        }
        let w: Vec<u16> = dir.as_os_str().to_string_lossy().encode_utf16().chain(std::iter::once(0)).collect();
        // SAFETY: a valid, null-terminated wide string; only changes where this process looks for DLLs
        if unsafe { SetDllDirectoryW(w.as_ptr()) } == 0 {
            return false;
        }
        // llvmpipe: Mesa's fast software rasteriser
        std::env::set_var("GALLIUM_DRIVER", "llvmpipe");
        ACTIVE.store(true, Ordering::Relaxed);
        true
    }
    #[cfg(not(windows))]
    {
        false
    }
}

/// Starts ManageBIM again (same files) with software rendering; the caller then ends this process.
pub fn restart_with_software() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let args: Vec<String> = std::env::args().skip(1).collect();
    std::process::Command::new(exe).args(args).env(ENV, "1").spawn().map(|_| ()).map_err(|e| e.to_string())
}
