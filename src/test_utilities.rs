use core::time::Duration;
use std::{
    env::temp_dir,
    panic,
    sync::{Mutex, MutexGuard, TryLockError, TryLockResult},
    thread::{self, yield_now},
    time::Instant,
};
use with_drop::*;

/// using [super] so we can include!() this in ../tests/intergration.rs)
use super::*;

/// Creates a unique string with information about where the macro was written.
/// # Form
/// `package_name /path/to/file.rs:68:20`
#[macro_export]
macro_rules! called_from {
    () => {{
        let mut call_location = env!("CARGO_PKG_NAME").to_owned();
        call_location.push_str(concat!(' ', file!(), ':', line!(), ':', column!()));
        call_location
    }};
}

#[test]
fn test_called_from() {
    assert_ne!(called_from!(), called_from!());
    let left = called_from!();
    let right = called_from!();
    assert_ne!(left, right);
}

/// Spawns the function in a new thread.
#[macro_export]
macro_rules! thread {
    ($name:expr, $f:expr) => {
        std::thread::scope(|scope| {
            std::thread::Builder::new()
                .name($name)
                .spawn_scoped(scope, $f)
                .expect("failed to spawn thread")
                .join()
        })
    };
    ($f:expr) => {
        thread!(format!("thread spawned at {}", called_from!()), $f)
    };
}

#[test]
fn test_thread() {
    assert_eq!(thread!(|| { Ok::<(), ()>(()) }).unwrap(), Ok(()));

    assert_eq!(
        thread!("named thread".to_owned(), || Ok::<(), ()>(())).unwrap(),
        Ok(())
    );

    assert_eq!(
        thread!(|| panic!("str panic"))
            .expect_err("panicked")
            .downcast_ref(),
        Some(&"str panic")
    );

    assert_eq!(
        thread!(|| panic::panic_any(58i32))
            .expect_err("panicked")
            .downcast_ref(),
        Some(&58i32)
    );

    let mv = String::from("panic String");
    assert_eq!(
        thread!(|| panic::panic_any(mv))
            .expect_err("panicked")
            .downcast_ref(),
        Some(&String::from("panic String"))
    );

    let by_ref = "panic &str";
    assert_eq!(
        thread!(|| panic::panic_any(by_ref))
            .expect_err("panicked")
            .downcast_ref(),
        Some(&"panic &str")
    );
    assert_eq!(by_ref, by_ref);
}

/// Creates a unique directory and any provided sub-directories that will be deleted on drop.
#[macro_export]
macro_rules! test_dir {
    ($($sub_path:expr),*) => {{
        let test_dir = with_drop::with_drop(
            std::env::temp_dir().join(called_from!().replace(std::path::MAIN_SEPARATOR_STR, "|")),
            |dir| {
                if dir.exists() {
                    std::fs::remove_dir_all(dir).expect("Can clean up test directory on drop")
                }
            },
        );
        std::fs::create_dir_all(&*test_dir$(.join($sub_path))*).expect("Can create test directory");
        test_dir
    }};
}

#[test]
fn test_test_dir() {
    let test_dir_1 = test_dir!();
    assert!(test_dir_1.exists());

    let test_dir_2 = test_dir!();
    assert!(test_dir_2.exists());

    assert_ne!(*test_dir_1, *test_dir_2);

    let test_dir_1_path = (*test_dir_1).clone();
    drop(test_dir_1);
    assert!(!test_dir_1_path.exists());

    let test_dir_2_path = (*test_dir_2).clone();
    drop(test_dir_2);
    assert!(!test_dir_2_path.exists());

    let test_dir_with_subs_1 = test_dir!("dir1", "dir2");
    assert!(test_dir_with_subs_1.join("dir1/dir2").exists());

    let test_dir_with_subs_2 = test_dir!("dir1", "dir2");
    assert!(test_dir_with_subs_2.join("dir1/dir2").exists());

    assert_ne!(*test_dir_with_subs_1, *test_dir_with_subs_2);

    let test_dir_with_subs_1_path = (*test_dir_with_subs_1).clone();
    drop(test_dir_with_subs_1);
    assert!(!test_dir_with_subs_1_path.exists());

    let test_dir_with_subs_2_path = (*test_dir_with_subs_2).clone();
    drop(test_dir_with_subs_2);
    assert!(!test_dir_with_subs_2_path.exists());
}

