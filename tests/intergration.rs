use current_dir::{prelude::*, *};
use std::{env, time::Duration};
use with_drop::with_drop;

use crate::test_utilities::reset_cwd;

mod test_utilities {
    include!("../src/test_utilities.rs");
}

#[test]
fn recursive_scopes() {
    let mut cwd = test_utilities::yield_poison_addressed(Cwd::mutex(), Duration::from_millis(500))
        .expect("no test failed to clean up poison");
    let mut cwd = test_utilities::reset_cwd(&mut cwd);

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
    use std::{env, fs, panic, path, thread};

    let test_dir = env::temp_dir().join(called_from!().replace(path::MAIN_SEPARATOR_STR, "|"));
    let test_dir = with_drop(&test_dir, |test_dir| {
        if test_dir.exists() {
            fs::remove_dir(test_dir).unwrap();
        }
    });
    fs::create_dir(*test_dir).unwrap();

    let panic = thread::scope(|s| {
        s.spawn(|| {
            let mut locked_cwd =
                test_utilities::yield_poison_addressed(Cwd::mutex(), Duration::from_millis(500))
                    .unwrap();
            let mut locked_cwd = reset_cwd(&mut locked_cwd);

            // cause panic in `_scope_locked_cwd` drop
            locked_cwd.set(*test_dir).unwrap();
            let _scope_locked_cwd = ScopedCwd::try_from(&mut **locked_cwd).unwrap();
            fs::remove_dir(*test_dir).unwrap();
        })
        .join()
    })
    .expect_err("thread panicked");

    let mut poisoned_locked_cwd = Cwd::mutex().lock().expect_err("cwd poisoned");
    let mut poisoned_scope_stack = CwdStack::from(&mut **poisoned_locked_cwd.get_mut());
    assert!(!poisoned_scope_stack.as_vec().is_empty(), "not dirty");
    assert_eq!(*poisoned_scope_stack.as_vec(), vec![(*test_dir).clone()]);

    // Fix poisoned cwd
    fs::create_dir(*test_dir).unwrap();
    assert_eq!(
        poisoned_scope_stack.pop_scope().unwrap(),
        Some((*test_dir).clone())
    );
    let _locked_cwd = poisoned_locked_cwd.into_inner();

    panic::resume_unwind(panic);
}
