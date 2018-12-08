//! nip index implementation
use super::serde_cbor;

use failure::Error;
use futures::Stream;
use git2::{Object, ObjectType, Oid, Repository};
use ipfs_api::IpfsClient;
use tokio_core::reactor::Core;

use std::{
    cmp::Ordering,
    collections::{BTreeMap, HashSet},
    io::Cursor,
};

use constants::{NIP_HEADER_LEN, NIP_PROTOCOL_VERSION};
use object::{NIPObject, NIPObjectMetadata};
use remote::NIPRemote;
use util::{gen_nip_header, ipns_deref, parse_nip_header};

/// The entrypoint data structure for every nip repo.
///
/// Every top-level nip IPFS link points at a `NIPIndex`. nip indices store information about all
/// git objects contained within a given nip repository.
#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct NIPIndex {
    /// All refs this repository knows; a {name -> sha1} mapping
    pub refs: BTreeMap<String, String>,
    /// All objects this repository contains; a {sha1 -> IPFS hash} map
    pub objects: BTreeMap<String, String>,
    /// The IPFS hash of the previous index
    pub prev_idx_hash: Option<String>,
}

impl NIPIndex {
    /// Download from IPFS and instantiate a NIPIndex
    pub fn from_nip_remote(remote: &NIPRemote, ipfs: &mut IpfsClient) -> Result<Self, Error> {
        match remote {
            NIPRemote::ExistingIPFS(ref hash) => {
                debug!("Fetching NIPIndex from /ipfs/{}", hash);
                let mut event_loop = Core::new()?;
                let req = ipfs.cat(hash).concat2();

                let bytes = event_loop.run(req)?;

                match String::from_utf8(bytes.to_vec()) {
                    Ok(s) => trace!("Received string:\n{}", s),
                    Err(_e) => trace!("Received raw bytes:\n{:?}", bytes),
                }

                let protocol_version = parse_nip_header(&bytes[..NIP_HEADER_LEN])?;
                debug!("Index protocol version {}", protocol_version);
                match protocol_version.cmp(&NIP_PROTOCOL_VERSION) {
                    Ordering::Less => {
                        debug!(
                            "nip index is {} protocol versions behind, migrating...",
                            NIP_PROTOCOL_VERSION - protocol_version
                        );
                        bail!("Migrations aren't a thing yet!");
                    }
                    Ordering::Equal => {}
                    Ordering::Greater => {
                        error!(
                            "nip index is {} protocol versions ahead, please upgrade nip to use it",
                            protocol_version - NIP_PROTOCOL_VERSION
                        );
                        bail!("Our nip is too old");
                    }
                }
                let idx: NIPIndex = serde_cbor::from_slice(&bytes[NIP_HEADER_LEN..])?;
                Ok(idx)
            }
            NIPRemote::ExistingIPNS(ref hash) => Ok(Self::from_nip_remote(
                &ipns_deref(hash.as_str(), ipfs)?.parse()?,
                ipfs,
            )?),
            NIPRemote::NewIPFS | NIPRemote::NewIPNS => {
                debug!("Creating new index");
                Ok(NIPIndex {
                    refs: BTreeMap::new(),
                    objects: BTreeMap::new(),
                    prev_idx_hash: None,
                })
            }
        }
    }

    /// Figure out what git hash `ref_src` points to in `repo` and add it to the index as `ref_dst`.
    pub fn push_ref_from_str(
        &mut self,
        ref_src: &str,
        ref_dst: &str,
        repo: &mut Repository,
        ipfs: &mut IpfsClient,
    ) -> Result<(), Error> {
        let reference = repo.find_reference(ref_src)?.resolve()?;

        // Differentiate between annotated tags and their commit representation
        let obj = reference
            .peel(ObjectType::Tag)
            .unwrap_or(reference.peel(ObjectType::Commit)?);

        debug!(
            "{:?} dereferenced to {:?} {}",
            reference.shorthand(),
            obj.kind(),
            obj.id()
        );

        let objs_for_push = self.enumerate_for_push(&obj.clone(), repo, ipfs)?;
        debug!(
            "Counted {} object(s) for push:\n{:#?}",
            objs_for_push.len(),
            objs_for_push
        );

        self.push_git_objects(&objs_for_push, repo, ipfs)?;
        self.refs
            .insert(ref_dst.to_owned(), format!("{}", obj.id()));
        Ok(())
    }

