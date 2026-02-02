//! WASM-compatible runtime implementation for tor-rtcompat.
//!
//! This module provides a runtime that can run in WebAssembly environments (browsers).
//! It implements the required traits for `Runtime` with some limitations:
//!
//! - **Blocking operations**: Stubbed - will panic if called. WASM has no threads.
//! - **Networking**: Requires external transport (WebSocket/WebRTC)
//! - **TLS**: Requires external TLS provider (e.g., subtle-tls)

use crate::coarse_time::RealCoarseTimeProvider;
use crate::traits::{
    Blocking, CertifiedConn, CoarseTimeProvider, NetStreamListener, NetStreamProvider,
    NoOpStreamOpsHandle, SleepProvider, StreamOps, TlsConnector, TlsProvider, UdpProvider,
    UdpSocket,
};
use crate::CoarseInstant;
use async_trait::async_trait;
use futures::task::{Spawn, SpawnError};
use futures::{stream, AsyncRead, AsyncWrite, Future};
use std::fmt::Debug;
use std::io::{self, Result as IoResult};
use std::net::SocketAddr;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::{Duration, SystemTime};
use tor_general_addr::unix;

/// A runtime for WASM environments.
///
/// This runtime implements the traits required by `tor-rtcompat::Runtime`,
/// but with significant limitations due to WASM constraints:
///
/// - No blocking operations (will panic)
/// - No direct TCP/UDP sockets (need WebSocket/WebRTC transport)
/// - No filesystem access
#[derive(Clone, Debug, Default)]
pub struct WasmRuntime {
    /// Coarse time provider
    coarse: RealCoarseTimeProvider,
}

impl WasmRuntime {
    /// Create a new WASM runtime.
    pub fn new() -> Self {
        Self {
            coarse: RealCoarseTimeProvider::new(),
        }
    }
}

// ============================================================================
// SleepProvider implementation
// ============================================================================

/// A sleep future for WASM using gloo-timers.
pub struct WasmSleepFuture {
    /// The underlying timeout future from gloo-timers
    #[cfg(all(target_arch = "wasm32", feature = "wasm"))]
    inner: gloo_timers::future::TimeoutFuture,
    /// Fallback for non-WASM (for testing)
    #[cfg(not(all(target_arch = "wasm32", feature = "wasm")))]
    rx: futures::channel::oneshot::Receiver<()>,
}

impl WasmSleepFuture {
    /// Create a new sleep future.
    fn new(duration: Duration) -> Self {
        #[cfg(all(target_arch = "wasm32", feature = "wasm"))]
        {
            let millis = duration.as_millis().min(u32::MAX as u128) as u32;
            Self {
                inner: gloo_timers::future::TimeoutFuture::new(millis),
            }
        }

        #[cfg(not(all(target_arch = "wasm32", feature = "wasm")))]
        {
            let (tx, rx) = futures::channel::oneshot::channel();
            std::thread::spawn(move || {
                std::thread::sleep(duration);
                let _ = tx.send(());
            });
            Self { rx }
        }
    }
}

impl Future for WasmSleepFuture {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        #[cfg(all(target_arch = "wasm32", feature = "wasm"))]
        {
            // SAFETY: We never move the inner future after pinning
            let inner = unsafe { self.map_unchecked_mut(|s| &mut s.inner) };
            inner.poll(cx)
        }

        #[cfg(not(all(target_arch = "wasm32", feature = "wasm")))]
        {
            use futures::FutureExt;
            let this = self.get_mut();
            match this.rx.poll_unpin(cx) {
                Poll::Ready(_) => Poll::Ready(()),
                Poll::Pending => Poll::Pending,
            }
        }
    }
}

// SAFETY: The future only contains thread-safe types
unsafe impl Send for WasmSleepFuture {}

impl SleepProvider for WasmRuntime {
    type SleepFuture = WasmSleepFuture;

    fn sleep(&self, duration: Duration) -> Self::SleepFuture {
        WasmSleepFuture::new(duration)
    }

    fn now(&self) -> crate::Instant {
        crate::Instant::now()
    }

    fn wallclock(&self) -> SystemTime {
        #[cfg(target_arch = "wasm32")]
        {
            // Use Performance.now() for WASM
            let millis = js_sys::Date::now();
            SystemTime::UNIX_EPOCH + Duration::from_millis(millis as u64)
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            SystemTime::now()
        }
    }
}

