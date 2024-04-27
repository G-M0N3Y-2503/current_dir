use current_dir::*;
use std::{env, fs, panic, path::PathBuf, sync::OnceLock, time::Duration};

use crate::test_utilities::yield_lock_poisoned;

mod test_utilities {
    include!("../src/test_utilities.rs");
}

#[cfg(test)]
macro_rules! mutex_test {
    ($test:block, $timeout:expr) => {
        assert!(
            mutex_block!($test, $timeout).is_some(),
            "test acquired mutual exclusion within {}s",
            $timeout.as_secs()
        )
    };
    ($mutex:expr, $test:expr, $timeout:expr) => {
        mutex_test!(
            {
                assert!(
                    test_utilities::yield_lock_poisoned($mutex, $timeout)
                        .map($test)
                        .is_some(),
                    "test acquired Cwd lock within {}s",
                    $timeout.as_secs_f64()
                )
            },
            $timeout
        )
    };
    ($($args:tt)+) => {
        mutex_test!($($args)+, core::time::Duration::from_millis(100))
    };
}

#[test]
fn recursive_guards() {
    let rm_test_dir = test_dir!("sub/sub");
    mutex_test!(Cwd::mutex(), |mut locked_cwd| {
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
    })
}

#[test]
fn clean_up_poisend() {
    let rm_test_dir = test_dir!();
    let test_dir = rm_test_dir.as_path();
    let initial_dir = OnceLock::<PathBuf>::new();
    mutex_test!({
        let panic = thread!(|| {
            let mut locked_cwd = yield_lock_poisoned(Cwd::mutex(), Duration::from_millis(100))
                .expect("test acquired Cwd lock within 100ms");
            initial_dir.set(locked_cwd.get().unwrap()).unwrap();

            // cause panic in `_cwd_guard` drop
            locked_cwd.set(test_dir).unwrap();
            let _cwd_guard = CwdGuard::try_from(&mut *locked_cwd).unwrap();
            fs::remove_dir(test_dir).unwrap();
        })
        .expect_err("panicked");

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
        Cwd::mutex().clear_poison();
        let mut locked_cwd = poisoned_locked_cwd.into_inner();
        assert_eq!(locked_cwd.get_expected().unwrap(), expected_cwd);

        locked_cwd.set(initial_dir.get().unwrap()).unwrap();
    });
}

#[test]
fn sub_guard_drop_panic_exception_safe() {
    let rm_test_dir = test_dir!("sub/sub");
    mutex_test!(Cwd::mutex(), |mut locked_cwd| {
        let mut reset_cwd = test_utilities::reset_cwd(&mut locked_cwd);

        let cwd = &mut **reset_cwd;
        let test_dir = rm_test_dir.as_path();

        cwd.set(test_dir).unwrap();
        assert_eq!(cwd.get().unwrap(), *test_dir);

        let panic = thread!(|| {
            let mut cwd_guard = CwdGuard::try_from(&mut *cwd).unwrap();
            cwd_guard.set("sub").unwrap();
            assert_eq!(cwd_guard.get().unwrap(), test_dir.join("sub"));
            let panic = thread!(|| {
                let mut sub_cwd_guard = CwdGuard::try_from(&mut cwd_guard).unwrap();
                sub_cwd_guard.set("sub").unwrap();
                assert_eq!(sub_cwd_guard.get().unwrap(), test_dir.join("sub/sub"));

                // cause panic on drop
                fs::remove_dir_all(test_dir.join("sub")).unwrap();
            })
            .expect_err("panicked");
            // test_dir/sub/sub is deleted too!
            assert_eq!(
                cwd_guard.get().unwrap_err().kind(),
                std::io::ErrorKind::NotFound
            );
            panic::resume_unwind(panic);
        })
        .expect_err("panicked");
        assert_eq!(
            panic.downcast_ref::<std::io::Error>().unwrap().to_string(),
            "No such file or directory (os error 2)"
        );
        assert_eq!(cwd.get().unwrap(), *test_dir);
    })
}

#[test]
fn guard_drop_panic_dirty_exception_safe() {
    let rm_test_dir = test_dir!("sub");
    mutex_test!(Cwd::mutex(), |mut locked_cwd| {
        let mut reset_cwd = test_utilities::reset_cwd(&mut locked_cwd);

        let cwd = &mut **reset_cwd;
        let test_dir = rm_test_dir.as_path();

        cwd.set(test_dir.join("sub")).unwrap();
        assert_eq!(cwd.get().unwrap(), test_dir.join("sub"));

        let panic = thread!(|| {
            let mut cwd_guard = CwdGuard::try_from(&mut *cwd).unwrap();
            cwd_guard.set(test_dir).unwrap();
            assert_eq!(cwd_guard.get().unwrap(), *test_dir);

            // cause panic on drop
            fs::remove_dir_all(test_dir.join("sub")).unwrap();
        })
        .expect_err("panicked");
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
    })
}

#[test]
fn external_panic_exception_safe() {
    let rm_test_dir = test_dir!("sub");
    mutex_test!(Cwd::mutex(), |mut locked_cwd| {
        let mut reset_cwd = test_utilities::reset_cwd(&mut locked_cwd);

        let cwd = &mut **reset_cwd;
        let test_dir = rm_test_dir.as_path();

        cwd.set(test_dir.join("sub")).unwrap();
        assert_eq!(cwd.get().unwrap(), test_dir.join("sub"));

        let panic = thread!(|| {
            let mut cwd_guard = CwdGuard::try_from(&mut *cwd).unwrap();
            cwd_guard.set(test_dir).unwrap();
            assert_eq!(cwd_guard.get().unwrap(), *test_dir);

            panic!("external panic")
        })
        .expect_err("panicked");
        assert_eq!(panic.downcast_ref(), Some(&"external panic"));
        assert_eq!(cwd.get().unwrap(), test_dir.join("sub"));
    })
}

#[test]
fn external_panic_mutex_dropped_exception_safe() {
    let rm_test_dir = test_dir!("sub");
    let test_dir = rm_test_dir.as_path();
    let initial_dir = OnceLock::<PathBuf>::new();
    mutex_test!({
        let panic = thread!(|| {
            let mut locked_cwd =
                yield_lock_poisoned(Cwd::mutex(), std::time::Duration::from_millis(100)).unwrap();
            let cwd = &mut *locked_cwd;
            initial_dir.set(cwd.get().unwrap()).unwrap();

            cwd.set(test_dir.join("sub")).unwrap();
            assert_eq!(cwd.get().unwrap(), test_dir.join("sub"));

            let panic = thread!(|| {
                let mut cwd_guard = CwdGuard::try_from(&mut *cwd).unwrap();
                cwd_guard.set(test_dir).unwrap();
                assert_eq!(cwd_guard.get().unwrap(), *test_dir);

                panic!("external panic")
            })
            .expect_err("panicked");
            assert_eq!(panic.downcast_ref(), Some(&"external panic"));
            assert_eq!(cwd.get().unwrap(), test_dir.join("sub"));

            panic::resume_unwind(panic)
        })
        .expect_err("panicked");
        assert_eq!(panic.downcast_ref(), Some(&"external panic"));

        let poisoned_locked_cwd = Cwd::mutex().lock().expect_err("cwd poisoned");
        Cwd::mutex().clear_poison();
        let mut cwd = poisoned_locked_cwd.into_inner();
        assert_eq!(cwd.get().unwrap(), test_dir.join("sub"));

        cwd.set(initial_dir.get().unwrap()).unwrap();
    });
}
