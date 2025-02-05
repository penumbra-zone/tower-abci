#![allow(clippy::doc_lazy_continuation)]
#![doc = include_str!("../README.md")]
/// A fork of tower::buffer @ `e1760d38` that has four queues feeding
/// the same worker task, with different priorities.
mod buffer4;

// #[cfg(feature = "v034")]
pub mod v034 {
    mod codec;
    mod server;
    pub mod split;
    pub use server::Server;
    pub use server::ServerBuilder;
}

// #[cfg(feature = "v037")]
pub mod v037 {
    mod codec;
    mod server;
    pub mod split;
    pub use server::Server;
    pub use server::ServerBuilder;
}

pub mod v038 {
    mod codec;
    mod server;
    pub mod split;
    pub use server::BoundTcpServer;
    pub use server::Server;
    pub use server::ServerBuilder;
}

/// A convenient error type alias.
pub type BoxError = Box<dyn std::error::Error + Send + Sync + 'static>;
