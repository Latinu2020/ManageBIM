//! Componenta comuna a tabelelor din aplicatie (Clash Detective, TimeLiner, ...).
//!
//! Ofera trei lucruri, la fel in toate tabelele:
//!  * **coloane trăsăbilă (stretch)** - utilizatorul trage separatorul din antet ca sa
//!    schimbe lățimea unei coloane; `Column::exact` ar bloca lățimea, de aceea se
//!    folosește `Column::initial` plus `TableBuilder::resizable(true)`;
//!  * **redenumire cu clic dreapta > Rename** - caseta de nume primeste focus automat;
//!  * aceleasi bare alternate si sensibilitate la clic pe rand, peste tot.
//!
//! Crearea unui rand ramane la fiecare tabel („➕ Add Test", „➕ Task"), pentru ca
//! numele si valorile implicite difera de la tabel la tabel.

use eframe::egui;
use egui_extras::{Column, TableBuilder};

/// `TableBuilder` cu setarile comune: bare alternate, coloane redimensionabile
/// prin drag pe separatorul din antet si clic pe rand.
pub fn table<'a>(ui: &'a mut egui::Ui) -> TableBuilder<'a> {
    TableBuilder::new(ui).striped(true).resizable(true).sense(egui::Sense::click())
}

/// Coloana de text: lățimea implicita de dinainte, trăsăbilă, cu conținutul taiat
/// daca nu incape (altfel o coloana s-ar largi singura dupa textul lung).
pub fn text(width: f32) -> Column {
    Column::initial(width).clip(true)
}

/// Coloana ingusta pentru numere (contoare) - trăsăbilă.
pub fn num(width: f32) -> Column {
    Column::initial(width)
}

/// Clic dreapta > Rename: marcheaza randul `row` ca fiind in redenumire.
/// Se apeleaza din interiorul `row.response().context_menu(...)`.
pub fn rename_item(ui: &mut egui::Ui, renaming: &mut Option<usize>, row: usize) {
    if ui.button("✏ Rename").clicked() {
        *renaming = Some(row);
        ui.close_menu();
    }
}

/// Celula de nume: caseta de text care se redenumeste inline (Enter sau click in
/// afara confirma). Daca `renaming` indica acest rand, caseta primeste focusul
/// automat - de aceea nu trebuie apasat de doua ori.
///
/// Intoarce noul nume atunci cand utilizatorul a confirmat o schimbare; apelantul
/// o sa-l aplice **dupa** randul de tabel (altfel s-ar calca imprumutul pe `self`).
pub fn name_cell(ui: &mut egui::Ui, key: egui::Id, row: usize, renaming: &mut Option<usize>, name: &str) -> Option<String> {
    let focus = *renaming == Some(row);
    if focus {
        *renaming = None;
    }
    // buffer-ul sta in memoria egui cat timp caseta are focus (nu se pierde la reRandare)
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
