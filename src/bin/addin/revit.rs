//! Installer per utilizator pentru add-in-ul ManageBIM din Revit (2023 - 2027).
//! Fisierele sunt incluse in .exe la compilare, din installers\payload\revit.

#[path = "common.rs"]
mod common;

include!(concat!(env!("OUT_DIR"), "/addin_payload_revit.rs"));

fn main() {
    common::run(common::Kind::Revit, FILES);
}
