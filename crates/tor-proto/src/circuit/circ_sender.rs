//! Implements a sender and a [`Stream`] type for sending messages
//! from a [channel](crate::channel) to a circuit,
//! prioritizing the delivery of `DESTROY` messages.
//!
//! [`CircuitRxSender`] and [`CircuitRxReceiver`] take any channel message,
//! because the receiving end can be either a client or a relay circuit reactor.
//! The reactor itself will convert into its restricted message set.

use std::pin::Pin;
use std::task::{self, Context, Poll};

use futures::{FutureExt as _, SinkExt as _, Stream, StreamExt as _};
use oneshot_fused_workaround as oneshot;
use tor_basic_utils::assert_val_impl_trait;
use tor_cell::chancell::msg::{AnyChanMsg, Destroy};
use tor_memquota::mq_queue::{self, ChannelSpec, MpscSpec};

/// The sending end of the SPSC queue for inbound data on its way from channel to circuit
///
/// A [`CircuitRxSender`] sender is closed for sending as soon as the first
/// `DESTROY` message is sent, and will discard any unflushed cells
/// from its underlying [`mq_queue`], by dropping it.
///
/// ## No [`Sink`](futures::Sink) implementation
///
/// This type intentionally does not implement [`Sink`](futures::Sink).
/// Instead it provides a [`send()`](CircuitRxSender::send) function
/// similar to [`SinkExt::send`](futures::SinkExt::send).
///
/// The reason for doing it this way is because we cannot provide
/// a correct `Sink::poll:ready()` implementation
/// that wouldn't block DESTROY cells from being sent
/// when our underlying MPSC sender is full:
/// `SinkExt::send()` calls `poll_ready()` followed by `start_send()`,
/// so in order for our `poll_ready()` implementation to not block DESTROY
/// on the MPSC queue's readiness, it would need to know whether
/// the cell that will be sent via `start_send()` is a DESTROY or not,
/// but that's not possible because of the way the `Sink`/`SinkExt` traits
/// are designed.
#[derive(Debug)]
pub(crate) struct CircuitRxSender(Option<CircuitRxSenderInner>);

/// The inner state of a [`CircuitRxSender`].
#[derive(Debug)]
struct CircuitRxSenderInner {
    /// Sender for sending `DESTROY` to [`CircuitRxReceiver`]
    destroy_tx: oneshot::Sender<Destroy>,
    /// Sender for sending all other [`AnyChanMsg`]s to [`CircuitRxReceiver`]
    cell_tx: mq_queue::Sender<AnyChanMsg, MpscSpec>,
}

/// The receiving end of the SPSC queue for inbound data on its way from channel to circuit
///
/// A [`CircuitRxReceiver`] stream ends as soon as the first `DESTROY` message
/// is received, causing the stream to discard any unflushed cells
/// from its underlying [`mq_queue`], by dropping it.
#[derive(Debug)]
pub(crate) struct CircuitRxReceiver(Option<CircuitRxReceiverInner>);

/// The inner state of a [`CircuitRxReceiver`].
#[derive(Debug)]
struct CircuitRxReceiverInner {
    /// Receiver for receiving `DESTROY` from [`CircuitRxSender`]
    destroy_rx: oneshot::Receiver<Destroy>,
    /// Receiver for receiving all other [`AnyChanMsg`]s from [`CircuitRxReceiver`]
    cell_rx: mq_queue::Receiver<AnyChanMsg, MpscSpec>,
}

/// Wrap the sender and receiver of an [`mq_queue`] channel
/// into [`CircuitRxSender`] and [`CircuitRxReceiver`].
///
/// The returned channel will ensure any DESTROY messages sent
/// over the [`CircuitRxSender`] will be delivered
/// by the [`CircuitRxReceiver`] immediately,
/// ahead of any other messages that might already be queued,
/// which will be discarded.
///
/// We are fine with the resulting data loss, because inbound DESTROY
/// can be indicative of malicious activity on the circuit.
/// We choose to err on the safe side, and free up the resources associated
/// with such circuits as soon as possible.
/// DESTROY messages are also sent by relays when they're about to hibernate,
/// and by clients once they've decided to stop using a circuit.
/// In the latter case, the lack of an `RELAY_COMMAND_END_ACK`
/// does mean that this prioritization can cause data loss
/// (if the client closes the circuit immediately after END-ing a stream).
/// However, this is a deficiency in the protocol,
/// and not something we want to fix by implementing custom flushing logic
/// in the reactor. See torspec#196 and the discussion in #2490.
///
/// Note: the underlying buffer of the [`mq_queue`] will only be freed
/// once both the [`CircuitRxSender`] and [`CircuitRxReceiver`] are dropped;
/// in other words, after a `DESTROY` cell has been obtained from the [`CircuitRxReceiver`],
/// via its [`Stream`] implementation
pub(crate) fn channel(
    cell_tx: mq_queue::Sender<AnyChanMsg, MpscSpec>,
    cell_rx: mq_queue::Receiver<AnyChanMsg, MpscSpec>,
) -> (CircuitRxSender, CircuitRxReceiver) {
    let (destroy_tx, destroy_rx) = oneshot::channel();
    let sender = CircuitRxSender(Some(CircuitRxSenderInner {
        destroy_tx,
        cell_tx,
    }));

    let receiver = CircuitRxReceiver(Some(CircuitRxReceiverInner {
        destroy_rx,
        cell_rx,
    }));

    (sender, receiver)
}

