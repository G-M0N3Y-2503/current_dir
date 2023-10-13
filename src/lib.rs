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
    clippy::single_call_fn, // Can't seem to override at instance
)]
#![cfg_attr(test, allow(clippy::expect_used, clippy::unwrap_used))]
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

pub mod aliases;
pub mod scoped;
mod sealed;
pub mod test_utilities;

/// Wrapper functions for [`env::set_current_dir()`] and [`env::current_dir()`] with [`Self`] borrowed.
/// This is only implemented on types that have a reference to [`CurrentWorkingDirectory::mutex()`].
pub trait CurrentWorkingDirectoryAccessor: sealed::Sealed {
    #![allow(clippy::missing_errors_doc)]

    /// Wrapper function to ensure [`env::current_dir()`] is called with [`Self`] borrowed.
    fn get(&self) -> io::Result<PathBuf> {
        env::current_dir()
    }

    /// Wrapper function to ensure [`env::set_current_dir()`] is called with [`Self`] borrowed.
    fn set<P: AsRef<Path>>(&mut self, path: P) -> io::Result<()> {
        env::set_current_dir(path)
    }
}

static CWD_MUTEX: Mutex<CurrentWorkingDirectory> = Mutex::new(CurrentWorkingDirectory::new());

/// Wrapper type to help the usage of the current working directory for the process.
#[derive(Debug)]
pub struct CurrentWorkingDirectory {
    scope_stack: Vec<PathBuf>,
}
impl CurrentWorkingDirectory {
    const fn new() -> Self {
        Self {
            scope_stack: Vec::new(),
        }
    }

    /// The [`Mutex`] ensuring the state of the current working directory.
    ///
    /// It is a logic error to call [`env::set_current_dir()`] or [`env::current_dir()`] without this lock acquired.
    #[must_use]
    pub fn mutex() -> &'static Mutex<Self> {
        &CWD_MUTEX
    }

    /// Creates a [`scoped::CurrentWorkingDirectory`] mutably borrowing the locked [`Self`].
    ///
    /// # Errors
    /// The current directory cannot be retrieved as per [`env::current_dir()`]
    pub fn scoped(&mut self) -> std::io::Result<scoped::CurrentWorkingDirectory<'_>> {
        scoped::CurrentWorkingDirectory::new_scoped(self.scope_stack())
    }

    /// Access to the stack of scopes used by [`scoped::CurrentWorkingDirectory`].</br>
    /// This is only useful for cleaning up if the [`Mutex`] if it was poisoned.
    ///
    /// ```
    /// # let test_dir =
    /// #     env::temp_dir().join(&(env!("CARGO_PKG_NAME").to_owned() + " scope_stack_doc_test"));
    /// # if !test_dir.exists() {
    /// #     fs::create_dir(&test_dir)?;
    /// # }
    /// #
    ///   use current_dir::aliases::*;
    ///   use std::{env, error::Error, fs, thread};
    ///
    ///   thread::scope(|s| {
    ///       s.spawn(|| -> Result<(), Box<dyn Error + Send + Sync>> {
    ///           let mut locked_cwd = Cwd::mutex().lock().unwrap();
    ///           locked_cwd.set(&test_dir)?;
    ///           let _scope_locked_cwd = locked_cwd.scoped()?;
    ///
    ///           // delete scoped cwd reset dir
    ///           fs::remove_dir(&test_dir)?;
    ///
    ///           Ok(())
    ///       })
    ///       .join()
    ///   })
    ///   .expect_err("thread panicked");
    ///
    ///   let mut poisoned_locked_cwd = Cwd::mutex().lock().expect_err("cwd poisoned");
    ///   let poisoned_scope_stack = poisoned_locked_cwd.get_mut().scope_stack();
    ///   assert_eq!(*poisoned_scope_stack.as_vec(), vec![test_dir.clone()]);
    ///
    ///   // Fix poisoned cwd
    ///   fs::create_dir(test_dir)?;
    ///   poisoned_scope_stack.pop_scope()?;
    ///   let _locked_cwd = poisoned_locked_cwd.into_inner();
    ///
    /// # Ok::<_, Box<dyn Error>>(())
    /// ```
    pub fn scope_stack(&mut self) -> scoped::stack::CurrentWorkingDirectoryStack<'_> {
        scoped::stack::CurrentWorkingDirectoryStack::from(self)
    }
}
#[allow(clippy::missing_trait_methods)]
impl CurrentWorkingDirectoryAccessor for CurrentWorkingDirectory {}
impl<'locked_cwd> TryFrom<&'locked_cwd mut CurrentWorkingDirectory>
    for scoped::CurrentWorkingDirectory<'locked_cwd>
{
    type Error = io::Error;

    /// See [`scoped()`][scoped]
    ///
    /// [scoped]: CurrentWorkingDirectory::scoped()
    fn try_from(locked_cwd: &'locked_cwd mut CurrentWorkingDirectory) -> Result<Self, Self::Error> {
        locked_cwd.scoped()
    }
}
