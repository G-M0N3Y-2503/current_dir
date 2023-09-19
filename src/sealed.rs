pub trait Sealed {}
impl Sealed for super::CurrentWorkingDirectory {}
impl Sealed for super::scoped::CurrentWorkingDirectory<'_> {}
