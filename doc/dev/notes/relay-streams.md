# Relay stream handling

Relays need the ability to accept

  * exit streams (`BEGIN`)
  * directory streams (`BEGIN_DIR`)
  * DNS streams (`RESOLVE`)

The first two (`BEGIN` and `BEGIN_DIR`) are data streams,
and will be implemented using the existing `DataStream` APIs.

DNS streams are a bit different, because the "stream" is immediately closed
after the `RESOLVED` response. This will require a new `IncomingStream` API
(see below).

All of these stream types will be modeled as `tor_proto::IncomingStream`s.


## Background/historical note

> Note: none of this is new; it's the same design we used
> on the onion service side, ported to the new multi-circuit-reactor system

The relay implementation will reuse much
of the existing `tor-proto` incoming stream handling
code that we originally added for the onion service implementation
(in order to reduce code duplication and to avoid reinventing the wheel).
As such, at the `tor-proto` level, the incoming stream requests
will be handled by an `IncomingStreamRequestHandler`,
within the `StreamReactor`.

> Note: An "incoming stream request" is a inbound message asking
> us to open a new stream.
> So `BEGIN`, `BEGIN_DIR`, and `RESOLVE` messages
> coming from the client are referred to as "incoming stream requests"

As with onion services, for each incoming stream request,
we validate the request at two different levels:

1. At the level of `tor-proto`, where we run some basic, quick checks
to see if we can reject the stream right away:

  * an `dyn IncomingStreamRequestFilter`
    quickly rejects any requests that are obviously no good
    (for example, the onion service `IncomingStreamRequestFilter`
    checks the current number of open streams on the circuit,
    and rejects the incoming stream request if it would cause the circuit
    to exceed the max allowed number of concurrent streams).
    `IncomingStreamRequestFilter` returns an `IncomingStreamRequestDisposition`,
    which instructs the reactor to either accept the stream request, or to reject it,
    either by closing the circuit (`CloseCircuit`),
    or by sending a given `END` cell (`RejectRequest(End)`)
  * a `CmdChecker` (`IncomingCmdChecker` in `tor-proto`), which checks if the message
    is one of the allowed stream-opening messages for the circuit.
    If it is not, its `CmdChecker::check_msg()` function will return a
    `StreamProto` error, causing the circuit to be torn down.
  * if the stream request arrived on a hop
    that is not expecting stream requests, it will be rejected immediately
    (this is irrelevant for relays, because they only have a single hop,
    namely themselves), and the circuit will be closed with a protocol violation
    error

If the stream request passes these checks,
the reactor will create a new stream entry in its stream map
(but note that the reactor doesn't actually respond to the
BEGIN/BEGIN_DIR/RESOLVE message,
it just initializes the stream entry).
This "stream request" information will be put in an `IncomingStream`,
which is then sent to an *external* incoming stream handler,
implemented outside of `tor-proto` (see below).

2. Outside of `tor-proto`, in the crate where the stream request is actually handled
(i.e. the actual application stream will be opened, and data will be forwarded
between it and the tor stream).
For onion services, this is done in `tor-hsrproxy`.
We will need something similar for exits, though the implementation
can probably live in `arti-relay`, and it will likely be much simpler
because we're not going to expose a public API for it.

As with onion services, the relay stream handler will get
a `futures::Stream` of `IncomingStream`s for each circuit,
and will call `accept_data()`, `reject()` or `discard()` on each of them,
to accept, reject, or ignore (i.e. discard without responding) the stream request.

-----

More concretely, a relay will need to run the following
(non-exhaustive) list of checks:

1. The exit policy

This will be implemented in the `IncomingStreamHandler` in `arti-relay`
(see `IncomingStreamHandler::handle_begin()` and
`IncomingStreamHandler::handle_resolve()` below).

This check applies to exit and DNS streams. For DNS streams, we don't look
at the full exit policy; we only check that we are indeed an exit relay.

