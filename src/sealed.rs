use super::prelude::*;

pub trait Sealed {}
impl Sealed for Cwd {}
impl Sealed for ScopedCwd<'_> {}
impl Sealed for CwdStack<'_> {}