// ============================================================================
// CoarseTimeProvider implementation
// ============================================================================

impl CoarseTimeProvider for WasmRuntime {
    fn now_coarse(&self) -> CoarseInstant {
        self.coarse.now_coarse()
    }
}

// ============================================================================
// Spawn implementation
// ============================================================================

impl Spawn for WasmRuntime {
    fn spawn_obj(&self, future: futures::task::FutureObj<'static, ()>) -> Result<(), SpawnError> {
        #[cfg(all(target_arch = "wasm32", feature = "wasm"))]
        {
            wasm_bindgen_futures::spawn_local(future);
            Ok(())
        }

        #[cfg(not(all(target_arch = "wasm32", feature = "wasm")))]
        {
            // Fallback for testing - just spawn a thread
            std::thread::spawn(move || {
                futures::executor::block_on(future);
            });
            Ok(())
        }
    }
}

// ============================================================================
// Blocking implementation (STUBBED - will panic)
// ============================================================================

impl Blocking for WasmRuntime {
    type ThreadHandle<T: Send + 'static> = StubThreadHandle<T>;

    fn spawn_blocking<F, T>(&self, _f: F) -> Self::ThreadHandle<T>
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
    {
        panic!(
            "WasmRuntime::spawn_blocking called - blocking operations are not supported in WASM. \
             This code path should not be reached. Please report this as a bug."
        );
    }

    fn reenter_block_on<F>(&self, _future: F) -> F::Output
    where
        F: Future,
        F::Output: Send + 'static,
    {
        panic!(
            "WasmRuntime::reenter_block_on called - blocking operations are not supported in WASM. \
             This code path should not be reached. Please report this as a bug."
        );
    }
}

/// Stub thread handle that will never be created (spawn_blocking panics).
pub struct StubThreadHandle<T> {
    _phantom: std::marker::PhantomData<T>,
}

impl<T: Send + 'static> Future for StubThreadHandle<T> {
    type Output = T;

    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        // This will never be called because spawn_blocking panics
        unreachable!("StubThreadHandle should never be polled")
    }
}

// ============================================================================
// NetStreamProvider implementation (STUBBED)
// ============================================================================

/// A stub stream that always returns errors.
///
/// Real WASM networking requires a WebSocket or WebRTC transport layer.
#[derive(Debug)]
pub struct StubStream;

impl AsyncRead for StubStream {
    fn poll_read(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        _buf: &mut [u8],
    ) -> Poll<IoResult<usize>> {
        Poll::Ready(Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "StubStream does not support reading - use a WebSocket transport",
        )))
    }
}

impl AsyncWrite for StubStream {
    fn poll_write(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        _buf: &[u8],
    ) -> Poll<IoResult<usize>> {
        Poll::Ready(Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "StubStream does not support writing - use a WebSocket transport",
        )))
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<IoResult<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_close(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<IoResult<()>> {
        Poll::Ready(Ok(()))
    }
}

impl StreamOps for StubStream {
    fn new_handle(&self) -> Box<dyn StreamOps + Send + Unpin> {
        Box::new(NoOpStreamOpsHandle)
    }
}

/// A stub listener that never accepts connections.
pub struct StubListener;

impl NetStreamListener<SocketAddr> for StubListener {
    type Stream = StubStream;
    type Incoming = stream::Empty<IoResult<(Self::Stream, SocketAddr)>>;

    fn incoming(self) -> Self::Incoming {
        stream::empty()
    }

    fn local_addr(&self) -> IoResult<SocketAddr> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "StubListener has no local address",
        ))
    }
}

impl NetStreamListener<unix::SocketAddr> for StubListener {
    type Stream = StubStream;
    type Incoming = stream::Empty<IoResult<(Self::Stream, unix::SocketAddr)>>;

    fn incoming(self) -> Self::Incoming {
        stream::empty()
    }

    fn local_addr(&self) -> IoResult<unix::SocketAddr> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "StubListener has no local address",
        ))
    }
}

#[async_trait]
impl NetStreamProvider<SocketAddr> for WasmRuntime {
    type Stream = StubStream;
    type Listener = StubListener;

