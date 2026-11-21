//! Sectiune din `impl App` (vezi fisierul parinte pentru restul).
use super::*;

impl App {
    pub(super) fn timeliner_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            if ui.button("➕ Task").clicked() {
                let start = self.sim_range().map(|r| r.1 + 1).unwrap_or_else(timeline::today);
                let n = self.tasks.len() + 1;
                self.tasks.push(timeline::Task { name: format!("Task {n}"), start, end: start + 13, kind: TaskKind::Construct, rule: String::new(), elems: Vec::new() });
                self.cur_task = Some(self.tasks.len() - 1);
                self.invalidate_sim();
            }
            if ui.add_enabled(self.cur_task.is_some() && !self.selected.is_empty(), egui::Button::new("Attach selection")).clicked() {
                self.attach_selection();
            }
            if ui.add_enabled(self.cur_task.is_some(), egui::Button::new("Detach all")).clicked() {
                if let Some(t) = self.cur_task.and_then(|t| self.tasks.get_mut(t)) {
                    t.elems.clear();
                }
                self.invalidate_sim();
            }
            if ui.button("Auto-attach using rules").on_hover_text("Rule: “set:SetName”, “Level=Ground Floor; IFC Type=IfcWall” or free text").clicked() {
                let mut tot = 0;
                for i in 0..self.tasks.len() {
                    let e = self.rule_elems(&self.tasks[i].rule.clone());
                    if !e.is_empty() {
                        tot += e.len();
                        self.tasks[i].elems = e;
                    }
                }
                self.invalidate_sim();
                self.toast(format!("Auto-attach: {tot} attachments."));
            }
            if ui.add_enabled(self.cur_task.is_some(), egui::Button::new("🗑 Delete")).clicked() {
                if let Some(t) = self.cur_task.take() {
                    if t < self.tasks.len() {
                        self.tasks.remove(t);
                    }
                }
                self.invalidate_sim();
            }
            ui.separator();
            if ui.button("Import…").on_hover_text("CSV (Name;Start;End;Type;Rule - dates YYYY-MM-DD or DD.MM.YYYY), MS Project XML or Primavera P6 XML.
In MS Project / P6, link a task with [Level=Level 1;Type=IfcSlab] at the end of its name.").clicked() {
                self.request_path(Purpose::ImportTasksCsv);
            }
            if ui.button("Export CSV").clicked() {
                self.request_path(Purpose::ExportTasksCsv);
            }
        });
        ui.separator();
        let avail = ui.available_size();
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(EV::new(avail.x * 0.52, avail.y), egui::Layout::top_down(egui::Align::Min), |ui| {
                ui.set_max_width(avail.x * 0.52);
                let mut clicked = None;
                let mut changed = false;
                let cur = self.cur_task;
                let tasks = &mut self.tasks;
                let mh = (ui.available_height() - 30.0).max(60.0);
                TableBuilder::new(ui)
                    .id_salt("tasks")
                    .max_scroll_height(mh)
                    .auto_shrink([false, true])
                    .striped(true)
                    .sense(Sense::click())
                    .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                    .column(Column::initial(140.0).clip(true))
                    .column(Column::exact(84.0))
                    .column(Column::exact(84.0))
                    .column(Column::exact(100.0))
                    .column(Column::initial(140.0).clip(true))
                    .column(Column::remainder().at_least(50.0))
                    .header(20.0, |mut h| {
                        for n in ["Name", "Start", "End", "Type", "Rule", "Items"] {
                            h.col(|ui| {
                                ui.strong(n);
                            });
                        }
                    })
                    .body(|body| {
                        body.rows(24.0, tasks.len(), |mut row| {
                            let i = row.index();
                            row.set_selected(cur == Some(i));
                            let t = &mut tasks[i];
                            row.col(|ui| {
                                ui.add(egui::TextEdit::singleline(&mut t.name).desired_width(f32::INFINITY));
                            });
                            row.col(|ui| {
                                changed |= date_edit(ui, egui::Id::new(("ts", i)), &mut t.start);
                            });
                            row.col(|ui| {
                                changed |= date_edit(ui, egui::Id::new(("te", i)), &mut t.end);
                            });
                            row.col(|ui| {
                                egui::ComboBox::from_id_salt(("tk", i)).width(90.0).selected_text(t.kind.label()).show_ui(ui, |ui| {
                                    for k in [TaskKind::Construct, TaskKind::Demolish, TaskKind::Temporary] {
                                        changed |= ui.selectable_value(&mut t.kind, k, k.label()).changed();
                                    }
                                });
                            });
                            row.col(|ui| {
                                ui.add(egui::TextEdit::singleline(&mut t.rule).desired_width(f32::INFINITY).hint_text("e.g. Level=Ground Floor"));
                            });
                            row.col(|ui| {
                                ui.label(t.elems.len().to_string());
                            });
                            if row.response().clicked() {
                                clicked = Some(i);
                            }
                        });
                    });
                if changed {
                    for t in self.tasks.iter_mut() {
                        t.end = t.end.max(t.start);
                    }
                    self.invalidate_sim();
                }
                if let Some(i) = clicked {
                    self.cur_task = Some(i);
                    let e = self.tasks[i].elems.clone();
                    if !e.is_empty() {
                        self.apply(Action::Select(e, false));
                    }
                }
                if self.tasks.is_empty() {
                    ui.label(RichText::new("Add tasks or import a CSV schedule, attach elements (selection or rules) and start the simulation.").weak());
                }
            });
            ui.separator();
            ui.vertical(|ui| self.simulation_ui(ui));
        });
    }

    pub(super) fn simulation_ui(&mut self, ui: &mut egui::Ui) {
        let range = self.sim_range();
        ui.horizontal_wrapped(|ui| {
            let was = self.sim_active;
            ui.checkbox(&mut self.sim_active, RichText::new("4D Simulation").strong());
            if self.sim_active && !was {
                if let Some((s, _)) = range {
                    if self.sim_day < s as f32 - 1.0 || self.sim_day > range.unwrap().1 as f32 + 1.0 {
                        self.sim_day = s as f32;
                    }
                }
                self.invalidate_sim();
            }
            if ui.checkbox(&mut self.sim_only_linked, "Show only scheduled elements").on_hover_text("Hide everything that is not attached to a task - you see only what is being built").clicked() {
                self.colors_dirty = true;
            }
            ui.add_enabled_ui(self.sim_active && range.is_some(), |ui| {
                if ui.button("⏮").clicked() {
                    self.sim_day = range.unwrap().0 as f32 - 0.5;
                    self.sim_play = false;
                }
                let pl = if self.sim_play { "⏸ Pause" } else { "▶ Play" };
                if ui.button(pl).clicked() {
                    if !self.sim_play && self.sim_day >= range.unwrap().1 as f32 + 0.99 {
                        self.sim_day = range.unwrap().0 as f32 - 0.5;
                    }
                    self.sim_play = !self.sim_play;
                }
                if ui.button("⏭").clicked() {
                    self.sim_day = range.unwrap().1 as f32 + 1.0;
                    self.sim_play = false;
                }
                ui.add(egui::DragValue::new(&mut self.sim_speed).range(0.5..=365.0).speed(0.5).suffix(" days/s"));
            });
        });
        let Some((s, e)) = range else {
            ui.label(RichText::new("No tasks.").weak());
            return;
        };
        ui.add_enabled_ui(self.sim_active, |ui| {
            ui.spacing_mut().slider_width = (ui.available_width() - 120.0).max(100.0);
            ui.add(egui::Slider::new(&mut self.sim_day, (s as f32 - 1.0)..=(e as f32 + 1.0)).show_value(false));
        });
        ui.label(RichText::new(format!("Date: {}   (range {} – {})", timeline::fmt_date(self.sim_day.floor() as i32), timeline::fmt_date(s), timeline::fmt_date(e))).strong());
        // ---- Gantt
        let h = (self.tasks.len() as f32 * 18.0 + 24.0).max(60.0);
        egui::ScrollArea::vertical().id_salt("gantt").auto_shrink([false, false]).show(ui, |ui| {
            let (rect, resp) = ui.allocate_exact_size(EV::new(ui.available_width(), h), Sense::click());
            let p = ui.painter_at(rect);
            let pal = self.pal();
            p.rect_filled(rect, 4.0, pal.gantt_bg);
            let span = (e - s + 1).max(1) as f32;
            let x = |d: f32| rect.left() + 6.0 + (d - s as f32) / span * (rect.width() - 12.0);
            // grila lunara
            let (mut y0, mut m0, _) = timeline::civil_from_days(s);
            loop {
                let d = timeline::days_from_civil(y0, m0, 1);
                if d > e {
                    break;
                }
                if d >= s {
                    let xx = x(d as f32);
                    p.line_segment([Pos2::new(xx, rect.top()), Pos2::new(xx, rect.bottom())], Stroke::new(1.0_f32, pal.gantt_grid));
                    p.text(Pos2::new(xx + 2.0, rect.top() + 2.0), Align2::LEFT_TOP, format!("{y0}-{m0:02}"), FontId::proportional(10.0), pal.gantt_text.gamma_multiply(0.7));
                }
                m0 += 1;
                if m0 > 12 {
                    m0 = 1;
                    y0 += 1;
                }
            }
            for (i, t) in self.tasks.iter().enumerate() {
                let yy = rect.top() + 18.0 + i as f32 * 18.0;
                let col = match t.kind {
                    TaskKind::Construct => timeline::COL_CONSTRUCT,
                    TaskKind::Demolish => timeline::COL_DEMOLISH,
                    TaskKind::Temporary => timeline::COL_TEMP,
                };
                let r = Rect::from_min_max(Pos2::new(x(t.start as f32), yy + 2.0), Pos2::new(x(t.end as f32 + 1.0).max(x(t.start as f32) + 3.0), yy + 15.0));
                let mut c = Color32::from_rgb(col[0], col[1], col[2]);
                if self.cur_task == Some(i) {
                    p.rect_stroke(r.expand(1.5), 3.0, Stroke::new(1.5_f32, pal.gantt_text));
                } else {
                    c = c.gamma_multiply(0.8);
                }
                p.rect_filled(r, 2.0, c);
                let g = p.layout_no_wrap(t.name.clone(), FontId::proportional(11.0), Color32::WHITE);
                let tp = if g.size().x + 6.0 < r.width() { Pos2::new(r.left() + 3.0, r.center().y - g.size().y / 2.0) } else { Pos2::new(r.right() + 4.0, r.center().y - g.size().y / 2.0) };
                let fg = if g.size().x + 6.0 < r.width() { Color32::from_rgb(15, 20, 25) } else { pal.gantt_text };
                p.galley_with_override_text_color(tp, g, fg);
            }
            let cx = x(self.sim_day);
            p.line_segment([Pos2::new(cx, rect.top()), Pos2::new(cx, rect.bottom())], Stroke::new(2.0_f32, Color32::from_rgb(255, 70, 70)));
            if resp.clicked() {
                if let Some(pp) = resp.interact_pointer_pos() {
                    let i = ((pp.y - rect.top() - 18.0) / 18.0).floor();
                    if i >= 0.0 && (i as usize) < self.tasks.len() {
                        self.cur_task = Some(i as usize);
                    }
                    if self.sim_view() {
                        self.sim_day = s as f32 + (pp.x - rect.left() - 6.0) / (rect.width() - 12.0) * span;
                    }
                }
            }
        });
    }

    // ------------------------------------------------------------------ viewpoints
    pub(super) fn viewpoints_ui(&mut self, ui: &mut egui::Ui) {
        use crate::markup::{PRIORITIES, STATUSES};
        ui.horizontal_wrapped(|ui| {
            if ui.button("📷 Save Current View").clicked() {
                self.save_viewpoint();
            }
            if ui.button("📁 New folder").on_hover_text("A folder for viewpoints / markups (inside the current folder). Drag viewpoints onto it; its ⋯ menu exports only its content.").clicked() {
                self.vp_new_folder();
            }
            ui.separator();
            ui.label("Export:");
            let can = !self.viewpoints.is_empty();
            if ui.add_enabled(can, egui::Button::new("📄 HTML report")).clicked() {
                self.request_path(Purpose::ExportVpHtml);
            }
            if ui.add_enabled(can, egui::Button::new("📊 Excel")).clicked() {
                self.request_path(Purpose::ExportVpXlsx);
            }
            if ui.add_enabled(can, egui::Button::new("📄 PDF sheets")).clicked() {
                self.request_path(Purpose::ExportVpPdf);
            }
            if ui.add_enabled(can, egui::Button::new("📤 BCF 2.1")).clicked() {
                self.request_path(Purpose::ExportBcf);
            }
            if ui.add_enabled(can, egui::Button::new("🧭 Navisworks XML")).on_hover_text("Saved Viewpoints > right-click > Import Viewpoints in Navisworks").clicked() {
                self.request_path(Purpose::ExportNavisXml);
            }
            if ui.button("📥 Import BCF…").on_hover_text("BCF 2.1 / 3.0 issues from Revit, ACC, Solibri, BIMcollab...: camera, status, comments, selected elements").clicked() {
                self.request_path(Purpose::ImportBcf);
            }
            if ui.button("📥 Import Navisworks XML…").on_hover_text("Viewpoints exported from Navisworks: camera, section box, comments, redlines").clicked() {
                self.request_path(Purpose::ImportNavisXml);
            }
            ui.label(RichText::new("Redline tools: ribbon Review. Markups belong to the viewpoint and show while you stay at it.").weak().small());
        });
        ui.separator();
        if self.viewpoints.is_empty() {
            ui.label(RichText::new("No saved viewpoints. Save one, or just draw a redline (Review tab) - a viewpoint is created automatically.").weak());
            return;
        }
        if self.vp_sel >= self.viewpoints.len() {
            self.vp_sel = self.viewpoints.len() - 1;
        }
        let mut restore = None;
        let mut del = None;
        let mut update = false;
        let w = ui.available_width();
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(EV::new((w * 0.36).max(220.0), ui.available_height()), egui::Layout::top_down(egui::Align::Min), |ui| {
                self.vp_tree_ui(ui, &mut restore);
            });
            ui.separator();
            ui.vertical(|ui| {
                let i = self.vp_sel;
                let who = crate::markup::user_name();
                let v = &mut self.viewpoints[i];
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut v.name).desired_width(240.0).font(egui::TextStyle::Heading));
                    if ui.button("▶ Go to").clicked() {
                        restore = Some(i);
                    }
                    if ui.button("⟳ Update view").on_hover_text("Store the current camera, section, hidden items and selection").clicked() {
                        update = true;
                    }
                    if ui.add_enabled(!v.redlines.is_empty(), egui::Button::new("Clear redlines")).clicked() {
                        v.redlines.clear();
                    }
                    if ui.button("🗑").on_hover_text("Delete viewpoint").clicked() {
                        del = Some(i);
                    }
                });
                egui::Grid::new("vp_issue").num_columns(4).spacing([10.0, 6.0]).show(ui, |ui| {
                    ui.label("Status");
                    egui::ComboBox::from_id_salt("vp_status").selected_text(v.issue.status.clone()).show_ui(ui, |ui| {
                        for st in STATUSES {
                            ui.selectable_value(&mut v.issue.status, st.to_string(), st);
                        }
                    });
                    ui.label("Priority");
                    egui::ComboBox::from_id_salt("vp_prio").selected_text(v.issue.priority.clone()).show_ui(ui, |ui| {
                        for p in PRIORITIES {
                            ui.selectable_value(&mut v.issue.priority, p.to_string(), p);
                        }
                    });
                    ui.end_row();
                    ui.label("Assigned to");
                    ui.add(egui::TextEdit::singleline(&mut v.issue.assigned).desired_width(160.0));
                    ui.label("Author");
                    ui.label(format!("{} · {}", v.issue.author, crate::markup::pretty_date(&v.issue.created)));
                    ui.end_row();
                });
                ui.label("Description");
                ui.add(egui::TextEdit::multiline(&mut v.comment).desired_rows(2).desired_width(f32::INFINITY));
                ui.label(RichText::new(format!("Comments ({})", v.comments.len())).strong());
                egui::ScrollArea::vertical().id_salt("vp_comments").max_height(110.0).show(ui, |ui| {
                    let mut rm = None;
                    for (k, c) in v.comments.iter().enumerate() {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(RichText::new(&c.author).strong());
                            ui.label(RichText::new(crate::markup::pretty_date(&c.date)).weak().small());
                            ui.label(&c.text);
                            if ui.small_button("✖").clicked() {
                                rm = Some(k);
                            }
                        });
                    }
                    if let Some(k) = rm {
                        v.comments.remove(k);
                    }
                });
                let id = ui.id().with("new_comment");
                let mut txt: String = ui.data_mut(|d| d.get_temp(id).unwrap_or_default());
                ui.horizontal(|ui| {
                    let r = ui.add(egui::TextEdit::singleline(&mut txt).hint_text("add a comment…").desired_width(320.0));
                    let send = ui.button("Add").clicked() || (r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)));
                    if send && !txt.trim().is_empty() {
                        v.comments.push(crate::markup::VpComment { author: who.clone(), date: crate::markup::now_iso(), text: txt.trim().to_string() });
                        txt.clear();
                    }
                });
                ui.data_mut(|d| d.insert_temp(id, txt));
            });
        });
        if let Some(i) = restore {
            self.restore_viewpoint(i);
        }
        if update {
            let i = self.vp_sel;
            let mut hidden: Vec<ElemId> = self.hidden.iter().copied().collect();
            hidden.sort();
            let sel: Vec<ElemId> = self.selected.iter().copied().collect();
            let v = &mut self.viewpoints[i];
            v.cam = self.cam;
            v.section = self.section.clone();
            v.hidden = hidden;
            v.selected = sel;
            self.active_vp = Some(i);
        }
        if let Some(i) = del {
            self.viewpoints.remove(i);
            self.active_vp = None;
            self.vp_sel = i.saturating_sub(1);
        }
    }

    // ------------------------------------------------------------------ render / animation
    /// Animator: seturile de obiecte animate (culoare, transparenta, vizibilitate) cu keyframe-urile lor.
    pub(super) fn anim_sets_ui(&mut self, ui: &mut egui::Ui, ai: usize) {
        use crate::anim::{AnimSet, ObjKey};
        let n_sets = self.animations[ai].sets.len();
        egui::CollapsingHeader::new(RichText::new(format!("Animation sets - objects ({n_sets})")).strong()).id_salt(("animsets", ai)).default_open(n_sets > 0).show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                let sel: Vec<ElemId> = self.selected.iter().copied().collect();
                if ui.add_enabled(!sel.is_empty(), egui::Button::new(format!("➕ From selection ({})", sel.len()))).on_hover_text("Animate the selected items (Navisworks: Add Animation Set > From current selection)").clicked() {
                    let n = self.animations[ai].sets.len() + 1;
                    self.animations[ai].sets.push(AnimSet { name: format!("Set {n}"), target: Selector { kind: SelKind::All, value: String::new() }, items: sel, keys: vec![ObjKey { t: self.anim_time, visible: true, color: None, transparency: 0.0 }], enabled: true });
                }
                ui.menu_button("➕ From set ⏷", |ui| {
                    if self.sets.is_empty() {
                        ui.label(RichText::new("No selection / search sets").weak());
                    }
                    for st in self.sets.clone() {
                        if ui.button(&st.name).clicked() {
                            self.animations[ai].sets.push(AnimSet { name: st.name.clone(), target: Selector { kind: SelKind::Set, value: st.name.clone() }, items: Vec::new(), keys: vec![ObjKey { t: self.anim_time, visible: true, color: None, transparency: 0.0 }], enabled: true });
                            ui.close_menu();
                        }
                    }
                });
            });
            let mut del_set = None;
            let mut goto = None;
            let now = self.anim_time;
            for si in 0..self.animations[ai].sets.len() {
                ui.separator();
                let mut changed = false;
                {
                    let st = &mut self.animations[ai].sets[si];
                    ui.horizontal(|ui| {
                        changed |= ui.checkbox(&mut st.enabled, "").on_hover_text("Active").changed();
                        ui.add(egui::TextEdit::singleline(&mut st.name).desired_width(120.0));
                        let what = if st.items.is_empty() { st.target.label() } else { format!("{} items", st.items.len()) };
                        ui.label(RichText::new(what).weak().small());
                        if ui.small_button("📷 Capture keyframe").on_hover_text("Adds a keyframe at the current time, copying the state at this moment - then change colour, transparency or visibility").clicked() {
                            let k = st.sample(now).unwrap_or(ObjKey { t: now, visible: true, color: None, transparency: 0.0 });
                            st.keys.retain(|x| (x.t - now).abs() > 1e-3);
                            st.keys.push(ObjKey { t: now, ..k });
                            st.sort();
                            changed = true;
                        }
                        if ui.small_button("🗑").clicked() {
                            del_set = Some(si);
                        }
                    });
                    let mut kdel = None;
                    egui::Grid::new(("objkeys", ai, si)).num_columns(6).striped(true).spacing([6.0, 2.0]).show(ui, |ui| {
                        for (k, key) in st.keys.iter_mut().enumerate() {
                            changed |= ui.add(egui::DragValue::new(&mut key.t).range(0.0..=3600.0).speed(0.05).fixed_decimals(2).suffix(" s")).changed();
                            changed |= ui.checkbox(&mut key.visible, "Visible").changed();
                            let mut has_c = key.color.is_some();
                            if ui.checkbox(&mut has_c, "Colour").changed() {
                                key.color = if has_c { Some([230, 60, 50]) } else { None };
                                changed = true;
                            }
                            if let Some(c) = key.color.as_mut() {
                                changed |= ui.color_edit_button_srgb(c).changed();
                            } else {
                                ui.label("");
                            }
                            let mut tr = key.transparency * 100.0;
                            if ui.add(egui::DragValue::new(&mut tr).range(0.0..=100.0).speed(1.0).suffix(" % transp.")).changed() {
                                key.transparency = tr / 100.0;
                                changed = true;
                            }
                            ui.horizontal(|ui| {
                                if ui.small_button("Go to").clicked() {
                                    goto = Some(key.t);
                                }
                                if ui.small_button("✖").clicked() {
                                    kdel = Some(k);
                                }
                            });
                            ui.end_row();
                        }
                    });
                    if let Some(k) = kdel {
                        st.keys.remove(k);
                        changed = true;
                    }
                    if changed && !ui.ctx().is_using_pointer() {
                        st.sort();
                    }
                }
                if changed {
                    self.anim_objects(ai, self.anim_time);
                }
            }
            if let Some(i) = del_set {
                self.animations[ai].sets.remove(i);
                self.anim_objects(ai, self.anim_time);
            }
            if let Some(t) = goto {
                self.anim_play = None;
                self.goto_anim_time(t);
            }
            if n_sets == 0 {
                ui.label(RichText::new("Animate objects like in Navisworks Animator: add a set from the selection, capture keyframes at different times and change colour, transparency or visibility (e.g. a crane appearing, a phase fading in).").weak().small());
            }
        });
    }

    pub(super) fn render_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.render_subtab, 0, "🖼 Image");
            ui.selectable_value(&mut self.render_subtab, 1, format!("🎨 Materials ({})", self.mat_assign.len()));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("🎞 Animations…").on_hover_text("Camera animations, 4D and video export are in their own window").clicked() {
                    self.open_panel(Panel::Animation);
                }
            });
        });
        ui.separator();
        if self.render_subtab == 1 {
            self.materials_ui(ui);
            return;
        }
        ui.columns(2, |cols| {
            let ui = &mut cols[0];
            ui.label(RichText::new("IMAGE SETTINGS").small().strong().color(ui.visuals().weak_text_color()));
            self.output_settings_grid(ui, false);
            let ui = &mut cols[1];
            ui.label(RichText::new("RENDER THE CURRENT VIEW").small().strong().color(ui.visuals().weak_text_color()));
            ui.label(format!("{}×{} PNG of what you see in the 3D view, with the current visual style ({}).", self.render.width, self.render.height, self.settings.visual_style.label()));
            ui.add_space(4.0);
            if ui.add(egui::Button::new(RichText::new("🖼 Render Image…").strong().color(Color32::WHITE)).fill(accent()).min_size(EV::new(180.0, 30.0))).clicked() {
                self.request_path(Purpose::RenderImage);
            }
            if ui.button("Path trace image…").on_hover_text("Progressive CPU path tracing with cancel and PNG export").clicked() {
                self.pathtrace_open = true;
            }
            ui.label(RichText::new("Path trace: preview builds up sample by sample; export any time.").weak().small());
            ui.add_space(4.0);
            ui.checkbox(&mut self.render.show_frame, "Show the image frame in the 3D view").on_hover_text("Compose the shot: shows the frame of the chosen resolution");
            ui.checkbox(&mut self.render.overlay_4d, "4D date & legend on images");
            ui.checkbox(&mut self.render.legend_on_images, "Colour legend (Appearance Profiler) on images");
        });
    }

    /// Setarile de iesire (rezolutie, cadre, format, calitate, fundal, anti-aliasing). `video` = si cadrele / formatul.
    pub(super) fn output_settings_grid(&mut self, ui: &mut egui::Ui, video: bool) {
        if self.ffmpeg.is_none() {
            let ok = crate::video::ffmpeg_available();
            self.ffmpeg = Some(ok);
            if !ok && self.render.format == VideoFormat::Mp4 {
                self.render.format = VideoFormat::Avi;
            }
        }
        let ff = self.ffmpeg.unwrap_or(false);
        let busy = self.render_job.is_some();
        ui.add_enabled_ui(!busy, |ui| {
                egui::Grid::new(("outgrid", video)).num_columns(2).spacing([10.0, 5.0]).show(ui, |ui| {
                    ui.label("Resolution");
                    ui.horizontal(|ui| {
                        let cur = crate::anim::RESOLUTIONS.iter().find(|r| r.0 == self.render.width && r.1 == self.render.height).map(|r| r.2).unwrap_or("Custom");
                        egui::ComboBox::from_id_salt("res").width(130.0).selected_text(cur).show_ui(ui, |ui| {
                            for (w, h, n) in crate::anim::RESOLUTIONS {
                                if ui.selectable_label(cur == n, format!("{n}  ({w}×{h})")).clicked() {
                                    self.render.width = w;
                                    self.render.height = h;
                                }
                            }
                        });
                        ui.add(egui::DragValue::new(&mut self.render.width).range(64..=7680));
                        ui.label("×");
                        ui.add(egui::DragValue::new(&mut self.render.height).range(64..=4320));
                    });
                    ui.end_row();
                    if video {
                    ui.label("Frame rate");
                    ui.horizontal(|ui| {
                        for f in [24, 25, 30, 60] {
                            ui.selectable_value(&mut self.render.fps, f, format!("{f}"));
                        }
                        ui.label("fps");
                    });
                    ui.end_row();
                    }
                    if video {
                    ui.label("Format");
                    egui::ComboBox::from_id_salt("fmt").width(260.0).selected_text(self.render.format.label()).show_ui(ui, |ui| {
                        for f in VideoFormat::ALL {
                            let en = f != VideoFormat::Mp4 || ff;
                            ui.add_enabled_ui(en, |ui| {
                                ui.selectable_value(&mut self.render.format, f, f.label());
                            });
                        }
                    });
                    ui.end_row();
                    }
                    ui.label("Quality");
                    ui.add(egui::Slider::new(&mut self.render.quality, 50..=100));
                    ui.end_row();
                    ui.label("Background");
                    egui::ComboBox::from_id_salt("bg").selected_text(self.render.background.label()).show_ui(ui, |ui| {
                        for b in [Background::Theme, Background::White, Background::Dark] {
                            ui.selectable_value(&mut self.render.background, b, b.label());
                        }
                    });
                    ui.end_row();
                    ui.label("Anti-aliasing");
                    ui.checkbox(&mut self.render.supersample, "2× supersampling");
                    ui.end_row();
                });
            if video && !ff {
                ui.label(RichText::new("MP4 needs ffmpeg on the PATH (ffmpeg.org). AVI plays in Windows, PowerPoint and VLC.").weak().small());
            }
        });
    }

    /// Editorul de keyframe-uri al camerei (avansat; ascuns implicit in fereastra Animations).
    pub(super) fn anim_keys_ui(&mut self, ui: &mut egui::Ui, ai: usize) {
                ui.horizontal_wrapped(|ui| {
                    if ui.button("📷 Capture keyframe").on_hover_text("Adds the current 3D view").clicked() {
                        let a = &mut self.animations[ai];
                        let t = if a.keys.is_empty() { 0.0 } else { a.duration() + self.key_step };
                        a.keys.push(crate::anim::Keyframe { t, cam: self.cam });
                    }
                    ui.menu_button("➕ Viewpoint ⏷", |ui| {
                        for v in self.viewpoints.clone() {
                            if ui.button(&v.name).clicked() {
                                let a = &mut self.animations[ai];
                                let t = if a.keys.is_empty() { 0.0 } else { a.duration() + self.key_step };
                                a.keys.push(crate::anim::Keyframe { t, cam: v.cam });
                                ui.close_menu();
                            }
                        }
                        if self.viewpoints.is_empty() {
                            ui.label(RichText::new("No viewpoints").weak());
                        }
                    });
                });
                let mut goto = None;
                let mut kdel = None;
                let mut kupd = None;
                let mut resort = false;
                egui::ScrollArea::vertical().id_salt("keys").max_height((ui.available_height() - 110.0).max(60.0)).show(ui, |ui| {
                    egui::Grid::new("keygrid").num_columns(4).striped(true).spacing([8.0, 3.0]).show(ui, |ui| {
                        for (k, key) in self.animations[ai].keys.iter_mut().enumerate() {
                            ui.label(format!("#{}", k + 1));
                            if ui.add(egui::DragValue::new(&mut key.t).range(0.0..=3600.0).speed(0.05).fixed_decimals(2).suffix(" s")).changed() {
                                resort = true;
                            }
                            ui.horizontal(|ui| {
                                if ui.small_button("Go to").clicked() {
                                    goto = Some(key.t);
                                }
                                if ui.small_button("Update").on_hover_text("Replace with the current view").clicked() {
                                    kupd = Some(k);
                                }
                            });
                            if ui.small_button("✖").clicked() {
                                kdel = Some(k);
                            }
                            ui.end_row();
                        }
                    });
                });
                if resort && !ui.ctx().is_using_pointer() {
                    self.animations[ai].sort();
                }
                if let Some(k) = kupd {
                    self.animations[ai].keys[k].cam = self.cam;
                }
                if let Some(k) = kdel {
                    self.animations[ai].keys.remove(k);
                }
                if let Some(t) = goto {
                    self.stop_anim();
                    self.goto_anim_time(t);
                }
    }

    /// Fereastra Animations: lista animatiilor cu Play pe fiecare, playerul, exportul video.
    pub(super) fn animations_ui(&mut self, ui: &mut egui::Ui) {
        if self.cur_anim >= self.animations.len() {
            self.cur_anim = self.animations.len().saturating_sub(1);
        }
        let busy = self.render_job.is_some();
        let weak = ui.visuals().weak_text_color();
        let head = |ui: &mut egui::Ui, t: &str| {
            ui.label(RichText::new(t).small().strong().color(weak));
        };
        ui.columns(3, |cols| {
            // ================= 1. lista animatiilor
            let ui = &mut cols[0];
            head(ui, "CREATE");
            ui.horizontal_wrapped(|ui| {
                let rec = self.anim_rec.is_some();
                let rb = egui::Button::new(RichText::new(if rec { "⏹ Stop recording" } else { "⏺ Record" }).color(if rec { Color32::WHITE } else { Color32::from_rgb(225, 75, 75) })).fill(if rec { Color32::from_rgb(200, 50, 50) } else { ui.visuals().widgets.inactive.weak_bg_fill });
                if ui.add_enabled(!busy, rb).on_hover_text("Record your navigation in the 3D view (orbit, walk, zoom) as a new animation").clicked() {
                    self.toggle_record();
                }
                if ui.add_enabled(!busy, egui::Button::new("🔄 Orbit 360°")).on_hover_text("A full turn around the model, at the current height and distance").clicked() {
                    let bb = self.scene_aabb();
                    let a = Animation::orbit(format!("Orbit {}", self.animations.len() + 1), &self.cam, &bb, self.orbit_secs);
                    self.animations.push(a);
                    self.cur_anim = self.animations.len() - 1;
                    self.anim_time = 0.0;
                }
                if ui.add_enabled(!busy && !self.viewpoints.is_empty(), egui::Button::new("📷 From viewpoints")).on_hover_text(if self.viewpoints.is_empty() { "Save some viewpoints first (Viewpoint › Save View)" } else { "Fly through all saved viewpoints, in order" }).clicked() {
                    self.anim_from_viewpoints();
                }
                let has_path = self.walk_path.len() >= 2;
                if ui.add_enabled(!busy && has_path, egui::Button::new("🚶 Walk path")).on_hover_text(if has_path { "The walk path you drew (Viewpoint › Walk Path › Draw) as an animation" } else { "Draw a walk path first: Viewpoint › Walk Path › Draw" }).clicked() {
                    self.walk_path_to_animation();
                }
                if ui.add_enabled(!busy, egui::Button::new("📅 Import 4D")).on_hover_text("The TimeLiner 4D construction sequence as an animation (imports a schedule first if there is none)").clicked() {
                    self.import_4d_animation();
                }
            });
            if let Some((start, keys)) = &self.anim_rec {
                egui::Frame::none().fill(Color32::from_rgb(200, 50, 50)).rounding(6.0).inner_margin(6.0).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new(format!("⏺ Recording {:.1} s ({} keys) - move around in the 3D view, then press Stop recording.", start.elapsed().as_secs_f32(), keys.len())).color(Color32::WHITE));
                });
                ui.ctx().request_repaint();
            }
            ui.add_space(6.0);
            head(ui, "ANIMATIONS");
            let mut del = None;
            let mut play_row: Option<usize> = None;
            egui::ScrollArea::vertical().id_salt("anim_list").auto_shrink([false, false]).show(ui, |ui| {
                if self.animations.is_empty() {
                    ui.label(RichText::new("No animations yet. Create one above: Record your navigation, an Orbit 360°, a walkthrough of your viewpoints, your walk path, or the 4D construction sequence.").color(weak));
                }
                for i in 0..self.animations.len() {
                    let cur = i == self.cur_anim;
                    let playing = cur && self.anim_play.is_some();
                    let fill = if cur { accent().gamma_multiply(0.18) } else { ui.visuals().faint_bg_color };
                    egui::Frame::none().fill(fill).rounding(6.0).inner_margin(egui::Margin::symmetric(6.0, 4.0)).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            let b = egui::Button::new(RichText::new(if playing { "⏸" } else { "▶" }).size(16.0).color(Color32::WHITE)).fill(if playing { Color32::from_rgb(230, 150, 40) } else { accent() }).min_size(EV::new(34.0, 28.0)).rounding(14.0);
                            if ui.add_enabled(!busy && self.animations[i].duration() > 0.0, b).on_hover_text(if playing { "Pause" } else { "Play this animation in the 3D view" }).clicked() {
                                play_row = Some(i);
                            }
                            let (a_icon, a_name, a_kind, d) = { let a = &self.animations[i]; (a.kind.icon(), a.name.clone(), a.kind.label(), a.duration()) };
                            ui.vertical(|ui| {
                                let r = ui.add(egui::Label::new(RichText::new(format!("{a_icon} {a_name}")).strong()).sense(Sense::click())).on_hover_text("Select");
                                if r.clicked() && !cur {
                                    self.stop_anim();
                                    self.cur_anim = i;
                                    self.anim_time = 0.0;
                                }
                                let t = if cur { self.anim_time } else { 0.0 };
                                ui.label(RichText::new(format!("{a_kind} · {:.1} / {:.1} s", t.min(d), d)).small().color(weak));
                            });
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.add_enabled(!busy, egui::Button::new("🗑").small()).on_hover_text("Delete this animation").clicked() {
                                    del = Some(i);
                                }
                            });
                        });
                        if cur {
                            let d = self.animations[i].duration().max(1e-3);
                            ui.add(egui::ProgressBar::new((self.anim_time / d).clamp(0.0, 1.0)).desired_height(4.0));
                        }
                    });
                    ui.add_space(3.0);
                }
            });
            if let Some(i) = play_row {
                if i == self.cur_anim && self.anim_play.is_some() {
                    self.anim_play = None;
                } else {
                    if i != self.cur_anim {
                        self.stop_anim();
                        self.cur_anim = i;
                        self.anim_time = 0.0;
                    }
                    self.play_anim(1.0);
                }
            }
            if let Some(i) = del {
                self.stop_anim();
                self.animations.remove(i);
                self.cur_anim = self.cur_anim.min(self.animations.len().saturating_sub(1));
                self.anim_time = 0.0;
            }

            // ================= 2. playerul
            let ui = &mut cols[1];
            let ai = self.cur_anim;
            if ai >= self.animations.len() {
                head(ui, "PLAYER");
                ui.label(RichText::new("Create or select an animation on the left.").color(weak));
            } else {
                head(ui, "PLAYER");
                let dur = self.animations[ai].duration();
                ui.label(RichText::new(format!("{} {}", self.animations[ai].kind.icon(), self.animations[ai].name)).strong().size(15.0));
                ui.label(RichText::new(format!("{:.1} s  /  {:.1} s", self.anim_time.min(dur), dur)).size(18.0).monospace());
                let can = !busy && dur > 0.0;
                ui.horizontal(|ui| {
                    let mut t = self.anim_time;
                    ui.spacing_mut().slider_width = (ui.available_width() - 10.0).max(80.0);
                    if ui.add_enabled(can, egui::Slider::new(&mut t, 0.0..=dur.max(0.01)).show_value(false)).changed() {
                        self.anim_play = None;
                        self.goto_anim_time(t);
                    }
                });
                ui.horizontal(|ui| {
                    let playing = self.anim_play.is_some();
                    let fwd = playing && self.anim_dir > 0.0;
                    let back = playing && self.anim_dir < 0.0;
                    let base = ui.visuals().widgets.inactive.weak_bg_fill;
                    let tb = |icon: &str, on: bool| egui::Button::new(RichText::new(icon).size(15.0)).min_size(EV::new(34.0, 28.0)).fill(if on { accent().gamma_multiply(0.5) } else { base });
                    if ui.add_enabled(can, tb("⏮", false)).on_hover_text("Back to the start").clicked() {
                        self.anim_play = None;
                        self.goto_anim_time(0.0);
                    }
                    if ui.add_enabled(can, tb("⏪", false)).on_hover_text("Previous keyframe").clicked() {
                        self.step_anim(-1);
                    }
                    if ui.add_enabled(can, tb("◀", back)).on_hover_text("Play backwards").clicked() {
                        self.play_anim(-1.0);
                    }
                    if ui.add_enabled(can && playing, tb("⏸", false)).on_hover_text("Pause").clicked() {
                        self.anim_play = None;
                    }
                    if ui.add_enabled(!busy, tb("⏹", false)).on_hover_text("Stop: back to the start").clicked() {
                        self.stop_anim();
                        self.goto_anim_time(0.0);
                        self.stop_anim();
                    }
                    let pb = egui::Button::new(RichText::new("▶").size(17.0).color(Color32::WHITE)).min_size(EV::new(44.0, 28.0)).fill(if fwd { Color32::from_rgb(230, 150, 40) } else { accent() });
                    if ui.add_enabled(can, pb).on_hover_text("Play").clicked() {
                        self.play_anim(1.0);
                    }
                    if ui.add_enabled(can, tb("⏩", false)).on_hover_text("Next keyframe").clicked() {
                        self.step_anim(1);
                    }
                    if ui.add_enabled(can, tb("⏭", false)).on_hover_text("Go to the end").clicked() {
                        self.anim_play = None;
                        self.goto_anim_time(dur);
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    if ui.checkbox(&mut self.anim_loop, "Loop").on_hover_text("Start again at the end").changed() && self.anim_loop {
                        self.anim_pp = false;
                    }
                    if ui.checkbox(&mut self.anim_pp, "Ping-pong").on_hover_text("At the end, play backwards, then forwards again").changed() && self.anim_pp {
                        self.anim_loop = false;
                    }
                    ui.checkbox(&mut self.animations[ai].ease, "Smooth start / end");
                    let has_tasks = !self.tasks.is_empty();
                    ui.add_enabled(has_tasks, egui::Checkbox::new(&mut self.animations[ai].sim_4d, "Play 4D")).on_hover_text(if has_tasks { "The TimeLiner construction sequence runs from start to end during the animation" } else { "No TimeLiner schedule - use Import 4D" });
                });
                ui.add_space(6.0);
                egui::CollapsingHeader::new(RichText::new("✏ Edit keyframes and object animation (advanced)").color(weak)).id_salt(("anim_adv", ai)).default_open(false).show(ui, |ui| {
                    egui::ScrollArea::vertical().id_salt("anim_adv_scroll").max_height(260.0).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label("Orbit length");
                            ui.add(egui::DragValue::new(&mut self.orbit_secs).range(2.0..=600.0).suffix(" s"));
                            ui.label("Key step");
                            ui.add(egui::DragValue::new(&mut self.key_step).range(0.5..=60.0).speed(0.1).suffix(" s"));
                        });
                        self.anim_keys_ui(ui, ai);
                        self.anim_sets_ui(ui, ai);
                    });
                });
            }

            // ================= 3. exportul video
            let ui = &mut cols[2];
            head(ui, "EXPORT VIDEO");
            if let Some(j) = &self.render_job {
                let f = j.frame as f32 / j.frames.max(1) as f32;
                let txt = if j.frame >= j.frames { "Finishing…".to_string() } else { format!("Frame {} / {}", j.frame, j.frames) };
                ui.label(RichText::new("Exporting - the 3D view shows each frame.").strong());
                ui.add(egui::ProgressBar::new(f).text(txt).desired_width(ui.available_width()));
                if ui.button("Cancel").clicked() {
                    self.cancel_render();
                }
                return;
            }
            let info = self.animations.get(self.cur_anim).map(|a| (a.name.clone(), a.duration(), a.keys.len(), a.sets.iter().filter(|s| s.enabled).count(), a.sim_4d));
            let fps = self.render.fps.max(1);
            let can = info.as_ref().map_or(false, |i| i.1 > 0.0 && i.2 >= 1) && self.anim_rec.is_none();
            egui::Frame::group(ui.style()).rounding(6.0).show(ui, |ui| {
                ui.set_width(ui.available_width());
                match &info {
                    None => {
                        ui.label(RichText::new("No animation to export yet.").color(Color32::from_rgb(230, 150, 40)));
                    }
                    Some((name, dur, _nk, ns, sim)) => {
                        let frames = (dur * fps as f32).ceil() as u32 + 1;
                        ui.label(RichText::new(format!("🎬 {name}")).strong().size(14.0));
                        let fmt = match self.render.format {
                            VideoFormat::PngSequence => "PNG images".to_string(),
                            f => f.ext().to_uppercase(),
                        };
                        ui.label(format!("{dur:.1} s · {frames} frames · {}×{} · {fps} fps · {fmt}", self.render.width, self.render.height));
                        let mut extra = Vec::new();
                        if *sim {
                            extra.push("4D sequence".to_string());
                        }
                        if *ns > 0 {
                            extra.push(format!("{ns} animated object sets"));
                        }
                        if !extra.is_empty() {
                            ui.label(RichText::new(format!("with {}", extra.join(", "))).small());
                        }
                    }
                }
            });
            ui.add_space(4.0);
            ui.checkbox(&mut self.render.with_materials, "🎨 Render with materials").on_hover_text("Realistic style for the video: materials and textures, reflections, sun shadows.\nThe 3D view goes back to your style after the export.");
            ui.add_space(4.0);
            let label = match &info {
                Some((name, ..)) => format!("🎞 Export \"{name}\"…"),
                None => "🎞 Export video…".into(),
            };
            if ui.add_enabled(can, egui::Button::new(RichText::new(label).strong().color(Color32::WHITE)).fill(accent()).min_size(EV::new(ui.available_width().min(260.0), 32.0))).on_hover_text("Choose where to save the video").clicked() {
                self.stop_anim();
                self.request_path(Purpose::ExportVideo);
            }
            if self.anim_rec.is_some() {
                ui.label(RichText::new("Stop the recording first.").color(Color32::from_rgb(230, 150, 40)));
            }
            ui.add_space(4.0);
            egui::CollapsingHeader::new("⚙ Video settings").id_salt("vid_settings").default_open(false).show(ui, |ui| {
                self.output_settings_grid(ui, true);
                ui.checkbox(&mut self.render.overlay_4d, "4D date & legend on the video");
                ui.checkbox(&mut self.render.legend_on_images, "Colour legend (Appearance Profiler)");
            });
        });
    }
}
