fn main() {
    let ctx = eframe::egui::Context::default();
    let _ = ctx.run(Default::default(), |_| {});
    let cands = "☁⭕◯○⬭➡→↗✏✎🖊🖍⌫🧽🗑🏷💬🗨✋🤚🔍🔎👁🚶🏃🦶👣↕⇕⟲🔄⬚▭◻⬜╱／⁄〰📝🅰🆎🔖📌📍🧭🔦⛶✂📐📏⏮⏭◀▶⏪⏩🎯🖐↔✥✣🔆🌀🔭📷🎨⚑🚩";
    let s: String = ctx.fonts(|f| cands.chars().filter(|c| f.has_glyph(&eframe::egui::FontId::proportional(14.0), *c)).collect());
    println!("{s}");
}
