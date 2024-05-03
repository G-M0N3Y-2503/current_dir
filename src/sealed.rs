//! Private module for the [`Sealed`] trait.

use super::{Cwd, CwdGuard};

/// Trait to protect against downstream implementations.
#[allow(dead_code)]
pub trait Sealed {}
impl Sealed for Cwd {}
impl Sealed for CwdGuard<'_> {}
