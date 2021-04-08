use std::convert::TryFrom;

use tendermint_proto::abci as tpa;

#[doc(inline)]
pub use tpa::RequestApplySnapshotChunk as ApplySnapshotChunk;
#[doc(inline)]
pub use tpa::RequestBeginBlock as BeginBlock;
#[doc(inline)]
pub use tpa::RequestCheckTx as CheckTx;
#[doc(inline)]
pub use tpa::RequestCommit as Commit;
#[doc(inline)]
pub use tpa::RequestDeliverTx as DeliverTx;
#[doc(inline)]
pub use tpa::RequestEcho as Echo;
#[doc(inline)]
pub use tpa::RequestEndBlock as EndBlock;
#[doc(inline)]
pub use tpa::RequestFlush as Flush;
#[doc(inline)]
pub use tpa::RequestInfo as Info;
#[doc(inline)]
pub use tpa::RequestInitChain as InitChain;
#[doc(inline)]
pub use tpa::RequestListSnapshots as ListSnapshots;
#[doc(inline)]
pub use tpa::RequestLoadSnapshotChunk as LoadSnapshotChunk;
#[doc(inline)]
pub use tpa::RequestOfferSnapshot as OfferSnapshot;
#[doc(inline)]
pub use tpa::RequestQuery as Query;
#[doc(inline)]
pub use tpa::RequestSetOption as SetOption;

/// An ABCI request.
#[derive(Clone, PartialEq, Debug)]
pub enum Request {
    Echo(Echo),
    Flush(Flush),
    Info(Info),
    SetOption(SetOption),
    InitChain(InitChain),
    Query(Query),
    BeginBlock(BeginBlock),
    CheckTx(CheckTx),
    DeliverTx(DeliverTx),
    EndBlock(EndBlock),
    Commit(Commit),
    ListSnapshots(ListSnapshots),
    OfferSnapshot(OfferSnapshot),
    LoadSnapshotChunk(LoadSnapshotChunk),
    ApplySnapshotChunk(ApplySnapshotChunk),
}

/// An ABCI request sent over the consensus connection.
#[derive(Clone, PartialEq, Debug)]
pub enum ConsensusRequest {
    InitChain(InitChain),
    BeginBlock(BeginBlock),
    DeliverTx(DeliverTx),
    EndBlock(EndBlock),
    Commit(Commit),
}

impl From<ConsensusRequest> for Request {
    fn from(req: ConsensusRequest) -> Self {
        match req {
            ConsensusRequest::InitChain(x) => Self::InitChain(x),
            ConsensusRequest::BeginBlock(x) => Self::BeginBlock(x),
            ConsensusRequest::DeliverTx(x) => Self::DeliverTx(x),
            ConsensusRequest::EndBlock(x) => Self::EndBlock(x),
            ConsensusRequest::Commit(x) => Self::Commit(x),
        }
    }
}

impl TryFrom<Request> for ConsensusRequest {
    type Error = &'static str;
    fn try_from(req: Request) -> Result<Self, Self::Error> {
        match req {
            Request::InitChain(x) => Ok(Self::InitChain(x)),
            Request::BeginBlock(x) => Ok(Self::BeginBlock(x)),
            Request::DeliverTx(x) => Ok(Self::DeliverTx(x)),
            Request::EndBlock(x) => Ok(Self::EndBlock(x)),
            Request::Commit(x) => Ok(Self::Commit(x)),
            _ => Err("wrong request type"),
        }
    }
}

/// An ABCI request sent over the mempool connection.
#[derive(Clone, PartialEq, Debug)]
pub enum MempoolRequest {
    CheckTx(CheckTx),
}

impl From<MempoolRequest> for Request {
    fn from(req: MempoolRequest) -> Self {
        match req {
            MempoolRequest::CheckTx(x) => Self::CheckTx(x),
        }
    }
}

impl TryFrom<Request> for MempoolRequest {
    type Error = &'static str;
    fn try_from(req: Request) -> Result<Self, Self::Error> {
        match req {
            Request::CheckTx(x) => Ok(Self::CheckTx(x)),
            _ => Err("wrong request type"),
        }
    }
}

/// An ABCI request sent over the info connection.
#[derive(Clone, PartialEq, Debug)]
pub enum InfoRequest {
    Info(Info),
    Query(Query),
}

impl From<InfoRequest> for Request {
    fn from(req: InfoRequest) -> Self {
        match req {
            InfoRequest::Info(x) => Self::Info(x),
            InfoRequest::Query(x) => Self::Query(x),
        }
    }
}

impl TryFrom<Request> for InfoRequest {
    type Error = &'static str;
    fn try_from(req: Request) -> Result<Self, Self::Error> {
        match req {
            Request::Info(x) => Ok(Self::Info(x)),
            Request::Query(x) => Ok(Self::Query(x)),
            _ => Err("wrong request type"),
        }
    }
}

/// An ABCI request sent over the snapshot connection.
#[derive(Clone, PartialEq, Debug)]
pub enum SnapshotRequest {
    ListSnapshots(ListSnapshots),
    OfferSnapshot(OfferSnapshot),
    LoadSnapshotChunk(LoadSnapshotChunk),
    ApplySnapshotChunk(ApplySnapshotChunk),
}

impl From<SnapshotRequest> for Request {
    fn from(req: SnapshotRequest) -> Self {
        match req {
            SnapshotRequest::ListSnapshots(x) => Self::ListSnapshots(x),
            SnapshotRequest::OfferSnapshot(x) => Self::OfferSnapshot(x),
            SnapshotRequest::LoadSnapshotChunk(x) => Self::LoadSnapshotChunk(x),
            SnapshotRequest::ApplySnapshotChunk(x) => Self::ApplySnapshotChunk(x),
        }
    }
}

impl TryFrom<Request> for SnapshotRequest {
    type Error = &'static str;
    fn try_from(req: Request) -> Result<Self, Self::Error> {
        match req {
            Request::ListSnapshots(x) => Ok(Self::ListSnapshots(x)),
            Request::OfferSnapshot(x) => Ok(Self::OfferSnapshot(x)),
            Request::LoadSnapshotChunk(x) => Ok(Self::LoadSnapshotChunk(x)),
            Request::ApplySnapshotChunk(x) => Ok(Self::ApplySnapshotChunk(x)),
            _ => Err("wrong request type"),
        }
    }
}