    /// Get a hash set of `obj`'s children present in `repo` but missing from `self`.
    pub fn enumerate_for_push(
        &self,
        obj: &Object,
        repo: &Repository,
        ipfs: &mut IpfsClient,
    ) -> Result<HashSet<Oid>, Error> {
        let mut ret = HashSet::new();

        if self.objects.contains_key(&obj.id().to_string()) {
            trace!("Object {} already in nip index", obj.id());
            return Ok(ret);
        }

        let obj_type = obj.kind().ok_or_else(|| {
            let msg = format!("Cannot determine type of object {}", obj.id());
            error!("{}", msg);
            format_err!("{}", msg)
        })?;

        ret.insert(obj.id());

        match obj_type {
            ObjectType::Commit => {
                let commit = obj
                    .as_commit()
                    .ok_or_else(|| format_err!("Could not view {:?} as a commit", obj))?;
                debug!("Counting commit {:?}", commit);

                let tree_obj = obj.peel(ObjectType::Tree)?;
                trace!("Commit {}: Handling tree {}", commit.id(), tree_obj.id());

                // Every commit has a tree
                ret = ret
                    .union(&self.enumerate_for_push(&tree_obj, repo, ipfs)?)
                    .cloned()
                    .collect();

                for parent in commit.parents() {
                    trace!(
                        "Commit {}: Handling parent commit {}",
                        commit.id(),
                        parent.id()
                    );
                    ret = ret
                        .union(&self.enumerate_for_push(&parent.into_object(), repo, ipfs)?)
                        .cloned()
                        .collect();
                }

                Ok(ret)
            }
            ObjectType::Tree => {
                let tree = obj
                    .as_tree()
                    .ok_or_else(|| format_err!("Could not view {:?} as a tree", obj))?;
                debug!("Counting tree {:?}", tree);

                for entry in tree.into_iter() {
                    trace!(
                        "Tree {}: Handling tree entry {} ({:?})",
                        tree.id(),
                        entry.id(),
                        entry.kind()
                    );
                    ret = ret
                        .union(&self.enumerate_for_push(
                            &repo.find_object(entry.id(), entry.kind())?,
                            repo,
                            ipfs,
                        )?)
                        .cloned()
                        .collect();
                }

                Ok(ret)
            }
            ObjectType::Blob => {
                let blob = obj
                    .as_blob()
                    .ok_or_else(|| format_err!("Could not view {:?} as a blob", obj))?;
                debug!("Counting blob {:?}", blob);

                Ok(ret)
            }
            ObjectType::Tag => {
                let tag = obj
                    .as_tag()
                    .ok_or_else(|| format_err!("Could not view {:?} as a tag", obj))?;
                debug!("Counting tag {:?}", tag);

                ret = ret
                    .union(&self.enumerate_for_push(&tag.target()?, repo, ipfs)?)
                    .cloned()
                    .collect();

                Ok(ret)
            }
            other => bail!("Don't know how to traverse a {}", other),
        }
    }

