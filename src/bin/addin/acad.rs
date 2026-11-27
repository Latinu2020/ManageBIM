//! Per-user installer for the ManageBIM add-in for AutoCAD / Plant 3D (2021+).
//! The files are included in the .exe at compile time, from installers\payload\acad.

#[path = "common.rs"]
mod common;

include!(concat!(env!("OUT_DIR"), "/addin_payload_acad.rs"));

fn main() {
    common::run(common::Kind::Acad, FILES);
}
