//! Compile-checks the cell viewer's command bodies without Tauri (see build.rs).
#![allow(clippy::all)]

include!(concat!(env!("OUT_DIR"), "/checked.rs"));
