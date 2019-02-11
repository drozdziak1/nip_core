//! Constants used by `nip_core`.

#[allow(missing_docs)]
pub const IPFS_HASH_LEN: usize = 46;

// Protocol header components, loosely placed just before serialized nip data structures to allow
// for backwards compat at all times (65k-entry, 2-byte version space, constant 8-byte width,
// independence from serde)

/// The first 6 bytes for every header that distinguish a serialized NIP data structure from random
/// bytes
pub static NIP_MAGIC: &[u8] = b"NIPNIP";

/// Current protocol version; must be bumped for every breaking format change
pub const NIP_PROTOCOL_VERSION: u16 = 2; // Bump on breaking data structure changes

#[allow(missing_docs)]
pub const NIP_HEADER_LEN: usize = 8;

/// A magic value used to signal that a hash is a submodule tip (to be obtained by git on its own).
/// Locally git knows a commit is a submodule tip because it's the only case when a tree entry is a
/// commit. However, this relationship is impossible to express in a NIP index implicitly.
pub static SUBMODULE_TIP_MARKER: &str = "submodule-tip";