    /// Take `oids` and upload underlying `repo` git objects to IPFS.
    pub fn push_git_objects(
        &mut self,
        oids: &HashSet<Oid>,
        repo: &Repository,
        ipfs: &mut IpfsClient,
    ) -> Result<(), Error> {
        for (i, oid) in oids.iter().enumerate() {
            let obj = repo.find_object(*oid, None)?;
            trace!("Current object: {:?} at {}", obj.kind(), obj.id());

            if self.objects.contains_key(&obj.id().to_string()) {
                warn!("push_objects: Object {} already in nip index", obj.id());
                continue;
            }

            let obj_type = obj.kind().ok_or_else(|| {
                let msg = format!("Cannot determine type of object {}", obj.id());
                error!("{}", msg);
                format_err!("{}", msg)
            })?;

            match obj_type {
                ObjectType::Commit => {
                    let commit = obj
                        .as_commit()
                        .ok_or_else(|| format_err!("Could not view {:?} as a commit", obj))?;
                    trace!("Pushing commit {:?}", commit);

                    let nip_object_hash =
                        NIPObject::from_git_commit(&commit, &repo.odb()?, ipfs)?.ipfs_add(ipfs)?;

                    self.objects
                        .insert(format!("{}", obj.id()), nip_object_hash.clone());
                    debug!(
                        "[{}/{}] Commit {} uploaded to {}",
                        i + 1,
                        oids.len(),
                        obj.id(),
                        nip_object_hash
                    );
                }
                ObjectType::Tree => {
                    let tree = obj
                        .as_tree()
                        .ok_or_else(|| format_err!("Could not view {:?} as a tree", obj))?;
                    trace!("Pushing tree {:?}", tree);

                    let nip_object_hash =
                        NIPObject::from_git_tree(&tree, &repo.odb()?, ipfs)?.ipfs_add(ipfs)?;

                    self.objects
                        .insert(format!("{}", obj.id()), nip_object_hash.clone());
                    debug!(
                        "[{}/{}] Tree {} uploaded to {}",
                        i + 1,
                        oids.len(),
                        obj.id(),
                        nip_object_hash
                    );
                }
                ObjectType::Blob => {
                    let blob = obj
                        .as_blob()
                        .ok_or_else(|| format_err!("Could not view {:?} as a blob", obj))?;
                    trace!("Pushing blob {:?}", blob);

                    let nip_object_hash =
                        NIPObject::from_git_blob(&blob, &repo.odb()?, ipfs)?.ipfs_add(ipfs)?;

                    self.objects
                        .insert(format!("{}", obj.id()), nip_object_hash.clone());
                    debug!(
                        "[{}/{}] Blob {} uploaded to {}",
                        i + 1,
                        oids.len(),
                        obj.id(),
                        nip_object_hash
                    );
                }
                ObjectType::Tag => {
                    let tag = obj
                        .as_tag()
                        .ok_or_else(|| format_err!("Could not view {:?} as a tag", obj))?;
                    trace!("Pushing tag {:?}", tag);

                    let nip_object_hash =
                        NIPObject::from_git_tag(&tag, &repo.odb()?, ipfs)?.ipfs_add(ipfs)?;

                    self.objects
                        .insert(format!("{}", obj.id()), nip_object_hash.clone());

                    debug!(
                        "[{}/{}] Tag {} uploaded to {}",
                        i + 1,
                        oids.len(),
                        obj.id(),
                        nip_object_hash
                    );
                }
                other => bail!("Don't know how to traverse a {}", other),
            }
        }
        Ok(())
    }

    /// Fetch `git_hash` from `self` to `repo`'s `ref_name` ref.
    pub fn fetch_to_ref_from_str(
        &self,
        git_hash: &str,
        ref_name: &str,
        repo: &mut Repository,
        ipfs: &mut IpfsClient,
    ) -> Result<(), Error> {
        debug!("Fetching {} for {}", git_hash, ref_name);

        let git_hash_oid = Oid::from_str(git_hash)?;
        let oids_for_fetch = self.enumerate_for_fetch(git_hash_oid, repo, ipfs)?;
        debug!(
            "Counted {} object(s) for fetch:\n{:#?}",
            oids_for_fetch.len(),
            oids_for_fetch
        );

        self.fetch_nip_objects(&oids_for_fetch, repo, ipfs)?;

        match repo.odb()?.read_header(git_hash_oid)?.1 {
            ObjectType::Commit if ref_name.starts_with("refs/tags") => {
                debug!("Not setting ref for lightweight tag {}", ref_name);
            }
            ObjectType::Commit => {
                repo.reference(ref_name, git_hash_oid, true, "nip fetch")?;
            }
            // Somehow git is upset when we set tag refs for it
            ObjectType::Tag => {
                debug!("Not setting ref for tag {}", ref_name);
            }
            other_type => {
                let msg = format!("New tip turned out to be a {} after fetch", other_type);
                error!("{}", msg);
                bail!("{}", msg);
            }
        }

        debug!("Fetched {} for {} OK.", git_hash, ref_name);
        Ok(())
    }

