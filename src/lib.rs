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

/// The per-process shared memory for avoiding current working directory race conditions.
static CWD_MUTEX: Mutex<Cwd> = Mutex::new(Cwd::new());

/// Wrapper type to help the usage of the current working directory for the process.
#[derive(Debug)]
pub struct Cwd {
    /// a stack of current working directories wrapped by [`CwdStack`] used by [`CwdGuard`].
    cwd_stack: Vec<PathBuf>,
}
impl Cwd {
    /// Creates the shared memory used by [`CwdGuard`]
    const fn new() -> Self {
        Self {
            cwd_stack: Vec::new(),
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

/// A version of [`Cwd`] that will [`reset()`][reset] the current working directory to it's previous state on [`drop()`][drop].
///
/// [`reset()`][reset] can be called manually to handle errors or automatically on [`drop()`][drop].
///
/// [reset]: Self::reset()
/// [drop]: Self::drop()
pub struct CwdGuard<'lock> {
    /// A reference to the stack of current working directories to handle saving and resetting.
    cwd_stack: CwdStack<'lock>,
    /// Guard against resetting more than once.
    /// most commonly [`reset()`](Self::reset()) followed by [`drop()`]
    has_reset: bool,
}
impl CwdGuard<'_> {
    /// Resets the current working directory to the initial current working directory at the time of `self`s creation.
    ///
    /// # Errors
    /// The current directory cannot be set as per [`env::set_current_dir()`]
    #[inline]
    pub fn reset(&mut self) -> io::Result<Option<PathBuf>> {
        if !self.has_reset {
            if let Some(reset_to) = self.cwd_stack.pop_cwd()? {
                self.has_reset = true;
                return Ok(Some(reset_to));
            }
        }
        Ok(None)
    }
}
#[allow(clippy::missing_trait_methods)]
impl CwdAccessor for CwdGuard<'_> {}
impl Drop for CwdGuard<'_> {
    #[inline]
    fn drop(&mut self) {
        if !self.has_reset {
            #[allow(clippy::expect_used)]
            self.reset()
                .expect("current working directory can be reset to the initial value")
                .expect("CwdGuard was created with somewhere to reset to");
        }
    }
}
impl<'lock> TryFrom<&'lock mut CwdGuard<'_>> for CwdGuard<'lock> {
    type Error = io::Error;

    /// Create a new [`CwdGuard`] under `cwd_guard` that will [`reset()`][reset] to `cwd_guard` when [`drop()`][drop] is called.
    ///
    /// # Errors
    /// The current directory cannot be retrieved as per [`env::current_dir()`]
    ///
    /// [reset]: Self::reset()
    /// [drop]: Self::drop()
    #[inline]
    fn try_from(cwd_guard: &'lock mut CwdGuard<'_>) -> Result<Self, Self::Error> {
        Self::try_from(CwdStack::from(&mut cwd_guard.cwd_stack))
    }
}
impl<'lock> TryFrom<&'lock mut Cwd> for CwdGuard<'lock> {
    type Error = io::Error;

    /// Creates a [`CwdGuard`] mutably borrowing the locked [`Self`].
    ///
    /// # Errors
    /// The current directory cannot be retrieved as per [`env::current_dir()`]
    #[inline]
    fn try_from(locked_cwd: &'lock mut Cwd) -> Result<Self, Self::Error> {
        Self::try_from(CwdStack::from(locked_cwd))
    }
}
impl<'lock> TryFrom<CwdStack<'lock>> for CwdGuard<'lock> {
    type Error = io::Error;

    #[inline]
    fn try_from(mut cwd_stack: CwdStack<'lock>) -> Result<Self, Self::Error> {
        cwd_stack.push_cwd()?;
        Ok(Self {
            cwd_stack,
            has_reset: false,
        })
    }
}

