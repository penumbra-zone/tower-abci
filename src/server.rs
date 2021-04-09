use tokio::net::unix::SocketAddr;
use tower::Service;

use crate::{
    BoxError, ConsensusRequest, ConsensusResponse, InfoRequest, InfoResponse, MempoolRequest,
    MempoolResponse, SnapshotRequest, SnapshotResponse,
};

/// An ABCI server which listens for connections and forwards requests to four
/// component ABCI [`Service`]s.
pub struct Server<C, M, I, S> {
    consensus: C,
    mempool: M,
    info: I,
    snapshot: S,
}

pub struct ServerBuilder<C, M, I, S> {
    consensus: Option<C>,
    mempool: Option<M>,
    info: Option<I>,
    snapshot: Option<S>,
}

impl<C, M, I, S> Default for ServerBuilder<C, M, I, S> {
    fn default() -> Self {
        Self {
            consensus: None,
            mempool: None,
            info: None,
            snapshot: None,
        }
    }
}

impl<C, M, I, S> ServerBuilder<C, M, I, S>
where
    C: Service<ConsensusRequest, Response = ConsensusResponse, Error = BoxError> + Clone,
    M: Service<MempoolRequest, Response = MempoolResponse, Error = BoxError> + Clone,
    I: Service<InfoRequest, Response = InfoResponse, Error = BoxError> + Clone,
    S: Service<SnapshotRequest, Response = SnapshotResponse, Error = BoxError> + Clone,
{
    pub fn consensus(mut self, consensus: C) -> Self {
        self.consensus = Some(consensus);
        self
    }

    pub fn mempool(mut self, mempool: M) -> Self {
        self.mempool = Some(mempool);
        self
    }

    pub fn info(mut self, info: I) -> Self {
        self.info = Some(info);
        self
    }

    pub fn snapshot(mut self, snapshot: S) -> Self {
        self.snapshot = Some(snapshot);
        self
    }

    pub fn finish(self) -> Option<Server<C, M, I, S>> {
        let consensus = if let Some(consensus) = self.consensus {
            consensus
        } else {
            return None;
        };

        let mempool = if let Some(mempool) = self.mempool {
            mempool
        } else {
            return None;
        };

        let info = if let Some(info) = self.info {
            info
        } else {
            return None;
        };

        let snapshot = if let Some(snapshot) = self.snapshot {
            snapshot
        } else {
            return None;
        };

        Some(Server {
            consensus,
            mempool,
            info,
            snapshot,
        })
    }
}

impl<C, M, I, S> Server<C, M, I, S>
where
    C: Service<ConsensusRequest, Response = ConsensusResponse, Error = BoxError> + Clone,
    M: Service<MempoolRequest, Response = MempoolResponse, Error = BoxError> + Clone,
    I: Service<InfoRequest, Response = InfoResponse, Error = BoxError> + Clone,
    S: Service<SnapshotRequest, Response = SnapshotResponse, Error = BoxError> + Clone,
{
    pub fn builder() -> ServerBuilder<C, M, I, S> {
        ServerBuilder::default()
    }

    pub async fn listen(self, addr: SocketAddr) -> Result<(), BoxError> {
        tracing::info!(?addr, "starting ABCI server");
        unimplemented!()
    }
}
