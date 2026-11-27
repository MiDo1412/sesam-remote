//! Tauri-Befehle, nach Bereich aufgeteilt; `lib.rs` hält nur `run()`.

pub(crate) mod library;
pub(crate) mod conn;
pub(crate) mod shell;
pub(crate) mod files;
pub(crate) mod system;
pub(crate) mod updates;
pub(crate) mod vnc;
pub(crate) mod rdp;