/// Access to the stack of current working directories used by [`CwdGuard`], you probably want to use
/// [`CwdGuard`] instead. This is only really useful for cleaning up if the [`Mutex`] if it was poisoned.
///
/// Notably, the mutex may be poisoned if the directory a [`CwdGuard`] should reset to is deleted or
/// similarly inaccessible when the [`CwdGuard`] is dropped.
///
/// ```
/// # let test_dir =
/// #     env::temp_dir().join(&(env!("CARGO_PKG_NAME").to_owned() + " cwd_stack_doc_test"));
/// # if !test_dir.exists() {
/// #     fs::create_dir(&test_dir)?;
/// # }
/// #
///   use current_dir::prelude::*;
///   use std::{env, error::Error, fs, thread};
///
///   thread::scope(|s| {
///       s.spawn(|| -> Result<(), Box<dyn Error + Send + Sync>> {
///           let mut locked_cwd = Cwd::mutex().lock().unwrap();
///
///           // delete cwd before the CwdGuard is dropped
///           locked_cwd.set(&test_dir)?;
///           let _cwd_guard = CwdGuard::try_from(&mut *locked_cwd)?;
///           fs::remove_dir(&test_dir)?;
///
///           Ok(())
///       })
///       .join()
///   })
///   .expect_err("thread panicked");
///
///   let mut poisoned_locked_cwd = Cwd::mutex().lock().expect_err("cwd poisoned");
///   let mut poisoned_cwd_stack = CwdStack::from(&mut **poisoned_locked_cwd.get_mut());
///   assert_eq!(*poisoned_cwd_stack.as_vec(), vec![test_dir.clone()]);
///
///   // Fix poisoned cwd
///   fs::create_dir(test_dir)?;
///   poisoned_cwd_stack.pop_cwd()?;
///   let _locked_cwd = poisoned_locked_cwd.into_inner();
///
/// # Ok::<_, Box<dyn Error>>(())
/// ```
pub struct CwdStack<'lock> {
    /// A reference to the stack of current working directories to handle saving and resetting.
    cwd_stack: &'lock mut Vec<PathBuf>,
}
impl CwdStack<'_> {
    /// Pushes the current working directory onto the stack.
    ///
    /// # Errors
    /// Calls [`env::current_dir()`] internally that can error.
    #[inline]
    pub fn push_cwd(&mut self) -> io::Result<()> {
        let cwd = self.get()?;
        self.as_mut_vec().push(cwd);
        Ok(())
    }

    /// Pops the previous current working directory saved with [`push_cwd()`](Self::push_cwd()) and sets it to the
    /// current working directory.
    ///
    /// # Errors
    /// Calls [`env::set_current_dir()`] internally that can error.
    #[inline]
    pub fn pop_cwd(&mut self) -> io::Result<Option<PathBuf>> {
        self.as_mut_vec().pop().map_or_else(
            || Ok(None),
            |previous| match self.set(&previous) {
                Ok(()) => Ok(Some(previous)),
                Err(err) => {
                    self.as_mut_vec().push(previous);
                    Err(err)
                }
            },
        )
    }

    /// Gets a reference to the internal collection.
    #[inline]
    #[must_use]
    pub fn as_vec(&self) -> &Vec<PathBuf> {
        self.cwd_stack
    }

    /// Gets a mutable reference to the internal collection.
    #[inline]
    #[must_use]
    pub fn as_mut_vec(&mut self) -> &mut Vec<PathBuf> {
        self.cwd_stack
    }
}
#[allow(clippy::missing_trait_methods)]
impl CwdAccessor for CwdStack<'_> {}
impl<'lock> From<&'lock mut Cwd> for CwdStack<'lock> {
    #[inline]
    fn from(locked_cwd: &'lock mut Cwd) -> Self {
        Self {
            cwd_stack: &mut locked_cwd.cwd_stack,
        }
    }
}
impl<'lock> From<&'lock mut CwdStack<'_>> for CwdStack<'lock> {
    #[inline]
    fn from(stack: &'lock mut CwdStack<'_>) -> Self {
        Self {
            cwd_stack: stack.cwd_stack,
        }
    }
}

#[cfg(test)]
mod test_utilities;

#[cfg(test)]
mod tests {
    use super::*;
    use core::{iter, ops::Range, time::Duration};
    use std::{fs, path};

    #[test]
    fn test_stack() {
        const TEST_RANGE: Range<usize> = 1..20;
        let mut locked_cwd =
            test_utilities::yield_poison_addressed(Cwd::mutex(), Duration::from_millis(500))
                .unwrap();

        let mut cwd_stack = CwdStack::from(&mut *locked_cwd);
        assert!(cwd_stack.as_vec().is_empty());

        let mut cwd_stack_iter = iter::repeat(cwd_stack.get().unwrap());
        let cwd_stack_ref = cwd_stack_iter.by_ref();
        assert!(cwd_stack.as_vec().is_empty());

        for i in TEST_RANGE {
            cwd_stack.push_cwd().unwrap();
            let expected = cwd_stack_ref.take(i).collect::<Vec<_>>();
            assert_eq!(
                *cwd_stack.as_vec(),
                expected,
                "left: {}, right: {}",
                cwd_stack.as_vec().len(),
                expected.len()
            );
        }
        for i in TEST_RANGE.rev() {
            assert_eq!(cwd_stack.pop_cwd().unwrap(), cwd_stack_ref.next());
            let expected = cwd_stack_ref.take(i - 1).collect::<Vec<_>>();
            assert_eq!(
                *cwd_stack.as_vec(),
                expected,
                "left: {}, right: {}",
                cwd_stack.as_vec().len(),
                expected.len()
            );
        }
        assert!(cwd_stack.as_vec().is_empty());
    }

