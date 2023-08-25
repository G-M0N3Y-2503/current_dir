//! A Scoped version of [`CurrentWorkingDirectory`](super::CurrentWorkingDirectory)

use super::*;

/// A Scoped version of [`CurrentWorkingDirectory`] that will [`reset()`][reset] the current working directory to it's previous state.
///
/// [`reset()`][reset] will be called automatically on [`drop()`][drop] or manually to handle errors at any time.
///
/// [reset]: Self::reset()
/// [drop]: Self::drop()
pub struct CurrentWorkingDirectory<'locked_cwd> {
    locked_cwd: &'locked_cwd mut super::CurrentWorkingDirectory,
    has_reset: bool,
}
impl CurrentWorkingDirectory<'_> {
    pub(super) fn new_scoped(
        locked_cwd: &mut super::CurrentWorkingDirectory,
    ) -> io::Result<CurrentWorkingDirectory> {
        locked_cwd.push_scope()?;
        Ok(CurrentWorkingDirectory {
            locked_cwd,
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
    pub fn new(&mut self) -> io::Result<CurrentWorkingDirectory> {
        CurrentWorkingDirectory::new_scoped(self.locked_cwd)
    }

    /// Resets the current working directory to the initial current working directory at the time of `self`s creation.
    ///
    /// # Errors
    /// The current directory cannot be set as per [`env::set_current_dir()`]
    pub fn reset(&mut self) -> io::Result<Option<PathBuf>> {
        if !self.has_reset {
            if let Some(reset_to) = self.locked_cwd.pop_scope()? {
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
