use std::convert::{TryFrom, TryInto};
use std::pin::Pin;
use std::sync::Arc;

use backoff::ExponentialBackoff;
use futures::future::{FutureExt, TryFutureExt};
use futures::sink::SinkExt;
use futures::stream::{FuturesOrdered, Peekable, StreamExt};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::Mutex;
use tokio::{
    net::{TcpListener, ToSocketAddrs},
    select,
};

use tokio_util::codec::{FramedRead, FramedWrite};
use tower::{Service, ServiceExt};

use crate::BoxError;
use tendermint::abci::MethodKind;

#[cfg(target_family = "unix")]
use std::path::Path;
#[cfg(target_family = "unix")]
use tokio::net::UnixListener;

use crate::v037::codec::{Decode, Encode};
use tendermint::v0_37::abci::{
    ConsensusRequest, ConsensusResponse, InfoRequest, InfoResponse, MempoolRequest,
    MempoolResponse, Request, Response, SnapshotRequest, SnapshotResponse,
};
use tendermint_proto::v0_37::abci as pb;

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
    C: Service<ConsensusRequest, Response = ConsensusResponse, Error = BoxError>
        + Send
        + Clone
        + 'static,
    C::Future: Send + 'static,
    M: Service<MempoolRequest, Response = MempoolResponse, Error = BoxError>
        + Send
        + Clone
        + 'static,
    M::Future: Send + 'static,
    I: Service<InfoRequest, Response = InfoResponse, Error = BoxError> + Send + Clone + 'static,
    I::Future: Send + 'static,
    S: Service<SnapshotRequest, Response = SnapshotResponse, Error = BoxError>
        + Send
        + Clone
        + 'static,
    S::Future: Send + 'static,
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
        let consensus = self.consensus?;
        let mempool = self.mempool?;
        let info = self.info?;
        let snapshot = self.snapshot?;

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
    C: Service<ConsensusRequest, Response = ConsensusResponse, Error = BoxError>
        + Send
        + Sync
        + Clone
        + 'static,
    C::Future: Send + 'static,
    M: Service<MempoolRequest, Response = MempoolResponse, Error = BoxError>
        + Send
        + Sync
        + Clone
        + 'static,
    M::Future: Send + 'static,
    I: Service<InfoRequest, Response = InfoResponse, Error = BoxError>
        + Send
        + Sync
        + Clone
        + 'static,
    I::Future: Send + 'static,
    S: Service<SnapshotRequest, Response = SnapshotResponse, Error = BoxError>
        + Send
        + Sync
        + Clone
        + 'static,
    S::Future: Send + 'static,
{
    pub fn builder() -> ServerBuilder<C, M, I, S> {
        ServerBuilder::default()
    }

    #[cfg(target_family = "unix")]
    pub async fn listen_unix(self, path: impl AsRef<Path>) -> Result<(), BoxError> {
        let listener = UnixListener::bind(path)?;
        let addr = listener.local_addr()?;
        tracing::info!(?addr, "ABCI server starting on uds");

        loop {
            match listener.accept().await {
                Ok((socket, _addr)) => {
                    tracing::debug!(?_addr, "accepted new connection");
                    let conn = Connection {
                        consensus: self.consensus.clone(),
                        mempool: self.mempool.clone(),
                        info: self.info.clone(),
                        snapshot: self.snapshot.clone(),
                    };
                    let (read, write) = socket.into_split();
                    tokio::spawn(async move { conn.run_with_backoff(read, write).await.unwrap() });
                }
                Err(e) => {
                    tracing::error!({ %e }, "error accepting new connection");
                }
            }
        }
    }

    pub async fn listen_tcp<A: ToSocketAddrs + std::fmt::Debug>(
        self,
        addr: A,
    ) -> Result<(), BoxError> {
        let listener = TcpListener::bind(addr).await?;
        let addr = listener.local_addr()?;
        tracing::info!(?addr, "ABCI server starting on tcp socket");

        loop {
            match listener.accept().await {
                Ok((socket, _addr)) => {
                    tracing::debug!(?_addr, "accepted new connection");
                    let conn = Connection {
                        consensus: self.consensus.clone(),
                        mempool: self.mempool.clone(),
                        info: self.info.clone(),
                        snapshot: self.snapshot.clone(),
                    };
                    let (read, write) = socket.into_split();
                    tokio::spawn(async move { conn.run_with_backoff(read, write).await.unwrap() });
                }
                Err(e) => {
                    tracing::error!({ %e }, "error accepting new connection");
                }
            }
        }
    }
}

#[derive(Clone)]
struct Connection<C, M, I, S> {
    consensus: C,
    mempool: M,
    info: I,
    snapshot: S,
}

type StreamAndSink<R, W> = (
    Peekable<FramedRead<R, Decode<pb::Request>>>,
    FramedWrite<W, Encode<pb::Response>>,
);

