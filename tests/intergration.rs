#![cfg_attr(
    all(feature = "unstable", feature = "nightly"),
    feature(mutex_unpoison)
)]
use current_dir::*;
use std::{env, fs, panic, path::PathBuf, sync::{OnceLock, MutexGuard}};

mod test_utilities {
    include!("../src/test_utilities.rs");
}

#[cfg(test)]
fn test_mutex<T>(
    mutex: &std::sync::Mutex<T>,
) -> Result<
    (std::sync::MutexGuard<'_, ()>, std::sync::MutexGuard<'_, T>),
    std::sync::TryLockError<(std::sync::MutexGuard<'_, ()>, std::sync::MutexGuard<'_, T>)>,
> {
    test_utilities::yield_test_mutex(mutex, std::time::Duration::from_millis(100))
}

#[test]
fn recursive_guards() {
    let rm_test_dir = test_dir!("sub/sub");
    let (_test_lock, mut locked_cwd) = test_mutex(Cwd::mutex()).unwrap();
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
fn clean_up_poisend() {
    let rm_test_dir = test_dir!();
    let test_dir = rm_test_dir.as_path();
    let initial_dir = OnceLock::<PathBuf>::new();

    let panic = expect_panic!(|| {
        let (_test_lock, mut locked_cwd) = test_mutex(Cwd::mutex()).unwrap();
        initial_dir.set(locked_cwd.get().unwrap()).unwrap();

        // cause panic in `_cwd_guard` drop
        locked_cwd.set(test_dir).unwrap();
        let _cwd_guard = CwdGuard::try_from(&mut *locked_cwd).unwrap();
        fs::remove_dir(test_dir).unwrap();
    });

    let mut poisoned_locked_cwd = Cwd::mutex().lock().expect_err("cwd poisoned");
    assert_eq!(
        panic.downcast_ref::<std::io::Error>().unwrap().to_string(),
        "No such file or directory (os error 2)"
    );
    let expected_cwd = poisoned_locked_cwd
        .get_ref()
        .get_expected()
        .expect("panic sets expected cwd")
        .to_owned();
    assert_eq!(expected_cwd, test_dir);

    // Fix poisoned cwd
    fs::create_dir_all(&expected_cwd).unwrap();
    poisoned_locked_cwd.get_mut().set(&expected_cwd).unwrap();
    #[cfg(all(feature = "unstable", feature = "nightly"))]
    Cwd::mutex().clear_poison();
    let mut locked_cwd = poisoned_locked_cwd.into_inner();
    assert_eq!(locked_cwd.get_expected().unwrap(), expected_cwd);

    locked_cwd.set(initial_dir.get().unwrap()).unwrap();
}

#[test]
fn sub_guard_drop_panic_exception_safe() {
    let rm_test_dir = test_dir!("sub/sub");
    let (_test_lock, mut locked_cwd) = test_mutex(Cwd::mutex()).unwrap();
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
        panic.downcast_ref::<std::io::Error>().unwrap().to_string(),
        "No such file or directory (os error 2)"
    );
    assert_eq!(cwd.get().unwrap(), *test_dir);
}

#[test]
fn guard_drop_panic_dirty_exception_safe() {
    let rm_test_dir = test_dir!("sub");
    let (_test_lock, mut locked_cwd) = test_mutex(Cwd::mutex()).unwrap();
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
        panic.downcast_ref::<std::io::Error>().unwrap().to_string(),
        "No such file or directory (os error 2)"
    );
    assert_eq!(cwd.get().unwrap(), *test_dir);
    let expected_cwd = cwd.get_expected().unwrap().to_owned();
    assert_eq!(*expected_cwd, test_dir.join("sub"));
    fs::create_dir(&expected_cwd).unwrap();
    cwd.set(&expected_cwd).unwrap();
    assert_eq!(cwd.get().unwrap(), expected_cwd);
}

#[test]
fn external_panic_exception_safe() {
    let rm_test_dir = test_dir!("sub");
    let (_test_lock, mut locked_cwd) = test_mutex(Cwd::mutex()).unwrap();
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
    let test_lock= OnceLock::<MutexGuard<()>>::new();

    let panic = expect_panic!(|| {
        let (test, mut locked_cwd) = test_mutex(Cwd::mutex()).unwrap();
        test_lock.set(test);
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
    #[cfg(all(feature = "unstable", feature = "nightly"))]
    Cwd::mutex().clear_poison();
    let mut cwd = poisoned_locked_cwd.into_inner();
    assert_eq!(cwd.get().unwrap(), test_dir.join("sub"));

    cwd.set(initial_dir.get().unwrap()).unwrap();
}
