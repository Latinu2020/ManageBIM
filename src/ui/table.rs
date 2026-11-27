//! The common table component of the application (Clash Detective, TimeLiner, ...).
//!
//! It provides three things, the same in all tables:
//!  * **draggable columns (stretch)** - the user drags the separator in the header to
//!    change the width of a column; `Column::exact` would lock the width, that is why
//!    `Column::initial` plus `TableBuilder::resizable(true)` is used;
//!  * **rename with right click > Rename** - the name box gets focus automatically;
//!  * the same alternating stripes and row click sensitivity, everywhere.
//!
//! Creating a row stays with each table („➕ Add Test", „➕ Task"), because
//! the names and default values differ from table to table.

use eframe::egui;
use egui_extras::{Column, TableBuilder};

/// `TableBuilder` with the common settings: alternating stripes, columns resizable
/// by dragging the separator in the header, and row click.
pub fn table<'a>(ui: &'a mut egui::Ui) -> TableBuilder<'a> {
    TableBuilder::new(ui).striped(true).resizable(true).sense(egui::Sense::click())
}

/// Text column: the previous default width, draggable, with the content clipped
/// if it does not fit (otherwise a column would widen by itself to the long text).
pub fn text(width: f32) -> Column {
    Column::initial(width).clip(true)
}

/// Narrow column for numbers (counters) - draggable.
pub fn num(width: f32) -> Column {
    Column::initial(width)
}

/// Right click > Rename: marks row `row` as being renamed.
/// It is called from inside `row.response().context_menu(...)`.
pub fn rename_item(ui: &mut egui::Ui, renaming: &mut Option<usize>, row: usize) {
    if ui.button("✏ Rename").clicked() {
        *renaming = Some(row);
        ui.close_menu();
    }
}

/// The name cell: the text box that renames inline (Enter or a click
/// outside confirms). If `renaming` points to this row, the box gets the focus
/// automatically - that is why it does not need to be pressed twice.
///
/// Returns the new name when the user confirmed a change; the caller
/// will apply it **after** the table row (otherwise the borrow on `self` would be violated).
pub fn name_cell(ui: &mut egui::Ui, key: egui::Id, row: usize, renaming: &mut Option<usize>, name: &str) -> Option<String> {
    let focus = *renaming == Some(row);
    if focus {
        *renaming = None;
    }
    // the buffer stays in egui memory while the box has focus (it is not lost on re-render)
    let buf_id = key.with(("name_cell", row));
    let mut buf: String = ui.data(|d| d.get_temp(buf_id)).unwrap_or_else(|| name.to_string());
    let resp = ui.add(egui::TextEdit::singleline(&mut buf).desired_width(f32::INFINITY));
    if focus {
        resp.request_focus();
    }
    if resp.has_focus() {
        ui.data_mut(|d| d.insert_temp(buf_id, buf.clone()));
        return None;
    }
    if resp.lost_focus() {
        ui.data_mut(|d| d.remove::<String>(buf_id));
        let new = buf.trim().to_string();
        if !new.is_empty() && new != name {
            return Some(new);
        }
    }
    None
}