    /// Get a hash set of `oid`'s children that are present in `self` but missing in `repo`.
    pub fn enumerate_for_fetch(
        &self,
        oid: Oid,
        repo: &mut Repository,
        ipfs: &mut IpfsClient,
    ) -> Result<HashSet<Oid>, Error> {
        let mut ret = HashSet::new();

        if repo.odb()?.read_header(oid).is_ok() {
            trace!("Object {} already present locally!", oid);
            return Ok(ret);
        }

        let nip_obj_ipfs_hash = self
            .objects
            .get(&format!("{}", oid))
            .ok_or_else(|| {
                let msg = format!("Could not find object {} in the index", oid);
                error!("{}", msg);
                format_err!("{}", msg)
            })?
            .clone();

        // Inserting only makes sense after we knowthat the object is there at all
        ret.insert(oid);

        let nip_obj = NIPObject::ipfs_get(&nip_obj_ipfs_hash, ipfs)?;

        match nip_obj.clone().metadata {
            NIPObjectMetadata::Commit {
                parent_git_hashes,
                tree_git_hash,
            } => {
                debug!("Counting nip commit {}", nip_obj_ipfs_hash);

                ret = ret
                    .union(&self.enumerate_for_fetch(Oid::from_str(&tree_git_hash)?, repo, ipfs)?)
                    .cloned()
                    .collect();

                for parent_git_hash in parent_git_hashes {
                    ret = ret
                        .union(&self.enumerate_for_fetch(
                            Oid::from_str(&parent_git_hash)?,
                            repo,
                            ipfs,
                        )?)
                        .cloned()
                        .collect();
                }
            }
            NIPObjectMetadata::Tag { target_git_hash } => {
                debug!("Counting nip tag {}", nip_obj_ipfs_hash);

                ret = ret
                    .union(&self.enumerate_for_fetch(
                        Oid::from_str(&target_git_hash)?,
                        repo,
                        ipfs,
                    )?)
                    .cloned()
                    .collect();
            }
            NIPObjectMetadata::Tree { entry_git_hashes } => {
                trace!("Counting nip tree {}", nip_obj_ipfs_hash);

                for entry_git_hash in entry_git_hashes {
                    ret = ret
                        .union(&self.enumerate_for_fetch(
                            Oid::from_str(&entry_git_hash)?,
                            repo,
                            ipfs,
                        )?)
                        .cloned()
                        .collect();
                }
            }
            NIPObjectMetadata::Blob => {
                trace!("Counting nip blob {}", nip_obj_ipfs_hash);
            }
        }

        Ok(ret)
    }

    /// Download git objects in `oids` from IPFS and instantiate them in `repo`.
    pub fn fetch_nip_objects(
        &self,
        oids: &HashSet<Oid>,
        repo: &mut Repository,
        ipfs: &mut IpfsClient,
    ) -> Result<(), Error> {
        for (i, &oid) in oids.iter().enumerate() {
            debug!("[{}/{}] Fetching object {}", i + 1, oids.len(), oid);

            let nip_obj_ipfs_hash = self.objects.get(&format!("{}", oid)).ok_or_else(|| {
                let msg = format!("Could not find object {} in nip index", oid);
                error!("{}", msg);
                format_err!("{}", msg)
            })?;

            let nip_obj = NIPObject::ipfs_get(nip_obj_ipfs_hash, ipfs)?;

            trace!("nip object at {}:\n{:#?}", nip_obj_ipfs_hash, nip_obj,);

            if repo.odb()?.read_header(oid).is_ok() {
                warn!("fetch_nip_objects: Object {} already present locally!", oid);
                continue;
            }

            let written_oid = nip_obj.write_raw_data(&mut repo.odb()?, ipfs)?;
            if written_oid != oid {
                let msg = format!("Object tree inconsistency detected: fetched {} from {}, but write result hashes to {}", oid, nip_obj_ipfs_hash, written_oid);
                error!("{}", msg);
                bail!("{}", msg);
            }
            trace!("Fetched object {} to {}", nip_obj_ipfs_hash, written_oid);
        }
        Ok(())
    }

    /// Upload `self` to IPFS and return the IPFS/IPNS link. Plain/IPNS link use is determined as
    /// per `prev_remote` variant (IPNS is used for both `NewIPNS` and `ExistingIPNS`, `None`
    /// assumes IPFS); `prev_remote` is later put in the `prev_idx_hash` field just before upload.
    pub fn ipfs_add(
        &mut self,
        ipfs: &mut IpfsClient,
        prev_remote: Option<&NIPRemote>,
    ) -> Result<NIPRemote, Error> {
        let mut event_loop = Core::new()?;

        self.prev_idx_hash = match prev_remote {
            Some(remote) => match remote {
                NIPRemote::ExistingIPFS(_) => Some(remote.to_string()),
                NIPRemote::ExistingIPNS(hash) => Some(ipns_deref(&hash, ipfs)?),
                NIPRemote::NewIPFS | NIPRemote::NewIPNS => None,
            },
            None => None,
        };

        // Encode
        let mut self_buf = gen_nip_header(None)?;
        self_buf.extend_from_slice(&serde_cbor::to_vec(self)?);

        // Upload
        let add_req = ipfs.add(Cursor::new(self_buf));
        let mut new_hash = format!("/ipfs/{}", event_loop.run(add_req)?.hash);

        // Publish on IPNS if applicable; prev_remote == None means no IPNS
        if prev_remote.map(|remote| remote.is_ipns()).unwrap_or(false) {
            debug!("Previous remote {:?} was IPNS, republishing", prev_remote);

            let publish_req = ipfs.name_publish(&new_hash, true, None, None, None);

            new_hash = format!("/ipns/{}", event_loop.run(publish_req)?.name)
        }

        Ok(new_hash.parse()?)
    }
}
