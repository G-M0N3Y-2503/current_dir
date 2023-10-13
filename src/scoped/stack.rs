use super::*;

pub struct CurrentWorkingDirectoryStack<'locked_cwd> {
    locked_cwd: &'locked_cwd mut crate::CurrentWorkingDirectory,
}
impl<'locked_cwd> CurrentWorkingDirectoryStack<'locked_cwd> {
    pub(super) fn new(&mut self) -> CurrentWorkingDirectoryStack<'_> {
        CurrentWorkingDirectoryStack {
            locked_cwd: self.locked_cwd,
        }
    }

    pub fn push_scope(&mut self) -> io::Result<()> {
        let cwd = self.get()?;
        self.as_mut_vec().push(cwd);
        Ok(())
    }

    pub fn pop_scope(&mut self) -> io::Result<Option<PathBuf>> {
        match self.as_mut_vec().pop() {
            Some(previous) => match self.set(&previous) {
                Ok(_) => Ok(Some(previous)),
                Err(err) => {
                    self.as_mut_vec().push(previous);
                    Err(err)
                }
            },
            None => Ok(None),
        }
    }

    pub fn as_vec(&self) -> &Vec<PathBuf> {
        &self.locked_cwd.scope_stack
    }

    pub fn as_mut_vec(&mut self) -> &mut Vec<PathBuf> {
        &mut self.locked_cwd.scope_stack
    }
}
impl CurrentWorkingDirectoryAccessor for CurrentWorkingDirectoryStack<'_> {}
impl<'locked_cwd> From<&'locked_cwd mut crate::CurrentWorkingDirectory>
    for CurrentWorkingDirectoryStack<'locked_cwd>
{
    fn from(locked_cwd: &'locked_cwd mut crate::CurrentWorkingDirectory) -> Self {
        Self { locked_cwd }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{aliases::*, test_utilities};
    use std::{error::Error, fs, iter, path, time::Duration};

    #[test]
    fn test_stack() -> Result<(), Box<dyn Error>> {
        let mut locked_cwd =
            test_utilities::yield_poison_addressed(Cwd::mutex(), Duration::from_millis(500))
                .unwrap();

        let mut scope_stack = CurrentWorkingDirectoryStack::from(&mut *locked_cwd);
        assert!(scope_stack.as_vec().is_empty());

        let mut cwd_stack = iter::repeat(scope_stack.get().unwrap());
        let cwd_stack = cwd_stack.by_ref();
        assert!(scope_stack.as_vec().is_empty());

        const TEST_RANGE: std::ops::Range<isize> = 1..20;
        for i in TEST_RANGE {
            scope_stack.push_scope().unwrap();
            let expected = cwd_stack.take(i as usize).collect::<Vec<_>>();
            assert_eq!(
                *scope_stack.as_vec(),
                expected,
                "left: {}, right: {}",
                scope_stack.as_vec().len(),
                expected.len()
            );
        }
        for i in TEST_RANGE.rev() {
            assert_eq!(scope_stack.pop_scope().unwrap(), cwd_stack.next());
            let expected = cwd_stack.take((i - 1) as usize).collect::<Vec<_>>();
            assert_eq!(
                *scope_stack.as_vec(),
                expected,
                "left: {}, right: {}",
                scope_stack.as_vec().len(),
                expected.len()
            );
        }
        assert!(scope_stack.as_vec().is_empty());

        Ok(())
    }

    #[test]
    fn test_delete_cwd() {
        let mut locked_cwd =
            test_utilities::yield_poison_addressed(Cwd::mutex(), Duration::from_millis(500))
                .unwrap();
        let mut locked_cwd = test_utilities::reset_cwd(&mut locked_cwd);
        let test_dir = env::temp_dir().join(called_from!().replace(path::MAIN_SEPARATOR_STR, "|"));
        fs::create_dir(&test_dir).unwrap();
        let _clean_up_test_dir = with_drop::with_drop((), |_| fs::remove_dir(&test_dir).unwrap());

        let mut scope_stack = CurrentWorkingDirectoryStack::from(&mut **locked_cwd);
        scope_stack.set(&test_dir).unwrap();

        let mut test_dir_repeat = iter::repeat(test_dir.clone());
        let test_dir_repeat = test_dir_repeat.by_ref();

        assert!(scope_stack.as_vec().is_empty());
        scope_stack.push_scope().unwrap();
        assert_eq!(
            *scope_stack.as_vec(),
            test_dir_repeat.take(1).collect::<Vec<_>>()
        );
        scope_stack.push_scope().unwrap();
        assert_eq!(
            *scope_stack.as_vec(),
            test_dir_repeat.take(2).collect::<Vec<_>>()
        );
        scope_stack.push_scope().unwrap();
        assert_eq!(
            *scope_stack.as_vec(),
            test_dir_repeat.take(3).collect::<Vec<_>>()
        );

        fs::remove_dir(&test_dir).unwrap();

        assert_eq!(
            scope_stack.pop_scope().unwrap_err().kind(),
            io::ErrorKind::NotFound
        );
        assert_eq!(
            *scope_stack.as_vec(),
            test_dir_repeat.take(3).collect::<Vec<_>>()
        );
        assert_eq!(
            scope_stack.push_scope().unwrap_err().kind(),
            io::ErrorKind::NotFound
        );
        assert_eq!(
            *scope_stack.as_vec(),
            test_dir_repeat.take(3).collect::<Vec<_>>()
        );

        fs::create_dir(&test_dir).unwrap();
        scope_stack.set(&test_dir).unwrap();

        scope_stack.push_scope().unwrap();
        assert_eq!(
            *scope_stack.as_vec(),
            test_dir_repeat.take(4).collect::<Vec<_>>()
        );

        assert_eq!(scope_stack.pop_scope().unwrap(), test_dir_repeat.next());
        assert_eq!(
            *scope_stack.as_vec(),
            test_dir_repeat.take(3).collect::<Vec<_>>()
        );
        assert_eq!(scope_stack.pop_scope().unwrap(), test_dir_repeat.next());
        assert_eq!(
            *scope_stack.as_vec(),
            test_dir_repeat.take(2).collect::<Vec<_>>()
        );
        assert_eq!(scope_stack.pop_scope().unwrap(), test_dir_repeat.next());
        assert_eq!(
            *scope_stack.as_vec(),
            test_dir_repeat.take(1).collect::<Vec<_>>()
        );
        assert_eq!(scope_stack.pop_scope().unwrap(), test_dir_repeat.next());
        assert!(scope_stack.as_vec().is_empty());
        assert_eq!(scope_stack.pop_scope().unwrap(), None);
        assert!(scope_stack.as_vec().is_empty());
    }

    #[test]
    fn test_pop_empty() {
        let mut locked_cwd =
            test_utilities::yield_poison_addressed(Cwd::mutex(), Duration::from_millis(500))
                .unwrap();
        let mut locked_cwd = test_utilities::reset_cwd(&mut locked_cwd);

        let test_dir = env::temp_dir().join(called_from!().replace(path::MAIN_SEPARATOR_STR, "|"));
        fs::create_dir(&test_dir).unwrap();
        let _clean_up_test_dir = with_drop::with_drop((), |_| fs::remove_dir(&test_dir).unwrap());

        let mut scope_stack = CurrentWorkingDirectoryStack::from(&mut **locked_cwd);
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
