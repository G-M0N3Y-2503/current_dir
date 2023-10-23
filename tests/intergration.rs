use current_dir::*;
use std::{env, fs, panic, path, thread, time::Duration};
use with_drop::with_drop;

mod test_utilities {
    include!("../src/test_utilities.rs");
}

#[test]
fn recursive_guards() {
    let root_test_dir = env::temp_dir().join(called_from!().replace(path::MAIN_SEPARATOR_STR, "|"));
    let rm_test_dir = with_drop(&root_test_dir, |dir| fs::remove_dir_all(dir).unwrap());
    let test_dir = *rm_test_dir;
    fs::create_dir_all(rm_test_dir.join("sub/sub")).unwrap();

    let mut locked_cwd =
        test_utilities::yield_poison_addressed(Cwd::mutex(), Duration::from_millis(500)).unwrap();
    locked_cwd.set(test_dir).unwrap();
    assert_eq!(locked_cwd.get().unwrap(), *test_dir);
    {
        let mut cwd_guard = CwdGuard::try_from(&mut *locked_cwd).unwrap();
        cwd_guard.set("sub").unwrap();
        assert_eq!(cwd_guard.get().unwrap(), test_dir.join("sub"));
        {
            let mut sub_cwd_guard = CwdGuard::try_from(&mut cwd_guard).unwrap();
            sub_cwd_guard.set("sub").unwrap();
            assert_eq!(sub_cwd_guard.get().unwrap(), test_dir.join("sub/sub"));
            {
                let mut sub_sub_cwd_guard = CwdGuard::try_from(&mut sub_cwd_guard).unwrap();
                sub_sub_cwd_guard.set(test_dir).unwrap();
                assert_eq!(sub_sub_cwd_guard.get().unwrap(), *test_dir);
            }
            assert_eq!(sub_cwd_guard.get().unwrap(), test_dir.join("sub/sub"));
        }
        assert_eq!(cwd_guard.get().unwrap(), test_dir.join("sub"));
    }
    assert_eq!(locked_cwd.get().unwrap(), *test_dir);
}

#[test]
#[should_panic(
    expected = "current working directory can be reset to the initial value: Os { code: 2, kind: NotFound, message: \"No such file or directory\" }"
)]
fn clean_up_poisend() {
    let root_test_dir = env::temp_dir().join(called_from!().replace(path::MAIN_SEPARATOR_STR, "|"));
    let rm_test_dir = with_drop(&root_test_dir, |dir| fs::remove_dir(dir).unwrap());
    let test_dir = *rm_test_dir;
    fs::create_dir_all(test_dir).unwrap();

    let panic = thread::scope(|scope| {
        scope
            .spawn(|| {
                let mut locked_cwd = test_utilities::yield_poison_addressed(
                    Cwd::mutex(),
                    Duration::from_millis(500),
                )
                .unwrap();
                let mut reset_cwd = test_utilities::reset_cwd(&mut locked_cwd);

                // cause panic in `_cwd_guard` drop
                reset_cwd.set(test_dir).unwrap();
                let _cwd_guard = CwdGuard::try_from(&mut **reset_cwd).unwrap();
                fs::remove_dir(test_dir).unwrap();
            })
            .join()
    })
    .expect_err("thread panicked");

    let mut poisoned_locked_cwd = Cwd::mutex().lock().expect_err("cwd poisoned");
    let mut poisoned_cwd_stack = CwdStack::from(&mut **poisoned_locked_cwd.get_mut());
    assert!(!poisoned_cwd_stack.as_vec().is_empty(), "not dirty");
    assert_eq!(*poisoned_cwd_stack.as_vec(), vec![(test_dir).clone()]);

    // Fix poisoned cwd
    fs::create_dir_all(test_dir).unwrap();
    assert_eq!(
        poisoned_cwd_stack.pop_cwd().unwrap(),
        Some((test_dir).clone())
    );
    let _locked_cwd = poisoned_locked_cwd.into_inner();

    panic::resume_unwind(panic);
}
