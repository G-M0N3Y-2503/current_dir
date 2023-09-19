#![cfg(test)]

use std::{sync::MutexGuard, thread::yield_now, time::Duration};

use super::*;

#[macro_export]
macro_rules! called_from {
    () => {
        env!("CARGO_PKG_NAME").to_owned() + concat!(' ', file!(), ':', line!(), ':', column!())
    };
}

/// Locks the mutex and returns the guard if it is not poisoned or the poisoned mutex has been fixed.
/// Otherwise, [`yield_now`] for up to `yield_timeout`.
pub fn yield_poison_fixed(
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
