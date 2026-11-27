// Customizable keybindings (0.27.12). A string of the form "Ctrl+Shift+O" is
// matched against the egui keyboard state. Only the overrides are saved in
// Settings::keybinds; the rest stay at the values in COMMANDS.

use eframe::egui::Key;

/// The commands that have a keybinding.
pub const COMMANDS: [(&str, &str); 8] = [
    ("open", "Ctrl+O"),
    ("save", "Ctrl+S"),
    ("find", "Ctrl+F"),
    ("palette", "Ctrl+K"),
    ("undo", "Ctrl+Z"),
    ("redo", "Ctrl+Y"),
    ("hide_panels", "F11"),
    ("section_up", "PgUp"),
];

/// Resolves the combination for a command: the override from the settings, otherwise the default.
pub fn binding(keybinds: &std::collections::BTreeMap<String, String>, cmd: &str) -> String {
    keybinds.get(cmd).cloned().unwrap_or_else(|| {
        COMMANDS.iter().find(|(c, _)| *c == cmd).map(|(_, k)| k.to_string()).unwrap_or_default()
    })
}

/// Parses "Ctrl+Shift+O" → (ctrl/cmd, shift, alt, Key).
fn parse(s: &str) -> Option<(bool, bool, bool, Key)> {
    let mut ctrl = false;
    let mut shift = false;
    let mut alt = false;
    let mut key = None;
    for part in s.split('+') {
        match part.trim().to_ascii_lowercase().as_str() {
            "ctrl" | "cmd" => ctrl = true,
            "shift" => shift = true,
            "alt" => alt = true,
            k => {
                key = match k {
                    "o" => Some(Key::O), "s" => Some(Key::S), "f" => Some(Key::F),
                    "k" => Some(Key::K), "z" => Some(Key::Z), "y" => Some(Key::Y),
                    "a" => Some(Key::A), "h" => Some(Key::H), "i" => Some(Key::I),
                    "e" => Some(Key::E), "r" => Some(Key::R), "t" => Some(Key::T),
                    "f11" => Some(Key::F11), "f3" => Some(Key::F3), "f5" => Some(Key::F5),
                    "pgup" | "pageup" => Some(Key::PageUp), "pgdn" | "pagedown" => Some(Key::PageDown),
                    "esc" | "escape" => Some(Key::Escape), "enter" => Some(Key::Enter),
                    "delete" | "del" => Some(Key::Delete), "home" => Some(Key::Home),
                    _ => None,
                };
            }
        }
    }
    key.map(|k| (ctrl, shift, alt, k))
}

/// True if the user pressed the given combination in this frame.
pub fn pressed(ctx: &eframe::egui::Context, spec: &str) -> bool {
    let Some((ctrl, shift, alt, key)) = parse(spec) else { return false };
    ctx.input(|i| {
        i.key_pressed(key)
            && i.modifiers.command == ctrl
            && i.modifiers.shift == shift
            && i.modifiers.alt == alt
    })
}

/// Checks whether a combination is already used by another command.
pub fn conflicts(keybinds: &std::collections::BTreeMap<String, String>, cmd: &str, spec: &str) -> Option<String> {
    let norm = |s: &str| s.split('+').map(|x| x.trim().to_ascii_lowercase()).collect::<Vec<_>>().join("+");
    let target = norm(spec);
    for (other, _) in COMMANDS {
        if other == cmd { continue; }
        if norm(&binding(keybinds, other)) == target {
            return Some(other.to_string());
        }
    }
    None
}
