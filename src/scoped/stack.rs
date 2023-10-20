//! Access to the stack of scopes used by [`ScopedCwd`], you probably want to use
//! [`ScopedCwd`] instead. This is only really useful for cleaning up if the [`Mutex`] if it was poisoned.
//!
//! Notably, the mutex may be poisoned if the directory a [`ScopedCwd`] should reset to is deleted or
//! similarly inaccessible when the scope ends.
//!
//! ```
//! # let test_dir =
//! #     env::temp_dir().join(&(env!("CARGO_PKG_NAME").to_owned() + " scope_stack_doc_test"));
//! # if !test_dir.exists() {
//! #     fs::create_dir(&test_dir)?;
//! # }
//! #
//!   use current_dir::prelude::*;
//!   use std::{env, error::Error, fs, thread};
//!
//!   thread::scope(|s| {
//!       s.spawn(|| -> Result<(), Box<dyn Error + Send + Sync>> {
//!           let mut locked_cwd = Cwd::mutex().lock().unwrap();
//!
//!           // delete cwd before the ScopedCwd is dropped
//!           locked_cwd.set(&test_dir)?;
//!           let _scope_locked_cwd = ScopedCwd::try_from(&mut *locked_cwd)?;
//!           fs::remove_dir(&test_dir)?;
//!
//!           Ok(())
//!       })
//!       .join()
//!   })
//!   .expect_err("thread panicked");
//!
//!   let mut poisoned_locked_cwd = Cwd::mutex().lock().expect_err("cwd poisoned");
//!   let mut poisoned_scope_stack = ScopeStack::from(&mut **poisoned_locked_cwd.get_mut());
//!   assert_eq!(*poisoned_scope_stack.as_vec(), vec![test_dir.clone()]);
//!
//!   // Fix poisoned cwd
//!   fs::create_dir(test_dir)?;
//!   poisoned_scope_stack.pop_scope()?;
//!   let _locked_cwd = poisoned_locked_cwd.into_inner();
//!
//! # Ok::<_, Box<dyn Error>>(())
//! ```

use super::*;

