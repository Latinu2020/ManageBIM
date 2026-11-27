//! The arguments and paths for an add-in installer; the actual logic is in
//! `bimrust::addin_install` (tested separately).
//!
//!   ManageBIM-Revit-AddIn.exe [--install | --uninstall]
//!
//! Running without arguments does the install. MB_INSTALL_ROOT redirects %APPDATA%
//! (same as -TestRoot in install.ps1), useful for tests.

use bimrust::addin_install::{self, Job};
use std::path::PathBuf;

pub use bimrust::addin_install::Kind;

pub fn run(kind: Kind, files: &[(&str, &[u8])]) {
    let uninstall = std::env::args().any(|a| a == "--uninstall" || a == "/uninstall");
    if files.is_empty() {
        eprintln!("ManageBIM add-in installer: no add-in files were included in this build.");
        eprintln!("Run the matching build script first (revit-addin\\build.ps1, acad-addin\\build.ps1 or navis-addin\\build.ps1),");
        eprintln!("then rebuild with: cargo build --release --features addins");
        std::process::exit(2);
    }
    let Some(appdata) = std::env::var_os("MB_INSTALL_ROOT")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("APPDATA").map(PathBuf::from))
    else {
        eprintln!("ManageBIM add-in installer: APPDATA is not set.");
        std::process::exit(1);
    };
    let program_files = std::env::var_os("ProgramFiles")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Program Files"));
    let job = Job { kind, files, appdata: &appdata, program_files: &program_files, uninstall };
    let msgs = addin_install::run(&job);
    if msgs.is_empty() {
        println!("{}: nothing to do", kind.label());
    }
    for m in msgs {
        println!("{m}");
    }
}
