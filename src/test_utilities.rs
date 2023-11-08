use core::time::Duration;
use std::{
    env::temp_dir,
    panic::{self, resume_unwind},
    sync::{Mutex, MutexGuard},
    thread::yield_now,
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

/// Spawns the function in a new thread and expects that function to panic.
#[macro_export]
macro_rules! expect_panic {
    ($f:expr) => {
        std::thread::scope(|scope| {
            std::thread::Builder::new()
                .name(format!("expect_panic in {}", called_from!()))
                .spawn_scoped(scope, $f)
                .expect("failed to spawn thread")
                .join()
        })
        .expect_err("function panicked")
    };
}

#[test]
fn test_expect_panic() {
    assert_eq!(
        expect_panic!(|| panic!("str panic")).downcast_ref(),
        Some(&"str panic")
    );

    assert_eq!(
        expect_panic!(|| panic::panic_any(58i32)).downcast_ref(),
        Some(&58i32)
    );

    let mv = String::from("panic String");
    assert_eq!(
        expect_panic!(|| panic::panic_any(mv)).downcast_ref(),
        Some(&String::from("panic String"))
    );

    let by_ref = "panic &str";
    assert_eq!(
        expect_panic!(|| panic::panic_any(by_ref)).downcast_ref(),
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

/// Global timeout accounting for the duration the [`Cwd`] may be locked for and the duration to clean up poisoned [`Cwd`]s.
static YIELD_POISON_ADDRESSED_TIMEOUT: Duration = Duration::from_millis(500);

/// Returns the locked and un-poisoned [`Cwd`] or, [yields](yield_now) for up to [`YIELD_POISON_ADDRESSED_TIMEOUT`]
/// until the poisoned [`Cwd`] has been addressed and can be locked.
/// Otherwise, errors with a timeout.
pub fn yield_poison_addressed(mutex: &Mutex<Cwd>) -> Result<MutexGuard<'_, Cwd>, String> {
    let now = Instant::now();
    loop {
        match mutex.lock() {
            Ok(locked_cwd) => break Ok(locked_cwd),
            Err(poisoned_locked_cwd) => {
                let mut locked_cwd = poisoned_locked_cwd.into_inner();
                if CwdStack::from(&mut *locked_cwd).as_vec().is_empty() {
                    break Ok(locked_cwd);
                } else if now.elapsed() > YIELD_POISON_ADDRESSED_TIMEOUT {
                    break Err(format!(
                        "acquiring addressed lock timed out after {}s",
                        YIELD_POISON_ADDRESSED_TIMEOUT.as_secs_f64()
                    ));
                } else {
                    yield_now();
                }
            }
        }
    }
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
    let mut locked_cwd_guard = yield_poison_addressed(Cwd::mutex()).unwrap();

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
#[should_panic(expected = "test panic")]
fn test_reset_cwd_panic() {
    let mut locked_cwd_guard = yield_poison_addressed(Cwd::mutex()).unwrap();

    assert_ne!(locked_cwd_guard.get().unwrap(), temp_dir());
    let mut locked_cwd = reset_cwd(&mut locked_cwd_guard);
    assert_ne!(locked_cwd.get().unwrap(), temp_dir());

    let test_cwd_panic = expect_panic!(move || {
        locked_cwd.set(temp_dir()).unwrap();
        assert_eq!(locked_cwd.get().unwrap(), temp_dir());
        panic!("test panic")
    });

    assert_ne!(locked_cwd_guard.get().unwrap(), temp_dir());
    resume_unwind(test_cwd_panic);
}
