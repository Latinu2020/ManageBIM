//! Reproduce incarcarea ca in aplicatie: open_path -> poll_jobs -> culori / arbore / cantitati, cu panic-uri prinse.
use bimrust::app::*;
fn main() {
    std::env::set_var("RUST_BACKTRACE", "1");
    for path in std::env::args().skip(1) {
        let p2 = path.clone();
        let r = std::panic::catch_unwind(move || {
            let mut app = App::new(vec![]);
            let t = std::time::Instant::now();
            app.open_path(&p2, false);
            while !app.loads.is_empty() {
                app.poll_jobs();
                std::thread::sleep(std::time::Duration::from_millis(20));
                if t.elapsed().as_secs() > 300 { println!("  TIMEOUT"); break; }
            }
            let m = app.loaded_models().next().map(|(_, m)| (m.elements.len(), m.tri_total()));
            app.colors_dirty = true;
            app.rebuild_colors();
            app.refresh_caches();
            app.qto_update();
            let ids = app.all_ids();
            app.selected = ids.iter().take(5).copied().collect();
            app.colors_dirty = true;
            app.rebuild_colors();
            (m, app.log.iter().rev().take(3).cloned().collect::<Vec<_>>(), t.elapsed().as_secs_f64())
        });
        match r {
            Ok((m, log, s)) => println!("OK    {:>5.1}s {:?}  <- {}\n      log: {:?}", s, m, path, log),
            Err(e) => {
                let msg = e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
                println!("PANIC <- {path}\n      {msg}");
            }
        }
    }
}
