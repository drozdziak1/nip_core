//! Constants used by `nip_core`.

#[allow(missing_docs)]
pub static IPFS_HASH_LEN: usize = 46;

// Protocol header components, loosely placed just before serialized nip data structures to allow
// for backwards compat at all times (65k-entry, 2-byte version space, constant 8-byte width,
// independence from serde)

/// The first 6 bytes for every header that distinguish a serialized NIP data structure from random
/// bytes
pub static NIP_MAGIC: &[u8] = b"NIPNIP";

/// Current protocol version; must be bumped for every breaking format change
pub static NIP_PROTOCOL_VERSION: u16 = 1; // Bump on breaking data structure changes

#[allow(missing_docs)]
pub static NIP_HEADER_LEN: usize = 8;