    async fn connect(&self, _addr: &SocketAddr) -> IoResult<Self::Stream> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "WasmRuntime does not support direct TCP connections. \
             Use a WebSocket or WebRTC transport layer instead.",
        ))
    }

    async fn listen(&self, _addr: &SocketAddr) -> IoResult<Self::Listener> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "WasmRuntime does not support listening on TCP sockets",
        ))
    }
}

#[async_trait]
impl NetStreamProvider<unix::SocketAddr> for WasmRuntime {
    type Stream = StubStream;
    type Listener = StubListener;

    async fn connect(&self, _addr: &unix::SocketAddr) -> IoResult<Self::Stream> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "WasmRuntime does not support Unix sockets",
        ))
    }

    async fn listen(&self, _addr: &unix::SocketAddr) -> IoResult<Self::Listener> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "WasmRuntime does not support Unix sockets",
        ))
    }
}

// ============================================================================
// TlsProvider implementation (STUBBED)
// ============================================================================

/// A stub TLS connector that always returns errors.
pub struct StubTlsConnector;

#[async_trait]
impl TlsConnector<StubStream> for StubTlsConnector {
    type Conn = StubTlsStream;

    async fn negotiate_unvalidated(
        &self,
        _stream: StubStream,
        _sni_hostname: &str,
    ) -> IoResult<Self::Conn> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "StubTlsConnector does not support TLS - use subtle-tls or similar",
        ))
    }
}

/// A stub TLS stream.
#[derive(Debug)]
pub struct StubTlsStream;

impl AsyncRead for StubTlsStream {
    fn poll_read(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        _buf: &mut [u8],
    ) -> Poll<IoResult<usize>> {
        Poll::Ready(Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "StubTlsStream does not support reading",
        )))
    }
}

impl AsyncWrite for StubTlsStream {
    fn poll_write(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        _buf: &[u8],
    ) -> Poll<IoResult<usize>> {
        Poll::Ready(Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "StubTlsStream does not support writing",
        )))
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<IoResult<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_close(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<IoResult<()>> {
        Poll::Ready(Ok(()))
    }
}

impl StreamOps for StubTlsStream {
    fn new_handle(&self) -> Box<dyn StreamOps + Send + Unpin> {
        Box::new(NoOpStreamOpsHandle)
    }
}

impl CertifiedConn for StubTlsStream {
    fn export_keying_material(
        &self,
        _len: usize,
        _label: &[u8],
        _context: Option<&[u8]>,
    ) -> IoResult<Vec<u8>> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "StubTlsStream does not support keying material export",
        ))
    }

    fn peer_certificate(&self) -> IoResult<Option<Vec<u8>>> {
        Ok(None)
    }
}

impl TlsProvider<StubStream> for WasmRuntime {
    type Connector = StubTlsConnector;
    type TlsStream = StubTlsStream;

    fn tls_connector(&self) -> Self::Connector {
        StubTlsConnector
    }

    fn supports_keying_material_export(&self) -> bool {
        false
    }
}

// ============================================================================
// UdpProvider implementation (STUBBED)
// ============================================================================

/// A stub UDP socket that always returns errors.
pub struct StubUdpSocket;

#[async_trait]
impl UdpSocket for StubUdpSocket {
    async fn recv(&self, _buf: &mut [u8]) -> IoResult<(usize, SocketAddr)> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "WasmRuntime does not support UDP sockets",
        ))
    }

    async fn send(&self, _buf: &[u8], _target: &SocketAddr) -> IoResult<usize> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "WasmRuntime does not support UDP sockets",
        ))
    }

    fn local_addr(&self) -> IoResult<SocketAddr> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "StubUdpSocket has no local address",
        ))
    }
}

#[async_trait]
impl UdpProvider for WasmRuntime {
    type UdpSocket = StubUdpSocket;

    async fn bind(&self, _addr: &SocketAddr) -> IoResult<Self::UdpSocket> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "WasmRuntime does not support UDP sockets",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wasm_runtime_creation() {
        let _rt = WasmRuntime::new();
    }

    #[test]
    fn test_sleep_provider() {
        let rt = WasmRuntime::new();
        let _future = rt.sleep(Duration::from_millis(100));
        // We can't actually await it without a runtime, but we can create it
    }

    #[test]
    fn test_coarse_time_provider() {
        let rt = WasmRuntime::new();
        let _now = rt.now_coarse();
    }
}