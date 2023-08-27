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
    scope_stack: scoped::ScopeStack,
}
impl CurrentWorkingDirectory {
    const fn new() -> Self {
        Self {
            scope_stack: scoped::ScopeStack::new(),
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
    pub fn scoped(&mut self) -> io::Result<scoped::CurrentWorkingDirectory<'_>> {
        scoped::CurrentWorkingDirectory::new_scoped(self)
    }

    /// Access to the stack of scopes used by [`scoped::CurrentWorkingDirectory`].</br>
    /// This is only useful for cleaning up if the [`Mutex`] if it was poisoned.
    ///
    /// ```
    /// # let test_dir = env::temp_dir().join(concat!(module_path!(), "cwd_poisoned"));
    /// # if !test_dir.exists() {
    /// #     fs::create_dir(&test_dir)?;
    /// # }
    /// #
    ///   use current_dir::aliases::*;
    ///   use std::{env, error::Error, fs, thread};
    ///
    ///   let test_dir_copy = test_dir.clone();
    ///   thread::spawn(|| -> Result<(), Box<dyn Error + Send + Sync>> {
    ///       let mut locked_cwd = Cwd::mutex().lock().unwrap();
    ///       locked_cwd.set(&test_dir_copy)?;
    ///       let _scope_locked_cwd = locked_cwd.scoped()?;
    ///
    ///       // delete scoped cwd reset dir
    ///       fs::remove_dir(test_dir_copy)?;
    ///
    ///       Ok(())
    ///   })
    ///   .join()
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
    pub fn scope_stack(&mut self) -> &mut scoped::ScopeStack {
        &mut self.scope_stack
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

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(clippy::significant_drop_tightening)] // false positive
    #[test]
    fn recursive_scopes() {
        use super::aliases::*;

        let mut cwd = Cwd::mutex().lock().unwrap();
        let initial_cwd = cwd.get().unwrap();
        cwd.set(env::temp_dir()).unwrap();

        {
            let mut scoped_cwd = cwd.scoped().unwrap();
            scoped_cwd.set(env::temp_dir()).unwrap();

            let mut sub_scoped_cwd = ScopedCwd::new(&mut scoped_cwd).unwrap();
            sub_scoped_cwd.set(env::temp_dir()).unwrap();

            let mut sub_sub_scoped_cwd = sub_scoped_cwd.new().unwrap();
            sub_sub_scoped_cwd.set(env::temp_dir()).unwrap();
        }

        cwd.set(initial_cwd).unwrap();
    }

    #[test]
    #[should_panic(
        expected = "current working directory can be set: Os { code: 2, kind: NotFound, message: \"No such file or directory\" }"
    )]
    fn clean_up_poisend() {
        use crate::aliases::*;
        use std::{env, fs, panic, thread};

        let test_dir = env::temp_dir().join(concat!(module_path!(), "clean_up_poisend"));
        if !test_dir.exists() {
            fs::create_dir(&test_dir).unwrap();
        }

        let thread_test_dir_copy = test_dir.clone();
        let thread_result = thread::spawn(|| {
            let mut locked_cwd = Cwd::mutex().lock().unwrap();
            locked_cwd.set(&thread_test_dir_copy).unwrap();
            let _scope_locked_cwd = locked_cwd.scoped().unwrap();

            // delete scoped cwd reset dir
            fs::remove_dir(thread_test_dir_copy).unwrap();
        })
        .join();

        let mut poisoned_locked_cwd = Cwd::mutex().lock().expect_err("cwd poisoned");
        let poisoned_scope_stack = poisoned_locked_cwd.get_mut().scope_stack();
        assert_eq!(*poisoned_scope_stack.as_vec(), vec![test_dir.clone()]);

        // Fix poisoned cwd
        fs::create_dir(&test_dir).unwrap();
        assert_eq!(poisoned_scope_stack.pop_scope().unwrap(), Some(test_dir));
        let _locked_cwd = poisoned_locked_cwd.into_inner();

        panic::resume_unwind(thread_result.expect_err("thread panicked"));
    }
}
