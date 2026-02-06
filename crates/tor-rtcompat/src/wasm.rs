//! WASM-compatible runtime implementation for tor-rtcompat.
//!
//! This module provides a runtime that can run in WebAssembly environments (browsers).
//! It implements the required traits for `Runtime` with some limitations:
//!
//! - **Blocking operations**: Stubbed - will panic if called. WASM has no threads.
//! - **Networking**: Requires external transport (WebSocket/WebRTC)
//! - **TLS**: Uses subtle-tls for TLS 1.3 via browser SubtleCrypto API

use crate::traits::{
    Blocking, CertifiedConn, NetStreamListener, NetStreamProvider, NoOpStreamOpsHandle,
    SleepProvider, StreamOps, TlsConnector, TlsProvider, UdpProvider, UdpSocket,
};
use tor_time::{CoarseInstant, CoarseTimeProvider, RealCoarseTimeProvider};
use tor_wasm_compat::async_trait;
use futures::task::{Spawn, SpawnError};
use futures::{stream, AsyncRead, AsyncWrite, Future};
use std::fmt::Debug;
use std::io::{self, Result as IoResult};
use std::net::SocketAddr;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;
use tor_time::{Instant, SystemTime, UNIX_EPOCH};
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
    #[cfg(target_arch = "wasm32")]
    inner: gloo_timers::future::TimeoutFuture,
    /// Fallback for non-WASM (for testing)
    #[cfg(not(target_arch = "wasm32"))]
    rx: futures::channel::oneshot::Receiver<()>,
}

impl WasmSleepFuture {
    /// Create a new sleep future.
    fn new(duration: Duration) -> Self {
        #[cfg(target_arch = "wasm32")]
        {
            let millis = duration.as_millis().min(u32::MAX as u128) as u32;
            Self {
                inner: gloo_timers::future::TimeoutFuture::new(millis),
            }
        }

        #[cfg(not(target_arch = "wasm32"))]
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
        #[cfg(target_arch = "wasm32")]
        {
            // SAFETY: We never move the inner future after pinning
            let inner = unsafe { self.map_unchecked_mut(|s| &mut s.inner) };
            inner.poll(cx)
        }

        #[cfg(not(target_arch = "wasm32"))]
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

    fn now(&self) -> Instant {
        Instant::now()
    }

    fn wallclock(&self) -> SystemTime {
        #[cfg(target_arch = "wasm32")]
        {
            // Use Performance.now() for WASM
            let millis = js_sys::Date::now();
            UNIX_EPOCH + Duration::from_millis(millis as u64)
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
        #[cfg(target_arch = "wasm32")]
        {
            wasm_bindgen_futures::spawn_local(future);
            Ok(())
        }

        #[cfg(not(target_arch = "wasm32"))]
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
// TlsProvider implementation using subtle-tls
// ============================================================================

/// TLS connector for WASM using subtle-tls.
///
/// This wraps subtle-tls's TlsConnector and configures it for Tor's requirements:
/// - Skips certificate verification (Tor validates via CERTS cells instead)
/// - Uses TLS 1.3
pub struct WasmTlsConnector {
    /// The underlying subtle-tls connector.
    inner: subtle_tls::TlsConnector,
}

impl WasmTlsConnector {
    /// Create a new WASM TLS connector.
    ///
    /// This connector skips certificate verification since Tor uses its own
    /// certificate validation via CERTS cells in the Tor protocol.
    pub fn new() -> Self {
        let config = subtle_tls::TlsConfig {
            // Skip WebPKI validation - Tor validates via CERTS cells
            skip_verification: true,
            alpn_protocols: vec![],
            version: subtle_tls::TlsVersion::Tls13,
        };
        Self {
            inner: subtle_tls::TlsConnector::with_config(config),
        }
    }
}

impl Default for WasmTlsConnector {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl<S> TlsConnector<S> for WasmTlsConnector
where
    S: AsyncRead + AsyncWrite + StreamOps + Unpin + Send + 'static,
{
    type Conn = subtle_tls::TlsStream<S>;

    async fn negotiate_unvalidated(
        &self,
        stream: S,
        sni_hostname: &str,
    ) -> IoResult<Self::Conn> {
        self.inner
            .connect(stream, sni_hostname)
            .await
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))
    }
}

impl<S> TlsProvider<S> for WasmRuntime
where
    S: AsyncRead + AsyncWrite + StreamOps + Unpin + Send + 'static,
{
    type Connector = WasmTlsConnector;
    type TlsStream = subtle_tls::TlsStream<S>;

    fn tls_connector(&self) -> Self::Connector {
        WasmTlsConnector::new()
    }

    fn supports_keying_material_export(&self) -> bool {
        // subtle-tls implements RFC 8446 keying material export
        true
    }
}

// Implement tor-rtcompat traits for subtle_tls::TlsStream
// (These were previously in subtle-tls but moved here to avoid circular dependency)

impl<S> StreamOps for subtle_tls::TlsStream<S>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    // Use default implementation
}

impl<S> CertifiedConn for subtle_tls::TlsStream<S>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    fn peer_certificate(&self) -> IoResult<Option<Vec<u8>>> {
        // subtle_tls::TlsStream::peer_certificate returns Option<&[u8]>
        Ok(self.peer_certificate().map(|s| s.to_vec()))
    }

    fn export_keying_material(
        &self,
        len: usize,
        label: &[u8],
        context: Option<&[u8]>,
    ) -> IoResult<Vec<u8>> {
        // Delegate to subtle_tls's implementation
        subtle_tls::TlsStream::export_keying_material(self, len, label, context)
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