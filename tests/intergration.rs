use current_dir::*;
use std::{env, time::Duration};

mod test_utilities {
    include!("../src/test_utilities.rs");
}

#[allow(clippy::significant_drop_tightening)] // false positive
#[test]
fn recursive_scopes() {
    std::thread::sleep(Duration::from_secs(1));
    let mut cwd =
        test_utilities::yield_poison_addressed(CurrentWorkingDirectory::mutex(), Duration::from_millis(500))
            .expect("no test failed to clean up poison");
    let initial_cwd = cwd.get().unwrap();
    cwd.set(env::temp_dir()).unwrap();

    {
        let mut scoped_cwd = cwd.scoped().unwrap();
        scoped_cwd.set(env::temp_dir()).unwrap();

        let mut sub_scoped_cwd = scoped::CurrentWorkingDirectory::new(&mut scoped_cwd).unwrap();
        sub_scoped_cwd.set(env::temp_dir()).unwrap();

        let mut sub_sub_scoped_cwd = sub_scoped_cwd.new().unwrap();
        sub_sub_scoped_cwd.set(env::temp_dir()).unwrap();
    };

    cwd.set(initial_cwd).unwrap();
}

#[test]
#[should_panic(
    expected = "current working directory can be set: Os { code: 2, kind: NotFound, message: \"No such file or directory\" }"
)]
fn clean_up_poisend() {
    use std::{env, fs, panic, path, thread};

    let test_dir =
        env::temp_dir().join(called_from!().replace(path::MAIN_SEPARATOR_STR, "|"));
    if !test_dir.exists() {
        fs::create_dir(&test_dir).unwrap();
    }

    let thread_result = thread::scope(|s| {
        s.spawn(|| {
            let mut locked_cwd = test_utilities::yield_poison_addressed(
                CurrentWorkingDirectory::mutex(),
                Duration::from_millis(500),
            )
            .expect("no test failed to clean up poison");
            locked_cwd.set(&test_dir).unwrap();
            let _scope_locked_cwd = locked_cwd.scoped().unwrap();

            // delete scoped cwd reset dir
            fs::remove_dir(&test_dir).unwrap();
        })
        .join()
    });

    let mut poisoned_locked_cwd = CurrentWorkingDirectory::mutex().lock().expect_err("cwd poisoned");
    let mut poisoned_scope_stack = poisoned_locked_cwd.get_mut().scope_stack();
    assert_eq!(*poisoned_scope_stack.as_vec(), vec![test_dir.clone()]);

    // Fix poisoned cwd
    fs::create_dir(&test_dir).unwrap();
    assert_eq!(poisoned_scope_stack.pop_scope().unwrap(), Some(test_dir));
    let _locked_cwd = poisoned_locked_cwd.into_inner();

    panic::resume_unwind(thread_result.expect_err("thread panicked"));
}
