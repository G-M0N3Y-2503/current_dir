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

## [`ScopedCwd`][ScopedCwd] Example
```rust
# fn mkdir<P: AsRef<std::path::Path>>(path: P) -> std::io::Result<()> {
#     let path = path.as_ref();
#     if !path.exists() {
#         std::fs::create_dir(path)
#     } else {
#         Ok(())
#     }
# }
#
# fn main() -> Result<(), Box<dyn std::error::Error>> {
      use std::env::temp_dir;
      use current_dir::*;

      let mut locked_cwd = Cwd::mutex().lock()?;
      locked_cwd.set(temp_dir())?;
      // cwd == /tmp
#     assert_eq!(locked_cwd.get()?, temp_dir());
      {
          let mut scope_locked_cwd = ScopedCwd::try_from(&mut *locked_cwd)?;
#         mkdir("sub")?;
          scope_locked_cwd.set("sub")?;
          // cwd == /tmp/sub
#         assert_eq!(scope_locked_cwd.get()?, temp_dir().join("sub"));
          {
              let mut sub_scope_locked_cwd = ScopedCwd::try_from(&mut scope_locked_cwd)?;
#             mkdir("sub")?;
              sub_scope_locked_cwd.set("sub")?;
              // cwd == /tmp/sub/sub
#             assert_eq!(sub_scope_locked_cwd.get()?, temp_dir().join("sub/sub"));
              {
                  let mut sub_sub_scope_locked_cwd = ScopedCwd::try_from(&mut sub_scope_locked_cwd)?;
                  sub_sub_scope_locked_cwd.set(temp_dir())?;
                  // cwd == /tmp
#                 assert_eq!(sub_sub_scope_locked_cwd.get()?, temp_dir());
              }
              // cwd == /tmp/sub/sub
#             assert_eq!(sub_scope_locked_cwd.get()?, temp_dir().join("sub/sub"));
          }
          // cwd == /tmp/sub
#         assert_eq!(scope_locked_cwd.get()?, temp_dir().join("sub"));
      }
      // cwd == /tmp
#     assert_eq!(locked_cwd.get()?, temp_dir());
#
#     Ok(())
# }
```

[Cwd]: https://docs.rs/current_dir/latest/current_dir/struct.CurrentWorkingDirectory.html
[ScopedCwd]: https://docs.rs/current_dir/latest/current_dir/scoped/struct.CurrentWorkingDirectory.html
[set_current_dir]: <https://doc.rust-lang.org/stable/std/env/fn.set_current_dir.html> "std::env::set_current_dir()"
[current_dir]: <https://doc.rust-lang.org/stable/std/env/fn.current_dir.html> "std::env::current_dir()"