impl Stream for CircuitRxReceiver {
    type Item = AnyChanMsg;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let Some(inner) = self.0.as_mut() else {
            return Poll::Ready(None);
        };

        // It's important that destroy_rx is fused,
        // because we call poll_unpin() unconditionally below.
        assert_val_impl_trait!(inner.destroy_rx, futures_util::future::FusedFuture);

        // First, check if we have a DESTROY message ready
        let destroy_cell = match inner.destroy_rx.poll_unpin(cx) {
            Poll::Ready(destroy) => {
                // If destroy.is_err(), it means the CircuitRxSender was dropped,
                // but there may be more data buffered in the underlying mpsc,
                // so we need to continue polling cell_rx.
                //
                // This is important, because we want to preserve the behavior
                // of the mq_queue, whose Receiver will continue yielding queued
                // messages even after the Sender is dropped.
                destroy.ok()
            }
            Poll::Pending => {
                // No DESTROY message yet, so it's time to poll the non-priority
                // message queue
                None
            }
        };

        if let Some(destroy) = destroy_cell {
            // Drop the inner state, closing this stream
            self.0 = None;
            return Poll::Ready(Some(AnyChanMsg::Destroy(destroy)));
        }

        let res = task::ready!(inner.cell_rx.poll_next_unpin(cx));

        // Our CircuitRxSender impl will never send DESTROY messages
        // on the cell_rx queue (they're always sent via the oneshot channel)
        debug_assert!(!matches!(res, Some(AnyChanMsg::Destroy(_))));

        Poll::Ready(res)
    }
}

/// Error returned when trying to write to a [`CircuitRxSender`]
#[derive(thiserror::Error, Clone, Debug)]
pub(crate) enum SendError {
    /// The underlying MPSC channel rejected the message
    #[error("{0}")]
    Channel(#[from] mq_queue::SendError<<MpscSpec as ChannelSpec>::SendError>),

    /// The receiver has dropped
    ///
    // Note: technically, there are two "Disconnected" variants:
    // this one, for the oneshot channel, and a second, hidden variant
    // inside mq_queue:SendError, for the mq_queue one.
    //
    // It would be nice if we only had one variant covering both cases,
    // but this will have to do for now.
    #[error("the receiver has dropped")]
    Disconnected,

    /// The sender is closed
    ///
    /// Returned if the [`CircuitRxSender`] is used after a DESTROY cell has been written to it.
    #[error("sender has closed")]
    Closed,
}

impl CircuitRxSender {
    /// Send a cell down this channel
    ///
    /// If the sender is already closed (i.e., if we have already sent DESTROY),
    /// this will return an error.
    ///
    // In practice, we never write more than 1 DESTROY cell to this sender,
    // because the channel reactor removes the circuit (and corresponding CircuitRxSender)
    // from its circ map after the first DESTROY.
    pub(crate) async fn send(&mut self, msg: AnyChanMsg) -> Result<(), SendError> {
        if let AnyChanMsg::Destroy(d) = msg {
            let inner = self.take_inner()?;

            if inner.destroy_tx.send(d).is_err() {
                return Err(SendError::Disconnected);
            }

            Ok(())
        } else {
            self.borrow_for_sending()?.cell_tx.send(msg).await?;
            Ok(())
        }
    }

    /// Borrow the [`CircuitRxSenderInner`] state for sending.
    ///
    /// Returns an error if the sender is closed.
    fn borrow_for_sending(&mut self) -> Result<&mut CircuitRxSenderInner, SendError> {
        self.0.as_mut().ok_or_else(|| SendError::Closed)
    }

    /// Take the inner [`CircuitRxSenderInner`], closing the sender.
    ///
    /// Returns an error if the sender is already closed.
    fn take_inner(&mut self) -> Result<CircuitRxSenderInner, SendError> {
        self.0.take().ok_or_else(|| SendError::Closed)
    }
}

#[cfg(test)]
pub(crate) mod test {
    // @@ begin test lint list maintained by maint/add_warning @@
    #![allow(clippy::bool_assert_comparison)]
    #![allow(clippy::clone_on_copy)]
    #![allow(clippy::dbg_macro)]
    #![allow(clippy::mixed_attributes_style)]
    #![allow(clippy::print_stderr)]
    #![allow(clippy::print_stdout)]
    #![allow(clippy::single_char_pattern)]
    #![allow(clippy::unwrap_used)]
    #![allow(clippy::unchecked_time_subtraction)]
    #![allow(clippy::useless_vec)]
    #![allow(clippy::needless_pass_by_value)]
    #![allow(clippy::string_slice)] // See arti#2571
    //! <!-- @@ end test lint list maintained by maint/add_warning @@ -->

