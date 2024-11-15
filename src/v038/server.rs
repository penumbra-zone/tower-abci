use std::convert::{TryFrom, TryInto};
use std::net::SocketAddr;

use futures::future::{FutureExt, TryFutureExt};
use futures::sink::SinkExt;
use futures::stream::{FuturesOrdered, StreamExt};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::{
    net::{TcpListener, ToSocketAddrs},
    select,
};
use tokio_util::codec::{FramedRead, FramedWrite};
use tower::{Service, ServiceExt};

use crate::BoxError;
use tendermint::abci::MethodKind;

use tendermint::v0_38::abci::{
    ConsensusRequest, ConsensusResponse, InfoRequest, InfoResponse, MempoolRequest,
    MempoolResponse, Request, Response, SnapshotRequest, SnapshotResponse,
};

/// An ABCI server which listens for connections and forwards requests to four
/// component ABCI [`Service`]s.
pub struct BoundTcpServer<C, M, I, S> {
    tcp_listener: TcpListener,
    addr: SocketAddr,
    consensus: C,
    mempool: M,
    info: I,
    snapshot: S,
}

impl<C, M, I, S> BoundTcpServer<C, M, I, S> {
    pub fn local_addr(&self) -> &SocketAddr {
        &self.addr
    }
}

impl<C, M, I, S> BoundTcpServer<C, M, I, S>
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
    pub async fn listen(self) -> Result<(), BoxError> {
        tracing::info!(?self.addr, "ABCI server listening on tcp socket");
        loop {
            match self.tcp_listener.accept().await {
                Ok((socket, _addr)) => {
                    tracing::debug!(?_addr, "accepted new connection");
                    let conn = Connection {
                        consensus: self.consensus.clone(),
                        mempool: self.mempool.clone(),
                        info: self.info.clone(),
                        snapshot: self.snapshot.clone(),
                    };
                    let (read, write) = socket.into_split();
                    tokio::spawn(async move { conn.run(read, write).await.unwrap() });
                }
                Err(e) => {
                    tracing::error!({ %e }, "error accepting new connection");
                }
            }
        }
    }
}

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

impl<C, M, I, S> Server<C, M, I, S> {
    pub async fn bind_tcp<A: ToSocketAddrs + std::fmt::Debug>(
        self,
        addr: A,
    ) -> Result<BoundTcpServer<C, M, I, S>, BoxError> {
        let tcp_listener = TcpListener::bind(addr).await?;
        let addr = tcp_listener.local_addr()?;
        let Self {
            consensus,
            mempool,
            info,
            snapshot,
        } = self;
        Ok(BoundTcpServer {
            tcp_listener,
            addr,
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
    pub fn builder() -> ServerBuilder<C, M, I, S> {
        ServerBuilder::default()
    }

    #[cfg(target_family = "unix")]
    pub async fn listen_unix(self, path: impl AsRef<std::path::Path>) -> Result<(), BoxError> {
        let listener = tokio::net::UnixListener::bind(path)?;
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
                    tokio::spawn(async move { conn.run(read, write).await.unwrap() });
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
        let server = self.bind_tcp(addr).await?;
        server.listen().await
    }
}

struct Connection<C, M, I, S> {
    consensus: C,
    mempool: M,
    info: I,
    snapshot: S,
}

impl<C, M, I, S> Connection<C, M, I, S>
where
    C: Service<ConsensusRequest, Response = ConsensusResponse, Error = BoxError> + Send + 'static,
    C::Future: Send + 'static,
    M: Service<MempoolRequest, Response = MempoolResponse, Error = BoxError> + Send + 'static,
    M::Future: Send + 'static,
    I: Service<InfoRequest, Response = InfoResponse, Error = BoxError> + Send + 'static,
    I::Future: Send + 'static,
    S: Service<SnapshotRequest, Response = SnapshotResponse, Error = BoxError> + Send + 'static,
    S::Future: Send + 'static,
{
    // XXX handle errors gracefully
    // figure out how / if to return errors to tendermint
    async fn run(
        mut self,
        read: impl AsyncReadExt + std::marker::Unpin,
        write: impl AsyncWriteExt + std::marker::Unpin,
    ) -> Result<(), BoxError> {
        tracing::info!("listening for requests");

        use tendermint_proto::v0_38::abci as pb;

        let (mut request_stream, mut response_sink) = {
            use crate::v038::codec::{Decode, Encode};
            (
                FramedRead::new(read, Decode::<pb::Request>::default()),
                FramedWrite::new(write, Encode::<pb::Response>::default()),
            )
        };

        let mut responses = FuturesOrdered::new();

        loop {
            select! {
                req = request_stream.next() => {
                    let proto = match req.transpose()? {
                        Some(proto) => proto,
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
                            // Now we need to tell Tendermint we've flushed responses
                            response_sink.send(Response::Flush.into()).await?;
                        }
                    }
                }
                rsp = responses.next(), if !responses.is_empty() => {
                    let response = rsp.expect("didn't poll when responses was empty");
                    // XXX: sometimes we might want to send errors to tendermint
                    // https://docs.tendermint.com/v0.32/spec/abci/abci.html#errors
                    tracing::debug!(?response, "sending response");
                    response_sink.send(response?.into()).await?;
                }
            }
        }
    }
}
