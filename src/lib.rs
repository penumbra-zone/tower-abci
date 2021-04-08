
/// ABCI requests.
pub mod request;
pub use request::{Request, ConsensusRequest, MempoolRequest, SnapshotRequest, InfoRequest};

/// ABCI responses.
pub mod response;
pub use response::{Response, ConsensusResponse, MempoolResponse, SnapshotResponse, InfoResponse};

mod server;
pub use server::Server;

pub type BoxError = Box<dyn std::error::Error + Send + Sync + 'static>;

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