pub fn try_lock_poisoned<T>(mutex: &Mutex<T>) -> Option<MutexGuard<'_, T>> {
    match mutex.try_lock() {
        Ok(locked_mutex) => Some(locked_mutex),
        Err(TryLockError::Poisoned(poisoned_locked_mutex)) => {
            Some(poisoned_locked_mutex.into_inner())
        }
        Err(TryLockError::WouldBlock) => None,
    }
}

/// locks mutexes as per [`try_lock_poisoned()`] but [`yield_now()`] up to `timeout` if [`try_lock_poisoned()`] would block.
pub fn yield_lock_poisoned<T>(mutex: &Mutex<T>, timeout: Duration) -> Option<MutexGuard<'_, T>> {
    let start = Instant::now();
    loop {
        match try_lock_poisoned(mutex) {
            None if start.elapsed() < timeout => yield_now(),
            res => break res,
        }
    }
}

// /// Locks the given mutex only if the static test mutex can also be locked.
// pub fn test_locked_mutex<'test_locked, 'mutex, T>(
//     mutex: &'mutex Mutex<T>,
//     test_lock: Option<MutexGuard<'test_locked, ()>>,
// ) -> TryLockResult<(MutexGuard<'test_locked, ()>, MutexGuard<'mutex, T>)> {
//     match (STATIC_MUTEX.try_lock(), mutex.try_lock()) {
//         (Ok(locked_test), Ok(locked_mutex)) => Ok((locked_test, locked_mutex)),
//         (Err(TryLockError::WouldBlock), _) | (_, Err(TryLockError::WouldBlock)) => {
//             Err(TryLockError::WouldBlock)
//         }
//         (locked_test_res, Err(TryLockError::Poisoned(poisoned_locked_mutex))) => {
//             Err(TryLockError::Poisoned(PoisonError::new((
//                 match locked_test_res {
//                     Ok(locked_test) => locked_test,
//                     Err(TryLockError::Poisoned(poisoned_locked_test)) => {
//                         poisoned_locked_test.into_inner()
//                     }
//                     Err(TryLockError::WouldBlock) => unreachable!(),
//                 },
//                 poisoned_locked_mutex.into_inner(),
//             ))))
//         }
//         (Err(TryLockError::Poisoned(poisoned_locked_test)), Ok(locked_mutex)) => {
//             Ok((poisoned_locked_test.into_inner(), locked_mutex))
//         }
//     }
// }

pub static STATIC_MUTEX: Mutex<()> = Mutex::new(());
#[macro_export]
macro_rules! mutex_thread {
    ($test:expr, $timeout:expr) => {
        test_utilities::yield_lock_poisoned(&test_utilities::STATIC_MUTEX, $timeout)
            .map(|_lock| thread!(format!("mutex_test at {}", called_from!()), $test))
    };
}

#[test]
fn test_mutex_tests() {
    static BLOCK_TEST: Mutex<bool> = Mutex::new(false);
    let t1 = thread::spawn(|| {
        assert_eq!(
            mutex_thread!(
                || {
                    *BLOCK_TEST.lock().unwrap() = true;
                    while *BLOCK_TEST.lock().unwrap() {
                        yield_now();
                    }

                    panic!("explicit panic")
                },
                Duration::from_millis(1)
            )
            .expect("acquired mutual exclusion")
            .expect_err("thread pannicked")
            .downcast_ref(),
            Some(&"explicit panic")
        )
    });

    thread::spawn(|| {
        let mut blocked = loop {
            match BLOCK_TEST.lock().unwrap() {
                blocked if *blocked => break blocked,
                _ => yield_now(),
            }
        };

        assert!(matches!(
            mutex_thread!(|| {}, Duration::from_millis(1))
                .("could not acquired mutual exclusion"),
            TryLockError::WouldBlock
        ));

        *blocked = false;
    })
    .join()
    .expect("thread didn't panicked");

    t1.join().expect("thread didn't panicked");

    mutex_thread!(|| {}, Duration::from_millis(1))
        .expect("acquired mutual exclusion")
        .expect("test didn't panic");
}

/// locks [`STATIC_MUTEX`] and the given `mutex` as per [`try_lock_poisoned()`] but [`yield_now()`] up to `timeout` if the mutexes would block.
pub fn yield_test_locked_mutex<T>(
    mutex: &Mutex<T>,
    timeout: Duration,
) -> TryLockResult<(MutexGuard<'_, ()>, MutexGuard<'_, T>)> {
    let start = Instant::now();
    loop {
        match (try_lock_poisoned(&STATIC_MUTEX), try_lock_poisoned(mutex)) {
            (Err(TryLockError::WouldBlock), _) | (_, Err(TryLockError::WouldBlock)) => {
                if start.elapsed() < timeout {
                    yield_now();
                }
            }
            (Err(TryLockError::Poisoned(_)), _) | (_, Err(TryLockError::Poisoned(_))) => {
                unreachable!()
            }
            (Ok(locked_test), Ok(locked_mutex)) => return Ok((locked_test, locked_mutex)),
        }
    }
}

