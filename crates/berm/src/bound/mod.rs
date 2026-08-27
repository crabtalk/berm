//! What bounds one invocation.
//!
//! Both members are the same shape: a guard taken in [`crate::Harness::call`]
//! before the guest is entered and dropped on the way out — [`depth`] bounding
//! how far a chain of harnesses may nest, [`watchdog`] how long one may run.
//! Neither is declinable by a guest, which is what separates them from a
//! [`crate::System`]: a system harness is something a guest reaches for, and
//! these are conditions it runs under.

pub(crate) mod depth;
pub(crate) mod watchdog;
