//! Arrangeable panels: every tool window can be docked left / right / bottom or floated.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub enum Panel {
    Tree,
    PropTree,
    Sets,
    Properties,
    Find,
    Clash,
    TimeLiner,
    Viewpoints,
    Section,
    Measure,
    Coords,
    Render,
    Animation,
    Project,
    Profiler,
    Compare,
    Quantities,
    Check,
}

pub const ALL_PANELS: [Panel; 18] = [
    Panel::Tree,
    Panel::PropTree,
    Panel::Sets,
    Panel::Properties,
    Panel::Find,
    Panel::Clash,
    Panel::TimeLiner,
    Panel::Viewpoints,
    Panel::Section,
    Panel::Measure,
    Panel::Coords,
    Panel::Render,
    Panel::Animation,
    Panel::Project,
    Panel::Profiler,
    Panel::Compare,
    Panel::Quantities,
    Panel::Check,
];

impl Panel {
    pub fn title(self) -> &'static str {
        crate::i18n::tr(self.title_en())
    }
    /// the title in English (the translation key)
    pub fn title_en(self) -> &'static str {
        match self {
            Panel::Tree => "🌲 Selection Tree",
            Panel::PropTree => "🔎 Property Tree",
            Panel::Sets => "📋 Sets",
            Panel::Properties => "ℹ Properties",
            Panel::Find => "🔍 Find Items",
            Panel::Clash => "⚠ Clash Detective",
            Panel::TimeLiner => "📅 TimeLiner",
            Panel::Viewpoints => "📷 Viewpoints",
            Panel::Section => "✂ Sectioning",
            Panel::Measure => "📏 Measure",
            Panel::Coords => "📐 Coordinates",
            Panel::Render => "🎬 Render",
            Panel::Animation => "🎬 Animations",
            Panel::Project => "📁 Project",
            Panel::Profiler => "🎨 Appearance Profiler",
            Panel::Compare => "⚖ Compare",
            Panel::Quantities => "📊 Quantification",
            Panel::Check => "✔ Model Checker",
        }
    }
    /// short name for the tabs in the side panels (the segmented control)
    pub fn short(self) -> &'static str {
        crate::i18n::tr(self.short_en())
    }
    pub fn short_en(self) -> &'static str {
        match self {
            Panel::Tree => "Tree",
            Panel::PropTree => "Prop Tree",
            Panel::Sets => "Sets",
            Panel::Properties => "Properties",
            Panel::Find => "Find",
            Panel::Clash => "Clash",
            Panel::TimeLiner => "TimeLiner",
            Panel::Viewpoints => "Viewpoints",
            Panel::Section => "Section",
            Panel::Measure => "Measure",
            Panel::Coords => "Coordinates",
            Panel::Render => "Render",
            Panel::Animation => "Animations",
            Panel::Project => "Project",
            Panel::Profiler => "Profiler",
            Panel::Compare => "Compare",
            Panel::Quantities => "Quantities",
            Panel::Check => "Checker",
        }
    }
    pub fn default_dock(self) -> Dock {
        match self {
            Panel::Tree | Panel::PropTree | Panel::Sets => Dock::Left,
            Panel::Properties => Dock::Right,
            _ => Dock::Bottom,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Dock {
    Left,
    Right,
    Bottom,
    Float,
}

impl Dock {
    pub fn idx(self) -> usize {
        match self {
            Dock::Left => 0,
            Dock::Right => 1,
            Dock::Bottom => 2,
            Dock::Float => 3,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Dock::Left => "Dock left",
            Dock::Right => "Dock right",
            Dock::Bottom => "Dock bottom",
            Dock::Float => "Floating window",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
pub struct PanelState {
    pub panel: Panel,
    pub dock: Dock,
    pub open: bool,
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct UiLayout {
    /// order = tab order inside each dock area
    pub panels: Vec<PanelState>,
    /// active tab of the left, right and bottom areas
    pub active: [Option<Panel>; 3],
    /// auto-hide (as in Navisworks): the zone is a thin strip with the tabs; the panel appears over the model on hover / click
    #[serde(default)]
    pub autohide: [bool; 3],
    /// all panels hidden (F11): the whole screen for the model
    #[serde(default)]
    pub hide_all: bool,
    /// the bottom zone raised (its tabs sit in the status bar; click on the active tab lowers it)
    #[serde(default = "crate::layout::yes")]
    pub bottom_open: bool,
    /// the bottom panel maximized (large height for many clashes / tests)
    #[serde(default)]
    pub bottom_tall: bool,
    /// the layout version (one-time migrations; 1 = the bottom zone starts lowered)
    #[serde(default)]
    pub version: u32,
}

impl Default for UiLayout {
    fn default() -> Self {
        UiLayout {
            // new panels start closed (they open from the ribbon), so they do not crowd the bottom zone
            panels: ALL_PANELS.iter().map(|&p| PanelState { panel: p, dock: p.default_dock(), open: !matches!(p, Panel::Profiler | Panel::Compare | Panel::Quantities | Panel::Check | Panel::Animation) }).collect(),
            active: [Some(Panel::Tree), Some(Panel::Properties), Some(Panel::Find)],
            autohide: [false; 3],
            hide_all: false,
            // the bottom zone starts lowered (the tabs in the status bar): the 3D view has all the space
            bottom_open: false,
            bottom_tall: false,
            version: 1,
        }
    }
}

impl UiLayout {
    /// repairs layouts saved by older versions (missing / duplicate panels)
    pub fn fixup(&mut self) {
        // 0.28.5: Find Items no longer takes a quarter of the screen at startup; the user's choice is kept afterwards
        if self.version < 1 {
            self.version = 1;
            self.bottom_open = false;
        }
        let mut seen = Vec::new();
        self.panels.retain(|s| {
            if seen.contains(&s.panel) {
                false
            } else {
                seen.push(s.panel);
                true
            }
        });
        for p in ALL_PANELS {
            if !seen.contains(&p) {
                self.panels.push(PanelState { panel: p, dock: p.default_dock(), open: false });
            }
        }
        for a in 0..3 {
            let ok = self.active[a].map(|p| self.get(p).map(|s| s.open && s.dock.idx() == a).unwrap_or(false)).unwrap_or(false);
            if !ok {
                self.active[a] = self.panels.iter().find(|s| s.open && s.dock.idx() == a).map(|s| s.panel);
            }
        }
    }
    pub fn get(&self, p: Panel) -> Option<&PanelState> {
        self.panels.iter().find(|s| s.panel == p)
    }
    fn get_mut(&mut self, p: Panel) -> &mut PanelState {
        if self.get(p).is_none() {
            self.panels.push(PanelState { panel: p, dock: p.default_dock(), open: false });
        }
        self.panels.iter_mut().find(|s| s.panel == p).unwrap()
    }
    /// open and bring to front
    pub fn open(&mut self, p: Panel) {
        let s = self.get_mut(p);
        s.open = true;
        let d = s.dock;
        if d != Dock::Float {
            self.active[d.idx()] = Some(p);
            if d == Dock::Bottom {
                self.bottom_open = true;
            }
        }
    }
    pub fn close(&mut self, p: Panel) {
        self.get_mut(p).open = false;
        self.fixup();
    }
    pub fn toggle(&mut self, p: Panel) {
        // bottom panel lowered: the button raises it (it does not close it)
        let lowered = !self.bottom_open && self.get(p).map(|s| s.dock == Dock::Bottom && s.open).unwrap_or(false);
        if lowered {
            self.open(p);
        } else if self.shown(p) {
            self.close(p);
        } else {
            self.open(p);
        }
    }
    pub fn move_to(&mut self, p: Panel, d: Dock) {
        let s = self.get_mut(p);
        s.dock = d;
        s.open = true;
        if d != Dock::Float {
            self.active[d.idx()] = Some(p);
        }
        self.fixup();
    }
    /// visible on screen right now (active tab of its area, or a floating window)
    /// the panel is actually visible on screen: shown, not hidden (F11), and the bottom zone raised (when it is at the bottom)
    pub fn visible(&self, p: Panel) -> bool {
        if !self.shown(p) || self.hide_all {
            return false;
        }
        match self.get(p) {
            Some(s) if s.dock == Dock::Bottom => self.bottom_open && !self.autohide[2],
            _ => true,
        }
    }

    pub fn shown(&self, p: Panel) -> bool {
        match self.get(p) {
            Some(s) if s.open => s.dock == Dock::Float || self.active[s.dock.idx()] == Some(p),
            _ => false,
        }
    }
    pub fn is_open(&self, p: Panel) -> bool {
        self.get(p).map(|s| s.open).unwrap_or(false)
    }
    pub fn in_area(&self, d: Dock) -> Vec<Panel> {
        self.panels.iter().filter(|s| s.open && s.dock == d).map(|s| s.panel).collect()
    }
}
pub fn yes() -> bool {
    true
}
