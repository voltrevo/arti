//! Functionality for negotiating protocol capabilities.

// TODO #2598: Remove duplicate code between tor_hsclient::caps,
// tor_hsservice::caps, and tor_proto::circuit (to the extent possible).

use cfg_if::cfg_if;

use tor_cell::relaycell::hs::intro_payload::IntroduceHandshakePayload;
use tor_netdoc::doc::hsdesc::HsDesc;
use tor_protover::Protocols;

cfg_if! {
    if #[cfg(feature = "negotiate-extensions")] {
        use tor_protover::{NamedSubver, NumberedSubver, named::*};

        /// An array of protocols that can be requested via a subprotocol
        /// request extension.
        ///
        /// Note that we don't necessarily support all of these ourselves!
        static REQUESTABLE_LIST: &[NamedSubver] = &[RELAY_CRYPT_CGO];
    }
}

/// A set of peer capabilities, derived from a service's hsdesc.
#[derive(Clone, Debug)]
pub(crate) struct PeerCaps {
    /// A list of the protocol capabilities that we share with the service,
    /// and are able to negotiate with it.  This includes flowctrl,
    /// which is negotiated with a different extension than the others.
    #[cfg(feature = "negotiate-extensions")]
    shared_protos: Protocols,

    /// A list of the protocols that we intend to request from the service
    /// via the SUBPROTOCOL_REQUEST extension.
    #[cfg(feature = "negotiate-extensions")]
    request_protos: Protocols,

    /// Our chosen cc_sendme_inc value to share with the service.
    ///
    /// This is present if and only if `shared_protos` contains `FLOWCTRL_CC`
    #[cfg(feature = "negotiate-extensions")]
    sendme_inc: Option<u8>,
}

#[cfg(feature = "negotiate-extensions")]
impl PeerCaps {
    /// Construct a new PeerCaps for a peer, given its descriptor.
    pub(crate) fn new(desc: &HsDesc) -> Self {
        let supported = tor_proto::supported_client_protocols();

        let (fc_protos, sendme_inc) = if let Some((fcp, inc)) = desc.flow_control()
            && fcp.supports_named_subver(FLOWCTRL_CC)
            && supported.supports_named_subver(FLOWCTRL_CC)
        {
            ([FLOWCTRL_CC].into_iter().collect(), Some(inc.into()))
        } else {
            (Protocols::new(), None)
        };

        let mut subproto_request = Vec::new();
        let mutual_protos = desc.declared_capabilities().intersection(&supported);

        // Note that we can't just do "include CGO if present" since it has prerequisites.
        if mutual_protos.supports_named_subver(RELAY_CRYPT_CGO)
            && mutual_protos.supports_named_subver(RELAY_NEGOTIATE_SUBPROTO)
            && fc_protos.supports_named_subver(FLOWCTRL_CC)
        {
            subproto_request.push(RELAY_CRYPT_CGO);
        }

        let request_protos: Protocols = subproto_request.into_iter().collect();

        let shared_protos = request_protos.union(&fc_protos);

        PeerCaps {
            shared_protos,
            request_protos,
            sendme_inc,
        }
    }

    /// Add all relevant request extensions for this PeerCaps into `intro`.
    pub(crate) fn add_extensions(&self, intro: &mut IntroduceHandshakePayload) {
        use tor_cell::relaycell::extend::{CcRequest, SubprotocolRequest};

        if self.sendme_inc.is_some() {
            intro.add_extension(CcRequest::default().into());
        }
        if !self.request_protos.is_empty() {
            let ext: SubprotocolRequest = REQUESTABLE_LIST
                .iter()
                .filter(|p| self.request_protos.supports_named_subver(**p))
                .map(|p| NumberedSubver::from(*p))
                .collect();
            intro.add_extension(ext.into());
        }
    }

    /// Return a list of the negotiable protocols that we share with the peer.
    pub(crate) fn shared_protos(&self) -> Protocols {
        self.shared_protos.clone()
    }

    /// Return the `sendme_inc` value that we decided to use.
    ///
    /// Returns None if we aren't using congestion control.
    pub(crate) fn cc_sendme_inc(&self) -> Option<u8> {
        self.sendme_inc
    }
}

#[cfg(not(feature = "negotiate-extensions"))]
impl PeerCaps {
    /// Construct a new PeerCaps for a peer, given its descriptor.
    pub(crate) fn new(_desc: &HsDesc) -> Self {
        Self {}
    }

    /// Add all relevant request extensions for this PeerCaps into `intro`.
    pub(crate) fn add_extensions(&self, _intro: &mut IntroduceHandshakePayload) {}

    /// Return a list of the negotiable protocols that we share with the peer.
    pub(crate) fn shared_protos(&self) -> Protocols {
        // Note that we actually share all the must-implement client protocols.
        // But we cannot assume that here; some protocols must be negotiated.
        Protocols::new()
    }

    /// Return the `sendme_inc` value that we decided to use.
    ///
    /// Returns None if we aren't using congestion control.
    pub(crate) fn cc_sendme_inc(&self) -> Option<u8> {
        None
    }
}
