//! `nip_core` is a library that lets you interact with [nip](https://github.com/drozdziak1/nip)
//! repositories programmatically.
//!
//! ```rust,no_run
//! extern crate failure;
//! extern crate git2;
//! extern crate ipfs_api;
//! extern crate nip_core;
//!
//! use failure::Error;
//! use git2::Repository;
//! use ipfs_api::IpfsClient;
//! use nip_core::{NIPIndex, NIPRemote};
//!
//! # fn main() -> Result<(), Error>{
//! // Open the local repository
//! let mut repo = Repository::open_from_env()?;
//!
//! // Get a handle for IPFS API
//! let mut ipfs = IpfsClient::default();
//!
//! // Instantiate a brand new nip index
//! let mut idx = NIPIndex::from_nip_remote(&NIPRemote::NewIPFS, &mut ipfs)?;
//!
//! // Upload the full object tree behind a specified local ref to IPFS
//! idx.push_ref_from_str("refs/heads/master", "refs/heads/master", &mut repo, &mut ipfs)?;
//!
//! // Also upload the brand new index itself
//! let nip_remote: NIPRemote = idx.ipfs_add(&mut ipfs, None)?;
//!
//! println!("Success! refs/heads/master uploaded to remote {}", nip_remote.to_string());
//! # Ok(())
//! # }
//! ```
#![deny(missing_docs)]
#[macro_use]
extern crate failure;
#[macro_use]
extern crate log;
#[macro_use]
extern crate serde_derive;

extern crate byteorder;
extern crate env_logger;
extern crate futures;
extern crate git2;
extern crate hyper;
extern crate ipfs_api;
extern crate serde;
extern crate serde_cbor;
extern crate tokio_core;

pub mod constants;
pub mod index;
pub mod object;
pub mod remote;
pub mod util;

pub use constants::*;
pub use index::*;
pub use object::*;
pub use remote::*;
pub use util::*;
