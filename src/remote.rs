//! nip remote implementation
use failure::Error;

use std::{str::FromStr, string::ToString};

use constants::IPFS_HASH_LEN;

#[derive(Clone, Debug, PartialEq)]
/// An enum for describing different nip remote types
pub enum NIPRemote {
    #[allow(missing_docs)]
    ExistingIPFS(String),
    #[allow(missing_docs)]
    ExistingIPNS(String),
    /// A placeholder for a remote that doesn't have an index yet
    NewIPFS,
    /// Same as `NewIPFS` except for IPNS
    NewIPNS,
}

#[derive(Debug, Fail, PartialEq)]
#[allow(missing_docs)]
pub enum NIPRemoteParseError {
    #[fail(display = "Got a hash {} chars long, expected {}", _0, _1)]
    InvalidHashLength(usize, usize),
    #[fail(display = "Invalid link format for string \"{}\"", _0)]
    InvalidLinkFormat(String),
    #[fail(display = "Failed to parse remote type: {}", _0)]
    Other(String),
}

impl NIPRemote {
    #[allow(missing_docs)]
    pub fn is_ipns(&self) -> bool {
        match self {
            NIPRemote::NewIPNS | NIPRemote::ExistingIPNS(_) => true,
            NIPRemote::NewIPFS | NIPRemote::ExistingIPFS(_) => false,
        }
    }

    /// Return the hash if `self` refers to an `Existing*` variant
    ///
    /// # Example
    /// ```rust
    /// # extern crate nip_core;
    /// # use nip_core::NIPRemote;
    ///
    /// let mut remote: NIPRemote = "new-ipns".parse().unwrap();
    /// assert_eq!(remote.get_hash(), None);
    ///
    /// let remote_string = "/ipfs/QmdT2sVhj8UicZsGY7x687FgdJPrzR9idGyavi5282CPH3".to_owned();
    ///
    /// remote = remote_string.parse().unwrap();
    /// assert_eq!(remote.get_hash(), Some(remote_string));
    /// ```
    pub fn get_hash(&self) -> Option<String> {
        match self {
            NIPRemote::NewIPFS | NIPRemote::NewIPNS => None,
            NIPRemote::ExistingIPFS(_) | NIPRemote::ExistingIPNS(_) => Some(self.to_string()),
        }
    }
}

impl FromStr for NIPRemote {
    type Err = Error;
    fn from_str(s: &str) -> Result<NIPRemote, Error> {
        match s {
            "new-ipfs" => Ok(NIPRemote::NewIPFS),
            "new-ipns" => Ok(NIPRemote::NewIPNS),
            existing_ipfs if existing_ipfs.starts_with("/ipfs/") => {
                let hash = existing_ipfs
                    .split('/')
                    .nth(2)
                    .ok_or_else(|| NIPRemoteParseError::Other("Invalid hash format".to_owned()))?;
                if hash.len() != IPFS_HASH_LEN {
                    return Err(
                        NIPRemoteParseError::InvalidHashLength(hash.len(), IPFS_HASH_LEN).into(),
                    );
                }
                Ok(NIPRemote::ExistingIPFS(hash.to_owned()))
            }
            existing_ipns if existing_ipns.starts_with("/ipns/") => {
                let hash = existing_ipns.split('/').nth(2).ok_or_else(|| {
                    NIPRemoteParseError::InvalidLinkFormat(existing_ipns.to_owned())
                })?;
                if hash.len() != IPFS_HASH_LEN {
                    return Err(
                        NIPRemoteParseError::InvalidHashLength(hash.len(), IPFS_HASH_LEN).into(),
                    );
                }
                Ok(NIPRemote::ExistingIPNS(hash.to_owned()))
            }
            other => Err(NIPRemoteParseError::InvalidLinkFormat(other.to_owned()).into()),
        }
    }
}

impl ToString for NIPRemote {
    fn to_string(&self) -> String {
        match self {
            NIPRemote::ExistingIPFS(ref hash) => format!("/ipfs/{}", hash),
            NIPRemote::ExistingIPNS(ref hash) => format!("/ipns/{}", hash),
            NIPRemote::NewIPFS => "new-ipfs".to_owned(),
            NIPRemote::NewIPNS => "new-ipns".to_owned(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parses_new_ipfs() {
        assert_eq!("new-ipfs".parse::<NIPRemote>().unwrap(), NIPRemote::NewIPFS);
    }

    #[test]
    fn test_parses_new_ipns() {
        assert_eq!("new-ipns".parse::<NIPRemote>().unwrap(), NIPRemote::NewIPNS);
    }

    #[test]
    fn test_invalid_link_err() {
        match "gibberish".parse::<NIPRemote>() {
            Err(e) => assert_eq!(
                e.downcast::<NIPRemoteParseError>().unwrap(),
                NIPRemoteParseError::InvalidLinkFormat("gibberish".to_owned())
            ),
            Ok(_) => panic!("Got an Ok, InvalidLinkFormat expected"),
        }
    }

    #[test]
    fn test_invalid_hash_len_err() {
        let bs_hash = "/ipfs/QmTooShort";
        match bs_hash.clone().parse::<NIPRemote>() {
            Err(e) => assert_eq!(
                e.downcast::<NIPRemoteParseError>().unwrap(),
                NIPRemoteParseError::InvalidHashLength(
                    bs_hash.len() - 6, // invalid hash len applies to the Qm* part only
                    IPFS_HASH_LEN
                )
            ),
            Ok(_) => panic!("Got an Ok, InvalidHashLength expected"),
        }
    }
}