/// A helper type exposing the stack of directories representing the history of current working directories.
pub struct CwdStack<'lock> {
    scope_stack: &'lock mut Vec<PathBuf>,
}
impl CwdStack<'_> {
    /// Pushes the current working directory onto the stack.
    ///
    /// # Errors
    /// Calls [`env::current_dir()`] internally that can error.
    #[inline]
    pub fn push_scope(&mut self) -> io::Result<()> {
        let cwd = self.get()?;
        self.as_mut_vec().push(cwd);
        Ok(())
    }

    /// Pops the previous current working directory saved with [`push_scope()`](Self::push_scope()) and sets it to the
    /// current working directory.
    ///
    /// # Errors
    /// Calls [`env::set_current_dir()`] internally that can error.
    #[inline]
    pub fn pop_scope(&mut self) -> io::Result<Option<PathBuf>> {
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
        self.scope_stack
    }

    /// Gets a mutable reference to the internal collection.
    #[inline]
    #[must_use]
    pub fn as_mut_vec(&mut self) -> &mut Vec<PathBuf> {
        self.scope_stack
    }
}
#[allow(clippy::missing_trait_methods)]
impl CwdAccessor for CwdStack<'_> {}
impl<'lock> From<&'lock mut Cwd> for CwdStack<'lock> {
    #[inline]
    fn from(locked_cwd: &'lock mut Cwd) -> Self {
        Self {
            scope_stack: &mut locked_cwd.scope_stack,
        }
    }
}
impl<'lock> From<&'lock mut CwdStack<'_>> for CwdStack<'lock> {
    #[inline]
    fn from(stack: &'lock mut CwdStack<'_>) -> Self {
        Self {
            scope_stack: stack.scope_stack,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utilities;
    use core::{iter, ops::Range, time::Duration};
    use std::{fs, path};

    #[test]
    fn test_stack() {
        const TEST_RANGE: Range<usize> = 1..20;
        let mut locked_cwd =
            test_utilities::yield_poison_addressed(Cwd::mutex(), Duration::from_millis(500))
                .unwrap();

        let mut scope_stack = CwdStack::from(&mut *locked_cwd);
        assert!(scope_stack.as_vec().is_empty());

        let mut cwd_stack = iter::repeat(scope_stack.get().unwrap());
        let cwd_stack_ref = cwd_stack.by_ref();
        assert!(scope_stack.as_vec().is_empty());

        for i in TEST_RANGE {
            scope_stack.push_scope().unwrap();
            let expected = cwd_stack_ref.take(i).collect::<Vec<_>>();
            assert_eq!(
                *scope_stack.as_vec(),
                expected,
                "left: {}, right: {}",
                scope_stack.as_vec().len(),
                expected.len()
            );
        }
        for i in TEST_RANGE.rev() {
            assert_eq!(scope_stack.pop_scope().unwrap(), cwd_stack_ref.next());
            let expected = cwd_stack_ref.take(i - 1).collect::<Vec<_>>();
            assert_eq!(
                *scope_stack.as_vec(),
                expected,
                "left: {}, right: {}",
                scope_stack.as_vec().len(),
                expected.len()
            );
        }
        assert!(scope_stack.as_vec().is_empty());
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

        let mut scope_stack = CwdStack::from(&mut **cwd);
        scope_stack.set(&test_dir).unwrap();

        let mut test_dir_repeat = iter::repeat(test_dir.clone());
        let test_dir_repeat_ref = test_dir_repeat.by_ref();

        assert!(scope_stack.as_vec().is_empty());
        scope_stack.push_scope().unwrap();
        assert_eq!(
            *scope_stack.as_vec(),
            test_dir_repeat_ref.take(1).collect::<Vec<_>>()
        );
        scope_stack.push_scope().unwrap();
        assert_eq!(
            *scope_stack.as_vec(),
            test_dir_repeat_ref.take(2).collect::<Vec<_>>()
        );
        scope_stack.push_scope().unwrap();
        assert_eq!(
            *scope_stack.as_vec(),
            test_dir_repeat_ref.take(3).collect::<Vec<_>>()
        );

        fs::remove_dir(&test_dir).unwrap();

        assert_eq!(
            scope_stack.pop_scope().unwrap_err().kind(),
            io::ErrorKind::NotFound
        );
        assert_eq!(
            *scope_stack.as_vec(),
            test_dir_repeat_ref.take(3).collect::<Vec<_>>()
        );
        assert_eq!(
            scope_stack.push_scope().unwrap_err().kind(),
            io::ErrorKind::NotFound
        );
        assert_eq!(
            *scope_stack.as_vec(),
            test_dir_repeat_ref.take(3).collect::<Vec<_>>()
        );

        fs::create_dir_all(&test_dir).unwrap();
        scope_stack.set(&test_dir).unwrap();

        scope_stack.push_scope().unwrap();
        assert_eq!(
            *scope_stack.as_vec(),
            test_dir_repeat_ref.take(4).collect::<Vec<_>>()
        );

        assert_eq!(scope_stack.pop_scope().unwrap(), test_dir_repeat_ref.next());
        assert_eq!(
            *scope_stack.as_vec(),
            test_dir_repeat_ref.take(3).collect::<Vec<_>>()
        );
        assert_eq!(scope_stack.pop_scope().unwrap(), test_dir_repeat_ref.next());
        assert_eq!(
            *scope_stack.as_vec(),
            test_dir_repeat_ref.take(2).collect::<Vec<_>>()
        );
        assert_eq!(scope_stack.pop_scope().unwrap(), test_dir_repeat_ref.next());
        assert_eq!(
            *scope_stack.as_vec(),
            test_dir_repeat_ref.take(1).collect::<Vec<_>>()
        );
        assert_eq!(scope_stack.pop_scope().unwrap(), test_dir_repeat_ref.next());
        assert!(scope_stack.as_vec().is_empty());
        assert_eq!(scope_stack.pop_scope().unwrap(), None);
        assert!(scope_stack.as_vec().is_empty());
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

        let mut scope_stack = CwdStack::from(&mut **cwd);
        scope_stack.set(&test_dir).unwrap();

        assert_eq!(scope_stack.get().unwrap(), test_dir);
        assert!(scope_stack.as_vec().is_empty());
        assert_eq!(scope_stack.pop_scope().unwrap(), None);
        assert_eq!(scope_stack.get().unwrap(), test_dir);
        assert!(scope_stack.as_vec().is_empty());

        scope_stack.push_scope().unwrap();
        assert_eq!(*scope_stack.as_vec(), vec![test_dir.clone()]);
        assert_eq!(scope_stack.pop_scope().unwrap(), Some(test_dir.clone()));

        assert_eq!(scope_stack.get().unwrap(), test_dir);
        assert!(scope_stack.as_vec().is_empty());
        assert_eq!(scope_stack.pop_scope().unwrap(), None);
        assert_eq!(scope_stack.get().unwrap(), test_dir);
        assert!(scope_stack.as_vec().is_empty());
    }
}
