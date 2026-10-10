//! What bounds one invocation.
//!
//! Two bounds, both carried on the [`crate::Callsite`] a nested call is made
//! from: the depth a chain of programs may nest to, and the [`watchdog`]
//! deadline it has to finish by. Neither is declinable by a guest, which is
//! what separates them from a [`crate::Syscall`]: a syscall is something a
//! guest reaches for, and these are conditions it runs under.

pub(crate) mod watchdog;
