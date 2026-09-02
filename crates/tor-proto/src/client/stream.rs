//! Implements Tor's "stream"s from a client perspective
//!
//! A stream is an anonymized conversation; multiple streams can be
//! multiplexed over a single circuit.
//!
//! To create a stream, use [crate::client::ClientTunnel::begin_stream].
//!
//! # Limitations
//!
//! There is no fairness, rate-limiting, or flow control.

#[cfg(feature = "stream-ctrl")]
mod ctrl;
mod data;
mod params;
mod resolve;

pub use data::{DataReader, DataStream, DataWriter};

pub use params::StreamParameters;
pub use resolve::ResolveStream;
pub(crate) use {data::OutboundDataCmdChecker, resolve::ResolveCmdChecker};

pub use tor_cell::relaycell::msg::IpVersionPreference;

#[cfg(feature = "stream-ctrl")]
pub use {ctrl::ClientStreamCtrl, data::ClientDataStreamCtrl};
