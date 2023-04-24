#![doc = include_str!("../README.md")]

use tendermint::abci::{
    ConsensusRequest, ConsensusResponse, InfoRequest, InfoResponse, MempoolRequest,
    MempoolResponse, Request, Response, SnapshotRequest, SnapshotResponse,
};

/// A fork of tower::buffer @ `e1760d38` that has four queues feeding
/// the same worker task, with different priorities.
mod buffer4;

pub mod v034 {
    mod codec;
    mod server;
    pub use server::Server;
    pub use server::ServerBuilder;
}

pub mod v037 {
    mod codec;
    mod server;
    pub use server::Server;
    pub use server::ServerBuilder;
}

pub mod split;
/// A convenient error type alias.
pub type BoxError = Box<dyn std::error::Error + Send + Sync + 'static>;
