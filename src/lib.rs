/// ABCI requests.
pub mod request;
pub use request::{ConsensusRequest, InfoRequest, MempoolRequest, Request, SnapshotRequest};

/// ABCI responses.
pub mod response;
pub use response::{ConsensusResponse, InfoResponse, MempoolResponse, Response, SnapshotResponse};

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
