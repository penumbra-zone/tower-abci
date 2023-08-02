//! Splits a single [`Service`] implementing all of ABCI into four cloneable
//! component services, each implementing one category of ABCI requests.
//!
//! The component services share access to the main service via message-passing
//! over buffered channels. This means that the component services can be cloned
//! to provide shared access to the ABCI application, which processes
//! requests sequentially with the following prioritization:
//!
//! 1. [`ConsensusRequest`]s sent to the [`Consensus`] service;
//! 2. [`MempoolRequest`]s sent to the [`Mempool`] service;
//! 3. [`SnapshotRequest`]s sent to the [`Snapshot`] service;
//! 4. [`InfoRequest`]s sent to the [`Info`] service.
//!
//! The ABCI service can execute these requests synchronously, in
//! [`Service::call`](tower::Service::call), or asynchronously, by immediately
//! returning a future that will be executed on the caller's task. Or, it can
//! split the difference and perform some amount of synchronous work and defer
//! the rest to be performed asynchronously.
//!
//! Because each category of requests is handled by a different service, request
//! behavior can be customized on a per-category basis using Tower
//! [`Layer`](tower::Layer)s. For instance, load-shedding can be added to
//! [`InfoRequest`]s but not [`ConsensusRequest`]s, or different categories can
//! have different timeout policies, or different types of instrumentation.

use std::task::{Context, Poll};

use tower::Service;

use crate::{buffer4::Buffer, BoxError};
use tendermint::v0_37::abci::{
    ConsensusRequest, ConsensusResponse, InfoRequest, InfoResponse, MempoolRequest,
    MempoolResponse, Request, Response, SnapshotRequest, SnapshotResponse,
};

/// Splits a single `service` implementing all of ABCI into four cloneable
/// component services, each implementing one category of ABCI requests. See the
/// module documentation for details.
///
/// The `bound` parameter bounds the size of each component's request queue. For
/// the same reason as in Tower's [`Buffer`](tower::buffer::Buffer) middleware,
/// it's advisable to set the `bound` to be at least the number of concurrent
/// requests to the component services. However, large buffers hide backpressure
/// from propagating to the caller.
pub fn service<S>(service: S, bound: usize) -> (Consensus<S>, Mempool<S>, Snapshot<S>, Info<S>)
where
    S: Service<Request, Response = Response, Error = BoxError> + Send + 'static,
    S::Future: Send + 'static,
{
    let bound = std::cmp::max(1, bound);
    let (buffer1, buffer2, buffer3, buffer4) = Buffer::new(service, bound);

    (
        Consensus { inner: buffer1 },
        Mempool { inner: buffer2 },
        Snapshot { inner: buffer3 },
        Info { inner: buffer4 },
    )
}

/// Forwards consensus requests to a shared backing service.
pub struct Consensus<S>
where
    S: Service<Request, Response = Response, Error = BoxError>,
{
    inner: Buffer<S, Request>,
}

// Implementing Clone manually avoids an (incorrect) derived S: Clone bound
impl<S> Clone for Consensus<S>
where
    S: Service<Request, Response = Response, Error = BoxError>,
{
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
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

/// Forwards mempool requests to a shared backing service.
pub struct Mempool<S>
where
    S: Service<Request, Response = Response, Error = BoxError>,
{
    inner: Buffer<S, Request>,
}

// Implementing Clone manually avoids an (incorrect) derived S: Clone bound
impl<S> Clone for Mempool<S>
where
    S: Service<Request, Response = Response, Error = BoxError>,
{
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
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

/// Forwards info requests to a shared backing service.
pub struct Info<S>
where
    S: Service<Request, Response = Response, Error = BoxError>,
{
    inner: Buffer<S, Request>,
}

// Implementing Clone manually avoids an (incorrect) derived S: Clone bound
impl<S> Clone for Info<S>
where
    S: Service<Request, Response = Response, Error = BoxError>,
{
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
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

/// Forwards snapshot requests to a shared backing service.
pub struct Snapshot<S>
where
    S: Service<Request, Response = Response, Error = BoxError>,
{
    inner: Buffer<S, Request>,
}

// Implementing Clone manually avoids an (incorrect) derived S: Clone bound
impl<S> Clone for Snapshot<S>
where
    S: Service<Request, Response = Response, Error = BoxError>,
{
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
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

/// Futures types.
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