impl<C, M, I, S> Connection<C, M, I, S>
where
    C: Service<ConsensusRequest, Response = ConsensusResponse, Error = BoxError>
        + Clone
        + Send
        + 'static,
    C::Future: Send + 'static,
    M: Service<MempoolRequest, Response = MempoolResponse, Error = BoxError>
        + Clone
        + Send
        + 'static,
    C: Service<ConsensusRequest, Response = ConsensusResponse, Error = BoxError>
        + Clone
        + Send
        + 'static,
    M::Future: Send + 'static,
    I: Service<InfoRequest, Response = InfoResponse, Error = BoxError> + Clone + Send + 'static,
    I::Future: Send + 'static,
    S: Service<SnapshotRequest, Response = SnapshotResponse, Error = BoxError>
        + Clone
        + Send
        + 'static,
    S::Future: Send + 'static,
{
    async fn run_with_backoff(
        self,
        read: impl AsyncReadExt + std::marker::Unpin,
        write: impl AsyncWriteExt + std::marker::Unpin,
    ) -> Result<(), BoxError> {
        let (request_stream, response_sink) = {
            (
                FramedRead::new(read, Decode::<pb::Request>::default()).peekable(),
                FramedWrite::new(write, Encode::<pb::Response>::default()),
            )
        };

        let stream_and_sink = Arc::new(Mutex::new((request_stream, response_sink)));
        backoff::future::retry::<_, BoxError, _, _, _>(ExponentialBackoff::default(), || async {
            let mut stream_and_sink = stream_and_sink.lock().await;
            let run_result = self.clone().run(&mut stream_and_sink).await;

            if let Err(e) = run_result {
                match e.downcast::<tower::load_shed::error::Overloaded>() {
                    Err(e) => {
                        tracing::error!("error {e} in a connection handler");
                        return Err(backoff::Error::Permanent(e));
                    }
                    Ok(e) => {
                        tracing::warn!("a service is overloaded - backing off");
                        return Err(backoff::Error::transient(e));
                    }
                }
            }
            Ok(())
        })
        .await
    }

    async fn run(
        mut self,
        stream_and_sink: &mut StreamAndSink<
            impl AsyncReadExt + std::marker::Unpin,
            impl AsyncWriteExt + std::marker::Unpin,
        >,
    ) -> Result<(), BoxError> {
        tracing::info!("listening for requests");

        let (request_stream, response_sink) = stream_and_sink;
        let mut pinned_stream = Pin::new(request_stream);
        let mut responses = FuturesOrdered::new();

        // We only peek the next request once it's popped from the request_stream
        // to avoid crashing Tendermint in case the service call fails because
        // it's e.g. overloaded.
        let mut peeked_req = false;
        loop {
            select! {
                req = pinned_stream.as_mut().peek(), if !peeked_req => {
                    peeked_req = true;
                    let proto = match req {
                        Some(Ok(proto)) => proto.clone(),
                        Some(Err(_)) => return Err(pinned_stream.next().await.unwrap().unwrap_err()),
                        None => return Ok(()),
                    };
                    let request = Request::try_from(proto)?;
                    tracing::debug!(?request, "new request");
                    match request.kind() {
                        MethodKind::Consensus => {
                            let request = request.try_into().expect("checked kind");
                            let response = self.consensus.ready().await?.call(request);
                            // Need to box here for type erasure
                            responses.push_back(response.map_ok(Response::from).boxed());
                        }
                        MethodKind::Mempool => {
                            let request = request.try_into().expect("checked kind");
                            let response = self.mempool.ready().await?.call(request);
                            responses.push_back(response.map_ok(Response::from).boxed());
                        }
                        MethodKind::Snapshot => {
                            let request = request.try_into().expect("checked kind");
                            let response = self.snapshot.ready().await?.call(request);
                            responses.push_back(response.map_ok(Response::from).boxed());
                        }
                        MethodKind::Info => {
                            let request = request.try_into().expect("checked kind");
                            let response = self.info.ready().await?.call(request);
                            responses.push_back(response.map_ok(Response::from).boxed());
                        }
                        MethodKind::Flush => {
                            // Instead of propagating Flush requests to the application,
                            // handle them here by awaiting all pending responses.
                            tracing::debug!(responses.len = responses.len(), "flushing responses");
                            while let Some(response) = responses.next().await {
                                // XXX: sometimes we might want to send errors to tendermint
                                // https://docs.tendermint.com/v0.32/spec/abci/abci.html#errors
                                tracing::debug!(?response, "flushing response");
                                response_sink.send(response?.into()).await?;
                            }
                            // Allow to peek next request if none of the `response?` above failed ...
                            peeked_req = false;
                            // ... and pop the last peeked request
                            pinned_stream.next().await.unwrap()?;
                            // Now we need to tell Tendermint we've flushed responses
                            response_sink.send(Response::Flush.into()).await?;
                        }
                    }
                }
                rsp = responses.next(), if !responses.is_empty() => {
                    let response = rsp.expect("didn't poll when responses was empty")?;
                    // Allow to peek next request if the `?` above didn't fail ...
                    peeked_req = false;
                    // ... and pop the last peeked request
                    pinned_stream.next().await.unwrap()?;
                    // XXX: sometimes we might want to send errors to tendermint
                    // https://docs.tendermint.com/v0.32/spec/abci/abci.html#errors
                    tracing::debug!(?response, "sending response");
                    response_sink.send(response.into()).await?;
                }
            }
        }
    }
}
