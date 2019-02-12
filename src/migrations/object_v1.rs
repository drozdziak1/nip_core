//! v1 nip object implementation
use failure::Error;
use futures::{Future, Stream};
use ipfs_api::IpfsClient;
use tokio::runtime::Runtime;

use std::collections::BTreeSet;

use crate::{
    constants::NIP_HEADER_LEN,
    object::{NIPObject as NIPObjectV2, NIPObjectMetadata as NIPObjectV2Metadata},
    util::parse_nip_header,
};

#[derive(Clone, Debug, Deserialize, Serialize)]
/// A nip representation of a git object
pub struct NIPObjectV1 {
    /// A link to the raw form of the object
    pub raw_data_ipfs_hash: String,
    /// Object-type-specific metadata
    pub metadata: NIPObjectV1Metadata,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
/// A helper type for determining a nip object's relationship with other nip objects.
pub enum NIPObjectV1Metadata {
    #[allow(missing_docs)]
    Commit {
        parent_git_hashes: BTreeSet<String>,
        tree_git_hash: String,
    },
    #[allow(missing_docs)]
    Tag { target_git_hash: String },
    #[allow(missing_docs)]
    Tree { entry_git_hashes: BTreeSet<String> },
    #[allow(missing_docs)]
    Blob,
}

impl NIPObjectV1 {
    /// Download from IPFS and instantiate a `NIPObjectV1`.
    pub fn ipfs_get(hash: &str, ipfs: &mut IpfsClient) -> Result<Self, Error> {
        let mut event_loop = Runtime::new()?;

        let object_bytes_req = ipfs.cat(hash).concat2();

        let object_bytes: Vec<u8> = event_loop.block_on(object_bytes_req)?.into_iter().collect();
        event_loop
            .shutdown_on_idle()
            .wait()
            .map_err(|()| format_err!("Could not shutdown the event loop"))?;

        let obj_nip_proto_version = parse_nip_header(&object_bytes)?;

        if obj_nip_proto_version != 1 {
            bail!(
                "Unsupported protocol version {} (NIPObjectV1 needs 1)",
                obj_nip_proto_version,
            );
        }

        Ok(serde_cbor::from_slice(&object_bytes[NIP_HEADER_LEN..])?)
    }

    pub fn to_v2(self, git_hash: &str) -> NIPObjectV2 {
        NIPObjectV2 {
            git_hash: git_hash.to_owned(),
            raw_data_ipfs_hash: self.raw_data_ipfs_hash,
            metadata: self.metadata.to_v2(),
        }
    }
}

impl NIPObjectV1Metadata {
    pub fn to_v2(self) -> NIPObjectV2Metadata {
        match self {
            NIPObjectV1Metadata::Commit {
                parent_git_hashes,
                tree_git_hash,
            } => NIPObjectV2Metadata::Commit {
                parent_git_hashes,
                tree_git_hash,
            },
            NIPObjectV1Metadata::Tag { target_git_hash } => {
                NIPObjectV2Metadata::Tag { target_git_hash }
            }
            NIPObjectV1Metadata::Tree { entry_git_hashes } => {
                NIPObjectV2Metadata::Tree { entry_git_hashes }
            }
            NIPObjectV1Metadata::Blob => NIPObjectV2Metadata::Blob,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_v2_test() {
        let v1 = NIPObjectV1 {
            raw_data_ipfs_hash: "/ipfs/ValueIrrelevant".to_owned(),
            metadata: NIPObjectV1Metadata::Commit {
                parent_git_hashes: vec!["AlsoIrrelevant".to_owned(), "Garbage".to_owned()]
                    .into_iter()
                    .collect(),
                tree_git_hash: "AnotherOne".to_owned(),
            },
        };

        let v2 = v1.clone().to_v2("WhoCares");

        assert_eq!(v2.git_hash.as_str(), "WhoCares");
        assert_eq!(v2.raw_data_ipfs_hash.as_str(), v1.raw_data_ipfs_hash);

        match (v1.metadata, v2.metadata) {
            (NIPObjectV1Metadata::Blob, NIPObjectV2Metadata::Blob) => {}
            (
                NIPObjectV1Metadata::Commit {
                    parent_git_hashes: v1_parents,
                    tree_git_hash: v1_tree,
                },
                NIPObjectV2Metadata::Commit {
                    parent_git_hashes: v2_parents,
                    tree_git_hash: v2_tree,
                },
            ) => {
                assert_eq!(v2_parents, v1_parents);
                assert_eq!(v2_tree, v1_tree);
            }
            (
                NIPObjectV1Metadata::Tag {
                    target_git_hash: v1_target,
                },
                NIPObjectV2Metadata::Tag {
                    target_git_hash: v2_target,
                },
            ) => {
                assert_eq!(v2_target, v1_target);
            }
            (
                NIPObjectV1Metadata::Tree {
                    entry_git_hashes: v1_entries,
                },
                NIPObjectV2Metadata::Tree {
                    entry_git_hashes: v2_entries,
                },
            ) => {
                assert_eq!(v2_entries, v1_entries);
            }
            (v1_differs, v2_differs) => panic!(
                "v1 {:?} and v2 {:?} metadata isn't even the same type!",
                v1_differs, v2_differs
            ),
        }
    }
}
