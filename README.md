# current_dir
A utility crate that helps using [`set_current_dir()`][set_current_dir] and [`current_dir()`][current_dir] in a thread safe manner.<br>
This is generally useful for `#[test]`s that depend on different current working directories each as they are run in multiple threads by default.

### Why can't I just use [`set_current_dir()`][set_current_dir] and [`current_dir()`][current_dir] directly?
The current working directory is global to the whole process, so if you only use a single thread or you never change the current working directory, go ahead!<br>
Otherwise, changing the current working directory without synchronising may lead to unexpected behaviour.

## [`Cwd`][Cwd] Example
```rust
# fn main() -> Result<(), Box<dyn std::error::Error>> {
      use current_dir::*;

      let mut locked_cwd = Cwd::mutex().lock()?;
      locked_cwd.set(std::env::temp_dir())?;
      // cwd == /tmp
#     assert_eq!(locked_cwd.get()?, std::env::temp_dir());
#
#     Ok(())
# }
```
or you can just use [`set_current_dir()`][set_current_dir] and [`current_dir()`][current_dir] with a locked current working directory.
```rust
# fn main() -> Result<(), Box<dyn std::error::Error>> {
      use std::env;
      use current_dir::*;

      let locked_cwd = Cwd::mutex().lock()?;
      env::set_current_dir(env::temp_dir())?;
      // cwd == /tmp
#     assert_eq!(locked_cwd.get()?, env::temp_dir());
#
#     Ok(())
# }
```

## [`CwdGuard`][CwdGuard] Example
```rust
# fn main() -> Result<(), Box<dyn std::error::Error>> {
      use std::env::temp_dir;
      use current_dir::*;
#
#     let test_dirs = temp_dir().join("sub/sub");
#     if !test_dirs.exists() {
#         std::fs::create_dir_all(&test_dirs)?;
#     }

      let mut locked_cwd = Cwd::mutex().lock()?;
      locked_cwd.set(temp_dir())?;
      // cwd == /tmp
#     assert_eq!(locked_cwd.get()?, temp_dir());
      {
          let mut cwd_guard = CwdGuard::try_from(&mut *locked_cwd)?;
          cwd_guard.set("sub")?;
          // cwd == /tmp/sub
#         assert_eq!(cwd_guard.get()?, temp_dir().join("sub"));
          {
              let mut sub_cwd_guard = CwdGuard::try_from(&mut cwd_guard)?;
              sub_cwd_guard.set("sub")?;
              // cwd == /tmp/sub/sub
#             assert_eq!(sub_cwd_guard.get()?, temp_dir().join("sub/sub"));
              {
                  let mut sub_sub_cwd_guard = CwdGuard::try_from(&mut sub_cwd_guard)?;
                  sub_sub_cwd_guard.set(temp_dir())?;
                  // cwd == /tmp
#                 assert_eq!(sub_sub_cwd_guard.get()?, temp_dir());
              }
              // cwd == /tmp/sub/sub
#             assert_eq!(sub_cwd_guard.get()?, temp_dir().join("sub/sub"));
          }
          // cwd == /tmp/sub
#         assert_eq!(cwd_guard.get()?, temp_dir().join("sub"));
      }
      // cwd == /tmp
#     assert_eq!(locked_cwd.get()?, temp_dir());
#
#     Ok(())
# }
```

## Poison cleanup Example
```rust
#![cfg_attr(all(feature = "unstable", feature = "nightly"), feature(mutex_unpoison))]
# fn main() -> Result<(), Box<dyn std::error::Error>> {
      use current_dir::*;
      use std::{env::temp_dir, error::Error, fs, panic};

      let test_dir = temp_dir().join("cwd");
#     if !test_dir.exists() {
#         fs::create_dir(&test_dir)?;
#     }

      panic::catch_unwind(|| {
          let mut locked_cwd = Cwd::mutex().lock().unwrap();
          locked_cwd.set(&test_dir)?;

          // removing the CWD before the CwdGuard is dropped will cause a panic on drop.
          let _cwd_guard = CwdGuard::try_from(&mut *locked_cwd)?;
          fs::remove_dir(&test_dir)?;
#
#         Ok::<_, Box<dyn Error>>(())
      }).expect_err("panicked");

      let mut poisoned_locked_cwd = Cwd::mutex().lock().expect_err("cwd poisoned");
      let expected_cwd = poisoned_locked_cwd.get_ref().get_expected().unwrap().to_owned();
#     assert_eq!(expected_cwd, test_dir.clone());

      // Fix poisoned cwd
      fs::create_dir(&expected_cwd)?;
      poisoned_locked_cwd.get_mut().set(&expected_cwd)?;
      #[cfg(all(feature = "unstable", feature = "nightly"))]
      Cwd::mutex().clear_poison();
      let _locked_cwd = poisoned_locked_cwd.into_inner();

#     Ok(())
# }
```

[Cwd]: https://docs.rs/current_dir/latest/current_dir/struct.Cwd.html
[CwdGuard]: https://docs.rs/current_dir/latest/current_dir/struct.CwdGuard.html
[CwdStack]: https://docs.rs/current_dir/latest/current_dir/struct.CwdStack.html
[set_current_dir]: <https://doc.rust-lang.org/stable/std/env/fn.set_current_dir.html> "std::env::set_current_dir()"
[current_dir]: <https://doc.rust-lang.org/stable/std/env/fn.current_dir.html> "std::env::current_dir()"
