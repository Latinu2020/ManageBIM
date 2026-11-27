//! Per-user installer for the ManageBIM add-in for Revit (2023 - 2027).
//! The files are embedded in the .exe at compile time, from installers\payload\revit.

#[path = "common.rs"]
mod common;

include!(concat!(env!("OUT_DIR"), "/addin_payload_revit.rs"));

fn main() {
    common::run(common::Kind::Revit, FILES);
}
