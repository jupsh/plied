//! Command output on stdout and stderr (FORMAT.md §11, *Streams*). A reader that goes away
//! (`plied list | head -1`) ends the output, not the command: the exit code still says what the
//! command did, so a write that was accepted never reports failure because nobody read its result.

use std::fmt;
use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};

static STDOUT_GONE: AtomicBool = AtomicBool::new(false);
static STDERR_GONE: AtomicBool = AtomicBool::new(false);

/// A line on stdout. After a closed pipe nothing more is written; any other failure is said once
/// on stderr.
pub fn out(args: fmt::Arguments) {
    if STDOUT_GONE.load(Ordering::Relaxed) {
        return;
    }
    if let Err(e) = writeln!(io::stdout().lock(), "{args}") {
        STDOUT_GONE.store(true, Ordering::Relaxed);
        if e.kind() != io::ErrorKind::BrokenPipe {
            err(format_args!("plied: cannot write output: {e}"));
        }
    }
}

/// A line on stderr. A failure here has nowhere to be reported, so it only ends stderr output.
pub fn err(args: fmt::Arguments) {
    if STDERR_GONE.load(Ordering::Relaxed) {
        return;
    }
    if writeln!(io::stderr().lock(), "{args}").is_err() {
        STDERR_GONE.store(true, Ordering::Relaxed);
    }
}

/// `println!` that never panics: see [`out`].
#[macro_export]
macro_rules! outln {
    () => { $crate::out::out(format_args!("")) };
    ($($arg:tt)*) => { $crate::out::out(format_args!($($arg)*)) };
}

/// `eprintln!` that never panics: see [`err`].
#[macro_export]
macro_rules! errln {
    () => { $crate::out::err(format_args!("")) };
    ($($arg:tt)*) => { $crate::out::err(format_args!($($arg)*)) };
}
