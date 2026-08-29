//! What bounds one invocation.
//!
//! Both members are the same shape: a guard taken in [`crate::Program::call`]
//! before the guest is entered and dropped on the way out — [`depth`] bounding
//! how far a chain of programs may nest, [`watchdog`] how long one may run.
//! Neither is declinable by a guest, which is what separates them from a
//! [`crate::Syscall`]: a syscall is something a guest reaches for, and
//! these are conditions it runs under.

pub(crate) mod depth;
pub(crate) mod watchdog;
