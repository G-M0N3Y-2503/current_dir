//! A Scoped version of [`CurrentWorkingDirectory`](crate::CurrentWorkingDirectory)

use super::*;
use core::ops::{Deref, DerefMut};
pub mod stack;

/// A Scoped version of [`CurrentWorkingDirectory`] that will [`reset()`][reset] the current working directory to it's previous state.
///
/// [`reset()`][reset] will be called automatically on [`drop()`][drop] or manually to handle errors at any time.
///
/// [reset]: Self::reset()
/// [drop]: Self::drop()
pub struct CurrentWorkingDirectory<'locked_cwd> {
    scope_stack: stack::Stack<'locked_cwd>,
    has_reset: bool,
}
impl<'locked_cwd> CurrentWorkingDirectory<'locked_cwd> {
    /// Resets the current working directory to the initial current working directory at the time of `self`s creation.
    ///
    /// # Errors
    /// The current directory cannot be set as per [`env::set_current_dir()`]
    #[inline]
    pub fn reset(&mut self) -> io::Result<Option<PathBuf>> {
        if !self.has_reset {
            if let Some(reset_to) = self.scope_stack.pop_scope()? {
                self.has_reset = true;
                return Ok(Some(reset_to));
            }
        }
        Ok(None)
    }
}
#[allow(clippy::missing_trait_methods)]
impl CurrentWorkingDirectoryAccessor for CurrentWorkingDirectory<'_> {}
impl Drop for CurrentWorkingDirectory<'_> {
    #[inline]
    fn drop(&mut self) {
        if !self.has_reset {
            #[allow(clippy::expect_used)]
            self.reset()
                .expect("current working directory can be reset to the initial value")
                .expect("CurrentWorkingDirectory was created with somewhere to reset to");
        }
    }
}
impl<'new_scoped_cwd> TryFrom<&'new_scoped_cwd mut CurrentWorkingDirectory<'_>>
    for CurrentWorkingDirectory<'new_scoped_cwd>
{
    type Error = io::Error;

    /// Create a new [`CurrentWorkingDirectory`] under `scoped_cwd` that will [`reset()`][reset] to `scoped_cwd` when [`drop()`][drop] is called.
    ///
    /// # Errors
    /// The current directory cannot be retrieved as per [`env::current_dir()`]
    ///
    /// [reset]: Self::reset()
    /// [drop]: Self::drop()
    #[inline]
    fn try_from(
        scoped_cwd: &'new_scoped_cwd mut CurrentWorkingDirectory<'_>,
    ) -> Result<Self, Self::Error> {
        Self::try_from(&mut *scoped_cwd.scope_stack)
    }
}
impl<'locked_cwd> TryFrom<&'locked_cwd mut crate::CurrentWorkingDirectory>
    for CurrentWorkingDirectory<'locked_cwd>
{
    type Error = io::Error;

    /// Creates a [`scoped::CurrentWorkingDirectory`] mutably borrowing the locked [`Self`].
    ///
    /// # Errors
    /// The current directory cannot be retrieved as per [`env::current_dir()`]
    #[inline]
    fn try_from(
        locked_cwd: &'locked_cwd mut crate::CurrentWorkingDirectory,
    ) -> Result<Self, Self::Error> {
        let mut scope_stack = stack::Stack::from(locked_cwd);
        scope_stack.push_scope()?;
        Ok(Self {
            scope_stack,
            has_reset: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{aliases::*, test_utilities};
    use core::time::Duration;
    use std::{fs, panic, path, thread};
    use with_drop::with_drop;

    // #[allow(clippy::significant_drop_tightening)] // false positive
    #[test]
    fn recursive_scopes() {
        let mut locked_cwd =
            test_utilities::yield_poison_addressed(Cwd::mutex(), Duration::from_millis(500))
                .unwrap();
        let mut cwd = test_utilities::reset_cwd(&mut locked_cwd);

        cwd.set(env::temp_dir()).unwrap();
        {
            let mut scoped_cwd = ScopedCwd::try_from(&mut **cwd).unwrap();
            scoped_cwd.set(env::temp_dir()).unwrap();
            {
                let mut sub_scoped_cwd = ScopedCwd::try_from(&mut scoped_cwd).unwrap();
                sub_scoped_cwd.set(env::temp_dir()).unwrap();
                {
                    let mut sub_sub_scoped_cwd = ScopedCwd::try_from(&mut sub_scoped_cwd).unwrap();
                    sub_sub_scoped_cwd.set(env::temp_dir()).unwrap();
                }
                sub_scoped_cwd.set(env::temp_dir()).unwrap();
            }
            scoped_cwd.set(env::temp_dir()).unwrap();
        }
        cwd.set(env::temp_dir()).unwrap();
    }

    #[test]
    #[should_panic(
        expected = "current working directory can be reset to the initial value: Os { code: 2, kind: NotFound, message: \"No such file or directory\" }"
    )]
    fn clean_up_poisend() {
        let test_dir = env::temp_dir().join(called_from!().replace(path::MAIN_SEPARATOR_STR, "|"));
        let rm_test_dir = with_drop(&test_dir, |dir| {
            if dir.exists() {
                fs::remove_dir(dir).unwrap();
            }
        });
        fs::create_dir_all(*rm_test_dir).unwrap();

        let panic = thread::scope(|scope| {
            scope
                .spawn(|| {
                    let mut locked_cwd = test_utilities::yield_poison_addressed(
                        Cwd::mutex(),
                        Duration::from_millis(500),
                    )
                    .unwrap();
                    let mut reset_cwd = test_utilities::reset_cwd(&mut locked_cwd);

                    // cause panic in `_scope_locked_cwd` drop
                    reset_cwd.set(*rm_test_dir).unwrap();
                    let _scope_locked_cwd = ScopedCwd::try_from(&mut **reset_cwd).unwrap();
                    fs::remove_dir(*rm_test_dir).unwrap();
                })
                .join()
        })
        .expect_err("thread panicked");

        let mut poisoned_locked_cwd = Cwd::mutex().lock().expect_err("cwd poisoned");
        let mut poisoned_scope_stack = ScopeStack::from(&mut **poisoned_locked_cwd.get_mut());
        assert!(!poisoned_scope_stack.as_vec().is_empty(), "not dirty");
        assert_eq!(*poisoned_scope_stack.as_vec(), vec![(*rm_test_dir).clone()]);

        // Fix poisoned cwd
        fs::create_dir_all(*rm_test_dir).unwrap();
        assert_eq!(
            poisoned_scope_stack.pop_scope().unwrap(),
            Some((*rm_test_dir).clone())
        );
        let _locked_cwd = poisoned_locked_cwd.into_inner();

        panic::resume_unwind(panic);
    }
}
