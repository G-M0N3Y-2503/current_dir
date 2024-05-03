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
    clippy::implicit_return, // idiomatic
)]
#![doc(test(attr(deny(
    unused,
    warnings,
    clippy::cargo,
    clippy::pedantic,
    clippy::restriction, // Easier to maintain an allow list for the time being
    clippy::nursery,
    rustdoc::all,
))))]
#![cfg_attr(test, allow(clippy::panic, clippy::unwrap_used, clippy::expect_used))]
#![doc = include_str!("../README.md")]
#![cfg_attr(all(feature = "unstable", feature = "nightly"), feature(test))]

use core::{
    cell::Cell,
    fmt,
    ops::{Deref, DerefMut},
};
#[allow(clippy::useless_attribute)] // false positive
#[allow(clippy::std_instead_of_core)] // false positive
use std::env;
use std::{
    io,
    path::{Path, PathBuf},
    sync::Mutex,
};

mod sealed;

#[cfg(test)]
mod test_utilities;

#[cfg(test)]
macro_rules! mutex_test {
    ($mutex:expr, $test:expr, $timeout:expr) => {
        assert!(
            mutex_block!(
                {
                    assert!(
                        test_utilities::yield_lock_poisoned($mutex, $timeout)
                            .map($test)
                            .is_some(),
                        "test acquired Cwd lock within {}s",
                        $timeout.as_secs_f64()
                    )
                },
                $timeout
            ).is_some(),
            "test acquired mutual exclusion within {}s",
            $timeout.as_secs()
        )
    };
    ($($args:tt)+) => {
        mutex_test!($($args)+, core::time::Duration::from_millis(100))
    };
}
/// Allows cloning the contense of a [`Cell`] that implement [`Default`] and [`Clone`]
fn clone_cell_value<T: Default + Clone>(cell: &Cell<T>) -> T {
    let value = cell.take();
    let clone = value.clone();
    cell.set(value);
    clone
}

#[cfg(test)]
mod cell_test {
    use super::*;

    #[test]
    fn test_clone_cell_value() {
        let cell = Cell::new(Some(58i32));
        assert_eq!(clone_cell_value(&cell), Some(58i32));
        assert_eq!(cell, Cell::new(Some(58i32)));
        cell.set(None);
        assert_eq!(clone_cell_value(&cell), None);
        assert_eq!(cell, Cell::new(None));
    }
}

/// The per-process shared memory for avoiding current working directory race conditions.
static CWD_MUTEX: Mutex<Cwd> = Mutex::new(Cwd::new());

/// Wrapper type to help the usage of the current working directory for the process.
pub struct Cwd {
    /// The expected current working directory.
    expected_cwd: Cell<Option<PathBuf>>,
}
impl Cwd {
    /// Creates the shared memory used by [`CwdGuard`]
    #[allow(clippy::single_call_fn)] // better readability
    const fn new() -> Self {
        Self {
            expected_cwd: Cell::new(None),
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

    /// Returns the expected current working directory if any.
    /// By default the only expectations set are when this crate produces a panic.
    #[must_use]
    #[inline]
    pub fn get_expected(&self) -> Option<PathBuf> {
        clone_cell_value(&self.expected_cwd).or_else(|| {
            if cfg!(feature = "full_expected_cwd") {
                self.get().ok()
            } else {
                None
            }
        })
    }

    /// Wrapper function to ensure [`env::current_dir()`] is called with the [`Cwd`] borrowed.
    #[inline]
    #[doc(alias = "current_dir")]
    #[allow(clippy::missing_errors_doc)] // Wrapper function
    pub fn get(&self) -> io::Result<PathBuf> {
        env::current_dir().map(|path| {
            if cfg!(feature = "full_expected_cwd") && clone_cell_value(&self.expected_cwd).is_none()
            {
                self.expected_cwd.set(Some(path.clone()));
            };
            path
        })
    }

    /// Wrapper function to ensure [`env::set_current_dir()`] is called with the [`Cwd`] borrowed.
    #[inline]
    #[doc(alias = "set_current_dir")]
    #[allow(clippy::needless_pass_by_ref_mut, clippy::missing_errors_doc)] // Wrapper function
    pub fn set<P: AsRef<Path>>(&mut self, path: P) -> io::Result<()> {
        env::set_current_dir(&path).map(|()| {
            if cfg!(feature = "full_expected_cwd") {
                self.expected_cwd.set(Some(path.as_ref().to_path_buf()));
            }
        })
    }
}
impl fmt::Debug for Cwd {
    #[inline]
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("Cwd")
            .field("expected_cwd", &clone_cell_value(&self.expected_cwd))
            .finish()
    }
}

#[cfg(test)]
#[cfg(feature = "full_expected_cwd")]
mod full_expected_cwd_tests {
    use super::*;

