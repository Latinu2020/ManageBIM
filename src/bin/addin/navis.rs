//! Installer per utilizator pentru add-in-ul ManageBIM din Navisworks (2022 - 2026).
//! Fisierele sunt incluse in .exe la compilare, din installers\payload\navisworks.

#[path = "common.rs"]
mod common;

include!(concat!(env!("OUT_DIR"), "/addin_payload_navisworks.rs"));

fn main() {
    common::run(common::Kind::Navisworks, FILES);
}
