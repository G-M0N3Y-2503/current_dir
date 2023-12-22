//! Private module for the [`Sealed`] trait.

use super::*;

/// Trait to protect against downstream implementations.
pub trait Sealed {}
impl Sealed for Cwd {}
impl Sealed for CwdGuard<'_> {}
