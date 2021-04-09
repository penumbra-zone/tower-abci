use std::task::{Context, Poll};

use crate::{
    buffer4::Buffer, BoxError, ConsensusRequest, ConsensusResponse, InfoRequest, InfoResponse,
    MempoolRequest, MempoolResponse, Request, Response, SnapshotRequest, SnapshotResponse,
};
use tower::Service;

/// Splits a single `service` implementing all of ABCI into four cloneable
/// component services, each implementing one category of ABCI requests.
///
/// The component services share access to the main service via message-passing
/// over buffered channels. The main ABCI service processes requests one at a time in
/// the following priority order:
///
/// 1. [`ConsensusRequest`]s sent to the [`Consensus`] service;
/// 2. [`MempoolRequest`]s sent to the [`Mempool`] service;
/// 3. [`SnapshotRequest`]s sent to the [`Snapshot`] service;
/// 4. [`InfoRequest`]s sent to the [`Info`] service.
///
pub fn services<S>(service: S, bound: usize) -> (Consensus<S>, Mempool<S>, Snapshot<S>, Info<S>)
where
    S: Service<Request, Response = Response, Error = BoxError> + Send + 'static,
    S::Future: Send + 'static,
{
    let bound = std::cmp::max(4, bound);
    let (buffer1, buffer2, buffer3, buffer4) = Buffer::new(service, bound);

    (
        Consensus { inner: buffer1 },
        Mempool { inner: buffer2 },
        Snapshot { inner: buffer3 },
        Info { inner: buffer4 },
    )
}

#[derive(Clone)]
pub struct Consensus<S>
where
    S: Service<Request, Response = Response, Error = BoxError>,
{
    inner: Buffer<S, Request>,
}

impl<S> Service<ConsensusRequest> for Consensus<S>
where
    S: Service<Request, Response = Response, Error = BoxError>,
{
    type Response = ConsensusResponse;
    type Error = BoxError;
    type Future = futures::ConsensusFuture<S>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: ConsensusRequest) -> Self::Future {
        futures::ConsensusFuture {
            inner: self.inner.call(req.into()),
        }
    }
}

#[derive(Clone)]
pub struct Mempool<S>
where
    S: Service<Request, Response = Response, Error = BoxError>,
{
    inner: Buffer<S, Request>,
}

impl<S> Service<MempoolRequest> for Mempool<S>
where
    S: Service<Request, Response = Response, Error = BoxError>,
{
    type Response = MempoolResponse;
    type Error = BoxError;
    type Future = futures::MempoolFuture<S>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: MempoolRequest) -> Self::Future {
        futures::MempoolFuture {
            inner: self.inner.call(req.into()),
        }
    }
}

#[derive(Clone)]
pub struct Info<S>
where
    S: Service<Request, Response = Response, Error = BoxError>,
{
    inner: Buffer<S, Request>,
}

impl<S> Service<InfoRequest> for Info<S>
where
    S: Service<Request, Response = Response, Error = BoxError>,
{
    type Response = InfoResponse;
    type Error = BoxError;
    type Future = futures::InfoFuture<S>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: InfoRequest) -> Self::Future {
        futures::InfoFuture {
            inner: self.inner.call(req.into()),
        }
    }
}

#[derive(Clone)]
pub struct Snapshot<S>
where
    S: Service<Request, Response = Response, Error = BoxError>,
{
    inner: Buffer<S, Request>,
}

impl<S> Service<SnapshotRequest> for Snapshot<S>
where
    S: Service<Request, Response = Response, Error = BoxError>,
{
    type Response = SnapshotResponse;
    type Error = BoxError;
    type Future = futures::SnapshotFuture<S>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: SnapshotRequest) -> Self::Future {
        futures::SnapshotFuture {
            inner: self.inner.call(req.into()),
        }
    }
}

// this is all "necessary" only because rust does not support full GATs or allow
// specifying a concrete (but unnameable) associated type using impl Trait.
// this means that Tower services either have to have handwritten futures
// or box the futures they return.  Boxing a few futures is not really a big deal
// but it's nice to avoid deeply nested boxes arising from service combinators like
// these ones.

// https://github.com/rust-lang/rust/issues/63063 fixes this

pub mod futures {
    use pin_project::pin_project;
    use std::{convert::TryInto, future::Future, pin::Pin};

    use super::*;

    #[pin_project]
    pub struct ConsensusFuture<S>
    where
        S: Service<Request, Response = Response, Error = BoxError>,
    {
        #[pin]
        pub(super) inner: <Buffer<S, Request> as Service<Request>>::Future,
    }

    impl<S> Future for ConsensusFuture<S>
    where
        S: Service<Request, Response = Response, Error = BoxError>,
    {
        type Output = Result<ConsensusResponse, BoxError>;

        fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            let this = self.project();
            match this.inner.poll(cx) {
                Poll::Ready(rsp) => Poll::Ready(
                    rsp.map(|rsp| rsp.try_into().expect("service gave wrong response type")),
                ),
                Poll::Pending => Poll::Pending,
            }
        }
    }

    #[pin_project]
    pub struct MempoolFuture<S>
    where
        S: Service<Request, Response = Response, Error = BoxError>,
    {
        #[pin]
        pub(super) inner: <Buffer<S, Request> as Service<Request>>::Future,
    }

    impl<S> Future for MempoolFuture<S>
    where
        S: Service<Request, Response = Response, Error = BoxError>,
    {
        type Output = Result<MempoolResponse, BoxError>;

        fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            let this = self.project();
            match this.inner.poll(cx) {
                Poll::Ready(rsp) => Poll::Ready(
                    rsp.map(|rsp| rsp.try_into().expect("service gave wrong response type")),
                ),
                Poll::Pending => Poll::Pending,
            }
        }
    }

    #[pin_project]
    pub struct InfoFuture<S>
    where
        S: Service<Request, Response = Response, Error = BoxError>,
    {
        #[pin]
        pub(super) inner: <Buffer<S, Request> as Service<Request>>::Future,
    }

    impl<S> Future for InfoFuture<S>
    where
        S: Service<Request, Response = Response, Error = BoxError>,
    {
        type Output = Result<InfoResponse, BoxError>;

        fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            let this = self.project();
            match this.inner.poll(cx) {
                Poll::Ready(rsp) => Poll::Ready(
                    rsp.map(|rsp| rsp.try_into().expect("service gave wrong response type")),
                ),
                Poll::Pending => Poll::Pending,
            }
        }
    }

    #[pin_project]
    pub struct SnapshotFuture<S>
    where
        S: Service<Request, Response = Response, Error = BoxError>,
    {
        #[pin]
        pub(super) inner: <Buffer<S, Request> as Service<Request>>::Future,
    }

    impl<S> Future for SnapshotFuture<S>
    where
        S: Service<Request, Response = Response, Error = BoxError>,
    {
        type Output = Result<SnapshotResponse, BoxError>;

        fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            let this = self.project();
            match this.inner.poll(cx) {
                Poll::Ready(rsp) => Poll::Ready(
                    rsp.map(|rsp| rsp.try_into().expect("service gave wrong response type")),
                ),
                Poll::Pending => Poll::Pending,
            }
        }
    }
}
