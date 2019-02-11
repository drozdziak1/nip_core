//! A collection of migration helpers for keeping older nip repos relevant; controlled by the
//! `migrations` feature.`

use failure::{Error, Fail};

use crate::{constants::NIP_PROTOCOL_VERSION, index::NIPIndex};

/// An error which happened during a migration
#[derive(Clone, Debug, Eq, Fail, PartialEq)]
pub enum MigrationError {
    /// The invalid version 0 was passed
    #[fail(display = "Version 0 is invalid")]
    ZeroVersion,
    /// The version comes from a nip protocol version newer than what we're running
    #[fail(display = "Version {} is too new! Please upgrade nip", _0)]
    TooNew(u16),
}

/// Take headerless `data` bytes containing an older index from nip version `version` and return a
/// present-day equivalent
pub fn migrate_index(data: &[u8], version: u16) -> Result<NIPIndex, Error> {
    match version {
        0 => Err(MigrationError::ZeroVersion.into()),
        // Index structure stayed the same between v1 and v2
        1 | NIP_PROTOCOL_VERSION => Ok(serde_cbor::from_slice(data)?),
        other if other > NIP_PROTOCOL_VERSION => Err(MigrationError::TooNew(other).into()),
        _ => unreachable!(),
    }
}

#[cfg(test)]
mod tests {
    use ipfs_api::IpfsClient;

    use super::*;

    #[test]
    fn zero_version_test() {
        let idx =
            NIPIndex::from_nip_remote(&"new-ipfs".parse().unwrap(), &mut IpfsClient::default())
                .unwrap();

        let payload = serde_cbor::to_vec(&idx).unwrap();

        if let Err(e) = migrate_index(payload.as_slice(), 0) {
            assert_eq!(
                e.downcast::<MigrationError>().unwrap(),
                MigrationError::ZeroVersion
            );
        } else {
            panic!("Did not get an error at all");
        }
    }

    #[test]
    fn too_new_test() {
        let idx =
            NIPIndex::from_nip_remote(&"new-ipfs".parse().unwrap(), &mut IpfsClient::default())
                .unwrap();

        let payload = serde_cbor::to_vec(&idx).unwrap();

        if let Err(e) = migrate_index(payload.as_slice(), NIP_PROTOCOL_VERSION + 1) {
            assert_eq!(
                e.downcast::<MigrationError>().unwrap(),
                MigrationError::TooNew(NIP_PROTOCOL_VERSION + 1)
            );
        } else {
            panic!("Did not get an error at all");
        }
    }
}
