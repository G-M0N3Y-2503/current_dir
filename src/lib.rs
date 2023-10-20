#![warn(
    clippy::cargo,
    clippy::pedantic,
    clippy::restriction, // Easier to maintain an allow list for the time being
    clippy::nursery,
    missing_docs,
    rustdoc::all,
)]
#![allow(
    clippy::blanket_clippy_restriction_lints,
    clippy::implicit_return,
    clippy::question_mark_used,
    clippy::redundant_else,
    clippy::self_named_module_files,
    clippy::semicolon_outside_block,
    clippy::significant_drop_tightening, // false positive
    clippy::single_call_fn, // Can't seem to override at instance
    clippy::wildcard_imports
)]
#![cfg_attr(test, allow(clippy::panic, clippy::unwrap_used, clippy::expect_used,))]
#![doc(test(attr(
    deny(warnings),
    deny(
        clippy::cargo,
        clippy::pedantic,
        clippy::restriction, // Easier to maintain an allow list for the time being
        clippy::nursery,
        rustdoc::all,
    )
)))]
#![doc = include_str!("../README.md")]

use std::{
    env, io,
    path::{Path, PathBuf},
    sync::Mutex,
};

pub mod prelude;
pub mod scoped;
mod sealed;

/// Wrapper functions for [`env::set_current_dir()`] and [`env::current_dir()`] with [`Self`] borrowed.
/// This is only implemented on types that have a reference to [`Cwd::mutex()`].
pub trait CwdAccessor: sealed::Sealed {
    #![allow(clippy::missing_errors_doc)]

    /// Wrapper function to ensure [`env::current_dir()`] is called with [`Self`] borrowed.
    #[inline]
    fn get(&self) -> io::Result<PathBuf> {
        env::current_dir()
    }

    /// Wrapper function to ensure [`env::set_current_dir()`] is called with [`Self`] borrowed.
    #[inline]
    fn set<P: AsRef<Path>>(&mut self, path: P) -> io::Result<()> {
        env::set_current_dir(path)
    }
}

static CWD_MUTEX: Mutex<Cwd> = Mutex::new(Cwd::new());

/// Wrapper type to help the usage of the current working directory for the process.
#[derive(Debug)]
pub struct Cwd {
    scope_stack: Vec<PathBuf>,
}
impl Cwd {
    const fn new() -> Self {
        Self {
            scope_stack: Vec::new(),
        }
    }

    /// The [`Mutex`] ensuring the state of the current working directory.
    ///
    /// It is a logic error to call [`env::set_current_dir()`] or [`env::current_dir()`] without this lock acquired.
    #[inline]
    #[must_use]
    pub fn mutex() -> &'static Mutex<Self> {
        &CWD_MUTEX
    }
}
#[allow(clippy::missing_trait_methods)]
impl CwdAccessor for Cwd {}

#[cfg(test)]
mod test_utilities;
