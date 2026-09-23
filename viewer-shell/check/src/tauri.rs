//! Minimal stand-in for the Tauri surface used by `viewer-shell/src/main.rs`.
//!
//! This deliberately models only the methods the real shell calls. Keeping it
//! small makes this check useful on CI runners that do not have Tauri's native
//! webview development libraries installed.

use std::str::FromStr;

pub struct Builder;

impl Builder {
    pub fn default() -> Self {
        Builder
    }

    pub fn setup<F>(self, f: F) -> Self
    where
        F: FnOnce(&mut App) -> Result<(), Box<dyn std::error::Error>> + Send + 'static,
    {
        // Run the setup closure too. This does not create a real window, but it
        // exercises argv-derived URL parsing and the builder call chain when a
        // small test binary is used against this stub.
        let mut app = App;
        let _ = f(&mut app);
        self
    }

    pub fn run(self, _context: ()) -> Result<(), Box<dyn std::error::Error>> {
        Ok(())
    }
}

pub struct App;

pub enum WebviewUrl {
    External(Url),
}

pub struct WebviewWindowBuilder;

impl WebviewWindowBuilder {
    pub fn new(_app: &mut App, _label: &str, _url: WebviewUrl) -> Self {
        WebviewWindowBuilder
    }

    pub fn title(self, _title: &str) -> Self {
        self
    }

    pub fn inner_size(self, _width: f64, _height: f64) -> Self {
        self
    }

    pub fn visible(self, _visible: bool) -> Self {
        self
    }

    pub fn build(self) -> Result<(), Box<dyn std::error::Error>> {
        Ok(())
    }
}

pub struct Url;

impl FromStr for Url {
    type Err = String;

    fn from_str(_value: &str) -> Result<Self, Self::Err> {
        Ok(Url)
    }
}

macro_rules! generate_context {
    () => {
        ()
    };
}

pub(crate) use generate_context;
