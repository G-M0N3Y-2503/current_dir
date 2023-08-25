//! The current directory is global to the whole process. So each [`ScopedCurrentWorkingDirectory`] uses
//! a lock so that they are accurate within the scope.
//! [`ScopedCurrentWorkingDirectory`] assumes that no other calls to [`std::env::set_current_dir()`] are
//! made elsewhere.

use std::{
    env, io,
    path::{Path, PathBuf},
    sync::Mutex,
};

static CWD_MUTEX: Mutex<CurrentWorkingDirectory> = Mutex::new(CurrentWorkingDirectory::new());

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
    pub fn mutex() -> &'static Mutex<CurrentWorkingDirectory> {
        &CWD_MUTEX
    }

    /// Wrapper function to ensure [`env::current_dir()`] is called with the locked [`Self`].
    pub fn get(&self) -> io::Result<PathBuf> {
        env::current_dir()
    }

    /// Wrapper function to ensure [`env::set_current_dir()`] is called with the locked [`Self`].
    pub fn set<P: AsRef<Path>>(&self, path: P) -> io::Result<()> {
        env::set_current_dir(path)
    }

    fn push_scope(&mut self) -> io::Result<()> {
        self.scope_stack.push(self.get()?);
        Ok(())
    }

    fn pop_scope(&mut self) -> io::Result<Option<PathBuf>> {
        if let Some(previous) = self.scope_stack.last() {
            self.set(previous)?;
        }
        Ok(self.scope_stack.pop())
    }

    /// Creates a [`ScopedCurrentWorkingDirectory`] mutably borrowing the locked [`Self`].
    pub fn scoped(&mut self) -> io::Result<ScopedCurrentWorkingDirectory<'_>> {
        ScopedCurrentWorkingDirectory::new_scoped(self)
    }

    pub fn drain_scoped(&mut self) -> &mut Vec<PathBuf> {
        &mut self.scope_stack
    }
}
impl<'locked_cwd> TryFrom<&'locked_cwd mut CurrentWorkingDirectory>
    for ScopedCurrentWorkingDirectory<'locked_cwd>
{
    type Error = io::Error;

    /// See [`scoped()`][scoped]
    ///
    /// [scoped]: CurrentWorkingDirectory::scoped()
    fn try_from(locked_cwd: &'locked_cwd mut CurrentWorkingDirectory) -> Result<Self, Self::Error> {
        locked_cwd.scoped()
    }
}

/// A Scoped version of [`CurrentWorkingDirectory`] that can later be [`reset()`][reset] to the current working directory at the time of this call.
///
/// [`reset()`][reset] will be called automatically on [`drop()`][drop].
///
/// [reset]: Self::reset()
/// [drop]: Self::drop()
pub struct ScopedCurrentWorkingDirectory<'locked_cwd> {
    locked_cwd: &'locked_cwd mut CurrentWorkingDirectory,
    has_reset: bool,
}
impl<'locked_cwd> ScopedCurrentWorkingDirectory<'locked_cwd> {
    fn new_scoped(locked_cwd: &'locked_cwd mut CurrentWorkingDirectory) -> io::Result<Self> {
        locked_cwd.push_scope()?;
        Ok(Self {
            locked_cwd,
            has_reset: false,
        })
    }

    pub fn new(&mut self) -> io::Result<ScopedCurrentWorkingDirectory> {
        ScopedCurrentWorkingDirectory::new_scoped(self.locked_cwd)
    }

    pub fn reset(&mut self) -> io::Result<Option<PathBuf>> {
        if !self.has_reset {
            if let Some(reset_to) = self.locked_cwd.pop_scope()? {
                self.has_reset = true;
                return Ok(Some(reset_to));
            }
        }
        Ok(None)
    }

    /// Wrapper function to ensure [`env::current_dir()`] is called with the locked [`CurrentWorkingDirectory`] borrowed.
    pub fn get(&self) -> io::Result<PathBuf> {
        self.locked_cwd.get()
    }

    /// Wrapper function to ensure [`env::set_current_dir()`] is called with the locked [`CurrentWorkingDirectory`] borrowed.
    pub fn set<P: AsRef<Path>>(&self, path: P) -> io::Result<()> {
        self.locked_cwd.set(path)
    }
}
impl Drop for ScopedCurrentWorkingDirectory<'_> {
    fn drop(&mut self) {
        if !self.has_reset {
            self.reset()
                .expect("current working directory can be set")
                .expect("ScopedCurrentWorkingDirectory was created with somewhere to reset to");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test() {
        let mut cwd = CurrentWorkingDirectory::mutex().lock().unwrap();
        let mut scoped_cwd = cwd.scoped().unwrap();
        scoped_cwd.set(env::temp_dir()).unwrap();

        let mut scoped_cwd = ScopedCurrentWorkingDirectory::new(&mut scoped_cwd).unwrap();
        scoped_cwd.set(env::temp_dir()).unwrap();

        let scoped_cwd = scoped_cwd.new().unwrap();
        scoped_cwd.set(env::temp_dir()).unwrap();
    }
}
