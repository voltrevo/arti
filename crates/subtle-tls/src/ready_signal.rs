//! A lightweight one-shot async signal for WASM (single-threaded).
//!
//! Used to let TLS certificate verification wait for the CA bundle to load
//! before rejecting an untrusted root.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// A one-shot signal that resolves waiters when [`set()`](ReadySignal::set) is called.
pub struct ReadySignal {
    ready: Cell<bool>,
    wakers: RefCell<Vec<std::task::Waker>>,
}

impl ReadySignal {
    /// Create a new signal (not yet set).
    pub fn new() -> Rc<Self> {
        Rc::new(Self {
            ready: Cell::new(false),
            wakers: RefCell::new(Vec::new()),
        })
    }

    /// Mark the signal as ready, waking all pending waiters.
    pub fn set(&self) {
        self.ready.set(true);
        for waker in self.wakers.borrow_mut().drain(..) {
            waker.wake();
        }
    }

    /// Returns a future that resolves when the signal is set.
    /// Resolves immediately if already set.
    pub fn wait(self: &Rc<Self>) -> ReadySignalFuture {
        ReadySignalFuture {
            signal: Rc::clone(self),
        }
    }
}

/// Future returned by [`ReadySignal::wait()`].
pub struct ReadySignalFuture {
    signal: Rc<ReadySignal>,
}

impl std::future::Future for ReadySignalFuture {
    type Output = ();

    fn poll(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<()> {
        if self.signal.ready.get() {
            std::task::Poll::Ready(())
        } else {
            self.signal.wakers.borrow_mut().push(cx.waker().clone());
            std::task::Poll::Pending
        }
    }
}