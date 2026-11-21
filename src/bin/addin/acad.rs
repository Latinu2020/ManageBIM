//! Installer per utilizator pentru add-in-ul ManageBIM din AutoCAD / Plant 3D (2021+).
//! Fisierele sunt incluse in .exe la compilare, din installers\payload\acad.

#[path = "common.rs"]
mod common;

include!(concat!(env!("OUT_DIR"), "/addin_payload_acad.rs"));

fn main() {
    common::run(common::Kind::Acad, FILES);
}
