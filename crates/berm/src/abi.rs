//! The wire between a program and its host.
//!
//! A syscall is identified by its *name*; the number carrying it is derived
//! from that name (RFC 0205). Adding a syscall therefore cannot collide with
//! an allocation someone else made, and there is no registry of integers to
//! keep.
//!
//! Every one takes a request at `(ptr, len)` and answers with a single word.
//! One shape for all of them: a call that needs no request is handed a pair it
//! ignores, and one whose answer is longer than a word stages it for the guest
//! to pull. That is what lets a backend register a table rather than a list of
//! signatures — rvtime puts the number in `a7` and the pair in `a0`/`a1`,
//! wasmtime takes all three as arguments to one import.
//!
//! The same hash is computed guest-side in `crates/lang/src/abi.rs`. The two
//! must agree — and cannot drift quietly if they don't: a mismatched name
//! hashes to a number no closure is registered for, so the first call traps
//! as an unknown host call rather than reaching the wrong syscall.

/// Write a UTF-8 message to the host log. `(ptr, len) -> 0`
pub const HOST_LOG: u64 = hash("berm.log");
/// Byte length of this invocation's argument blob. `() -> len`
pub const HOST_ARG_LEN: u64 = hash("berm.args.len");
/// Copy the argument blob into guest memory. `(ptr, cap) -> full length`
pub const HOST_ARG_READ: u64 = hash("berm.args.read");
/// Finish this invocation with a result. `(ptr, len) -> 0`
pub const HOST_DONE: u64 = hash("berm.done");
/// Fail this invocation with a message. `(ptr, len) -> 0`
///
/// The other half of [`HOST_DONE`], and the reason neither is a return value:
/// what a tool answers with travels the same way everything else does, so an
/// export takes nothing and returns nothing on every backend. A pointer pair
/// in registers would have been two words on RV64 and a hidden out-parameter
/// on wasm32, which is one convention too many for one ABI.
pub const HOST_FAIL: u64 = hash("berm.fail");
/// Copy the staged syscall result into guest memory. `(ptr, cap) -> full length`
pub const HOST_RESULT_READ: u64 = hash("berm.result.read");

/// Set on a staged length when the bytes are an error message rather than a
/// result. A length never reaches this bit on its own, so one return value
/// carries the outcome and the size together.
pub const ERROR: u64 = 1 << 63;

/// Set beside [`ERROR`] when the host refused the call outright — nothing ran
/// on the other side of it.
///
/// The same split the host's own API makes everywhere else: `Berm::call`'s
/// outer `Result` against its inner one, `Output::Failed` against a status. A
/// guest that called a program which is not deployed learns something it can
/// act on; one whose target ran and said no learns something else.
pub const REFUSED: u64 = 1 << 62;

/// FNV-1a over the syscall's name, evaluated at compile time.
pub const fn hash(name: &str) -> u64 {
    let bytes = name.as_bytes();
    let mut result: u64 = 0xcbf2_9ce4_8422_2325;
    let mut at = 0;
    while at < bytes.len() {
        result ^= bytes[at] as u64;
        result = result.wrapping_mul(0x0000_0100_0000_01b3);
        at += 1;
    }
    result
}

/// What a host registers to serve [`HOST_CALL`], as [`crate::Syscall::name`]
/// wants it: the name, not the number it hashes to.
pub const CALL: &str = "berm.call";

/// Call a tool on another program. `(ptr, len) -> staged length`
///
/// Request fields are the program, the tool, and the argument blob; the reply
/// is the tool's result, staged like any other.
///
/// berm serves this one itself: resolving a name needs only the set of
/// deployed programs, which is what a [`crate::Berm`] already is.
pub const HOST_CALL: u64 = hash(CALL);

/// What a host registers to serve [`HOST_CALL_AFTER`].
pub const CALL_AFTER: &str = "berm.call.after";

/// Call a tool later. `(ptr, len) -> staged length`
///
/// Request fields are the delay in milliseconds, the program, the tool and the
/// argument blob; the reply is empty. A duration rather than an instant
/// because the runtime stamps the deadline as it takes the call: a guest
/// reading the clock and then arming would drift by however long it ran in
/// between, and only the host can close that gap.
///
/// One wake per program that arms it. Arming again replaces what was pending,
/// which is what bounds a program to one and keeps it from fanning out.
pub const HOST_CALL_AFTER: u64 = hash(CALL_AFTER);

/// What a host registers to serve [`HOST_GET`] and [`HOST_SET`].
pub const GET: &str = "berm.get";
pub const SET: &str = "berm.set";

/// Read one of this program's own keys. `(ptr, len) -> staged length`
///
/// The request is the key; the reply is one field when the key is set and no
/// fields when it is not, so an empty value and an absent one are told apart.
///
/// Neither this nor [`HOST_SET`] carries a program: the keyspace is whichever
/// program is asking, which the host reads off the [`crate::Callsite`]. Another
/// program's keys are not refused, they are unaddressable.
pub const HOST_GET: u64 = hash(GET);

/// Write one. `(ptr, len) -> staged length`
///
/// Request fields are the key and the value; the reply is empty.
pub const HOST_SET: u64 = hash(SET);

/// What a host registers to serve the socket doors. berm serves none of them:
/// a dialer needs an allowlist and a frame cap, which are a host's decisions.
pub const WS_OPEN: &str = "berm.ws.open";
pub const WS_SEND: &str = "berm.ws.send";
pub const WS_CLOSE: &str = "berm.ws.close";

/// Open a connection, naming the tool its events reach. `(ptr, len) -> staged length`
///
/// Request fields are the URL, the program and the tool, then header names and
/// values in turn; the reply is the id the other two doors take. The dial
/// outlives this call, so a connection that never comes up says so through a
/// [`WS_EVENT_OPEN`] event carrying the error.
pub const HOST_WS_OPEN: u64 = hash(WS_OPEN);

/// Queue bytes on a connection. `(ptr, len) -> staged length`
///
/// Request fields are the id and the payload; the reply is empty. Queued: a
/// guest that waited for the far end would hold its thread for a round trip.
pub const HOST_WS_SEND: u64 = hash(WS_SEND);

/// Close a connection. `(ptr, len) -> staged length`
///
/// The request is the id; the reply is empty.
pub const HOST_WS_CLOSE: u64 = hash(WS_CLOSE);

/// The first field of every invocation a connection starts, saying which of
/// the three things happened. The body follows as the second field.
pub const WS_EVENT_OPEN: &str = "open";
pub const WS_EVENT_MESSAGE: &str = "message";
pub const WS_EVENT_CLOSE: &str = "close";

/// Milliseconds since the Unix epoch, as the host reads its clock. `() -> millis`
///
/// berm serves this one itself: a clock needs no root, no cap and no
/// allowlist, so a host has nothing to decide about it. A program an event
/// woke has no other way to tell how long it was away — a wake that came due
/// while the process was down arrives late, and says so only against this.
pub const HOST_NOW: u64 = hash("berm.now");

/// Where this guest's heap starts. `() -> address`
pub const HOST_HEAP_START: u64 = hash("berm.heap.start");
/// How many bytes of it there are. `() -> length`
pub const HOST_HEAP_SIZE: u64 = hash("berm.heap.size");
/// Prefix on every tool's exported symbol. A tool is resolved by name like any
/// other symbol; the prefix keeps one called `init` from colliding with the
/// exports the ABI reserves.
///
/// A tool takes nothing and returns nothing — see [`HOST_FAIL`].
pub const TOOL_PREFIX: &str = "berm_tool_";
