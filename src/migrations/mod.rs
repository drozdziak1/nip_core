//! A collection of migration helpers for keeping older nip repos relevant; controlled by the
//! `migrations` feature.`

mod object_v1;

use failure::{Error, Fail};
use ipfs_api::IpfsClient;

use crate::{
    constants::{NIP_PROTOCOL_VERSION, SUBMODULE_TIP_MARKER},
    index::NIPIndex,
    object::NIPObject,
    util::ipfs_cat,
};

use object_v1::NIPObjectV1;

// Once we go beyond NIP_PROTOCOL_VERSION 2 import the real V1V2 here.
type NIPIndexV1V2 = NIPIndex;

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
/// recursively updated present-day equivalent.
pub fn migrate_index(data: &[u8], version: u16, ipfs: &mut IpfsClient) -> Result<NIPIndex, Error> {
    match version {
        0 => Err(MigrationError::ZeroVersion.into()),
        1 => {
            debug!("Migrating index: version 1 -> 2 ");
            // Index structure stayed the same between v1 and v2
            let mut idx: NIPIndexV1V2 = serde_cbor::from_slice(data)?;
            for (git_hash, ipfs_hash) in idx.objects.iter_mut() {
                // V2 has string-based submodule tip markers though
                if ipfs_hash == SUBMODULE_TIP_MARKER {
                    trace!("Skipping submodule tip {}", git_hash);
                    continue;
                }
                let new_hash = NIPObjectV1::from_slice(&ipfs_cat(ipfs_hash, ipfs)?[..])?
                    .to_v2(git_hash)
                    .ipfs_add(ipfs)?;

                trace!("Object {}: {} -> {}", git_hash, ipfs_hash, new_hash);

                *ipfs_hash = new_hash;
            }
            Ok(idx)
        }
        NIP_PROTOCOL_VERSION => {
            debug!("Trivial migration of current version {}, deserializing", NIP_PROTOCOL_VERSION);
            Ok(serde_cbor::from_slice(data)?)},
        other if other > NIP_PROTOCOL_VERSION => Err(MigrationError::TooNew(other).into()),
        _ => unreachable!(),
    }
}

/// Take an older headerless object under `data` from nip version `version` and return a
/// present-day equivalent.
pub fn migrate_object(
    data: &[u8],
    git_hash: &str,
    version: u16,
) -> Result<NIPObject, Error> {
    match version {
        0 => Err(MigrationError::ZeroVersion.into()),
        1 => Ok(serde_cbor::from_slice::<NIPObjectV1>(data)?.to_v2(git_hash)),
        NIP_PROTOCOL_VERSION => Ok(serde_cbor::from_slice(data)?),
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
        let mut ipfs = IpfsClient::default();
        let idx = NIPIndex::from_nip_remote(&"new-ipfs".parse().unwrap(), &mut ipfs).unwrap();

        let payload = serde_cbor::to_vec(&idx).unwrap();

        if let Err(e) = migrate_index(payload.as_slice(), 0, &mut ipfs) {
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
        let mut ipfs = IpfsClient::default();
        let idx = NIPIndex::from_nip_remote(&"new-ipfs".parse().unwrap(), &mut ipfs).unwrap();

        let payload = serde_cbor::to_vec(&idx).unwrap();

        if let Err(e) = migrate_index(payload.as_slice(), NIP_PROTOCOL_VERSION + 1, &mut ipfs) {
            assert_eq!(
                e.downcast::<MigrationError>().unwrap(),
                MigrationError::TooNew(NIP_PROTOCOL_VERSION + 1)
            );
        } else {
            panic!("Did not get an error at all");
        }
    }
}
