use core::time::Duration;
use std::{
    env::temp_dir,
    panic::resume_unwind,
    sync::{Mutex, MutexGuard},
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

/// Returns the locked and un-poisoned [`Cwd`] or, [yields](yield_now) for up to `yield_timeout`
/// until the poisoned [`Cwd`] has been addressed and can be locked.
/// Otherwise, [`None`]
pub fn yield_poison_addressed(
    mutex: &Mutex<Cwd>,
    yield_timeout: Duration,
) -> Option<MutexGuard<'_, Cwd>> {
    let now = Instant::now();
    loop {
        match mutex.lock() {
            Ok(locked_cwd) => break Some(locked_cwd),
            Err(poisoned_locked_cwd) => {
                let mut locked_cwd = poisoned_locked_cwd.into_inner();
                if CwdStack::from(&mut *locked_cwd).as_vec().is_empty() {
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
fn test_cwd_test() {
    let mut locked_cwd_guard =
        yield_poison_addressed(Cwd::mutex(), Duration::from_millis(500)).unwrap();

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
fn test_cwd_test_panic() {
    let mut locked_cwd_guard =
        yield_poison_addressed(Cwd::mutex(), Duration::from_millis(500)).unwrap();

    assert_ne!(locked_cwd_guard.get().unwrap(), temp_dir());
    let mut locked_cwd = reset_cwd(&mut locked_cwd_guard);
    assert_ne!(locked_cwd.get().unwrap(), temp_dir());

    let test_cwd_panic = thread::scope(|scope| {
        scope
            .spawn(move || {
                locked_cwd.set(temp_dir()).unwrap();
                assert_eq!(locked_cwd.get().unwrap(), temp_dir());
                panic!("test panic")
            })
            .join()
    })
    .expect_err("Test panicked");

    assert_ne!(locked_cwd_guard.get().unwrap(), temp_dir());
    resume_unwind(test_cwd_panic);
}
