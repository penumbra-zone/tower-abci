An interface for [ABCI] built on [Tower]'s [`Service`][svc] abstraction.

[ABCI] is the interface between Tendermint (a consensus engine for BFT
replication of a state machine), and an arbitrary application (the state
machine to be replicated). The ABCI interface consists of a set of requests
and responses the consensus engine makes to drive the application state.
[Tower] is a library of modular components for building networking clients
and servers. Tower defines a core abstraction, the [`Service`][svc] trait,
which represents an asynchronous function with backpressure, and then
provides combinators that allow generic composition of additional behavior,
e.g., timeouts, buffering, load-shedding, rate-limiting, instrumentation,
etc.

This crate uses Tower to define an asynchronous ABCI interface.  It has two parts:

1. An ABCI server, which listens for connections and forwards ABCI requests to one of four user-provided [`Service`][svc]s, each responsible for processing one category of requests (consensus, mempool, info, or snapshot).

2. Middleware that splits a single [`Service`][svc] implementing all of ABCI into four cloneable component services, each implementing one category of requests. The component services use message-passing to share access to the main service, which processes requests with the following category-based prioritization:
    1. `ConsensusRequest`s sent to the `Consensus` service;
    2. `MempoolRequest`s sent to the `Mempool` service;
    3. `SnapshotRequest`s sent to the `Snapshot` service;
    4. `InfoRequest`s sent to the `Info` service.

Because the ABCI server takes one service per category, users can apply Tower
layers to the services they pass to the ABCI `Server` to add
category-specific behavior, such as load-shedding, buffering, etc.

These parts can be combined in different ways to provide different points on
the tradeoff curve between implementation complexity and performance:

1. At the lowest level of complexity, application developers can implement an ABCI application entirely synchronously. To do this, they implement `Service<Request>` so that `Service::call` performs request processing and returns a ready future. Then they use `split::service` to create four component services that share access to their application, and use those to construct the ABCI `Server`. The application developer does not need to manage synchronization of shared state between different clones of their application, because there is only one copy of their application.

2. At the next level of complexity, application developers can implement an ABCI application partially synchronously. As before, they implement `Service<Request>` to create a single ABCI application, but instead of processing all requests in the body of `Service::call`, they can defer processing of some requests by immediately returning a future that will be executed on the caller's task. Although all requests are still received by the application task, not all request processing needs to happen on the application task. At this level the developer must pay closer attention to utilising Tower layers to control the concurrency of the individual services mentioned above. In particular the `Consensus` service should be wrapped with `ServiceBuilder::concurrency_limit` of 1 to avoid a potential reordering of consensus message effects caused by concurrent execution, as well as `ServiceBuilder::buffer` to avoid any deadlocks in message handling in `Connection` due to the limited concurrency.

3. At the highest level of complexity, application developers can implement multiple distinct `Service`s and manually control synchronization of shared state between them, then use these to construct the ABCI `Server`.

Because these use the same interfaces in different ways, application
developers can move gradually along this curve according to their performance
requirements, starting with a synchronous application, then refactoring it to
do some processing asynchronously, then doing more processing asynchronously,
then splitting out one standalone service, then using entirely distinct
services, etc.

[ABCI]: https://docs.tendermint.com/master/spec/abci/
[Tower]: https://docs.rs/tower
[svc]: https://docs.rs/tower/0.4.6/tower/trait.Service.html
