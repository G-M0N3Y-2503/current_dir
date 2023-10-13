//! A Scoped version of [`CurrentWorkingDirectory`](crate::CurrentWorkingDirectory)

use super::*;
pub mod stack;

/// A Scoped version of [`CurrentWorkingDirectory`] that will [`reset()`][reset] the current working directory to it's previous state.
///
/// [`reset()`][reset] will be called automatically on [`drop()`][drop] or manually to handle errors at any time.
///
/// [reset]: Self::reset()
/// [drop]: Self::drop()
pub struct CurrentWorkingDirectory<'locked_cwd> {
    scope_stack: stack::CurrentWorkingDirectoryStack<'locked_cwd>,
    has_reset: bool,
}
impl<'locked_cwd> CurrentWorkingDirectory<'locked_cwd> {
    pub(super) fn new_scoped(
        mut scope_stack: stack::CurrentWorkingDirectoryStack<'locked_cwd>,
    ) -> io::Result<Self> {
        scope_stack.push_scope()?;
        Ok(Self {
            scope_stack,
            has_reset: false,
        })
    }

    /// Create a new [`CurrentWorkingDirectory`] under `self` that will [`reset()`][reset] to `self` when [`drop()`][drop] is called.
    ///
    /// # Errors
    /// The current directory cannot be retrieved as per [`env::current_dir()`]
    ///
    /// [reset]: Self::reset()
    /// [drop]: Self::drop()
    pub fn new(&mut self) -> io::Result<CurrentWorkingDirectory<'_>> {
        CurrentWorkingDirectory::new_scoped(self.scope_stack.new())
    }

    /// Resets the current working directory to the initial current working directory at the time of `self`s creation.
    ///
    /// # Errors
    /// The current directory cannot be set as per [`env::set_current_dir()`]
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
    fn drop(&mut self) {
        if !self.has_reset {
            #[allow(clippy::expect_used)]
            self.reset()
                .expect("current working directory can be set")
                .expect("CurrentWorkingDirectory was created with somewhere to reset to");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{aliases::*, *};
    use std::{error::Error, fs, panic, path, sync::OnceLock, thread, time::Duration};

    #[allow(clippy::significant_drop_tightening)] // false positive
    #[test]
    fn recursive_scopes() {
        let mut locked_cwd =
            test_utilities::yield_poison_fixed(Cwd::mutex(), Duration::from_millis(500))
                .expect("no test failed to clean up poison");
        let initial_cwd = locked_cwd.get().unwrap();
        locked_cwd.set(env::temp_dir()).unwrap();

        let locked_cwd_ref = &mut *locked_cwd;
        let test_res = thread::scope(|s| {
            s.spawn(|| {
                let mut scoped_cwd = locked_cwd_ref.scoped().unwrap();
                scoped_cwd.set(env::temp_dir()).unwrap();

                let mut sub_scoped_cwd = ScopedCwd::new(&mut scoped_cwd).unwrap();
                sub_scoped_cwd.set(env::temp_dir()).unwrap();

                let mut sub_sub_scoped_cwd = sub_scoped_cwd.new().unwrap();
                sub_sub_scoped_cwd.set(env::temp_dir()).unwrap();
            })
            .join()
        });

        let clean_up_res = locked_cwd.set(initial_cwd);

        if let Err(panic) = test_res {
            panic::resume_unwind(panic)
        }

        clean_up_res.unwrap();
    }

    #[test]
    #[should_panic(
        expected = "current working directory can be set: Os { code: 2, kind: NotFound, message: \"No such file or directory\" }"
    )]
    fn clean_up_poisend() {
        let initial_cwd = OnceLock::new();
        let test_dir = env::temp_dir().join(called_from!().replace(path::MAIN_SEPARATOR_STR, "|"));
        fs::create_dir(&test_dir).unwrap();

        let test_res = thread::scope(|s| {
            s.spawn(|| {
                let mut locked_cwd =
                    test_utilities::yield_poison_fixed(Cwd::mutex(), Duration::from_millis(500))
                        .expect("no test failed to clean up poison");

                initial_cwd.set(locked_cwd.get().unwrap()).unwrap();
                locked_cwd.set(&test_dir).unwrap();

                let _scope_locked_cwd = locked_cwd.scoped().unwrap();

                fs::remove_dir(&test_dir).unwrap();
            })
            .join()
        });

        let mut poisoned_locked_cwd = Cwd::mutex().lock().expect_err("cwd poisoned");
        let mut poisoned_scope_stack = poisoned_locked_cwd.get_mut().scope_stack();
        assert!(!poisoned_scope_stack.as_vec().is_empty(), "not dirty");
        assert_eq!(*poisoned_scope_stack.as_vec(), vec![test_dir.clone()],);

        // Fix poisoned cwd
        fs::create_dir(&test_dir).unwrap();
        assert_eq!(
            poisoned_scope_stack.pop_scope().unwrap(),
            Some(test_dir.clone())
        );
        assert!(poisoned_scope_stack.as_vec().is_empty());
        let mut locked_cwd = poisoned_locked_cwd.into_inner();

        let _clean_up_res = (|| -> Result<(), Box<dyn Error>> {
            locked_cwd.set(initial_cwd.get().ok_or(io::Error::new(
                io::ErrorKind::NotFound,
                "initial cwd was not set",
            ))?)?;
            fs::remove_dir(&test_dir)?;
            Ok(())
        })();

        panic::resume_unwind(test_res.expect_err("thread panicked"));
    }
}
