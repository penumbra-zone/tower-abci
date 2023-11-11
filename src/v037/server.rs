use std::convert::{TryFrom, TryInto};

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

#[cfg(target_family = "unix")]
use std::path::Path;
#[cfg(target_family = "unix")]
use tokio::net::UnixListener;

use tendermint::v0_37::abci::response::Exception;
use tendermint::v0_37::abci::{
    ConsensusRequest, ConsensusResponse, InfoRequest, InfoResponse, MempoolRequest,
    MempoolResponse, Request, Response, SnapshotRequest, SnapshotResponse,
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
                    tokio::spawn(async move { conn.run(read, write).await.unwrap() });
                }
                Err(e) => {
                    tracing::error!({ %e }, "error accepting new connection");
                }
            }
        }
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
    async fn run(
        mut self,
        read: impl AsyncReadExt + std::marker::Unpin,
        write: impl AsyncWriteExt + std::marker::Unpin,
    ) -> Result<(), BoxError> {
        tracing::info!("listening for requests");

        use tendermint_proto::v0_37::abci as pb;

        let (mut request_stream, mut response_sink) = {
            use crate::v037::codec::{Decode, Encode};
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
                    let kind = request.kind();
                    match &kind {
                        MethodKind::Consensus => {
                            let request = request.try_into().expect("checked kind");
                            let response = self.consensus.ready().await?.call(request);
                            // Need to box here for type erasure
                            responses.push_back(response.map_ok(Response::from).map(|r| (r, kind)).boxed());
                        }
                        MethodKind::Mempool => {
                            let request = request.try_into().expect("checked kind");
                            let response = self.mempool.ready().await?.call(request);
                            responses.push_back(response.map_ok(Response::from).map(|r| (r, kind)).boxed());
                        }
                        MethodKind::Snapshot => {
                            let request = request.try_into().expect("checked kind");
                            let response = self.snapshot.ready().await?.call(request);
                            responses.push_back(response.map_ok(Response::from).map(|r| (r, kind)).boxed());
                        }
                        MethodKind::Info => {
                            let request = request.try_into().expect("checked kind");
                            let response = self.info.ready().await?.call(request);
                            responses.push_back(response.map_ok(Response::from).map(|r| (r, kind)).boxed());
                        }
                        MethodKind::Flush => {
                            // Instead of propagating Flush requests to the application,
                            // handle them here by awaiting all pending responses.
                            tracing::debug!(responses.len = responses.len(), "flushing responses");
                            while let Some((response, kind)) = responses.next().await {
                                let response = match response {
                                    Ok(rsp) => rsp,
                                    Err(err) => match kind {
                                        // TODO: allow to fail on Snapshot?
                                        MethodKind::Info =>
                                            Response::Exception(Exception{error:err.to_string()}),
                                        _ => return Err(err)
                                    }
                                };
                                tracing::debug!(?response, "flushing response");
                                response_sink.send(response.into()).await?;
                            }
                            // Now we need to tell Tendermint we've flushed responses
                            response_sink.send(Response::Flush.into()).await?;
                        }
                    }
                }
                rsp = responses.next(), if !responses.is_empty() => {
                    let (rsp, kind) = rsp.expect("didn't poll when responses was empty");
                    let response = match rsp {
                        Ok(rsp) => rsp,
                        Err(err) => match kind {
                            // TODO: allow to fail on Snapshot?
                            MethodKind::Info =>
                                Response::Exception(Exception{error:err.to_string()}),
                            _ => return Err(err)
                        }
                    };
                    tracing::debug!(?response, "sending response");
                    response_sink.send(response.into()).await?;
                }
            }
        }
    }
}
