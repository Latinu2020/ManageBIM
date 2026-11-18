use bimrust::app::*;
use std::sync::Arc;
use std::time::Instant;
fn main() {
    let p = |_: f32, _: &str| {};
    let mut app = App::new(vec![]);
    for f in std::env::args().skip(1) {
        let raw = bimrust::bimc::read(&f).unwrap();
        let o = raw.center();
        app.models.push(Some(Arc::new(bimrust::build_model(raw, &f, o))));
    }
    let _ = p;
    let t = Instant::now();
    app.refresh_caches();
    println!("index: {} categories, {} properties, {:.0} ms", app.prop_index.cats.len(), app.prop_index.count(), t.elapsed().as_secs_f64() * 1e3);
    let t = Instant::now();
    let r = app.find(&[SearchRule::new(ITEM_CAT, "IFC Type", SearchOp::Equals, "IfcColumn", false), SearchRule::new(ITEM_CAT, "IFC Type", SearchOp::Equals, "IfcDuctSegment", true)]);
    println!("find: {} items in {:.0} ms", r.len(), t.elapsed().as_secs_f64() * 1e3);
}
