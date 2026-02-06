use crate::time::system_time_now;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;
use tor_time::CoarseInstant;
use tor_time::RealCoarseTimeProvider;
use tor_rtcompat::SleepProvider;
use tor_time::CoarseTimeProvider;

#[derive(Clone, Debug, Default)]
pub struct WasmRuntime {
    coarse: RealCoarseTimeProvider,
}

impl CoarseTimeProvider for WasmRuntime {
    fn now_coarse(&self) -> CoarseInstant {
        self.coarse.now_coarse()
    }
}

impl SleepProvider for WasmRuntime {
    type SleepFuture = WasmSleep;

    fn sleep(&self, duration: Duration) -> Self::SleepFuture {
        WasmSleep::new(duration)
    }

    fn now(&self) -> tor_time::Instant {
        // tor_rtcompat now uses web_time::Instant which works on WASM
        tor_time::Instant::now()
    }

    fn wallclock(&self) -> tor_time::SystemTime {
        system_time_now()
    }
}

/// Wrapper to make gloo Timeout Send on WASM (which is single-threaded anyway)
#[cfg(target_arch = "wasm32")]
struct SendTimeout(gloo_timers::callback::Timeout);

#[cfg(target_arch = "wasm32")]
// SAFETY: WASM is single-threaded, so Send is safe
unsafe impl Send for SendTimeout {}

pub struct WasmSleep {
    rx: futures::channel::oneshot::Receiver<()>,
    // Keep the timeout handle alive so it doesn't get cancelled
    #[cfg(target_arch = "wasm32")]
    _timeout: SendTimeout,
}

// SAFETY: WASM is single-threaded, so Send is safe
#[cfg(target_arch = "wasm32")]
unsafe impl Send for WasmSleep {}

impl WasmSleep {
    fn new(duration: Duration) -> Self {
        let (tx, rx) = futures::channel::oneshot::channel();

        #[cfg(target_arch = "wasm32")]
        {
            // gloo-timers works in both browsers and Node.js
            let millis = u32::try_from(duration.as_millis().min(u32::MAX as u128)).unwrap_or(u32::MAX);
            let timeout = gloo_timers::callback::Timeout::new(millis, move || {
                let _ = tx.send(());
            });
            Self { rx, _timeout: SendTimeout(timeout) }
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            std::thread::spawn(move || {
                std::thread::sleep(duration);
                let _ = tx.send(());
            });
            Self { rx }
        }
    }
}

impl Future for WasmSleep {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        use futures::FutureExt;
        match self.rx.poll_unpin(cx) {
            Poll::Ready(_) => Poll::Ready(()),
            Poll::Pending => Poll::Pending,
        }
    }
}
