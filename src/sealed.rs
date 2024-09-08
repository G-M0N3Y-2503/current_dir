//! Private module for the [`Sealed`] trait.

use super::{Cwd, CwdGuard};

/// Trait to protect against downstream implementations.
#[expect(dead_code, reason = "Designed to prevent use")]
pub trait Sealed {}
impl Sealed for Cwd {}
impl Sealed for CwdGuard<'_> {}
