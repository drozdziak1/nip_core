//! nip object implementation
use failure::Error;
use futures::Stream;
use git2::{Blob, Commit, ObjectType, Odb, OdbObject, Oid, Tag, Tree};
use ipfs_api::IpfsClient;
use tokio::runtime::current_thread;

use std::{collections::BTreeSet, io::Cursor};

use crate::{
    constants::{NIP_HEADER_LEN, NIP_PROTOCOL_VERSION},
    util::{gen_nip_header, parse_nip_header},
};

#[derive(Clone, Debug, Deserialize, Serialize)]
/// A nip representation of a git object
pub struct NIPObject {
    /// The git hash of the underlying git object
    pub git_hash: String,
    /// A link to the raw form of the object
    pub raw_data_ipfs_hash: String,
    /// Object-type-specific metadata
    pub metadata: NIPObjectMetadata,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
/// A helper type for determining a nip object's relationship with other nip objects.
pub enum NIPObjectMetadata {
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

impl NIPObject {
    /// Instantiate a `NIPObject` from a blob object.
    pub fn from_git_blob(blob: &Blob, odb: &Odb, ipfs: &mut IpfsClient) -> Result<Self, Error> {
        let odb_obj = odb.read(blob.id())?;
        let raw_data_ipfs_hash = Self::upload_odb_obj(&odb_obj, ipfs)?;

        Ok(Self {
            git_hash: blob.id().to_string(),
            raw_data_ipfs_hash,
            metadata: NIPObjectMetadata::Blob,
        })
    }

    /// Instantiate a `NIPObject` from a commit object.
    pub fn from_git_commit(
        commit: &Commit,
        odb: &Odb,
        ipfs: &mut IpfsClient,
    ) -> Result<Self, Error> {
        let odb_obj = odb.read(commit.id())?;
        let raw_data_ipfs_hash = Self::upload_odb_obj(&odb_obj, ipfs)?;
        let parent_git_hashes: BTreeSet<String> = commit
            .parent_ids()
            .map(|parent_id| format!("{}", parent_id))
            .collect();

        let tree_git_hash = format!("{}", commit.tree()?.id());

        Ok(Self {
            git_hash: commit.id().to_string(),
            raw_data_ipfs_hash,
            metadata: NIPObjectMetadata::Commit {
                parent_git_hashes,
                tree_git_hash,
            },
        })
    }

    /// Instantiate a `NIPObject` from an annotated/signed tag object.
    pub fn from_git_tag(tag: &Tag, odb: &Odb, ipfs: &mut IpfsClient) -> Result<Self, Error> {
        let odb_obj = odb.read(tag.id())?;
        let raw_data_ipfs_hash = Self::upload_odb_obj(&odb_obj, ipfs)?;

        Ok(Self {
            git_hash: tag.id().to_string(),
            raw_data_ipfs_hash,
            metadata: NIPObjectMetadata::Tag {
                target_git_hash: format!("{}", tag.target_id()),
            },
        })
    }

    /// Instantiate a `NIPObject` from a tree object.
    pub fn from_git_tree(tree: &Tree, odb: &Odb, ipfs: &mut IpfsClient) -> Result<Self, Error> {
        let odb_obj = odb.read(tree.id())?;
        let raw_data_ipfs_hash = Self::upload_odb_obj(&odb_obj, ipfs)?;

        let entry_git_hashes: BTreeSet<String> =
            tree.iter().map(|entry| format!("{}", entry.id())).collect();

        Ok(Self {
            git_hash: tree.id().to_string(),
            raw_data_ipfs_hash,
            metadata: NIPObjectMetadata::Tree { entry_git_hashes },
        })
    }

    /// Deserialize raw NIPObject bytes
    pub fn from_slice(bytes: &[u8]) -> Result<Self, Error> {
        let obj_nip_proto_version = parse_nip_header(&bytes)?;

        if obj_nip_proto_version != NIP_PROTOCOL_VERSION {
            bail!(
                "Unsupported protocol version {} (We're at {})",
                obj_nip_proto_version,
                NIP_PROTOCOL_VERSION
            );
        }

        Ok(serde_cbor::from_slice(&bytes[NIP_HEADER_LEN..])?)
    }

    /// Download from IPFS and instantiate a `NIPObject`.
    pub fn ipfs_get(hash: &str, ipfs: &mut IpfsClient) -> Result<Self, Error> {
        let object_bytes_req = ipfs.cat(hash).concat2();

        let object_bytes: Vec<u8> = current_thread::block_on_all(object_bytes_req)?
            .into_iter()
            .collect();

        Ok(Self::from_slice(&object_bytes[..])?)
    }

    /// Put `self` on IPFS and return the link.
    pub fn ipfs_add(&self, ipfs: &mut IpfsClient) -> Result<String, Error> {
        let mut self_buf = gen_nip_header(None)?;

        self_buf.extend_from_slice(&serde_cbor::to_vec(self)?);

        let req = ipfs.add(Cursor::new(self_buf));
        let ipfs_hash = format!("/ipfs/{}", current_thread::block_on_all(req)?.hash);

        Ok(ipfs_hash)
    }

    /// Upload `odb_obj` to IPFS and return the link.
    fn upload_odb_obj(odb_obj: &OdbObject, ipfs: &mut IpfsClient) -> Result<String, Error> {
        let obj_buf = odb_obj.data().to_vec();

        let raw_data_req = ipfs.add(Cursor::new(obj_buf));
        let ipfs_hash = current_thread::block_on_all(raw_data_req)?.hash;
        Ok(format!("/ipfs/{}", ipfs_hash))
    }

    /// Download `self.raw_data_ipfs_hash` from IPFS and use it to instantiate `self` in `odb`.
    pub fn write_raw_data(&self, odb: &mut Odb, ipfs: &mut IpfsClient) -> Result<Oid, Error> {
        let req = ipfs.cat(&self.raw_data_ipfs_hash).concat2();

        let bytes = current_thread::block_on_all(req)?;

        let obj_type = match self.metadata {
            NIPObjectMetadata::Blob => ObjectType::Blob,
            NIPObjectMetadata::Commit { .. } => ObjectType::Commit,
            NIPObjectMetadata::Tag { .. } => ObjectType::Tag,
            NIPObjectMetadata::Tree { .. } => ObjectType::Tree,
        };

        Ok(odb.write(obj_type, &bytes)?)
    }
}
