use crate::{BoxError, ConsensusRequest, Request, Response};
use tower::Service;

/*
fn split<S>(svc: S) -> (Consensus<S>, Mempool<S>, Info<S>, Snapshot<S>)
where
    S: Service<Request, Response = Response, BoxError>,
{
    todo!()
}

pub struct Consensus<S> {
    inner: S,
}

impl Service<ConsensusRequest> for Consensus<S> {

}

pub struct Mempool<S> {
    inner: S,
}

pub struct Info<S> {
    inner: S,
}

pub struct Snapshot<S> {
    inner: S,
}

// common code between handles (?)
struct Handle<S> {
    // todo
}

*/
