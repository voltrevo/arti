//! Error types from [`Uploader`](super::Uploader)s
//! and [`Publisher`](super::Publisher)s.
use std::{sync::Arc, time::Duration};
use tor_error::{Bug, HasKind, internal};

/// Trait to represent the kinds of errors that we generally return in Arti.
//
// TODO: Move this to tor-error?
// NOTE: Adding clone makes this no longer dyn-safe.
pub trait TorError:
    HasKind + std::fmt::Debug + std::fmt::Display + std::error::Error + Send + Sync + 'static
{
}
impl<T> TorError for T where
    T: HasKind + std::fmt::Debug + std::fmt::Display + std::error::Error + Send + Sync + 'static
{
}

/// A failure to upload a document to a single target.
#[derive(Clone, Debug, thiserror::Error)]
#[non_exhaustive]
pub enum UploadError {
    /// The target refused the document, and we should not retry.
    #[error("Directory rejected document, saying {0}")]
    Rejected(Rejection),

    /// We were unable to make a direct connection to a directory.
    #[error("Unable to connect to directory")]
    Connect(#[source] Arc<std::io::Error>),

    /// The response we received appears to violate some part of the protocol.
    #[error("Invalid response: {0}")]
    InvalidResponse(#[source] Arc<dyn TorError>),

    /// Our attempt to upload the document took too long.
    #[error("Upload attempt took too long")]
    Timeout,

    /// The target told us to come back later.
    ///
    /// If a duration is provided, we should not come back
    /// before the provided duration.
    #[error("Directory told us to come back later.")]
    Deferred {
        /// The status code we received
        status_code: u16,

        /// The HTTP message we received
        message: String,

        /// The amount of time we were told to wait (or decided to wait)
        how_long: Option<Duration>,
    },

    /// An error from tor_dirclient that did not fall into any other category.
    ///
    /// (Don't construct this directly; instead, use From.)
    #[error("Unable to upload document")]
    SendRequest(#[source] Box<tor_dirclient::RequestError>),

    /// The upload failed for some other non-retriable reason.
    #[error("Document upload failed; cannot retry at this target")]
    DocumentFailedPermanently(#[source] Arc<dyn TorError>),

    /// The upload failed for some other retriable reason.
    ///
    /// (This variant is here so we can construct [`UploadError`]s from anywhere in Arti.)
    #[error("Directory upload failed")]
    Other(#[source] Arc<dyn TorError>),

    /// A bug occurred.
    #[error("Bug occurred while uploading")]
    Bug(#[source] Bug),
}

impl UploadError {
    /// If this error has a suggested delay before retrying, return that delay.
    pub(crate) fn suggested_delay(&self) -> Option<Duration> {
        match self {
            UploadError::Deferred { how_long, .. } => *how_long,
            _ => None,
        }
    }
}

/// A record of a rejection from an upload target.
#[derive(Clone, Debug)]
pub struct Rejection {
    /// The provided message when the document was rejected.
    message: String,
}

impl Rejection {
    /// Create a new rejection from a message returned by an upload target.
    pub fn from_message(message: String) -> Self {
        Rejection { message }
    }

    /// Return the message returned by the upload target.
    pub fn message(&self) -> &str {
        self.message.as_str()
    }
}

impl std::fmt::Display for Rejection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.message)
    }
}

impl From<tor_dirclient::RequestError> for UploadError {
    fn from(err: tor_dirclient::RequestError) -> Self {
        use UploadError as E;
        use tor_dirclient::RequestError as RE;
        match err {
            RE::DirTimeout => E::Timeout,
            RE::ResponseTooLong(_)
            | RE::HeadersTooLong(_)
            | RE::Utf8Encoding(_)
            | RE::HttpError(_)
            | RE::HttparseError(_) => E::InvalidResponse(Arc::new(err)),
            RE::HttpStatus(status_code, message) => {
                match Self::from_http_response(status_code, Some(message)) {
                    Some(e) => e,
                    None => E::Bug(internal!(
                        "Unexpected successful status code {status_code:?}"
                    )),
                }
            }
            RE::IoError(_)
            | RE::TruncatedHeaders
            | RE::Proto(_)
            | RE::Tunnel(_)
            | RE::TooMuchClockSkew
            | RE::EmptyRequest
            | RE::EmptyResponse
            | RE::ContentEncoding(_) => E::SendRequest(Box::new(err)),
            // We need this catch-all since the tor_dirclient::RequestError is non-exhaustive.
            other => E::SendRequest(Box::new(other)),
        }
    }
}

impl From<tor_dirclient::Error> for UploadError {
    fn from(err: tor_dirclient::Error) -> Self {
        use tor_dirclient::Error as DE;
        match err {
            DE::CircMgr(error) => Self::Other(Arc::new(error) as _),
            DE::RequestFailed(request_failed_error) => Self::from(request_failed_error.error),
            DE::Bug(bug) => Self::Bug(bug),
            other => Self::Other(Arc::new(other) as _),
        }
    }
}

/// Error type for an invalid http status code.
#[derive(Clone, Debug, thiserror::Error)]
#[error("Invalid http response code {0:?}")]
struct BadStatusCode(u16);

impl tor_error::HasKind for BadStatusCode {
    fn kind(&self) -> tor_error::ErrorKind {
        tor_error::ErrorKind::TorProtocolViolation
    }
}

impl UploadError {
    /// Construct a `Result<(), UploadError>` from a [`tor_dirclient::DirResponse`].
    pub(crate) fn from_directory_response(
        response: Result<tor_dirclient::DirResponse, tor_dirclient::Error>,
    ) -> Result<(), UploadError> {
        let response = response.map_err(Self::from)?;
        if let Some(e) = response.error() {
            return Err(Self::from(e.clone()));
        }

        match Self::from_http_response(
            response.status_code(),
            response.status_message().map(String::from),
        ) {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    /// If the status code was 503, and no delay was provided,
    /// set the delay to `duration`
    pub(crate) fn set_503_delay(mut self, duration: Duration) -> Self {
        match &mut self {
            Self::Deferred {
                status_code: 503,
                how_long,
                ..
            } if how_long.is_none() => {
                *how_long = Some(duration);
            }
            _ => {}
        }
        self
    }

    /// Return true if we can retry after this error.
    pub(crate) fn is_retriable(&self) -> bool {
        match self {
            UploadError::Rejected(_) => false,
            UploadError::DocumentFailedPermanently(_) => false,

            UploadError::Connect(_) => true,
            UploadError::InvalidResponse(_) => true,
            UploadError::Timeout => true,
            UploadError::Deferred { .. } => true,
            UploadError::SendRequest(_) => true,
            UploadError::Other(_) => true,
            UploadError::Bug(_) => true,
        }
    }

    /// Construct an [`UploadError`] from a status code and message.
    ///
    /// Return None if the status code indicates success.
    fn from_http_response(status_code: u16, message: Option<String>) -> Option<Self> {
        use UploadError as E;
        Some(match status_code {
            200 => return None,
            400..=499 => E::Rejected(Rejection::from_message(
                message.unwrap_or_else(|| "Refused".into()),
            )),
            500..=599 => E::Deferred {
                status_code,
                message: message.unwrap_or_else(|| "Server Error".into()),
                how_long: None,
            },
            // TODO: We don't historically handle 3xx or 1xx or any 2xx except 200 while uploading,
            // but probably we should.
            100..=399 => return None,
            // Anything else is invalid HTTP.
            _ => E::InvalidResponse(Arc::new(BadStatusCode(status_code)) as _),
        })
    }
}

impl tor_error::HasKind for UploadError {
    fn kind(&self) -> tor_error::ErrorKind {
        use UploadError as E;
        use tor_error::ErrorKind as EK;
        match self {
            E::Rejected(_) => EK::TorDocumentRejected,
            E::Connect(_) => EK::LocalNetworkError,
            E::InvalidResponse(_) => EK::TorProtocolViolation,
            E::Timeout => EK::TorNetworkTimeout,
            E::Deferred { .. } => EK::RelayTooBusy,
            E::SendRequest(e) => e.kind(),
            E::DocumentFailedPermanently(e) => e.kind(),
            E::Other(e) => e.kind(),
            E::Bug(e) => e.kind(),
        }
    }
}
