//! The byte cursor, the buffer and the Windows-1252 codec, shared with the NIF model.
//!
//! Round 18gg: these started here and moved to [`crate::bytes`] when the typed NIF model
//! (`netimmerse`) needed the same three things. This name stays so `tes3::bytes::Reader`
//! keeps reading the way it did.

pub use crate::bytes::*;