    use super::*;

    use tor_cell::chancell::msg::{self, DestroyReason};
    use tor_rtmock::MockRuntime;

    use std::task::Waker;

    /// Make an MPSC queue, of the type we use to send cells
    /// from the channel reactor to the circuit reactor,
    /// but a fake one for testing
    #[cfg(test)]
    pub(crate) fn fake_mpsc(buffer: usize) -> (CircuitRxSender, CircuitRxReceiver) {
        let (tx, rx) = crate::fake_mpsc(buffer);

        crate::circuit::circ_sender::channel(tx, rx)
    }

    /// A DESTROY message
    fn destroy_msg(reason: DestroyReason) -> AnyChanMsg {
        AnyChanMsg::Destroy(msg::Destroy::new(reason))
    }

    /// A RELAY message
    fn relay_msg() -> AnyChanMsg {
        AnyChanMsg::Relay(msg::Relay::new(b"hello"))
    }

    macro_rules! assert_eos {
        ($tx:expr, $rx:expr) => {{
            assert!($rx.next().await.is_none());
            // Cannot send any more cells once the sender is closed
            let err = $tx.send(relay_msg()).await.unwrap_err();
            assert!(matches!(err, SendError::Closed));
        }};
    }

    /// The buffer size to use for the fake MPSC queues
    const BUFFER_SIZE: usize = 16;

    #[test]
    fn destroy_skips_queue() {
        MockRuntime::test_with_various(|_rt| async move {
            let (mut tx, mut rx) = fake_mpsc(BUFFER_SIZE);

            tx.send(relay_msg()).await.unwrap();
            tx.send(destroy_msg(DestroyReason::HIBERNATING))
                .await
                .unwrap();

            // Destroy skips the queue
            let destroy = rx.next().await.unwrap();

            assert!(matches!(destroy, AnyChanMsg::Destroy(_)));
            // And we've reached EOS
            assert_eos!(tx, rx);
        });
    }

    #[test]
    fn destroy_on_empty_queue() {
        MockRuntime::test_with_various(|_rt| async move {
            let (mut tx, mut rx) = fake_mpsc(BUFFER_SIZE);

            tx.send(destroy_msg(DestroyReason::HIBERNATING))
                .await
                .unwrap();
            let destroy = rx.next().await.unwrap();

            assert!(matches!(destroy, AnyChanMsg::Destroy(_)));
            // And we've reached EOS
            assert_eos!(tx, rx);
        });
    }

    #[test]
    fn destroy_after_data() {
        MockRuntime::test_with_various(|_rt| async move {
            let (mut tx, mut rx) = fake_mpsc(BUFFER_SIZE);

            for _ in 0..3 {
                tx.send(relay_msg()).await.unwrap();
            }

            for _ in 0..3 {
                let data = rx.next().await.unwrap();
                assert!(matches!(data, AnyChanMsg::Relay(_)));
            }

            let mut noop_cx = Context::from_waker(Waker::noop());
            // The queue is now empty
            assert!(rx.poll_next_unpin(&mut noop_cx).is_pending());

            tx.send(destroy_msg(DestroyReason::PROTOCOL)).await.unwrap();

            let destroy = rx.next().await.unwrap();
            assert!(matches!(destroy, AnyChanMsg::Destroy(_)));
            // And we've reached EOS
            assert_eos!(tx, rx);
        });
    }

    #[test]
    fn destroy_full_queue() {
        MockRuntime::test_with_various(|_rt| async move {
            let (mut tx, mut rx) = fake_mpsc(BUFFER_SIZE);

            // Fill the queue with data...
            loop {
                let fut = Box::pin(tx.send(relay_msg()));
                match futures::poll!(fut) {
                    Poll::Pending => {
                        // Full, time to break
                        break;
                    }
                    Poll::Ready(res) => {
                        let () = res.unwrap();
                    }
                }
            }
            // ...followed by a destroy
            tx.send(destroy_msg(DestroyReason::INTERNAL)).await.unwrap();

            // The destroy cell goes through even though the queue is full,
            // ahead of all the queued data
            let destroy = rx.next().await.unwrap();

            assert!(matches!(destroy, AnyChanMsg::Destroy(_)));
            // And we've reached EOS
            assert_eos!(tx, rx);
        });
    }
}