    #[test]
    fn test_delete_cwd() {
        let mut locked_cwd =
            test_utilities::yield_poison_addressed(Cwd::mutex(), Duration::from_millis(500))
                .unwrap();
        let mut cwd = test_utilities::reset_cwd(&mut locked_cwd);
        let test_dir = env::temp_dir().join(called_from!().replace(path::MAIN_SEPARATOR_STR, "|"));
        fs::create_dir_all(&test_dir).unwrap();
        let _clean_up_test_dir = with_drop::with_drop((), |()| fs::remove_dir(&test_dir).unwrap());

        let mut cwd_stack = CwdStack::from(&mut **cwd);
        cwd_stack.set(&test_dir).unwrap();

        let mut test_dir_repeat = iter::repeat(test_dir.clone());
        let test_dir_repeat_ref = test_dir_repeat.by_ref();

        assert!(cwd_stack.as_vec().is_empty());
        cwd_stack.push_cwd().unwrap();
        assert_eq!(
            *cwd_stack.as_vec(),
            test_dir_repeat_ref.take(1).collect::<Vec<_>>()
        );
        cwd_stack.push_cwd().unwrap();
        assert_eq!(
            *cwd_stack.as_vec(),
            test_dir_repeat_ref.take(2).collect::<Vec<_>>()
        );
        cwd_stack.push_cwd().unwrap();
        assert_eq!(
            *cwd_stack.as_vec(),
            test_dir_repeat_ref.take(3).collect::<Vec<_>>()
        );

        fs::remove_dir(&test_dir).unwrap();

        assert_eq!(
            cwd_stack.pop_cwd().unwrap_err().kind(),
            io::ErrorKind::NotFound
        );
        assert_eq!(
            *cwd_stack.as_vec(),
            test_dir_repeat_ref.take(3).collect::<Vec<_>>()
        );
        assert_eq!(
            cwd_stack.push_cwd().unwrap_err().kind(),
            io::ErrorKind::NotFound
        );
        assert_eq!(
            *cwd_stack.as_vec(),
            test_dir_repeat_ref.take(3).collect::<Vec<_>>()
        );

        fs::create_dir_all(&test_dir).unwrap();
        cwd_stack.set(&test_dir).unwrap();

        cwd_stack.push_cwd().unwrap();
        assert_eq!(
            *cwd_stack.as_vec(),
            test_dir_repeat_ref.take(4).collect::<Vec<_>>()
        );

        assert_eq!(cwd_stack.pop_cwd().unwrap(), test_dir_repeat_ref.next());
        assert_eq!(
            *cwd_stack.as_vec(),
            test_dir_repeat_ref.take(3).collect::<Vec<_>>()
        );
        assert_eq!(cwd_stack.pop_cwd().unwrap(), test_dir_repeat_ref.next());
        assert_eq!(
            *cwd_stack.as_vec(),
            test_dir_repeat_ref.take(2).collect::<Vec<_>>()
        );
        assert_eq!(cwd_stack.pop_cwd().unwrap(), test_dir_repeat_ref.next());
        assert_eq!(
            *cwd_stack.as_vec(),
            test_dir_repeat_ref.take(1).collect::<Vec<_>>()
        );
        assert_eq!(cwd_stack.pop_cwd().unwrap(), test_dir_repeat_ref.next());
        assert!(cwd_stack.as_vec().is_empty());
        assert_eq!(cwd_stack.pop_cwd().unwrap(), None);
        assert!(cwd_stack.as_vec().is_empty());
    }

    #[test]
    fn test_pop_empty() {
        let mut locked_cwd =
            test_utilities::yield_poison_addressed(Cwd::mutex(), Duration::from_millis(500))
                .unwrap();
        let mut cwd = test_utilities::reset_cwd(&mut locked_cwd);

        let test_dir = env::temp_dir().join(called_from!().replace(path::MAIN_SEPARATOR_STR, "|"));
        fs::create_dir_all(&test_dir).unwrap();
        let _clean_up_test_dir = with_drop::with_drop((), |()| fs::remove_dir(&test_dir).unwrap());

        let mut cwd_stack = CwdStack::from(&mut **cwd);
        cwd_stack.set(&test_dir).unwrap();

        assert_eq!(cwd_stack.get().unwrap(), test_dir);
        assert!(cwd_stack.as_vec().is_empty());
        assert_eq!(cwd_stack.pop_cwd().unwrap(), None);
        assert_eq!(cwd_stack.get().unwrap(), test_dir);
        assert!(cwd_stack.as_vec().is_empty());

        cwd_stack.push_cwd().unwrap();
        assert_eq!(*cwd_stack.as_vec(), vec![test_dir.clone()]);
        assert_eq!(cwd_stack.pop_cwd().unwrap(), Some(test_dir.clone()));

        assert_eq!(cwd_stack.get().unwrap(), test_dir);
        assert!(cwd_stack.as_vec().is_empty());
        assert_eq!(cwd_stack.pop_cwd().unwrap(), None);
        assert_eq!(cwd_stack.get().unwrap(), test_dir);
        assert!(cwd_stack.as_vec().is_empty());
    }
}
