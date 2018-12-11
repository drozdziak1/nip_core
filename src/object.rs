//! nip object implementation
use failure::Error;
use futures::{Future, Stream};
use git2::{Blob, Commit, ObjectType, Odb, OdbObject, Oid, Tag, Tree};
use ipfs_api::IpfsClient;
use tokio::runtime::Runtime;

use std::{collections::BTreeSet, io::Cursor};

use crate::{
    constants::{NIP_HEADER_LEN, NIP_PROTOCOL_VERSION},
    util::{gen_nip_header, parse_nip_header},
};

#[derive(Clone, Debug, Deserialize, Serialize)]
/// A nip representation of a git object
pub struct NIPObject {
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
            raw_data_ipfs_hash,
            metadata: NIPObjectMetadata::Tree { entry_git_hashes },
        })
    }

    /// Download from IPFS and instantiate a `NIPObject`.
    pub fn ipfs_get(hash: &str, ipfs: &mut IpfsClient) -> Result<Self, Error> {
        let mut event_loop = Runtime::new()?;

        let object_bytes_req = ipfs.cat(hash).concat2();

        let object_bytes: Vec<u8> = event_loop.block_on(object_bytes_req)?.into_iter().collect();
        event_loop
            .shutdown_on_idle()
            .wait()
            .map_err(|()| format_err!("Could not shutdown the event loop"))?;

        let obj_nip_proto_version = parse_nip_header(&object_bytes)?;

        if obj_nip_proto_version != NIP_PROTOCOL_VERSION {
            bail!(
                "Unsupported protocol version {} (We're at {})",
                obj_nip_proto_version,
                NIP_PROTOCOL_VERSION
            );
        }

        Ok(serde_cbor::from_slice(&object_bytes[NIP_HEADER_LEN..])?)
    }

    /// Put `self` on IPFS and return the link.
    pub fn ipfs_add(&self, ipfs: &mut IpfsClient) -> Result<String, Error> {
        let mut event_loop = Runtime::new()?;
        let mut self_buf = gen_nip_header(None)?;

        self_buf.extend_from_slice(&serde_cbor::to_vec(self)?);

        let req = ipfs.add(Cursor::new(self_buf));
        let ipfs_hash = format!("/ipfs/{}", event_loop.block_on(req)?.hash);
        event_loop
            .shutdown_on_idle()
            .wait()
            .map_err(|()| format_err!("Could not shutdown the event loop"))?;

        Ok(ipfs_hash)
    }

    /// Upload `odb_obj` to IPFS and return the link.
    fn upload_odb_obj(odb_obj: &OdbObject, ipfs: &mut IpfsClient) -> Result<String, Error> {
        let mut event_loop = Runtime::new()?;

        let obj_buf = odb_obj.data().to_vec();

        let raw_data_req = ipfs.add(Cursor::new(obj_buf));
        let ipfs_hash = event_loop.block_on(raw_data_req)?.hash;
        event_loop
            .shutdown_on_idle()
            .wait()
            .map_err(|()| format_err!("Could not shutdown the event loop"))?;
        Ok(format!("/ipfs/{}", ipfs_hash))
    }

    /// Download `self.raw_data_ipfs_hash` from IPFS and use it to instantiate `self` in `odb`.
    pub fn write_raw_data(&self, odb: &mut Odb, ipfs: &mut IpfsClient) -> Result<Oid, Error> {
        let mut event_loop = Runtime::new()?;
        let req = ipfs.cat(&self.raw_data_ipfs_hash).concat2();

        let bytes = event_loop.block_on(req)?;
        event_loop
            .shutdown_on_idle()
            .wait()
            .map_err(|()| format_err!("Could not shutdown the event loop"))?;

        let obj_type = match self.metadata {
            NIPObjectMetadata::Blob => ObjectType::Blob,
            NIPObjectMetadata::Commit { .. } => ObjectType::Commit,
            NIPObjectMetadata::Tag { .. } => ObjectType::Tag,
            NIPObjectMetadata::Tree { .. } => ObjectType::Tree,
        };

        Ok(odb.write(obj_type, &bytes)?)
    }
}
