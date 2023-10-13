use std::{
    sync::{Mutex, MutexGuard},
    thread::yield_now,
    time::Duration,
};
use with_drop::*;

/// using [super] so we can include!() this in ../tests/intergration.rs)
use super::*;

#[macro_export]
macro_rules! called_from {
    () => {
        env!("CARGO_PKG_NAME").to_owned() + concat!(' ', file!(), ':', line!(), ':', column!())
    };
}

/// Returns the locked and un-poisoned [CurrentWorkingDirectory] or, [yields](yield_now) for up to `yield_timeout`
/// until the poisoned [CurrentWorkingDirectory] has been addressed and can be locked.
/// Otherwise, [`None`]
pub fn yield_poison_addressed(
    mutex: &Mutex<CurrentWorkingDirectory>,
    yield_timeout: Duration,
) -> Option<MutexGuard<'_, CurrentWorkingDirectory>> {
    let now = std::time::Instant::now();
    loop {
        match mutex.lock() {
            Ok(locked_cwd) => break Some(locked_cwd),
            Err(poisoned_locked_cwd) => {
                let mut locked_cwd = poisoned_locked_cwd.into_inner();
                if locked_cwd.scope_stack().as_vec().is_empty() {
                    break Some(locked_cwd);
                } else if now.elapsed() <= yield_timeout {
                    yield_now();
                    continue;
                } else {
                    break None;
                }
            }
        }
    }
}

/// Returns the locked_cwd that will reset to the current working directory when dropped.
/// # Panics
/// The returned closure panics if the current working directory cannot be set to the current working
/// directory cached at the time of the call to [reset_cwd()].
pub fn reset_cwd(
    locked_cwd: &mut CurrentWorkingDirectory,
) -> WithDrop<&mut CurrentWorkingDirectory, impl FnOnce(&mut CurrentWorkingDirectory)> {
    let initial_cwd = locked_cwd.get().unwrap();
    let reset_cwd_fn = move |locked_cwd: &mut CurrentWorkingDirectory| {
        locked_cwd
            .set(&initial_cwd)
            .expect("initial CWD should still be valid")
    };
    with_drop(locked_cwd, reset_cwd_fn)
}

#[test]
fn test_cwd_test() {
    let mut locked_cwd_guard =
        yield_poison_addressed(CurrentWorkingDirectory::mutex(), Duration::from_millis(500))
            .unwrap();

    assert_ne!(locked_cwd_guard.get().unwrap(), std::env::temp_dir());
    let mut locked_cwd = reset_cwd(&mut locked_cwd_guard);
    assert_ne!(locked_cwd.get().unwrap(), std::env::temp_dir());

    (move || {
        locked_cwd.set(std::env::temp_dir()).unwrap();
        assert_eq!(locked_cwd.get().unwrap(), std::env::temp_dir());
    })();

    assert_ne!(locked_cwd_guard.get().unwrap(), std::env::temp_dir());
}

#[test]
#[should_panic(expected = "test panic")]
fn test_cwd_test_panic() {
    let mut locked_cwd_guard =
        yield_poison_addressed(CurrentWorkingDirectory::mutex(), Duration::from_millis(500))
            .unwrap();

    assert_ne!(locked_cwd_guard.get().unwrap(), std::env::temp_dir());
    let mut locked_cwd = reset_cwd(&mut locked_cwd_guard);
    assert_ne!(locked_cwd.get().unwrap(), std::env::temp_dir());

    let test_cwd_panic = std::thread::scope(|s| {
        s.spawn(move || {
            locked_cwd.set(std::env::temp_dir()).unwrap();
            assert_eq!(locked_cwd.get().unwrap(), std::env::temp_dir());
            panic!("test panic")
        })
        .join()
    })
    .expect_err("Test panicked");

    assert_ne!(locked_cwd_guard.get().unwrap(), std::env::temp_dir());
    std::panic::resume_unwind(test_cwd_panic);
}
