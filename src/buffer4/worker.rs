use super::{
    error::{Closed, ServiceError},
    message::Message,
};
use futures::stream::StreamExt;
use std::future::Future;
use std::sync::{Arc, Mutex, Weak};
use tokio::{
    select,
    sync::{mpsc, Semaphore},
};
use tokio_stream::wrappers::UnboundedReceiverStream;
use tower::{Service, ServiceExt};
use tracing::Instrument;

pub struct Worker<T, Request>
where
    T: Service<Request>,
    T::Error: Into<crate::BoxError>,
{
    service: T,
    handle: Handle,
    failed: Option<ServiceError>,

    rx1: Option<mpsc::UnboundedReceiver<Message<Request, T::Future>>>,
    rx2: Option<mpsc::UnboundedReceiver<Message<Request, T::Future>>>,
    rx3: Option<mpsc::UnboundedReceiver<Message<Request, T::Future>>>,
    rx4: Option<mpsc::UnboundedReceiver<Message<Request, T::Future>>>,

    close1: Option<Weak<Semaphore>>,
    close2: Option<Weak<Semaphore>>,
    close3: Option<Weak<Semaphore>>,
    close4: Option<Weak<Semaphore>>,
}

/// Get the error out
#[derive(Debug, Clone)]
pub(crate) struct Handle {
    inner: Arc<Mutex<Option<ServiceError>>>,
}

impl<T, Request> Worker<T, Request>
where
    T: Service<Request>,
    T::Error: Into<crate::BoxError>,
{
    pub(crate) fn new(
        service: T,
        rx1: mpsc::UnboundedReceiver<Message<Request, T::Future>>,
        semaphore1: &Arc<Semaphore>,
        rx2: mpsc::UnboundedReceiver<Message<Request, T::Future>>,
        semaphore2: &Arc<Semaphore>,
        rx3: mpsc::UnboundedReceiver<Message<Request, T::Future>>,
        semaphore3: &Arc<Semaphore>,
        rx4: mpsc::UnboundedReceiver<Message<Request, T::Future>>,
        semaphore4: &Arc<Semaphore>,
    ) -> (Handle, Worker<T, Request>) {
        let handle = Handle {
            inner: Arc::new(Mutex::new(None)),
        };

        let close1 = Some(Arc::downgrade(semaphore1));
        let close2 = Some(Arc::downgrade(semaphore2));
        let close3 = Some(Arc::downgrade(semaphore3));
        let close4 = Some(Arc::downgrade(semaphore4));

        let worker = Worker {
            service,
            handle: handle.clone(),
            failed: None,
            rx1: Some(rx1),
            rx2: Some(rx2),
            rx3: Some(rx3),
            rx4: Some(rx4),
            close1,
            close2,
            close3,
            close4,
        };

        (handle, worker)
    }

    fn shutdown(&mut self) {
        if let Some(close1) = self.close1.take().as_ref().and_then(Weak::upgrade) {
            tracing::debug!("buffer 1 closing; waking pending tasks");
            close1.close();
        }
        if let Some(close2) = self.close2.take().as_ref().and_then(Weak::upgrade) {
            tracing::debug!("buffer 2 closing; waking pending tasks");
            close2.close();
        }
        if let Some(close3) = self.close3.take().as_ref().and_then(Weak::upgrade) {
            tracing::debug!("buffer 3 closing; waking pending tasks");
            close3.close();
        }
        if let Some(close4) = self.close4.take().as_ref().and_then(Weak::upgrade) {
            tracing::debug!("buffer 4 closing; waking pending tasks");
            close4.close();
        }
    }

    fn failed(&mut self, error: crate::BoxError) {
        tracing::debug!({ %error }, "service failed");
        let error = ServiceError::new(error);
        let mut inner = self.handle.inner.lock().unwrap();

        if inner.is_some() {
            unreachable!("cannot fail twice");
        }

        *inner = Some(error.clone());
        drop(inner);
        self.rx1.as_mut().map(|chan| chan.close());
        self.rx2.as_mut().map(|chan| chan.close());
        self.rx3.as_mut().map(|chan| chan.close());
        self.rx4.as_mut().map(|chan| chan.close());

        self.failed = Some(error);
    }

    async fn process(&mut self, msg: Message<Request, T::Future>) {
        match self.service.ready().await {
            Ok(svc) => {
                tracing::trace!("dispatching request to service");
                let response = svc.call(msg.request);
                tracing::trace!("returning response future");
                let _ = msg.tx.send(Ok(response));
            }
            Err(e) => {
                self.failed(e.into());
                let error = self.failed.as_ref().expect("just set error").clone();
                let _ = msg.tx.send(Err(error));
            }
        }
    }

    pub(crate) async fn run(mut self) {
        loop {
            if let Some(ref failed) = self.failed {
                tracing::trace!("flushing pending requests after worker failure");
                // We've failed and closed all channels.
                // Now we flush any pending channel entries.
                flush_channel(failed, self.rx1.take()).await;
                flush_channel(failed, self.rx2.take()).await;
                flush_channel(failed, self.rx3.take()).await;
                flush_channel(failed, self.rx4.take()).await;

                self.shutdown();
                return;
            }

            select! {
                // Using a biased select means the channels will be polled
                // in priority order, not in a random (fair) order.
                biased;
                msg = option_and_then(self.rx1.as_mut(), |rx| rx.recv()) => {
                    match msg {
                        Some(msg) => {
                            let span = msg.span.clone();
                            self.process(msg).instrument(span).await
                        }
                        None => self.rx1 = None,
                    }
                }
                msg = option_and_then(self.rx2.as_mut(), |rx| rx.recv()) => {
                    match msg {
                        Some(msg) => {
                            let span = msg.span.clone();
                            self.process(msg).instrument(span).await
                        }
                        None => self.rx2 = None,
                    }
                }
                msg = option_and_then(self.rx3.as_mut(), |rx| rx.recv()) => {
                    match msg {
                        Some(msg) => {
                            let span = msg.span.clone();
                            self.process(msg).instrument(span).await
                        }
                        None => self.rx3 = None,
                    }
                }
                msg = option_and_then(self.rx4.as_mut(), |rx| rx.recv()) => {
                    match msg {
                        Some(msg) => {
                            let span = msg.span.clone();
                            self.process(msg).instrument(span).await
                        }
                        None => self.rx4 = None,
                    }
                }
            };

            if self.rx1.is_none() && self.rx2.is_none() && self.rx3.is_none() && self.rx4.is_none()
            {
                tracing::trace!("all senders closed, shutting down");
                self.shutdown();
                return;
            }
        }
    }
}

async fn flush_channel<T, F>(
    failed: &ServiceError,
    rx: Option<mpsc::UnboundedReceiver<Message<T, F>>>,
) {
    if let Some(chan) = rx {
        let mut s = UnboundedReceiverStream::new(chan);
        while let Some(msg) = s.next().await {
            let _guard = msg.span.enter();
            tracing::trace!("notifying caller about worker failure");
            let _ = msg.tx.send(Err(failed.clone()));
        }
    }
}

impl Handle {
    pub(crate) fn get_error_on_closed(&self) -> crate::BoxError {
        self.inner
            .lock()
            .unwrap()
            .as_ref()
            .map(|svc_err| svc_err.clone().into())
            .unwrap_or_else(|| Closed::new().into())
    }
}

async fn option_and_then<T, U, F, Fut>(x: Option<T>, f: F) -> Option<U>
where
    F: FnOnce(T) -> Fut,
    Fut: Future<Output = Option<U>>,
{
    f(x?).await
}
