//! Notes for whoever runs a command: worth knowing, but not the command's result (an earlier write
//! recovered first, a read that may mix old and new files, OCR text). Printed to stderr as they
//! happen, or, with `--json`, collected for the result envelope (FORMAT.md §11).

use std::cell::{Cell, RefCell};

thread_local! {
    static COLLECT: Cell<bool> = const { Cell::new(false) };
    static NOTES: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

/// Collect notes from now on instead of printing them.
pub fn collect() {
    COLLECT.set(true);
}

pub fn note(msg: impl Into<String>) {
    let msg = msg.into();
    if COLLECT.get() {
        NOTES.with_borrow_mut(|n| n.push(msg));
    } else {
        crate::errln!("plied: {msg}");
    }
}

/// The notes collected so far, emptied.
pub fn take() -> Vec<String> {
    NOTES.with_borrow_mut(std::mem::take)
}
