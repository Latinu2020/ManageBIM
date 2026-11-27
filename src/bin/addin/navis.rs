//! Per-user installer for the ManageBIM add-in in Navisworks (2022 - 2026).
//! The files are embedded in the .exe at compile time, from installers\payload\navisworks.

#[path = "common.rs"]
mod common;

include!(concat!(env!("OUT_DIR"), "/addin_payload_navisworks.rs"));

fn main() {
    common::run(common::Kind::Navisworks, FILES);
}
