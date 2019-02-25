//! Error types

#[derive(Debug, Fail)]
/// General nip-specific errors
pub enum NIPError {
    /// We attempted to use an object from a newer version of NIP than this one
    #[fail(display = "NIP version {} does not match current version", _0)]
    InvalidVersion(u16),
    /// Internal error, probably not the user's fault
    #[fail(display = "Internal error: {}", _0)]
    InternalError(String),
}
