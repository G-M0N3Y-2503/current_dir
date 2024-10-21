use core::{
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};
use std::{
    env::temp_dir,
    panic,
    sync::{Mutex, MutexGuard, TryLockError},
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
#[expect(clippy::panic, reason = "testing panic behaviour")]
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
        thread!(|| panic::panic_any(58_i32))
            .expect_err("panicked")
            .downcast_ref(),
        Some(&58_i32)
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

#[expect(clippy::single_call_fn, reason = "readability and logical separation")]
pub fn try_lock_poisoned<T>(mutex: &Mutex<T>) -> Option<MutexGuard<'_, T>> {
    match mutex.try_lock() {
        Ok(locked_mutex) => Some(locked_mutex),
        Err(TryLockError::Poisoned(poisoned_locked_mutex)) => {
            Some(poisoned_locked_mutex.into_inner())
        }
        Err(TryLockError::WouldBlock) => None,
    }
}

pub fn yield_lock_poisoned<T>(mutex: &Mutex<T>, timeout: Duration) -> Option<MutexGuard<'_, T>> {
    let start = Instant::now();
    loop {
        match try_lock_poisoned(mutex) {
            None if start.elapsed() < timeout => yield_now(),
            res => break res,
        }
    }
}

pub static STATIC_MUTEX: Mutex<()> = Mutex::new(());
macro_rules! mutex_block {
    ($block:block, $timeout:expr) => {
        test_utilities::yield_lock_poisoned(&test_utilities::STATIC_MUTEX, $timeout)
            .map(|_lock| $block)
    };
}
pub(super) use mutex_block;

macro_rules! mutex_block_timeout_10s {
    ($block:block) => {
        mutex_block!($block, Duration::from_secs(10))
    };
}

#[test]
fn test_mutex_tests() {
    static BLOCK_TEST: AtomicBool = AtomicBool::new(false);
    let t1 = thread::spawn(|| {
        mutex_block_timeout_10s!({
            BLOCK_TEST.store(true, Ordering::Relaxed);
            while BLOCK_TEST.load(Ordering::Relaxed) {
                yield_now();
            }
        })
        .expect("acquired mutual exclusion");
    });

    thread::spawn(|| {
        loop {
            if BLOCK_TEST.load(Ordering::Acquire) {
                break;
            }
            yield_now();
        }

        assert!(
            mutex_block!({}, Duration::from_nanos(1)).is_none(),
            "could not acquired mutual exclusion"
        );

        BLOCK_TEST.store(false, Ordering::Release);
    })
    .join()
    .expect("thread didn't panicked");

    t1.join().expect("thread didn't panicked");

    mutex_block_timeout_10s!({}).expect("acquired mutual exclusion");
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
    mutex_block_timeout_10s!({
        let mut locked_cwd_guard =
            yield_lock_poisoned(Cwd::mutex(), Duration::from_millis(1)).unwrap();
        assert_ne!(locked_cwd_guard.get().unwrap(), temp_dir());

        {
            let mut locked_cwd = reset_cwd(&mut locked_cwd_guard);
            assert_ne!(locked_cwd.get().unwrap(), temp_dir());
            locked_cwd.set(temp_dir()).unwrap();
            assert_eq!(locked_cwd.get().unwrap(), temp_dir());
        };

        assert_ne!(locked_cwd_guard.get().unwrap(), temp_dir());
        drop(locked_cwd_guard);
    })
    .expect("acquired mutual exclusion");
}

#[test]
#[expect(clippy::panic, reason = "testing panic behaviour")]
fn test_reset_cwd_panic() {
    mutex_block_timeout_10s!({
        let mut locked_cwd_guard =
            yield_lock_poisoned(Cwd::mutex(), Duration::from_millis(1)).unwrap();

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
        drop(locked_cwd_guard);
    })
    .expect("acquired mutual exclusion");
}
