//! Utility routines that don't necessarily fit anywhere else.

use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
use env_logger::Builder;
use failure::Error;
use futures::Stream;
use ipfs_api::IpfsClient;
use log::LevelFilter;
use tokio::runtime::current_thread;

use std::env;

use crate::constants::{NIP_HEADER_LEN, NIP_MAGIC, NIP_PROTOCOL_VERSION};

/// This helper function initializes logging on the supplied level unless RUST_LOG was specified
pub fn init_logging(default_lvl: LevelFilter) {
    match env::var("RUST_LOG") {
        Ok(_) => env_logger::init(),
        Err(_) => Builder::new().filter_level(default_lvl).init(),
    }
}

/// Parse a nip header and return the protocol version.
///
/// PS.: Comedy Gold Best Pun Award 2018 goes to: parsnip header
pub fn parse_nip_header(header: &[u8]) -> Result<u16, Error> {
    if header.len() < NIP_HEADER_LEN {
        let msg = "Supplied slice wouldn't even fit the header".to_owned();
        error!("{}", msg);
        bail!("{}", msg);
    }

    if &header[..NIP_MAGIC.len()] != NIP_MAGIC {
        let msg = format!(
            "Malformed magic: {:?}, expected {:?}",
            &header[..NIP_MAGIC.len()],
            NIP_MAGIC
        );
        error!("{}", msg);
        bail!("{}", msg);
    }

    Ok((&header[NIP_MAGIC.len()..NIP_HEADER_LEN]).read_u16::<BigEndian>()?)
}

/// Returns a serialized 8-byte nip header. A version of None means the caller wants the currently
/// running version.
pub fn gen_nip_header(version: Option<u16>) -> Result<Vec<u8>, Error> {
    let mut ret = Vec::with_capacity(NIP_HEADER_LEN);
    ret.extend_from_slice(NIP_MAGIC);
    ret.write_u16::<BigEndian>(version.unwrap_or(NIP_PROTOCOL_VERSION))?;
    Ok(ret)
}

/// A blocking shortcut to download `hash` from IPFS and return the object's bytes
pub fn ipfs_cat(hash: &str, ipfs: &mut IpfsClient) -> Result<Vec<u8>, Error> {
    let req = ipfs.cat(hash).concat2();

    Ok((&current_thread::block_on_all(req)?[..]).to_vec())
}

/// Returns the underlying IPFS link from an IPNS record
pub fn ipns_deref(ipns_hash: &str, ipfs: &mut IpfsClient) -> Result<String, Error> {
    let req = ipfs.name_resolve(Some(&ipns_hash), true, false);
    let ipfs_hash = current_thread::block_on_all(req)?;

    Ok(format!("/ipfs/{}", ipfs_hash.path))
}
