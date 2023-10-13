pub trait Sealed {}
impl Sealed for super::CurrentWorkingDirectory {}
impl Sealed for super::scoped::CurrentWorkingDirectory<'_> {}
impl Sealed for super::scoped::stack::CurrentWorkingDirectoryStack<'_> {}