2. Network re-entry: by default, relays don't allow connecting back into the
network, but this can be controlled with the `allow-network-reentry`
consensus param.

This needs to happen after hostname resolution,
so it will need to be handled in `IncomingStreamHandler::handle_begin()`
(note that the `IncomingStreamHandler::handle_begin()` implementation sketch
doesn't cover this, but it will need to be implemented).

The `IncomingStreamHandler` will check if the target address
is one of the relays in the consensus, and reject the stream request if it is
and the `allow-network-reentry` consensus param is not set.

This check only applies to exit streams.

3. Single-hop exit streams: these are always disallowed for exit streams.
We will, however, allow single-hop BEGIN_DIR and RESOLVE streams.

C Tor uses two checks for this: `channel_is_client()` and
`connection_or_digest_is_known_relay()`. AFAICT, C Tor sets
the `is_client` flag used in the `channel_is_client()` check using
`channel_mark_client()`, which sets the flag if the channel is not
authenticated.

We should do the same in Arti: the previous hop is a client (or a bridge!)
if either of the following is true:

  * the channel is not authenticated
  * the relay is not in the consensus (we will need something similar to C
    Tor's `connection_or_digest_is_known_relay`)

For the first check, we can look at the `ChannelType` of the channel.
For the second, we'll need to look up the address of the previous
hop in the consensus.

Both of these need access to the internals of the circuit
(the previous hop's `ChannelType` and address),
so we can implement one or both of these in one of two places:

   * in the stream reactor's `handle_incoming_stream_request()`
   * in an `IncomingStreamRequestFilter`, implemented in `arti-relay`
   * in the `IncomingStreamRequestHandler` from `arti-relay`

The stream reactor feels like the wrong place to implement this,
because this check is specific to `BEGIN` streams,
and the stream reactor generally only performs general, implementation-independent checks.
The implementation-specific checks are generally handled by the
`IncomingStreamRequestFilter`, or by the `IncomingStream` handler
outside of `tor-proto` (the one in `arti-relay`, in our case).

The `IncomingStreamRequestHandler` from `arti-relay` would also not be an ideal
place for this, because the `IncomingStreamRequestHandler` only has access
to the information from the `IncomingStream`, which, by design,
doesn't include any information about the channel.

The `IncomingStreamRequestFilter::disposition()` impl
is a plausible location for this check.
The `IncomingStreamRequestFilter` implementer will need access to the previous hop's
`ChannelType` and `PeerAddr`. This means the `CircHopSyncView`,
which is passed as an arg to `IncomingStreamHandler::disposition()`,
will need to have this information.
The type implementing `IncomingStreamRequestFilter` will need to have
an `Arc<dyn NetDirProvider>` so that it can obtain a fresh consensus
each time this check is run (we can use `timely_netdir()` for this).
But we should check how `timely_netdir()` is implemented,
because `IncomingStreamRequestFilter::disposition()` must be fast,
and not block the reactor.

Note: this is not set in stone, the exact design is left up to the implementer

4. Per-circuit stream-rate limiting, if the `DoSStreamCreationEnabled`
consensus parameter is set

Judging from C tor's `dos_stream_new_begin_or_resolve_cell()`,
this should apply to exit and DNS streams.

The exact behavior when the stream limit is exceeded is controlled
by the `DoSStreamCreationDefenseType` param:

```
    "DoSStreamCreationDefenseType" -- Defense type applied to a
    stream for the stream creation mitigation.
        1: No defense.
        2: Reject the stream or resolve request.
        3: Close the underlying circuit.
    First appeared: 0.4.9.0-alpha-dev.
```

This can easily be implemented in the `IncomingStreamRequestFilter`.
It will be very similar to the `RequestFilter` impl from `tor-hsservice`,
which closes any circuit exceeding `max_concurrent_streams` concurrent streams.


## Relay work

### Configuration

  * exit policy configuration ([#2261])
  * support for runtime configuration changes (`reconfigure()`) ([#2581]).
  * dir cache configuration, needed for building DirMirror. We will also need a
    new config option that says whether the relay is happy to act as a dir cache
    or not (similar to C Tor's `DirCache`)
  * consensus parameters for controlling stream handling behavior ([#2586])

#### Reconfiguration

Runtime config changes will need to be propagated to the `IncomingStreamRequestHandler`.
We will not apply any configuration or consensus changes retroactively,
to already opened streams.

The consensus parameters can be overridden using the `override_net_params`
config option. These overrides are handled by `tor-dirmgr`,
assuming its `reconfigure()` function is called to update its internal state.
We will need some logic for handling config changes by calling `reconfigure()` as
needed (including `DirMgr::reconfigure()`, to make any new parameter overrides
accessible via `NetDir::params()`).

### Allowing incoming streams

Currently, to allow incoming stream requests on a circuit,
you first need to call `RelayCirc::allow_stream_requests()`
to install a `CmdChecker` and `IncomingStreamRequestFilter`.
This is not ideal, because `allow_stream_requests()` will need to be
called unconditionally, on each `RelayCirc`,
right after it's created in the `CreateHandler` impl
(which in turn, would mean making `handle_create()` async too,
because `allow_stream_requests()` is async, which wouldn't be great).

So, the first step here is to rework the `RelayCirc` API to make relay circuits
be constructable with a list of allowed `RelayCmd`s and `IncomingStreamRequestFilter`
from the get-go ([#2582]), and to get rid of `allow_stream_requests()`,
which will enable the `CREATE*` handler to remain non-`async`.

In any case, the `CREATE*` handler will still require some changes,
because it needs to be initialized with an `IncomingStreamRequestFilter`,
which needs to be passed to the circuit reactor's constructor.
In turn, the circuit reactor's constructor will need to change
to additionally return a `futures::Stream` of `IncomingStream`s
(currently, it returns this from `Reactor::allow_stream_requests()`,
but we can easily change it to make it return the stream of tor stream
requests from the constructor instead).
We will also need to somehow thread the resulting
`futures::Stream` of `IncomingStream`s through to `arti-relay` for handling.

To achieve all this, we will need to modify the `CreateHandler` like so:

```diff
 /// Everything needed to handle CREATE* messages on channels.
 #[derive(derive_more::Debug)]
@@ -50,6 +52,28 @@ pub struct CreateRequestHandler {
     /// The circuit extension keys.
     #[debug(skip)]
     ntor_keys: RwLock<RelayNtorKeys>,
+    /// An [`IncomingStreamRequestFilter`] factory for checking whether the user wants
+    /// this request, or wants to reject it immediately.
+    ///
+    /// Used for building the [`IncomingStreamRequestFilter`]s shared with each circuit reactor.
+    #[debug(skip)]
+        incoming_filter_gen: Box<dyn Fn() -> Box<dyn IncomingStreamRequestFilter> + Send + Sync>,
+    /// A sender for the [`Stream`]s of `IncomingStream` of all circuits.
+    ///
+    /// The receiver will receive one [`Stream`] (of tor streams) per circuit.
+    ///
+    /// TODO: this MPSC is not associated with any particular circuit or channel,
+    /// so we cannot assign it a memquota account. Arguably, this is a design smell,
+    /// so we might want to revisit this.
+    ///
+    /// This being a bounded channel might seem a bit risky, because at first glance,
+    /// this queue might seem like it could block.
+    /// In practice, however, it should never block (or buffer very much at all,
+    /// for that matter), because the user (arti-relay) is expected to spawn a
+    /// background task that reads from this channel, spawning a new handling task
+    /// for each `Stream`.
+    #[debug(skip)]
+    incoming_stream_tx: mpsc::Sender<Box<dyn Stream<Item = IncomingStream> + Send + Sync + Unpin>>,
 }
```

The `CreateHandler` impl will then need to send the stream of `IncomingStream`s of each circuit
to the external handling task from `arti-relay`, right after building the circuit reactor:

```diff
        // Build the relay circuit reactor.
-        let (reactor, circ) = Reactor::new(
+        let (reactor, circ, incoming_streams) = Reactor::new(
             runtime.clone(),
             channel,
             circ_id,
@@ -177,12 +206,17 @@ impl CreateRequestHandler {
             chan_provider,
             padding_ctrl.clone(),
             padding_stream,
+            (self.incoming_filter_gen)(),
             &memquota,
         )
         .map_err(into_internal!("Failed to start circuit reactor"))?;

+
+        let mut tx = self.incoming_stream_tx.clone();
         // Start the reactor in a task.
-        let () = runtime.spawn(async {
+        let () = runtime.spawn(async move {
+            tx.send(Box::new(incoming_streams)).await.unwrap();
```


### Stream opening and handling

`arti-relay` will need to spawn a bunch of extra tasks

  * a `DirMirror` task, if we are a dir cache
  * a single task that receives the `Box<dyn Stream<Item = IncomingStream> + Send + Sync + Unpin>`s
    of all circuits. For each `dyn Stream<<Item = IncomingStream>`, it launches...
  * a per-circuit task  that handles a *single* `future::Stream` of `IncomingStreams`:
    this task listens for the `IncomingStreams` of a given circuit.
    For each of them, it launches...
  * a per-stream task that proxies data between the stream reactor
    and the local application stream

So we will have one task per circuit (for listening for `IncomingStreams`),
and one task for each stream, plus the DirMirror and top-level singleton task.

The task reading the `futures::Stream` of `IncomingStream`s will do very little work
beyond some simple checks + spawning of the handling task for each stream,
to avoid it becoming a performance bottleneck.

> Note: All of these tasks will rely on the runtime for scheduling/prioritization,
> which will very likely be suboptimal, especially on very busy relays.
> In the future, we will probably want to implemement our own scheduler,
> but that's going to be a project on its own, and not something we can implement
> right now.


As mentioned above, in the `arti-relay` crate, we will need to spawn the `DirMirror` task
(for handling directory requests), as well as the main `IncomingStream` handling task:

```rust
impl<R: Runtime> TorRelay<R> {
    pub(crate) async fn run(self) -> anyhow::Result<void::Void> {
        ...

        // TODO: this can block if the DirMirror task is not reading the requests
        // fast enough, but Unbounded wouldn't be much better...
        let (begin_dir_tx, begin_dir_rx) =
            mpsc::channel::<tor_proto::Result<DataStream>>(BEGIN_DIR_BUF_SIZE);

        // Spawn a directory mirror server task, if we are a dir cache.
        // TODO(relay): we need a config that says whether we are a dircache
        // Possibly, the begin_dir_{tx,rx} should be Option<>s,
        // and set to None if the config says we're not supposed to be a dir cache
        // (and we probably won't support changing this at runtime)
        task_handles.spawn({
            // TODO: we don't seem to have a config for this?
            let dir_mirror: DirMirror = todo!();

            async {
                dir_mirror.serve(begin_dir_rx).await?;
                Err(anyhow::anyhow!("dir mirror exited"))
            }
        });

        // Listen for new Tor streams
        task_handles.spawn({
            let runtime = self.runtime.clone();

            // Note: this is a postage::watch channel
            let config = config_rx.clone();

            // self.stream_tx is the RX end of the MPSC channel shared with the CreateHandler.
            //
            // The CreateHandler is what builds and spawn circuit reactor,
            // and therefore receives the stream of IncomingStreams from its
            // constructor.
            async {
                handle_incoming_streams(runtime, config, handler, begin_dir_tx, self.stream_rx)
                    .await
                    .context("Failed to run stream handling task")

            }

        });

    }
```

And the stream handling logic will look roughly like this:

```rust
//! arti_relay/tasks/stream.rs

/// Handles all incoming stream requests (BEGIN, BEGIN_DIR, RESOLVE),
/// by dispatching to the appropriate handler.
//
// TODO: we might end up rewriting to be a &self method on a struct
// (for example on the IncomingStreamHandler), or something.
async fn handle_incoming_streams<R>(
    runtime: R,
    config: postage::watch::Receiver<<Arc<ExitConfig>>>,
    handler: Arc<IncomingStreamHandler>,
    begin_dir_tx: mpsc::Sender<tor_proto::Result<DataStream>>,
    incoming_streams: impl Stream<Item = IncomingStream>,
) {
      loop {
          // These are the Stream of IncomingStreams of a newly-created circuit
          let circuit_streams = select_biased! {
              // TODO: who sends the shutdown signal?
              _ = shutdown_rx => return Ok(()),
              s = incoming_streams.next() => match s {
                  None => return Ok(()),
                  Some(s) => s,
              }
          };

            // Each circuit gets its own stream-handling task
            let rt = runtime.clone();
            let begin_dir_tx = begin_dir_tx.clone();
            runtime.spawn(async move {
                while let Some(incoming_stream) = circuit_streams.next().await {
                  // Each new stream will get a snapshot of the
                  // config (e.g. the current exit policy, assuming
                  // we're going to allow it to change during runtime).
                  // Existing streams will not be affected by runtime config changes.
                  //
                  // The code updating this will live somewhere in arti-relay:
                  // it will watch for config changes (or a signal),
                  // and will update the postage::watch channel
                  // with the new config
                  let config = config_rx.borrow();

                  // TODO: the handler stores the state that's
                  // shared by all the streams (such as the DNS cache).
                  // We might choose to put the config inside of it.
                  let handler = Arc::clone(&handler);
                  let _= rt.spawn(async move {
                      let res = match incoming_stream.request() {
                          IncomingStreamRequest::Begin(begin) => {
                            handler.handle_begin(incoming_stream, config).await
                          }
                          IncomingStreamRequest::BeginDir(_) => {
                            handler.handle_begin_dir(incoming_stream, begin_dir_tx).await
                          }
                          IncomingStreamRequest::Resolve(_) => {
                            handler.handle_resolve(incoming_stream).await
                          }
                      };
                  });
                }
            })?;

      }
}

// TODO: this is going to be a type holding all the state we need
// for handling incoming streams (the exact design is TBD)
struct IncomingStreamHandler { .. }

```

As for the different stream types...

#### `BEGIN` streams

These will be handled according to the configured exit policy config.

```rust
// TODO: this is not set in stone, and we will likely come up
// with a different design once we address #2261
struct ExitConfig {
    /// Whether this is an exit relay
    ///
    /// If set to false, no traffic is allowed to exit,
    /// and the ExitPolicyConfig is ignored.
    ///
    /// Equivalent to C Tor's ExitRelay option.
    enabled: bool,
    /// The exit policy config
    ///
    // TODO(#2261): figure out what this config should look like
    policy: ExitPolicyConfig
}

impl ExitConfig {
    /// Whether this configuration allows connecting
    /// to the specified SocketAddr.
    fn allows_addr_and_port(&self, socket_addr: SocketAddr) -> bool { .. }
}


impl IncomingStreamHandler {
    /// Handle an IncomingStream known to be a BEGIN_DIR
    async fn handle_begin(
        &mut self,
        config: Arc<ExitConfig>,
        incoming: IncomingStream
    ) -> Result<()> {
        // Note: this API is fictional and not necessarily what we will end up
        // implementing, but we will need a function that resolves
        // the addr and port in the Begin to an (IP, port) we can connect to
        // (because the addr part of the BEGIN cell could be a hostname)
        //
        // Internally, this will use a caching DnsResolver (design TBD,
        // see the note under "`RESOLVE` streams" below).
        // It also picks the right addr to use, based on the BeginFlags
        let addr: SocketAddr = self.resolve_begin_addr_and_port(s.request(), &config);

        if config.allows_addr_and_port(addr) {
            // TODO: get the options from somewhere, possibly the config
            let connect_options = Default::default();
            let stream = self.runtime.connect(addr, &connect_options);
            // Note: forward_connection() will call incoming.accept_data()
            // or incoming.reject(), depending on whether the stream
            // could be opened successfully.
            //
            // If successful, it will spawn a task that uses
            // futures_copy::copy_buf_bidirectional to pipe data between
            // the local stream and the tor stream.
            // This will be similar to the `forward_connection` function in
            // `tor-hsrproxy` (we might even be able to refactor the two
            // to share the same base implementation)
            forward_connection(&self.runtime, incoming, stream).await
        } else {
            // An End messgae with an appropriate END_REASON
            let end = todo!();
            incoming.reject(end);
        }

        Ok(())
    }
}
```

TODO: design work for DNS caching (see `RESOLVE` streams below)


#### `BEGIN_DIR` streams

Directory mirrors need to respond to `BEGIN_DIR`.

Our handler will need to call `.accept_data()` on each `BEGIN_DIR` `IncomingStream`s
to obtain a `DataStream`, which can then be sent over the `begin_dir_tx` MPSC channel
(whose receiver was passed to `DirMirror::serve()` in the snippet above):

```rust
impl IncomingStreamHandler {
    /// Handle an IncomingStream known to be a BEGIN_DIR
    async fn handle_begin_dir(&mut self, s: IncomingStream) -> Result<()> {
        // TODO: check if we are a dircache/dirmirror,
        // and reject the stream if we are not?
        let data_stream = s.accept_data().await?;

        // This sends the newly accepted data stream
        // to the DirMirror task for handling
        // DirMirror::serve() will forward data between
        // our data stream and the http endpoint
        self.begindir_tx.send(data_stream).await?;
        Ok(())
    }
}
```

#### `RESOLVE` streams

`IncomingStream` will need a new public method for handling `RESOLVE` streams ([#2572]):

```rust
impl IncomingStream {
    /// Send a RESOLVED messagge to the client
    pub async fn resolved(mut self, message: msg::Resolved) { .. }
}
```


And in `arti-relay`, these will be handled by `handle_resolve()`:

```rust
impl IncomingStreamHandler {
    /// Handle an IncomingStream known to be a RESOLVE
    async fn handle_resolve(
        &mut self,
        config: Arc<ExitConfig>,
        incoming: IncomingStream
    ) -> Result<()> {
        // We only respond to RESOLVE if we're configured as an exit.
        // The exit policy doesn't matter
        // TODO: do we need to check anything else here?
        if !config.enabled {
            // An END message with an appropruate END_REASON
            let end = todo!();
            incoming.reject(end).await?;
            return Ok(());
        }

        let addrs = self.resolve_addrs(incoming.request(), &config);

        // TODO: the RESOLVED response, built from addrs
        let resolved: Resolved = todo!();

        // Send the RESOLVED and close the stream
        incoming.resolved(resolved).await?;

        Ok(())
    }
}
```

We will of course also need some code for the actual DNS lookup.

We will likely need a DNS cache, but I think this will be tricky
to get right because it can open us up to timing attacks,
so it needs some more discussion and design work ([#1448]).


[#1448]: https://gitlab.torproject.org/tpo/core/arti/-/work_items/1448
[#2572]: https://gitlab.torproject.org/tpo/core/arti/-/work_items/2572
[#2261]: https://gitlab.torproject.org/tpo/core/arti/-/work_items/2261
[#2256]: https://gitlab.torproject.org/tpo/core/arti/-/work_items/2256
[#2581]: https://gitlab.torproject.org/tpo/core/arti/-/work_items/2581
[#2582]: https://gitlab.torproject.org/tpo/core/arti/-/work_items/2582
