/// ABCI requests.
pub mod request;
pub use request::{ConsensusRequest, InfoRequest, MempoolRequest, Request, SnapshotRequest};

/// ABCI responses.
pub mod response;
pub use response::{ConsensusResponse, InfoResponse, MempoolResponse, Response, SnapshotResponse};

/// A fork of tower::buffer @ `e1760d38` that has four queues feeding
/// the same worker task, with different priorities.
mod quad_buffer;

mod server;
pub use server::Server;

pub mod service;

pub type BoxError = Box<dyn std::error::Error + Send + Sync + 'static>;

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
