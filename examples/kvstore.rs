use std::unimplemented;

use tower::{Service, ServiceBuilder};

use tower_abci::{BoxError, Request, Response};

#[derive(Default, Clone, Debug)]
pub struct KVStore {
    // stuff
}

impl Service<Request> for KVStore {
    type Response = Response;
    type Error = BoxError;
    type Future = Pin<Box<dyn Future<Output = Result<Response, BoxError>> + Send + 'static>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: Request) -> Self::Future {
        match req {
            Request::Echo(_) => {
                todo!()
            }
            _ => todo!(),
        }

        unimplemented!()
    }
}

#[tokio::main]
async fn main() {
    let (consensus, mempool, info, snapshot) = tower_abci::service::split(KVStore::default());

    tokio::spawn(
        Server::builder()
            .with_consensus(consensus)
            .with_snapshot(snapshot)
            .with_mempool(
                ServiceBuilder::new()
                    .load_shed()
                    .buffer(10)
                    .service(mempool),
            )
            .with_info(ServiceBuilder::new().load_shed().buffer(100).service(info))
            .finish()
            .unwrap()
            .run(),
    )
}
