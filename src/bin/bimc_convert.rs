//! bimc-convert: converteste modele (IFC, glTF, STEP, DWG, OBJ, FBX...) in formatul nativ ManageBIM (.bimc),
//! din linia de comanda. Fisierul .bimc se deschide instant in ManageBIM.
//!
//!   bimc-convert model.ifc                 -> model.ifc.bimc (langa model)
//!   bimc-convert model.ifc iesire.bimc      -> iesire.bimc
//!   bimc-convert a.ifc b.ifc c.dwg          -> cate un .bimc pentru fiecare
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args.iter().any(|a| a == "-h" || a == "--help" || a == "/?") {
        eprintln!("bimc-convert {} - convert models to ManageBIM .bimc", env!("CARGO_PKG_VERSION"));
        eprintln!("usage: bimc-convert <model> [output.bimc]");
        eprintln!("       bimc-convert <model1> <model2> ...   (one .bimc next to each model)");
        return ExitCode::from(if args.is_empty() { 2 } else { 0 });
    }
    // doua argumente si al doilea e .bimc = intrare + iesire; altfel fiecare argument e un model
    let jobs: Vec<(String, String)> = if args.len() == 2 && args[1].to_lowercase().ends_with(".bimc") && !args[0].to_lowercase().ends_with(".bimc") {
        vec![(args[0].clone(), args[1].clone())]
    } else {
        args.iter().map(|a| (a.clone(), bimrust::bimc::cache_path(a))).collect()
    };
    let mut failed = 0;
    for (src, dst) in jobs {
        let t = std::time::Instant::now();
        match bimrust::load_any(&src, &|_, _| {}).and_then(|raw| {
            let n = raw.elements.len();
            bimrust::bimc::write(&raw, &dst).map(|bytes| (n, bytes))
        }) {
            Ok((n, bytes)) => println!("{src} -> {dst}: {n} elements, {:.1} MB, {:.1} s", bytes as f64 / 1e6, t.elapsed().as_secs_f32()),
            Err(e) => {
                eprintln!("{src}: {e}");
                failed += 1;
            }
        }
    }
    if failed > 0 {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}
