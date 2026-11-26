//! Fereastra Path trace: setarile in stanga, previewul mare al randarii in dreapta.
//! Mutata din ui.rs in 0.28 (fisierul depasea 6 000 de linii).
use super::*;

impl App {
    pub(super) fn pathtrace_window(&mut self, ctx: &egui::Context) {
        let mut open = self.pt.open;
        let running = self.pt.job.as_ref().map(|j| !j.done.load(std::sync::atomic::Ordering::Relaxed)).unwrap_or(false);
        egui::Window::new("Path trace (progressive CPU)")
            .open(&mut open)
            // fereastra mare: setarile in stanga, previewul randarii in dreapta (0.28)
            .default_size([(ctx.screen_rect().width() * 0.72).clamp(760.0, 1400.0), (ctx.screen_rect().height() * 0.75).clamp(520.0, 900.0)])
            .min_width(640.0)
            .min_height(400.0)
            .resizable(true)
            .show(ctx, |ui| {
                ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                ui.set_width(360.0);
                ui.add_enabled_ui(!running, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Resolution");
                        let (mut w, h0) = (self.pt.params.0, self.pt.params.1);
                        // proportia: patrat / 16:9 / 4:3 (inaltimea se calculeaza din latime)
                        let mut ratio = if h0 == w { 0 } else if h0 == w * 9 / 16 { 1 } else if h0 == w * 3 / 4 { 2 } else { 0 };
                        egui::ComboBox::from_id_salt("pt_res").width(80.0).selected_text(format!("{w} px")).show_ui(ui, |ui| {
                            for r in [128u32, 256, 512, 1024, 1920, 2048, 4096] {
                                ui.selectable_value(&mut w, r, format!("{r} px"));
                            }
                        });
                        egui::ComboBox::from_id_salt("pt_ratio").width(70.0).selected_text(["1:1", "16:9", "4:3"][ratio]).show_ui(ui, |ui| {
                            for (k, l) in ["1:1", "16:9", "4:3"].into_iter().enumerate() {
                                ui.selectable_value(&mut ratio, k, l);
                            }
                        });
                        let h = match ratio { 1 => w * 9 / 16, 2 => w * 3 / 4, _ => w };
                        self.pt.params.0 = w;
                        self.pt.params.1 = h;
                        ui.label(RichText::new(format!("{w}×{h}")).weak());
                    });
                    ui.horizontal(|ui| {
                        ui.label("Samples");
                        ui.add(egui::DragValue::new(&mut self.pt.params.2).range(1..=1024).speed(4.0));
                        ui.label("Time limit (s, 0 = off)");
                        ui.add(egui::DragValue::new(&mut self.pt.params.3).range(0.0..=600.0).speed(1.0));
                    });
                    ui.checkbox(&mut self.render.with_materials, "🎨 Render with materials")
                        .on_hover_text("Uses the assigned materials (colours, image textures, brick / tile / wood patterns, gloss, metal, glass). With no materials assigned yet, they are added automatically by IFC type. Hidden elements and the section are respected.");
                    // ---- lumini
                    ui.separator();
                    ui.label(RichText::new("LIGHTS").small().strong().color(accent()));
                    ui.horizontal(|ui| {
                        ui.label("Environment");
                        for (m, label, tip) in [(0u8, "Outdoor", "Sky with clouds and the sun - natural light"), (1, "Studio", "Neutral softbox light - good for product shots"), (2, "Sunset", "Warm orange horizon")] {
                            if ui.selectable_label(self.render.env_mode == m, label).on_hover_text(tip).clicked() {
                                self.render.env_mode = m;
                            }
                        }
                    });
                    ui.horizontal(|ui| {
                        if ui.button("🗺 Load HDRI / photo…").on_hover_text("Radiance .hdr or an equirectangular PNG/JPG photo. This replaces the procedural sky and gives real reflections and light.").clicked() {
                            self.request_path(Purpose::LoadHdri);
                        }
                        if !self.render.hdri_path.is_empty() {
                            ui.label(RichText::new(crate::file_name(&self.render.hdri_path)).small().color(accent()))
                                .on_hover_text(&self.render.hdri_path);
                            if ui.small_button("✖").on_hover_text("Back to the procedural sky").clicked() {
                                self.render.hdri_path.clear();
                            }
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("Environment ×").on_hover_text("How strong the image-based light is");
                        ui.add(egui::Slider::new(&mut self.render.env_intensity, 0.0..=3.0).step_by(0.05));
                        ui.label("rotate");
                        ui.add(egui::DragValue::new(&mut self.render.env_rotation).range(-360.0..=360.0).speed(1.0).suffix("°"));
                    });
                    ui.horizontal(|ui| {
                        ui.label("Sun").on_hover_text("Direction and strength of the sun");
                        ui.add(egui::DragValue::new(&mut self.render.sun_az).range(-180.0..=180.0).speed(1.0).suffix("°"));
                        ui.add(egui::DragValue::new(&mut self.render.sun_el).range(1.0..=90.0).speed(1.0).suffix("°"));
                        ui.add(egui::Slider::new(&mut self.render.sun_power, 0.0..=3.0).step_by(0.05));
                    });
                    ui.horizontal(|ui| {
                        ui.label("Exposure").on_hover_text("Overall brightness (ACES tonemap keeps the highlights)");
                        ui.add(egui::Slider::new(&mut self.render.exposure, 0.1..=crate::anim::MAX_BRIGHTNESS).logarithmic(true).fixed_decimals(2));
                    });
                    ui.horizontal(|ui| {
                        if ui.checkbox(&mut self.settings.pt_gpu, "Render on the graphics card (GPU)").on_hover_text("Many times faster than the processor, same light and materials. Turn off if the graphics card has problems - the render then uses all processor cores.").changed() {
                            self.settings.save();
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("Ray Trace view samples").on_hover_text("Interactive Ray Trace (Render tab): samples per pixel before the view stops refining and is denoised");
                        if ui.add(egui::DragValue::new(&mut self.settings.rt_samples).range(16..=4096)).changed() {
                            self.settings.save();
                        }
                    });
                    ui.horizontal(|ui| {
                        let avail = crate::oidn::available();
                        let r = ui.add_enabled(avail.is_ok(), egui::Checkbox::new(&mut self.settings.render_denoise, "Denoise (Intel Open Image Denoise)"));
                        if r.changed() {
                            self.settings.save();
                        }
                        match &avail {
                            Ok(_) => {
                                r.on_hover_text("Removes the render noise at the end with AI (Intel OIDN, on the CPU). A clean image needs far fewer samples - 16-32 are often enough.");
                            }
                            Err(e) => {
                                r.on_disabled_hover_text(e.clone());
                                ui.label(RichText::new("not installed").small().weak());
                            }
                        }
                    });
                });
                ui.horizontal(|ui| {
                    if !running {
                        if ui.add(egui::Button::new(RichText::new("▶ Start").strong().color(Color32::WHITE)).fill(accent())).clicked() {
                            self.start_pathtrace();
                        }
                    } else {
                        if ui.button("⏸ Cancel").clicked() {
                            self.cancel_pathtrace();
                        }
                    }
                    if ui.add_enabled(self.pt.tex.is_some(), egui::Button::new("💾 Export PNG…")).clicked() {
                        self.request_path(Purpose::ExportPathTrace);
                    }
                });
                if let Some(job) = &self.pt.job {
                    let n = job.count.load(std::sync::atomic::Ordering::Relaxed);
                    let total = job.samples_target;
                    let frac = if total > 0 { n as f32 / total as f32 } else { 0.0 };
                    let done = job.done.load(std::sync::atomic::Ordering::Relaxed);
                    // timerul se opreste la „Ready": pastram secundele de la terminare
                    let secs = if done { self.pt.done_at.unwrap_or(job.started.elapsed().as_secs_f32()) } else { job.started.elapsed().as_secs_f32() };
                    let fmt_dur = |s: f32| -> String {
                        if s < 60.0 { format!("{s:.0}s") } else { format!("{}m {:02}s", (s / 60.0) as u32, (s % 60.0) as u32) }
                    };
                    let eta = if n > 0 && !done {
                        let rate = n as f32 / secs.max(0.1);
                        let by_samples = total.saturating_sub(n) as f32 / rate.max(1e-6);
                        let by_limit = if job.time_limit_secs > 0.0 { (job.time_limit_secs - secs).max(0.0) } else { f32::INFINITY };
                        let e = by_samples.min(by_limit);
                        if e.is_finite() { format!(" · ~{} {}", fmt_dur(e), crate::i18n::tr("left")) } else { String::new() }
                    } else {
                        String::new()
                    };
                    let txt = if n == 0 {
                        format!("{} · {:.1} s", crate::i18n::tr("Preparing scene / first sample"), secs)
                    } else if done {
                        format!("{} · {n} {} · {:.1} s", crate::i18n::tr("Finished"), crate::i18n::tr("samples"), secs)
                    } else {
                        format!("{n} / {total} {} · {:.1} s{eta}", crate::i18n::tr("samples"), secs)
                    };
                    let dn = job.denoise_state.load(std::sync::atomic::Ordering::Relaxed);
                    let txt = if dn == crate::pathtrace::DN_RUNNING { format!("{n} {} · {}", crate::i18n::tr("samples"), crate::i18n::tr("Denoising…")) } else { txt };
                    ui.add(egui::ProgressBar::new(frac).text(txt).animate(dn == crate::pathtrace::DN_RUNNING));
                    match dn {
                        crate::pathtrace::DN_DONE => {
                            ui.label(RichText::new("✔ Denoised with Intel Open Image Denoise").small().color(Color32::from_rgb(80, 200, 120)));
                        }
                        crate::pathtrace::DN_FAILED => {
                            let m = job.denoise_msg.lock().map(|m| m.clone()).unwrap_or_default();
                            ui.label(RichText::new(format!("⚠ Denoise failed - the noisy image is kept. {m}")).small().color(Color32::from_rgb(230, 170, 60)));
                        }
                        _ => {}
                    }
                    if done && n > 0 {
                        ui.label(RichText::new(crate::i18n::tr("Ready - press Export PNG to save the image.")).strong().color(Color32::from_rgb(80, 200, 120)));
                    }
                }
                });
                ui.separator();
                // ---- previewul: ocupa tot restul ferestrei, pe fundal inchis, imaginea in proportia ei
                let size = ui.available_size().max(egui::vec2(280.0, 240.0));
                let (area, _) = ui.allocate_exact_size(size, Sense::hover());
                ui.painter().rect_filled(area, crate::theme::RADIUS_MD, Color32::from_rgb(18, 20, 26));
                if let Some(tex) = &self.pt.tex {
                    let ts = tex.size_vec2();
                    let inner = area.shrink(8.0);
                    let k = (inner.width() / ts.x.max(1.0)).min(inner.height() / ts.y.max(1.0)).max(0.05);
                    let r = Rect::from_center_size(inner.center(), ts * k);
                    ui.painter().image(tex.id(), r, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
                    ui.painter().text(area.left_bottom() + EV::new(10.0, -8.0), Align2::LEFT_BOTTOM, format!("{}×{} · preview {:.0}%", ts.x as u32, ts.y as u32, k * 100.0), FontId::proportional(11.5), Color32::from_gray(170));
                } else {
                    ui.painter().text(area.center() - EV::new(0.0, 12.0), Align2::CENTER_CENTER, crate::theme::ph("🖼"), crate::theme::icon_font(40.0), Color32::from_gray(90));
                    ui.painter().text(area.center() + EV::new(0.0, 24.0), Align2::CENTER_CENTER, "Press Start - the image builds up here sample by sample", FontId::proportional(13.0), Color32::from_gray(150));
                }
                });
            });
        self.pt.open = open;
    }
}
