//! Compile the real viewer-shell source against a tiny Tauri-shaped stub.
//!
//! This catches changes to `viewer-shell/src/main.rs` without requiring the
//! platform WebKit/GTK stack or the full Tauri dependency tree. It is a
//! compiler pass, not a second implementation of the viewer.

#![allow(dead_code, unused_imports)]

mod tauri;

include!(concat!(env!("OUT_DIR"), "/checked.rs"));
