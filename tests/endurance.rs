//! Endurance: reloading the same model many times (Refresh, Ctrl+F5) must not keep using more memory.
//! Each cycle reloads the model through the normal load path and rebuilds the colours, the draw lists'
//! caches and the levels; after a warm-up, the process memory and the app's caches must stay flat.
use bimrust::app::*;
use bimrust::*;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Memory the process uses now (bytes): working set on Windows, resident size elsewhere.
fn process_memory() -> u64 {
    #[cfg(windows)]
    {
        #[repr(C)]
        #[derive(Default)]
        struct Counters {
            cb: u32,
            page_faults: u32,
            peak_working_set: usize,
            working_set: usize,
            quota_peak_paged: usize,
            quota_paged: usize,
            quota_peak_nonpaged: usize,
            quota_nonpaged: usize,
            pagefile: usize,
            peak_pagefile: usize,
        }
        extern "system" {
            fn GetCurrentProcess() -> *mut std::ffi::c_void;
            fn K32GetProcessMemoryInfo(p: *mut std::ffi::c_void, c: *mut Counters, cb: u32) -> i32;
        }
        let mut c = Counters { cb: std::mem::size_of::<Counters>() as u32, ..Default::default() };
        // SAFETY: the structure matches PROCESS_MEMORY_COUNTERS of the Win32 API
        unsafe { K32GetProcessMemoryInfo(GetCurrentProcess(), &mut c, c.cb) };
        c.working_set as u64
    }
    #[cfg(not(windows))]
    {
        let s = std::fs::read_to_string("/proc/self/statm").unwrap_or_default();
        s.split_whitespace().nth(1).and_then(|v| v.parse::<u64>().ok()).unwrap_or(0) * 4096
    }
}

fn wait_loads(app: &mut App) {
    let t = Instant::now();
    while !app.loads.is_empty() {
        app.poll_jobs();
        assert!(t.elapsed() < Duration::from_secs(60), "reload timed out");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn reloading_a_model_does_not_grow_memory() {
    // a private copy, so nothing of the user's (Recent list, caches next to the samples) is touched
    let dir = std::env::temp_dir().join(format!("managebim_endurance_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("Block_A_MEP.ifc").to_string_lossy().to_string();
    std::fs::copy("samples/Block_A_MEP.ifc", &path).unwrap();

    let raw = load_any(&path, &|_, _| {}).unwrap();
    let o = raw.center();
    let mut app = App::new(vec![]);
    app.settings.use_cache = false;
    app.settings.auto_cache = false;
    *app.origin.lock().unwrap() = Some(o);
    let m = Arc::new(build_model(raw, &path, o));
    app.push_slot();
    app.base_models[0] = Some(m.clone());
    app.models[0] = Some(m);

    let cycle = |app: &mut App| {
        app.refresh_models(true);
        wait_loads(app);
        app.colors_dirty = true;
        app.rebuild_colors();
        let _ = app.scene_grids();
    };
    for _ in 0..5 {
        cycle(&mut app);
    }
    let before = process_memory();
    for _ in 0..40 {
        cycle(&mut app);
    }
    let after = process_memory();
    let grown = after.saturating_sub(before) as f64 / 1_048_576.0;
    let _ = std::fs::remove_dir_all(&dir);

    // the caches hold one entry per loaded model, not one per reload
    assert_eq!(app.loaded_models().count(), 1);
    assert!(app.grid_cache.borrow().len() <= 1, "grid cache: {}", app.grid_cache.borrow().len());
    assert!(app.looks.len() <= 1, "looks: {}", app.looks.len());
    // 40 reloads of a 20 KB IFC: a leak of even one model per reload would add many MB
    assert!(grown < 40.0, "memory grew by {grown:.1} MB over 40 reloads ({} -> {} MB)", before / 1_048_576, after / 1_048_576);
}
