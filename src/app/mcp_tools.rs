//! Uneltele serverului MCP, executate pe firul interfetei (au acces la `App`).

use super::*;
use serde_json::{json, Value};

const MAX_PROPS_ELEMS: usize = 50;

impl App {
    /// Porneste / opreste serverul dupa setari si raspunde cererilor sosite (apelat in fiecare cadru).
    pub fn mcp_step(&mut self, ctx: &eframe::egui::Context) {
        let want = self.settings.mcp.enabled;
        if want && self.mcp.is_none() && self.mcp_status.is_empty() {
            self.mcp_start(ctx);
        } else if !want && self.mcp.is_some() {
            self.mcp = None;
            self.mcp_status.clear();
        }
        let mut calls = Vec::new();
        if let Some(s) = &self.mcp {
            while let Ok(c) = s.rx.try_recv() {
                calls.push(c);
            }
        }
        for c in calls {
            let r = if crate::mcp::is_edit(&c.tool) && !self.settings.mcp.allow_edit {
                Err("Changes from AI assistants are turned off in ManageBIM (Collaborate › AI connection › Allow changes).".to_string())
            } else {
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.mcp_tool(&c.tool, &c.args))).unwrap_or_else(|_| Err("internal error".into()))
            };
            let _ = c.reply.send(r);
        }
    }

    pub fn mcp_start(&mut self, ctx: &eframe::egui::Context) {
        if self.settings.mcp.token.is_empty() {
            self.settings.mcp.token = crate::mcp::new_token();
            self.settings.save();
        }
        let c = ctx.clone();
        match crate::mcp::start(self.settings.mcp.port, &self.settings.mcp.token, move || c.request_repaint()) {
            Ok(s) => {
                self.mcp = Some(s);
                self.mcp_status.clear();
            }
            Err(e) => {
                // nu reincercam in fiecare cadru: utilizatorul schimba portul sau apasa Restart
                self.mcp_status = e;
            }
        }
    }

    pub fn mcp_restart(&mut self, ctx: &eframe::egui::Context) {
        self.mcp = None;
        self.mcp_status.clear();
        if self.settings.mcp.enabled {
            self.mcp_start(ctx);
        }
    }

    fn mcp_ids(&self, args: &Value) -> Result<Vec<ElemId>, String> {
        match args.get("ids").and_then(|v| v.as_array()) {
            Some(a) if !a.is_empty() => {
                let mut out = Vec::new();
                for v in a {
                    let s = v.as_str().map(|s| s.to_string()).unwrap_or_else(|| v.to_string());
                    let id = parse_id(&s).ok_or_else(|| format!("bad id {s:?} (expected \"model:element\")"))?;
                    if self.elem(id).is_none() {
                        return Err(format!("no element {s}"));
                    }
                    out.push(id);
                }
                Ok(out)
            }
            _ => Ok(self.selected.iter().copied().collect()),
        }
    }

    fn mcp_elem_brief(&self, id: ElemId) -> Value {
        match self.elem(id) {
            Some((_, e)) => json!({ "id": fmt_id(id), "name": e.name, "category": e.ifc_type, "level": e.storey, "guid": e.guid }),
            None => json!({ "id": fmt_id(id) }),
        }
    }

    pub fn mcp_tool(&mut self, tool: &str, args: &Value) -> Result<Value, String> {
        let s = |k: &str| args.get(k).and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
        match tool {
            "get_model_info" => {
                let mut models = Vec::new();
                for (mi, m) in self.loaded_models() {
                    let mut cats: std::collections::BTreeMap<&str, usize> = Default::default();
                    let mut levels: std::collections::BTreeSet<&str> = Default::default();
                    for e in &m.elements {
                        *cats.entry(e.ifc_type.as_str()).or_default() += 1;
                        if !e.storey.is_empty() {
                            levels.insert(e.storey.as_str());
                        }
                    }
                    let size = m.aabb.max - m.aabb.min;
                    models.push(json!({
                        "model": mi,
                        "name": m.name,
                        "file": m.path,
                        "elements": m.elements.len(),
                        "triangles": m.pos.len() / 3,
                        "points": m.cloud.as_ref().map(|c| c.pos.len()).unwrap_or(0),
                        "size_m": [r3(size.x), r3(size.y), r3(size.z)],
                        "levels": levels,
                        "categories": cats,
                    }));
                }
                Ok(json!({
                    "app": format!("ManageBIM {}", env!("CARGO_PKG_VERSION")),
                    "models": models,
                    "selected": self.selected.len(),
                    "hidden": self.hidden.len(),
                    "clash_tests": self.clash_tests.len(),
                    "units": "metres",
                }))
            }
            "find_elements" => {
                let low = |k: &str| s(k).to_lowercase();
                let (text, cat, lvl, prop, val) = (low("text"), low("category"), low("level"), low("property"), low("value"));
                let vis_only = args.get("visible_only").and_then(|v| v.as_bool()).unwrap_or(false);
                let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(100).clamp(1, 5000) as usize;
                let mut found = Vec::new();
                let mut total = 0usize;
                for (mi, m) in self.loaded_models() {
                    for (ei, e) in m.elements.iter().enumerate() {
                        let id = ElemId { m: mi as u32, e: ei as u32 };
                        if vis_only && !self.is_visible(id) {
                            continue;
                        }
                        if !cat.is_empty() && !e.ifc_type.to_lowercase().contains(&cat) {
                            continue;
                        }
                        if !lvl.is_empty() && !e.storey.to_lowercase().contains(&lvl) {
                            continue;
                        }
                        if !prop.is_empty() {
                            let hit = e.props.iter().any(|p| {
                                let n = p.name.to_lowercase();
                                let gn = format!("{}|{}", p.group.to_lowercase(), n);
                                (n == prop || gn == prop) && (val.is_empty() || p.value.to_lowercase().contains(&val))
                            });
                            if !hit {
                                continue;
                            }
                        } else if !val.is_empty() && !e.props.iter().any(|p| p.value.to_lowercase().contains(&val)) {
                            continue;
                        }
                        if !text.is_empty() {
                            let hit = e.name.to_lowercase().contains(&text)
                                || e.ifc_type.to_lowercase().contains(&text)
                                || e.storey.to_lowercase().contains(&text)
                                || e.guid.to_lowercase() == text
                                || e.props.iter().any(|p| p.value.to_lowercase().contains(&text));
                            if !hit {
                                continue;
                            }
                        }
                        total += 1;
                        if found.len() < limit {
                            found.push(id);
                        }
                    }
                }
                let items: Vec<Value> = found.iter().map(|i| self.mcp_elem_brief(*i)).collect();
                Ok(json!({ "total": total, "returned": items.len(), "elements": items }))
            }
            "get_properties" => {
                let ids = self.mcp_ids(args)?;
                if ids.is_empty() {
                    return Err("nothing selected - pass ids".into());
                }
                let mut out = Vec::new();
                for id in ids.iter().take(MAX_PROPS_ELEMS) {
                    let Some((m, e)) = self.elem(*id) else { continue };
                    let mut groups: std::collections::BTreeMap<&str, serde_json::Map<String, Value>> = Default::default();
                    for p in &e.props {
                        groups.entry(if p.group.is_empty() { "General" } else { p.group.as_str() }).or_default().insert(p.name.clone(), Value::String(p.value.clone()));
                    }
                    let size = e.aabb.max - e.aabb.min;
                    out.push(json!({
                        "id": fmt_id(*id),
                        "model": m.name,
                        "name": e.name,
                        "category": e.ifc_type,
                        "level": e.storey,
                        "guid": e.guid,
                        "bbox_size_m": [r3(size.x), r3(size.y), r3(size.z)],
                        "properties": groups,
                    }));
                }
                Ok(json!({ "count": ids.len(), "returned": out.len(), "elements": out }))
            }
            "get_selection" => {
                let items: Vec<Value> = self.selected.iter().take(500).map(|i| self.mcp_elem_brief(*i)).collect();
                Ok(json!({ "count": self.selected.len(), "elements": items }))
            }
            "select_elements" => {
                let ids = self.mcp_ids(args)?;
                if args.get("ids").and_then(|v| v.as_array()).map(|a| a.is_empty()).unwrap_or(true) {
                    self.apply(Action::Select(Vec::new(), false));
                    return Ok(json!("Selection cleared."));
                }
                let n = ids.len();
                self.apply(Action::Select(ids.clone(), false));
                if args.get("zoom").and_then(|v| v.as_bool()).unwrap_or(false) {
                    self.apply(Action::Focus(ids));
                }
                Ok(json!(format!("Selected {n} element(s).")))
            }
            "set_visibility" => {
                let action = s("action");
                let ids = if action == "show_all" { Vec::new() } else { self.mcp_ids(args)? };
                if action != "show_all" && ids.is_empty() {
                    return Err("no elements (pass ids or select something)".into());
                }
                let n = ids.len();
                match action.as_str() {
                    "hide" => self.apply(Action::Hide(ids)),
                    "show" => self.apply(Action::Show(ids)),
                    "isolate" => self.apply(Action::Isolate(ids)),
                    "show_all" => self.apply(Action::ShowAll),
                    a => return Err(format!("unknown action {a:?} (hide, show, isolate, show_all)")),
                }
                Ok(json!(format!("{action}: {n} element(s); hidden now: {}", self.hidden.len())))
            }
            "zoom_to" => {
                let all = args.get("all").and_then(|v| v.as_bool()).unwrap_or(false);
                let ids = if all { Vec::new() } else { self.mcp_ids(args)? };
                self.apply(Action::Focus(ids));
                Ok(json!("View fitted."))
            }
            "set_property" => {
                let (name, value) = (s("name"), args.get("value").map(|v| v.as_str().map(|s| s.to_string()).unwrap_or_else(|| v.to_string())).unwrap_or_default());
                if name.is_empty() {
                    return Err("name is required".into());
                }
                let ids = self.mcp_ids(args)?;
                if ids.is_empty() {
                    return Err("no elements (pass ids or select something)".into());
                }
                let n = self.set_user_param(&ids, &name, &value);
                self.toast(format!("AI assistant: set {name} on {n} element(s)."));
                Ok(json!(format!("Set {name} = {value:?} on {n} element(s).")))
            }
            "list_clash_tests" => {
                let tests: Vec<Value> = self.clash_tests.iter().enumerate().map(|(i, t)| self.mcp_clash_summary(i, t, 0)).collect();
                Ok(json!({ "tests": tests }))
            }
            "run_clash_test" => {
                let key = args.get("test").map(|v| v.as_str().map(|s| s.to_string()).unwrap_or_else(|| v.to_string())).unwrap_or_default();
                let idx = key
                    .trim()
                    .parse::<usize>()
                    .ok()
                    .filter(|i| *i < self.clash_tests.len())
                    .or_else(|| self.clash_tests.iter().position(|t| t.name.eq_ignore_ascii_case(key.trim())))
                    .ok_or_else(|| format!("no clash test {key:?}; use list_clash_tests"))?;
                if self.clash_job.is_some() {
                    return Err("a clash test is already running in ManageBIM".into());
                }
                self.run_clash_blocking(idx);
                self.cur_test = idx;
                self.colors_dirty = true;
                let max = args.get("max_results").and_then(|v| v.as_u64()).unwrap_or(25).min(500) as usize;
                Ok(self.mcp_clash_summary(idx, &self.clash_tests[idx], max))
            }
            "run_script" => {
                let src = s("source");
                if src.is_empty() {
                    return Err("source is required".into());
                }
                let input = self.script_input();
                let out = crate::scripting::run(&src, &input)?;
                let logs = out.logs.clone();
                let n = out.actions.len();
                self.run_source("AI script", &src);
                Ok(json!({ "log": logs, "actions": n, "selected": self.selected.len() }))
            }
            "open_model" => {
                let p = s("path");
                if !std::path::Path::new(&p).is_file() {
                    return Err(format!("file not found: {p}"));
                }
                self.open_path(&p, true);
                Ok(json!(format!("Opening {p} - call get_model_info in a few seconds to see it.")))
            }
            t => Err(format!("unknown tool {t}")),
        }
    }

    fn mcp_clash_summary(&self, i: usize, t: &ClashTest, max: usize) -> Value {
        let mut by_status: std::collections::BTreeMap<String, usize> = Default::default();
        let mut by_sev: std::collections::BTreeMap<&str, usize> = Default::default();
        for r in &t.results {
            *by_status.entry(format!("{:?}", r.status)).or_default() += 1;
            *by_sev.entry(t.severity(r.dist).label()).or_default() += 1;
        }
        let results: Vec<Value> = t
            .results
            .iter()
            .take(max)
            .map(|r| {
                json!({
                    "name": r.name,
                    "a": self.mcp_elem_brief(r.a),
                    "b": self.mcp_elem_brief(r.b),
                    "distance_mm": (r.dist * 1000.0 * 10.0).round() / 10.0,
                    "point": r.point,
                    "status": format!("{:?}", r.status),
                    "severity": t.severity(r.dist).label(),
                    "group": r.group,
                })
            })
            .collect();
        let mut v = json!({
            "index": i,
            "name": t.name,
            "type": format!("{:?}", t.ty),
            "tolerance_mm": t.tol_mm,
            "last_run": t.last_run,
            "results": t.results.len(),
            "by_status": by_status,
            "by_severity": by_sev,
        });
        if max > 0 {
            v["first_results"] = Value::Array(results);
        }
        v
    }
}

fn fmt_id(id: ElemId) -> String {
    format!("{}:{}", id.m, id.e)
}

fn parse_id(s: &str) -> Option<ElemId> {
    let (a, b) = s.trim().trim_matches('"').split_once(':')?;
    Some(ElemId { m: a.trim().parse().ok()?, e: b.trim().parse().ok()? })
}

/// metri cu 3 zecimale (mm) - fara zgomotul f32 in JSON
fn r3(v: f32) -> f64 {
    (v as f64 * 1000.0).round() / 1000.0
}
