#![cfg_attr(feature = "doc", feature(external_doc))]
#![cfg_attr(feature = "doc", doc(include = "../README.md"))]

/// ABCI requests.
pub mod request;
#[doc(inline)]
pub use request::{ConsensusRequest, InfoRequest, MempoolRequest, Request, SnapshotRequest};

/// ABCI responses.
pub mod response;
#[doc(inline)]
pub use response::{ConsensusResponse, InfoResponse, MempoolResponse, Response, SnapshotResponse};

/// A fork of tower::buffer @ `e1760d38` that has four queues feeding
/// the same worker task, with different priorities.
mod buffer4;

mod server;
pub use server::Server;

pub mod split;

/// A convenient error type alias.
pub type BoxError = Box<dyn std::error::Error + Send + Sync + 'static>;