#[test]
fn test_test_locked_mutex() {
    let mutex = Mutex::new(true);

    let (locked_test, locked_mutex) = yield_test_locked_mutex(&mutex, Duration::MAX)
        .expect("test lock should never return poisoned");
    assert!(matches!(
        yield_test_locked_mutex(&mutex, Duration::from_millis(1)),
        Err(TryLockError::WouldBlock)
    ));

    drop(locked_mutex);
    assert!(matches!(
        yield_test_locked_mutex(&mutex, Duration::from_millis(1)),
        Err(TryLockError::WouldBlock)
    ));

    let locked_mutex = mutex.lock().unwrap();
    assert!(matches!(
        yield_test_locked_mutex(&mutex, Duration::from_millis(1)),
        Err(TryLockError::WouldBlock)
    ));

    drop(locked_test);
    assert!(matches!(
        yield_test_locked_mutex(&mutex, Duration::from_millis(1)),
        Err(TryLockError::WouldBlock)
    ));

    drop(locked_mutex);
    let (_locked_test, _locked_mutex) = yield_test_locked_mutex(&mutex, Duration::MAX)
        .expect("test lock should never return poisoned");
}

#[test]
fn test_test_locked_mutex_not_poisoned() {
    let mutex = Mutex::new(true);

    thread!(|| {
        let _locks = yield_test_locked_mutex(&mutex, Duration::MAX)
            .expect("test lock should never return poisoned");
        panic!();
    })
    .expect_err("panicked");

    assert!(matches!(
        yield_test_locked_mutex(&mutex, Duration::MAX),
        Err(TryLockError::Poisoned(_))
    ));

    let mutex = Mutex::new(true);
    let _locks = yield_test_locked_mutex(&mutex, Duration::MAX)
        .expect("test lock should never return poisoned");
}

/// Returns the `locked_cwd` that will reset to the current working directory when dropped.
/// # Panics
/// The returned closure panics if the current working directory cannot be set to the current working
/// directory cached at the time of the call to [`reset_cwd()`].
pub fn reset_cwd(locked_cwd: &mut Cwd) -> WithDrop<&mut Cwd, impl FnOnce(&mut Cwd)> {
    let initial_cwd = locked_cwd.get().unwrap();
    let reset_cwd_fn = move |cwd: &mut Cwd| {
        cwd.set(&initial_cwd)
            .expect("initial CWD should still be valid");
    };
    with_drop(locked_cwd, reset_cwd_fn)
}

#[test]
fn test_reset_cwd() {
    let (_unused, mut locked_cwd_guard) =
        yield_test_locked_mutex(Cwd::mutex(), Duration::MAX).unwrap();

    assert_ne!(locked_cwd_guard.get().unwrap(), temp_dir());

    {
        let mut locked_cwd = reset_cwd(&mut locked_cwd_guard);
        assert_ne!(locked_cwd.get().unwrap(), temp_dir());
        locked_cwd.set(temp_dir()).unwrap();
        assert_eq!(locked_cwd.get().unwrap(), temp_dir());
    };

    assert_ne!(locked_cwd_guard.get().unwrap(), temp_dir());
}

#[test]
fn test_reset_cwd_panic() {
    let (_unused, mut locked_cwd_guard) =
        yield_test_locked_mutex(Cwd::mutex(), Duration::MAX).unwrap();

    assert_ne!(locked_cwd_guard.get().unwrap(), temp_dir());
    let mut locked_cwd = reset_cwd(&mut locked_cwd_guard);
    assert_ne!(locked_cwd.get().unwrap(), temp_dir());

    let test_cwd_panic = thread!(move || {
        locked_cwd.set(temp_dir()).unwrap();
        assert_eq!(locked_cwd.get().unwrap(), temp_dir());
        panic!("test panic")
    })
    .expect_err("panicked");
    assert_eq!(test_cwd_panic.downcast_ref(), Some(&"test panic"));

    assert_ne!(locked_cwd_guard.get().unwrap(), temp_dir());
}
