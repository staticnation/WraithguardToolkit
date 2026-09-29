//! Just enough Tauri to build against, and to run against.
//!
//! This began as a typecheck stub: the real shell needs its whole dependency tree from
//! crates.io, so a signature change in `viewcore` could break it somewhere the build
//! never ran. Compiling the command bodies against these shapes is that missing pass.
//!
//! It is also what lets the commands actually run without Tauri, which is how the
//! browser test suite drives the real engine (`a headless caller`). Nothing here talks
//! to a window — `State` is a borrow of the app state and `Response` is a bag of bytes
//! — but every call into `viewcore` keeps the shape it has in the real crate, which is
//! the only property either use needs.

use std::ops::Deref;

pub struct State<'a, T>(pub &'a T);

impl<'a, T> State<'a, T> {
    pub fn new(inner: &'a T) -> State<'a, T> {
        State(inner)
    }
}

impl<'a, T> Deref for State<'a, T> {
    type Target = T;
    fn deref(&self) -> &T {
        self.0
    }
}

/* Round 18r: the window, for the one command that moves the mouse pointer.
 *
 * A shape, not a window: `a headless caller` has no window at all and the browser suites
 * that drive it have no pointer to move, so both methods answer "done" and change
 * nothing. What this buys is the compiler pass - `cursor_park`'s body is checked against
 * the same signatures the real crate offers, so a Tauri that renames or re-types either
 * of them stops the build here rather than on Robin's machine. */
#[derive(Default, Clone)]
pub struct Window {
    /* What was asked of it, in order. Nothing here moves a pointer - there is none - but
       the *order* is a promise the command makes ("hidden before it moves, moved before it
       is shown") and a promise with no window to keep it in is still worth a test. */
    pub log: std::sync::Arc<std::sync::Mutex<Vec<&'static str>>>,
}

impl Window {
    fn note(&self, what: &'static str) {
        if let Ok(mut l) = self.log.lock() {
            l.push(what);
        }
    }
    pub fn set_cursor_visible(&self, visible: bool) -> Result<(), String> {
        self.note(if visible { "show" } else { "hide" });
        Ok(())
    }
    pub fn set_cursor_position<P: Into<Position>>(&self, _position: P) -> Result<(), String> {
        self.note("move");
        Ok(())
    }
}

/// A point in logical pixels — the units `set_cursor_position` takes.
pub struct LogicalPosition<T> {
    pub x: T,
    pub y: T,
}

impl<T> LogicalPosition<T> {
    pub fn new(x: T, y: T) -> LogicalPosition<T> {
        LogicalPosition { x, y }
    }
}

pub enum Position {
    Logical(LogicalPosition<f64>),
}

impl From<LogicalPosition<f64>> for Position {
    fn from(p: LogicalPosition<f64>) -> Position {
        Position::Logical(p)
    }
}

pub mod ipc {
    /// Tauri hands raw bytes back to the webview without going through JSON. Here it
    /// is just the bytes, so a caller outside Tauri can take them.
    pub struct Response(pub Vec<u8>);
    impl Response {
        pub fn new(bytes: Vec<u8>) -> Response {
            Response(bytes)
        }
    }
}
