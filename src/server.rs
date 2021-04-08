// how des the api go???
//
// user writes S: Service<Request, Response=Response>,
// machi

use std::future::Future;

use tokio::net::unix::SocketAddr;
use tower::Service;

use crate::{
    BoxError, ConsensusRequest, ConsensusResponse, InfoRequest, InfoResponse, MempoolRequest,
    MempoolResponse, Request, Response, SnapshotRequest, SnapshotResponse,
};

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

impl<C, M, I, S> ServerBuilder<C, M, I, S>
where
    C: Service<ConsensusRequest, Response = ConsensusResponse, Error = BoxError> + Clone,
    M: Service<MempoolRequest, Response = MempoolResponse, Error = BoxError> + Clone,
    I: Service<InfoRequest, Response = InfoResponse, Error = BoxError> + Clone,
    S: Service<SnapshotRequest, Response = SnapshotResponse, Error = BoxError> + Clone,
{
    pub fn with_consensus(mut self, consensus: C) -> Self {
        self.consensus = Some(consensus);
        self
    }

    pub fn with_mempool(mut self, mempool: M) -> Self {
        self.mempool = Some(mempool);
        self
    }

    pub fn with_info(mut self, info: I) -> Self {
        self.info = Some(info);
        self
    }

    pub fn with_snapshot(mut self, snapshot: S) -> Self {
        self.snapshot = Some(snapshot);
        self
    }

    pub fn finish(mut self) -> Option<Server<C, M, I, S>> {
        todo!()
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
        todo!()
    }
    /*
    pub fn run(self) -> impl Future<Output = Result<(), BoxError>> {
    // return a task that can be spawned
        todo!()
    }
    */
}
