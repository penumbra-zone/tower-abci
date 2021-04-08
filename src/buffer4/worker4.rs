use super::{
    error::{Closed, ServiceError},
    message::Message,
};
use futures::{future::FutureExt, ready, stream::StreamExt};
use pin_project::pin_project;
use std::sync::{Arc, Mutex, Weak};
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::{
    select,
    sync::{mpsc, Semaphore},
};
use tokio_stream::wrappers::UnboundedReceiverStream;
use tower::{Service, ServiceExt};

pub struct Worker<T, Request>
where
    T: Service<Request>,
    T::Error: Into<crate::BoxError>,
{
    service: T,
    handle: Handle,
    close: Option<Weak<Semaphore>>,
    failed: Option<ServiceError>,

    rx1: Option<mpsc::UnboundedReceiver<Message<Request, T::Future>>>,
    rx2: Option<mpsc::UnboundedReceiver<Message<Request, T::Future>>>,
    rx3: Option<mpsc::UnboundedReceiver<Message<Request, T::Future>>>,
    rx4: Option<mpsc::UnboundedReceiver<Message<Request, T::Future>>>,
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
        rx2: mpsc::UnboundedReceiver<Message<Request, T::Future>>,
        rx3: mpsc::UnboundedReceiver<Message<Request, T::Future>>,
        rx4: mpsc::UnboundedReceiver<Message<Request, T::Future>>,
        semaphore: &Arc<Semaphore>,
    ) -> (Handle, Worker<T, Request>) {
        let handle = Handle {
            inner: Arc::new(Mutex::new(None)),
        };

        let close = Some(Arc::downgrade(semaphore));

        let worker = Worker {
            service,
            handle: handle.clone(),
            close,
            failed: None,
            rx1: Some(rx1),
            rx2: Some(rx2),
            rx3: Some(rx3),
            rx4: Some(rx4),
        };

        (handle, worker)
    }

    fn failed(&mut self, error: crate::BoxError) {
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
                let error = e.into();
                tracing::debug!({ %error }, "service failed");

                todo!()
            }
        }
    }

    pub(crate) async fn run(mut self) {
        loop {
            if let Some(ref failed) = self.failed {
                tracing::trace!("flushing pending requests after worker failure");
                // We've failed and closed all channels.
                // Now we flush any pending channel entries.
                flush_channel(failed, self.rx1).await;
                flush_channel(failed, self.rx2).await;
                flush_channel(failed, self.rx3).await;
                flush_channel(failed, self.rx4).await;

                return;
            }

            select! {
                // Using a biased select means the channels will be polled
                // in priority order, not in a random (fair) order.
                biased;
                msg = self.rx1.as_mut().unwrap().recv().fuse(), if self.rx1.is_some() => {
                    match msg {
                        Some(msg) => self.process(msg).await,
                        None => self.rx1 = None,
                    }
                }
                msg = self.rx2.as_mut().unwrap().recv().fuse(), if self.rx2.is_some() => {
                    match msg {
                        Some(msg) => self.process(msg).await,
                        None => self.rx2 = None,
                    }
                }
                msg = self.rx3.as_mut().unwrap().recv().fuse(), if self.rx2.is_some() => {
                    match msg {
                        Some(msg) => self.process(msg).await,
                        None => self.rx3 = None,
                    }
                }
                msg = self.rx4.as_mut().unwrap().recv().fuse(), if self.rx2.is_some() => {
                    match msg {
                        Some(msg) => self.process(msg).await,
                        None => self.rx4 = None,
                    }
                }
            };

            if self.rx1.is_none() && self.rx2.is_none() && self.rx3.is_none() && self.rx4.is_none()
            {
                tracing::trace!("all senders closed, shutting down");
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