    #[test]
    #[ignore = "Test needs to be run standalone"]
    fn test_get_expected_inits_expected() {
        mutex_test!(Cwd::mutex(), |mut locked_cwd| {
            assert_eq!(
                *locked_cwd.expected_cwd.get_mut(),
                None,
                "test not run standalone"
            );
            locked_cwd.get_expected().unwrap();
            assert_eq!(
                *locked_cwd.expected_cwd.get_mut(),
                Some(env::current_dir().unwrap())
            );
        });
    }

    #[test]
    #[ignore = "Test needs to be run standalone"]
    fn test_get_inits_expected() {
        mutex_test!(Cwd::mutex(), |mut locked_cwd| {
            assert_eq!(
                *locked_cwd.expected_cwd.get_mut(),
                None,
                "test not run standalone"
            );
            locked_cwd.get().unwrap();
            assert_eq!(
                *locked_cwd.expected_cwd.get_mut(),
                Some(env::current_dir().unwrap())
            );
        });
    }

    #[test]
    #[ignore = "Test needs to be run standalone"]
    fn test_set_inits_expected() {
        let test_dir = test_dir!();
        mutex_test!(Cwd::mutex(), |mut locked_cwd| {
            assert_eq!(
                *locked_cwd.expected_cwd.get_mut(),
                None,
                "test not run standalone"
            );
            locked_cwd.set(&*test_dir).unwrap();
            assert_eq!(
                locked_cwd.expected_cwd.get_mut().as_deref(),
                Some(test_dir.as_path())
            );
        });
    }

    #[test]
    fn test_unexpected_set() {
        let test_dir = test_dir!("dir1");
        mutex_test!(Cwd::mutex(), |mut locked_cwd| {
            let mut reset_cwd = test_utilities::reset_cwd(&mut locked_cwd);
            let cwd = &mut **reset_cwd;

            let initial_cwd = cwd.get().unwrap();
            assert_eq!(cwd.get_expected().unwrap(), initial_cwd);

            env::set_current_dir(&*test_dir).unwrap();
            {
                let expected_path = cwd.get_expected().unwrap();
                let cwd_path = cwd.get().unwrap();
                assert_ne!(cwd_path, expected_path);
                assert_eq!(expected_path, initial_cwd);
                assert_eq!(cwd_path, *test_dir);

                // test stable
                assert_eq!(cwd.get_expected().unwrap(), expected_path);
                assert_eq!(cwd.get().unwrap(), *test_dir);
            }

            // set new expectation
            cwd.set(test_dir.join("dir1")).unwrap();
            {
                let expected_path = cwd.get_expected().unwrap();
                let cwd_path = cwd.get().unwrap();
                assert_eq!(cwd_path, expected_path);
                assert_eq!(expected_path, test_dir.join("dir1"));
                assert_eq!(cwd_path, test_dir.join("dir1"));
            }
        });
    }
}

#[cfg(test)]
#[cfg(all(feature = "unstable", feature = "nightly"))]
mod cwd_bench {
    extern crate test;
    use super::*;

    #[bench]
    fn bench_get(bencher: &mut test::Bencher) {
        mutex_test!(Cwd::mutex(), |mut locked_cwd| {
            let mut reset_cwd = test_utilities::reset_cwd(&mut locked_cwd);
            let cwd = &mut **reset_cwd;

            bencher.iter(|| cwd.get().unwrap());
        });
    }

    #[bench]
    fn bench_set(bencher: &mut test::Bencher) {
        let test_dir = test_dir!();
        mutex_test!(Cwd::mutex(), |mut locked_cwd| {
            let mut reset_cwd = test_utilities::reset_cwd(&mut locked_cwd);
            let cwd = &mut **reset_cwd;

            bencher.iter(|| cwd.set(&*test_dir).unwrap());
        });
    }

    #[bench]
    fn bench_set_and_get(bencher: &mut test::Bencher) {
        let test_dir = test_dir!();
        mutex_test!(Cwd::mutex(), |mut locked_cwd| {
            let mut reset_cwd = test_utilities::reset_cwd(&mut locked_cwd);
            let cwd = &mut **reset_cwd;

            cwd.set(&*test_dir).unwrap();

            bencher.iter(|| cwd.set(cwd.get().unwrap()).unwrap());
        });
    }
}

/// A version of [`Cwd`] that will [`reset()`][reset] the current working directory to it's previous state on [`drop()`][drop].
///
/// [`reset()`][reset] can be called manually to handle errors or automatically on [`drop()`][drop].
///
/// [reset]: Self::reset()
/// [drop]: Self::drop()
pub struct CwdGuard<'lock> {
    /// A reference to the Current working directory.
    cwd: &'lock mut Cwd,
    /// The initial directory to reset to.
    initial_cwd: PathBuf,
}
impl CwdGuard<'_> {
    /// Resets the current working directory to the initial current working directory at the time of `self`s creation.
    ///
    /// # Errors
    /// The current directory cannot be set as per [`env::set_current_dir()`]
    #[inline]
    pub fn reset(&mut self) -> io::Result<()> {
        self.cwd.set(&self.initial_cwd)
    }
}
impl Drop for CwdGuard<'_> {
    /// # Panics
    /// If the current directory cannot be [`reset()`](Self::reset())
    #[inline]
    fn drop(&mut self) {
        use std::panic;
        if let Err(err) = self.reset() {
            self.cwd.expected_cwd.set(Some(self.initial_cwd.clone()));
            panic::panic_any(err)
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
        Self::try_from(&mut *cwd_guard.cwd)
    }
}
impl<'lock> TryFrom<&'lock mut Cwd> for CwdGuard<'lock> {
    type Error = io::Error;

    /// Creates a [`CwdGuard`] mutably borrowing the locked [`Self`].
    ///
    /// # Errors
    /// The current directory cannot be retrieved as per [`env::current_dir()`]
    #[inline]
    fn try_from(cwd: &'lock mut Cwd) -> Result<Self, Self::Error> {
        cwd.get().map(|initial_cwd| Self { cwd, initial_cwd })
    }
}
impl Deref for CwdGuard<'_> {
    type Target = Cwd;

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.cwd
    }
}
impl DerefMut for CwdGuard<'_> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.cwd
    }
}

