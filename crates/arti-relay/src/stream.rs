//! Stream handling logic

use tor_proto::circuit::CircHopSyncView;
use tor_proto::stream::{
    IncomingStreamRequestContext, IncomingStreamRequestDisposition, IncomingStreamRequestFilter,
};

/// Filter callback used to enforce early requirements on streams,
/// acting as an [`IncomingStreamRequestFilter`].
#[derive(Clone, Debug, Default)]
pub(crate) struct RequestFilter {
    // TODO(relay): implement
}

impl IncomingStreamRequestFilter for RequestFilter {
    fn disposition(
        &mut self,
        _ctx: &IncomingStreamRequestContext<'_>,
        _circ: &CircHopSyncView<'_>,
    ) -> tor_proto::Result<IncomingStreamRequestDisposition> {
        // TODO(relay): enforce the checks mentioned in relay-streams.md
        Ok(IncomingStreamRequestDisposition::Accept)
    }
}
