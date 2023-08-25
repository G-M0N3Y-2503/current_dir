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

/// Wrapper functions for [`env::set_current_dir()`] and [`env::current_dir()`] with [`Self`] borrowed.
/// This is only implemented on types that have a reference to [`CurrentWorkingDirectory::mutex()`].
pub trait CurrentWorkingDirectoryAccessor: private::Sealed {
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
mod private {
    pub trait Sealed {}
    impl Sealed for super::CurrentWorkingDirectory {}
    impl Sealed for super::scoped::CurrentWorkingDirectory<'_> {}
}

static CWD_MUTEX: Mutex<CurrentWorkingDirectory> = Mutex::new(CurrentWorkingDirectory::new());

/// Wrapper type to help the usage of the current working directory for the process.
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

    fn push_scope(&mut self) -> io::Result<()> {
        self.scope_stack.push(self.get()?);
        Ok(())
    }

    fn pop_scope(&mut self) -> io::Result<Option<PathBuf>> {
        match self.scope_stack.pop() {
            Some(previous) => match self.set(&previous) {
                Ok(_) => Ok(Some(previous)),
                Err(err) => {
                    self.scope_stack.push(previous);
                    Err(err)
                }
            },
            None => Ok(None),
        }
    }

    /// Creates a [`scoped::CurrentWorkingDirectory`] mutably borrowing the locked [`Self`].
    ///
    /// # Errors
    /// The current directory cannot be retrieved as per [`env::current_dir()`]
    pub fn scoped(&mut self) -> io::Result<scoped::CurrentWorkingDirectory<'_>> {
        scoped::CurrentWorkingDirectory::new_scoped(self)
    }

    // fn drain_scoped(&mut self) -> &mut Vec<PathBuf> {
    //     &mut self.scope_stack
    // }
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
    fn test() {
        use super::aliases::*;

        let mut cwd = Cwd::mutex().lock().unwrap();
        cwd.set(env::temp_dir()).unwrap();

        let mut scoped_cwd = cwd.scoped().unwrap();
        scoped_cwd.set(env::temp_dir()).unwrap();

        let mut sub_scoped_cwd = ScopedCwd::new(&mut scoped_cwd).unwrap();
        sub_scoped_cwd.set(env::temp_dir()).unwrap();

        let mut sub_sub_scoped_cwd = sub_scoped_cwd.new().unwrap();
        sub_sub_scoped_cwd.set(env::temp_dir()).unwrap();
    }
}
