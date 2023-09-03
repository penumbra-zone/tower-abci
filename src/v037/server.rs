use std::convert::{TryFrom, TryInto};
use std::io;
use std::path::Path;

use futures::future::{FutureExt, TryFutureExt};
use futures::sink::SinkExt;
use futures::stream::{FuturesOrdered, StreamExt};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::{
    net::{TcpListener, TcpStream, ToSocketAddrs, UnixListener, UnixStream},
    select,
};
use tokio_util::codec::{FramedRead, FramedWrite};
use tower::{Service, ServiceExt};

use tendermint::abci::MethodKind;
use tracing::warn;

use crate::BoxError;

use tendermint::v0_37::abci::{
    ConsensusRequest, ConsensusResponse, InfoRequest, InfoResponse, MempoolRequest,
    MempoolResponse, Request, Response, SnapshotRequest, SnapshotResponse,
};

enum SocketKind {
    Tcp(TcpStream),
    Uds(UnixStream),
}

impl SocketKind {
    fn split<'a>(
        &'a mut self,
    ) -> (
        Box<dyn 'a + AsyncRead + Send + Unpin>,
        Box<dyn 'a + AsyncWrite + Send + Unpin>,
    ) {
        match self {
            Self::Tcp(tcp) => {
                let (read, write) = tcp.split();
                (Box::new(read), Box::new(write))
            }
            Self::Uds(uds) => {
                let (read, write) = uds.split();
                (Box::new(read), Box::new(write))
            }
        }
    }
}

impl From<TcpStream> for SocketKind {
    fn from(value: TcpStream) -> Self {
        Self::Tcp(value)
    }
}

impl From<UnixStream> for SocketKind {
    fn from(value: UnixStream) -> Self {
        Self::Uds(value)
    }
}

/// An ABCI server which listens for connections and forwards requests to four
/// component ABCI [`Service`]s.
pub struct Server<C, M, I, S> {
    consensus: C,
    mempool: M,
    info: I,
    snapshot: S,
    tcp_listener: Option<TcpListener>,
    uds_listener: Option<UnixListener>,
}

pub struct ServerBuilder<C, M, I, S> {
    consensus: Option<C>,
    mempool: Option<M>,
    info: Option<I>,
    snapshot: Option<S>,
    tcp_listener: Option<TcpListener>,
    uds_listener: Option<UnixListener>,
}

impl<C, M, I, S> Default for ServerBuilder<C, M, I, S> {
    fn default() -> Self {
        Self {
            consensus: None,
            mempool: None,
            info: None,
            snapshot: None,
            tcp_listener: None,
            uds_listener: None,
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

    pub fn tcp_listener(mut self, tcp_listener: TcpListener) -> Self {
        self.tcp_listener = Some(tcp_listener);
        self
    }

    pub async fn with_tcp_addr(mut self, addr: impl ToSocketAddrs) -> io::Result<Self> {
        self.tcp_listener = Some(TcpListener::bind(addr).await?);
        Ok(self)
    }

    pub fn uds_listener(mut self, uds_listener: UnixListener) -> Self {
        self.uds_listener = Some(uds_listener);
        self
    }

    pub fn with_uds_path(mut self, path: impl AsRef<Path>) -> io::Result<Self> {
        self.uds_listener = Some(UnixListener::bind(path)?);
        Ok(self)
    }

    pub fn finish(self) -> Option<Server<C, M, I, S>> {
        let consensus = self.consensus?;
        let mempool = self.mempool?;
        let info = self.info?;
        let snapshot = self.snapshot?;
        let tcp_listener = self.tcp_listener;
        let uds_listener = self.uds_listener;
        if tcp_listener.is_none() && uds_listener.is_none() {
            return None;
        }

        Some(Server {
            consensus,
            mempool,
            info,
            snapshot,
            tcp_listener,
            uds_listener,
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

    fn handle_new_connection(&self, socket: impl Into<SocketKind>) {
        let socket = socket.into();
        // set parent: None for the connection span, as it should
        // exist independently of the listener's spans.
        //let span = tracing::span!(parent: None, Level::ERROR, "abci", ?addr);
        let conn = Connection {
            consensus: self.consensus.clone(),
            mempool: self.mempool.clone(),
            info: self.info.clone(),
            snapshot: self.snapshot.clone(),
        };
        //tokio::spawn(async move { conn.run(socket).await.unwrap() }.instrument(span));
        tokio::spawn(async move { conn.run(socket).await.unwrap() });
    }

    pub async fn start(self) -> Result<(), BoxError> {
        tracing::info!("starting ABCI server");

        let tcp_addr = self
            .tcp_listener
            .as_ref()
            .map(TcpListener::local_addr)
            .transpose()
            .map_err(|e| {
                warn!(err.msg = %e, err.cause_chain = ?e, "failed getting tcp local addr");
                e
            })
            .ok()
            .flatten();
        let uds_addr = self
            .uds_listener
            .as_ref()
            .map(UnixListener::local_addr)
            .transpose()
            .map_err(|e| {
                warn!(err.msg = %e, err.cause_chain = ?e, "failed getting uds local addr");
                e
            })
            .ok()
            .flatten();

        tracing::info!(
            addr.tcp = tcp_addr.map(debug),
            addr.uds = uds_addr.map(debug),
            "listening on local addresses"
        );

        loop {
            select!(
                tcp = async { self.tcp_listener.as_ref().unwrap().accept().await }, if self.tcp_listener.is_some() => {
                    match tcp {
                        Ok((socket, _addr)) => {
                            self.handle_new_connection(socket);
                        }
                        Err(e) => {
                            tracing::warn!({ %e }, "error accepting new tcp connection");
                        }
                    }
                }

                uds = async { self.uds_listener.as_ref().unwrap().accept().await }, if self.uds_listener.is_some() => {
                    match uds {
                        Ok((socket, _addr)) => {
                            self.handle_new_connection(socket);
                        }
                        Err(e) => {
                            tracing::warn!({ %e }, "error accepting new uds connection");
                        }
                    }
                }
            )
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
    // XXX handle errors gracefully
    // figure out how / if to return errors to tendermint
    async fn run(mut self, mut socket: SocketKind) -> Result<(), BoxError> {
        use tendermint_proto::v0_37::abci as pb;

        match &socket {
            SocketKind::Tcp(tcp) => {
                tracing::info!(addr = ?tcp.local_addr().ok(), "listening for tcp requests");
            }
            SocketKind::Uds(uds) => {
                tracing::info!(addr = ?uds.local_addr().ok(), "listening for uds requests");
            }
        }

        let (mut request_stream, mut response_sink) = {
            use crate::v037::codec::{Decode, Encode};
            let (read, write) = SocketKind::split(&mut socket);
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