#[cfg(test)]
mod guard_tests {
    use super::*;

    #[test]
    fn test_guard_reset() {
        let test_dir = test_dir!();
        mutex_test!(Cwd::mutex(), |mut locked_cwd| {
            let mut reset_cwd = test_utilities::reset_cwd(&mut locked_cwd);

            let cwd = &mut **reset_cwd;
            let initial_cwd = cwd.get().unwrap();

            assert_ne!(initial_cwd, *test_dir);

            let mut cwd_guard = CwdGuard::try_from(&mut *cwd).unwrap();
            assert_eq!(cwd_guard.get().unwrap(), initial_cwd);

            cwd_guard.set(&*test_dir).unwrap();
            assert_eq!(cwd_guard.get().unwrap(), *test_dir);

            cwd_guard.reset().unwrap();
            assert_eq!(cwd_guard.get().unwrap(), initial_cwd);

            cwd_guard.set(&*test_dir).unwrap();
            assert_eq!(cwd_guard.get().unwrap(), *test_dir);

            cwd_guard.reset().unwrap();
            assert_eq!(cwd_guard.get().unwrap(), initial_cwd);
        });
    }

    #[test]
    fn test_guard_drop() {
        let test_dir = test_dir!();
        mutex_test!(Cwd::mutex(), |mut locked_cwd| {
            let mut reset_cwd = test_utilities::reset_cwd(&mut locked_cwd);

            let cwd = &mut **reset_cwd;
            let initial_cwd = cwd.get().unwrap();

            assert_ne!(initial_cwd, *test_dir);

            {
                let mut cwd_guard = CwdGuard::try_from(&mut *cwd).unwrap();
                assert_eq!(cwd_guard.get().unwrap(), initial_cwd);

                cwd_guard.set(&*test_dir).unwrap();
                assert_eq!(cwd_guard.get().unwrap(), *test_dir);
            }
            assert_eq!(cwd.get().unwrap(), initial_cwd);

            {
                let mut cwd_guard = CwdGuard::try_from(&mut *cwd).unwrap();
                assert_eq!(cwd_guard.get().unwrap(), initial_cwd);

                cwd_guard.set(&*test_dir).unwrap();
                assert_eq!(cwd_guard.get().unwrap(), *test_dir);

                cwd_guard.reset().unwrap();
                assert_eq!(cwd_guard.get().unwrap(), initial_cwd);
            }
            assert_eq!(cwd.get().unwrap(), initial_cwd);
        });
    }

    #[test]
    fn test_guard_recursive() {
        let test_dir = test_dir!("dir1/dir2");
        mutex_test!(Cwd::mutex(), |mut locked_cwd| {
            let mut reset_cwd = test_utilities::reset_cwd(&mut locked_cwd);
            let cwd = &mut **reset_cwd;

            let mut cwd_guard = CwdGuard::try_from(&mut *cwd).unwrap();
            cwd_guard.set(&*test_dir).unwrap();
            assert_eq!(cwd_guard.get().unwrap(), *test_dir);
            {
                let mut sub_cwd_guard = CwdGuard::try_from(&mut cwd_guard).unwrap();
                assert_eq!(sub_cwd_guard.get().unwrap(), *test_dir);
                sub_cwd_guard.set(test_dir.join("dir1")).unwrap();
                assert_eq!(sub_cwd_guard.get().unwrap(), test_dir.join("dir1"));
                {
                    let mut sub_sub_cwd_guard = CwdGuard::try_from(&mut sub_cwd_guard).unwrap();
                    assert_eq!(sub_sub_cwd_guard.get().unwrap(), test_dir.join("dir1"));
                    sub_sub_cwd_guard.set(test_dir.join("dir1/dir2")).unwrap();
                    assert_eq!(sub_sub_cwd_guard.get().unwrap(), test_dir.join("dir1/dir2"));
                }
                assert_eq!(sub_cwd_guard.get().unwrap(), test_dir.join("dir1"));
            }
            assert_eq!(cwd_guard.get().unwrap(), *test_dir);
        });
    }
}
