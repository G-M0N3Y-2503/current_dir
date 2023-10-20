//! Shorter names for this crates types
#![allow(clippy::pub_use)]

#[doc(inline)]
pub use super::scoped::stack::Stack as ScopeStack;
#[doc(inline)]
pub use super::scoped::CurrentWorkingDirectory as ScopedCwd;
#[doc(inline)]
pub use super::CurrentWorkingDirectory as Cwd;
#[doc(inline)]
pub use super::CurrentWorkingDirectoryAccessor as CwdAccessor;
