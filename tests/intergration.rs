use current_dir::*;
use std::{env, fs, panic, path::PathBuf, sync::OnceLock};

mod test_utilities {
    include!("../src/test_utilities.rs");
}

#[test]
fn recursive_guards() {
    let rm_test_dir = test_dir!("sub/sub");
    let mut locked_cwd = test_utilities::yield_poison_addressed(Cwd::mutex()).unwrap();
    let mut reset_cwd = test_utilities::reset_cwd(&mut locked_cwd);

    let cwd = &mut **reset_cwd;
    let test_dir = rm_test_dir.as_path();

    cwd.set(test_dir).unwrap();
    assert_eq!(cwd.get().unwrap(), *test_dir);
    {
        let mut cwd_guard = CwdGuard::try_from(&mut *cwd).unwrap();
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
    assert_eq!(cwd.get().unwrap(), *test_dir);
}

#[test]
#[should_panic(
    expected = "current working directory can be reset to the initial value: Os { code: 2, kind: NotFound, message: \"No such file or directory\" }"
)]
fn clean_up_poisend() {
    let rm_test_dir = test_dir!();
    let test_dir = rm_test_dir.as_path();
    let initial_dir = OnceLock::<PathBuf>::new();

    let panic = expect_panic!(|| {
        let mut locked_cwd = test_utilities::yield_poison_addressed(Cwd::mutex()).unwrap();
        initial_dir.set(locked_cwd.get().unwrap()).unwrap();

        // cause panic in `_cwd_guard` drop
        locked_cwd.set(test_dir).unwrap();
        let _cwd_guard = CwdGuard::try_from(&mut *locked_cwd).unwrap();
        fs::remove_dir(test_dir).unwrap();
    });

    let mut poisoned_locked_cwd = Cwd::mutex().lock().expect_err("cwd poisoned");
    let mut poisoned_cwd_stack = CwdStack::from(&mut **poisoned_locked_cwd.get_mut());
    assert!(!poisoned_cwd_stack.as_vec().is_empty(), "not dirty");
    assert_eq!(*poisoned_cwd_stack.as_vec(), vec![test_dir]);

    // Fix poisoned cwd
    fs::create_dir_all(test_dir).unwrap();
    assert_eq!(
        poisoned_cwd_stack.pop_cwd().unwrap(),
        Some(test_dir.to_path_buf())
    );
    assert!(poisoned_cwd_stack.as_vec().is_empty());
    let mut locked_cwd = poisoned_locked_cwd.into_inner();

    locked_cwd.set(initial_dir.get().unwrap()).unwrap();

    panic::resume_unwind(panic);
}

#[test]
fn sub_guard_drop_panic_exception_safe() {
    let rm_test_dir = test_dir!("sub/sub");
    let mut locked_cwd = test_utilities::yield_poison_addressed(Cwd::mutex()).unwrap();
    let mut reset_cwd = test_utilities::reset_cwd(&mut locked_cwd);

    let cwd = &mut **reset_cwd;
    let test_dir = rm_test_dir.as_path();

    cwd.set(test_dir).unwrap();
    assert_eq!(cwd.get().unwrap(), *test_dir);

    let panic = expect_panic!(|| {
        let mut cwd_guard = CwdGuard::try_from(&mut *cwd).unwrap();
        cwd_guard.set("sub").unwrap();
        assert_eq!(cwd_guard.get().unwrap(), test_dir.join("sub"));
        let panic = expect_panic!(|| {
            let mut sub_cwd_guard = CwdGuard::try_from(&mut cwd_guard).unwrap();
            sub_cwd_guard.set("sub").unwrap();
            assert_eq!(sub_cwd_guard.get().unwrap(), test_dir.join("sub/sub"));

            // cause panic on drop
            fs::remove_dir_all(test_dir.join("sub")).unwrap();
        });
        // test_dir/sub/sub is deleted too!
        assert_eq!(
            cwd_guard.get().unwrap_err().kind(),
            std::io::ErrorKind::NotFound
        );
        panic::resume_unwind(panic);
    });
    assert_eq!(
        panic
        .downcast_ref(),
        Some(&String::from("current working directory can be reset to the initial value: Os { code: 2, kind: NotFound, message: \"No such file or directory\" }"))
    );
    assert_eq!(cwd.get().unwrap(), *test_dir);
}

