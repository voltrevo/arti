//! Directory streams

use futures::SinkExt as _;
use futures::channel::mpsc;
use tor_cell::relaycell::msg::Connected;
use tor_proto::stream::IncomingStream;

// TODO(#2570): once we dust settles on the implementation,
// we need to factor this out of the client module.
use tor_proto::client::stream::DataStream;

/// Handle an incoming directory stream
pub(super) async fn handle_begin_dir(
    incoming: IncomingStream,
    mut begin_dir_tx: mpsc::Sender<tor_proto::Result<DataStream>>,
) -> anyhow::Result<()> {
    // TODO(relay): we unconditionally accept all BEGIN_DIR requests,
    // because we don't currently have configuration options for disabling directory mirroring
    // (and we may never do).
    //
    // See https://gitlab.torproject.org/tpo/core/arti/-/merge_requests/4107#note_3426447
    let data = incoming.accept_data(Connected::new_empty()).await;
    let _ = begin_dir_tx.send(data).await;

    Ok(())
}