#[test]
fn guard_drop_panic_dirty_exception_safe() {
    let rm_test_dir = test_dir!("sub");
    let mut locked_cwd = test_utilities::yield_poison_addressed(Cwd::mutex()).unwrap();
    let mut reset_cwd = test_utilities::reset_cwd(&mut locked_cwd);

    let cwd = &mut **reset_cwd;
    let test_dir = rm_test_dir.as_path();

    cwd.set(test_dir.join("sub")).unwrap();
    assert_eq!(cwd.get().unwrap(), test_dir.join("sub"));

    let panic = expect_panic!(|| {
        let mut cwd_guard = CwdGuard::try_from(&mut *cwd).unwrap();
        cwd_guard.set(test_dir).unwrap();
        assert_eq!(cwd_guard.get().unwrap(), *test_dir);

        // cause panic on drop
        fs::remove_dir_all(test_dir.join("sub")).unwrap();
    });
    assert_eq!(
        panic
        .downcast_ref(),
        Some(&String::from("current working directory can be reset to the initial value: Os { code: 2, kind: NotFound, message: \"No such file or directory\" }"))
    );
    assert_eq!(cwd.get().unwrap(), *test_dir);
    let mut stack = CwdStack::from(&mut *cwd);
    assert_eq!(*stack.as_vec(), vec![test_dir.join("sub")]);
    fs::create_dir(stack.as_vec().last().unwrap()).unwrap();
    stack.pop_cwd().unwrap();
    assert_eq!(cwd.get().unwrap(), test_dir.join("sub"));
}

#[test]
fn external_panic_exception_safe() {
    let rm_test_dir = test_dir!("sub");
    let mut locked_cwd = test_utilities::yield_poison_addressed(Cwd::mutex()).unwrap();
    let mut reset_cwd = test_utilities::reset_cwd(&mut locked_cwd);

    let cwd = &mut **reset_cwd;
    let test_dir = rm_test_dir.as_path();

    cwd.set(test_dir.join("sub")).unwrap();
    assert_eq!(cwd.get().unwrap(), test_dir.join("sub"));

    let panic = expect_panic!(|| {
        let mut cwd_guard = CwdGuard::try_from(&mut *cwd).unwrap();
        cwd_guard.set(test_dir).unwrap();
        assert_eq!(cwd_guard.get().unwrap(), *test_dir);

        panic!("external panic")
    });
    assert_eq!(panic.downcast_ref(), Some(&"external panic"));
    assert_eq!(cwd.get().unwrap(), test_dir.join("sub"));
}

#[test]
fn external_panic_mutex_dropped_exception_safe() {
    let rm_test_dir = test_dir!("sub");
    let test_dir = rm_test_dir.as_path();
    let initial_dir = OnceLock::<PathBuf>::new();

    let panic = expect_panic!(|| {
        let mut locked_cwd = test_utilities::yield_poison_addressed(Cwd::mutex()).unwrap();
        let cwd = &mut *locked_cwd;
        initial_dir.set(cwd.get().unwrap()).unwrap();

        cwd.set(test_dir.join("sub")).unwrap();
        assert_eq!(cwd.get().unwrap(), test_dir.join("sub"));

        let panic = expect_panic!(|| {
            let mut cwd_guard = CwdGuard::try_from(&mut *cwd).unwrap();
            cwd_guard.set(test_dir).unwrap();
            assert_eq!(cwd_guard.get().unwrap(), *test_dir);

            panic!("external panic")
        });
        assert_eq!(panic.downcast_ref(), Some(&"external panic"));
        assert_eq!(cwd.get().unwrap(), test_dir.join("sub"));

        panic::resume_unwind(panic)
    });
    assert_eq!(panic.downcast_ref(), Some(&"external panic"));

    let poisoned_locked_cwd = Cwd::mutex().lock().expect_err("cwd poisoned");
    let mut cwd = poisoned_locked_cwd.into_inner();
    assert_eq!(cwd.get().unwrap(), test_dir.join("sub"));

    cwd.set(initial_dir.get().unwrap()).unwrap();
}
