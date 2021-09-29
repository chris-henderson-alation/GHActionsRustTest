use error::*;

#[derive(Error, Kind, AcmError, HttpCode, Debug)]
pub enum ApiError {
    #[error("The Kubernetes API server rejected our request")]
    #[code(Status::InternalServerError)]
    Api(#[source] kube::Error),
    #[error("Failed to connect to the Kubernetes API server")]
    #[code(Status::InternalServerError)]
    Connection(#[source] kube::Error),
    // @TODO so many things can go wrong in theory. Too little time to explicitly account for them all.
    #[error("The Kubernetes API server rejected our request")]
    #[code(Status::InternalServerError)]
    Rest(#[source] kube::Error),
}

impl From<kube::Error> for ApiError {
    fn from(err: kube::Error) -> Self {
        match err {
            kube::Error::Api(_) => ApiError::Api(err),
            kube::Error::Connection(_) => ApiError::Connection(err),
            // @TODO there are a LOT of things that go wrong. The above are the most common
            // but just look at this list...it's good to know but we have received far too
            // much pressure to release early to sit down and account and test for all of these.
            _ => ApiError::Rest(err), // Error::HyperError(_) => {}
                                      // Error::Service(_) => {}
                                      // Error::FromUtf8(_) => {}
                                      // Error::LinesCodecMaxLineLengthExceeded => {}
                                      // Error::ReadEvents(_) => {}
                                      // Error::HttpError(_) => {}
                                      // Error::InvalidUri(_) => {}
                                      // Error::SerdeError(_) => {}
                                      // Error::RequestBuild => {}
                                      // Error::RequestSend => {}
                                      // Error::RequestParse => {}
                                      // Error::RequestValidation(_) => {}
                                      // Error::Kubeconfig(_) => {}
                                      // Error::Discovery(_) => {}
                                      // Error::SslError(_) => {}
                                      // Error::OpensslError(_) => {}
                                      // Error::ProtocolSwitch(_) => {}
                                      // Error::MissingUpgradeWebSocketHeader => {}
                                      // Error::MissingConnectionUpgradeHeader => {}
                                      // Error::SecWebSocketAcceptKeyMismatch => {}
                                      // Error::SecWebSocketProtocolMismatch => {}
        }
    }
}

// This is a copy paste of the API errors possible just for keeping notes to myself.

// #[cfg_attr(docsrs, doc(cfg(any(feature = "config", feature = "client"))))]
// #[derive(Error, Debug)]
// pub enum Error {
//     /// ApiError for when things fail
//     ///
//     /// This can be parsed into as an error handling fallback.
//     /// It's also used in `WatchEvent` from watch calls.
//     ///
//     /// It's quite common to get a `410 Gone` when the `resourceVersion` is too old.
//     #[error("ApiError: {0} ({0:?})")]
//     Api(#[source] ErrorResponse),
//
//     /// ConnectionError for when TcpStream fails to connect.
//     #[error("ConnectionError: {0}")]
//     Connection(std::io::Error),
//
//     /// Hyper error
//     #[cfg(feature = "client")]
//     #[error("HyperError: {0}")]
//     HyperError(#[from] hyper::Error),
//     /// Service error
//     #[cfg(feature = "client")]
//     #[error("ServiceError: {0}")]
//     Service(tower::BoxError),
//
//     /// UTF-8 Error
//     #[error("UTF-8 Error: {0}")]
//     FromUtf8(#[from] std::string::FromUtf8Error),
//
//     /// Returned when failed to find a newline character within max length.
//     /// Only returned by `Client::request_events` and this should never happen as
//     /// the max is `usize::MAX`.
//     #[error("Error finding newline character")]
//     LinesCodecMaxLineLengthExceeded,
//
//     /// Returned on `std::io::Error` when reading event stream.
//     #[error("Error reading events stream: {0}")]
//     ReadEvents(std::io::Error),
//
//     /// Http based error
//     #[error("HttpError: {0}")]
//     HttpError(#[from] http::Error),
//
//     /// Failed to construct a URI.
//     #[error(transparent)]
//     InvalidUri(#[from] http::uri::InvalidUri),
//
//     /// Common error case when requesting parsing into own structs
//     #[error("Error deserializing response")]
//     SerdeError(#[from] serde_json::Error),
//
//     /// Error building a request
//     #[error("Error building request")]
//     RequestBuild,
//
//     /// Error sending a request
//     #[error("Error executing request")]
//     RequestSend,
//
//     /// Error parsing a response
//     #[error("Error parsing response")]
//     RequestParse,
//
//     /// A request validation failed
//     #[error("Request validation failed with {0}")]
//     RequestValidation(String),
//
//     /// Configuration error
//     #[error("Error loading kubeconfig: {0}")]
//     Kubeconfig(#[from] ConfigError),
//
//     /// Discovery errors
//     #[error("Error from discovery: {0}")]
//     Discovery(#[from] DiscoveryError),
//
//     /// An error with configuring SSL occured
//     #[error("SslError: {0}")]
//     SslError(String),
//
//     /// An error from openssl when handling configuration
//     #[cfg(feature = "native-tls")]
//     #[cfg_attr(docsrs, doc(cfg(feature = "native-tls")))]
//     #[error("OpensslError: {0}")]
//     OpensslError(#[from] openssl::error::ErrorStack),
//
//     /// The server did not respond with [`SWITCHING_PROTOCOLS`] status when upgrading the
//     /// connection.
//     ///
//     /// [`SWITCHING_PROTOCOLS`]: http::status::StatusCode::SWITCHING_PROTOCOLS
//     #[cfg(feature = "ws")]
//     #[cfg_attr(docsrs, doc(cfg(feature = "ws")))]
//     #[error("Failed to switch protocol. Status code: {0}")]
//     ProtocolSwitch(http::status::StatusCode),
//
//     /// `Upgrade` header was not set to `websocket` (case insensitive)
//     #[cfg(feature = "ws")]
//     #[cfg_attr(docsrs, doc(cfg(feature = "ws")))]
//     #[error("Upgrade header was not set to websocket")]
//     MissingUpgradeWebSocketHeader,
//
//     /// `Connection` header was not set to `Upgrade` (case insensitive)
//     #[cfg(feature = "ws")]
//     #[cfg_attr(docsrs, doc(cfg(feature = "ws")))]
//     #[error("Connection header was not set to Upgrade")]
//     MissingConnectionUpgradeHeader,
//
//     /// `Sec-WebSocket-Accept` key mismatched.
//     #[cfg(feature = "ws")]
//     #[cfg_attr(docsrs, doc(cfg(feature = "ws")))]
//     #[error("Sec-WebSocket-Accept key mismatched")]
//     SecWebSocketAcceptKeyMismatch,
//
//     /// `Sec-WebSocket-Protocol` mismatched.
//     #[cfg(feature = "ws")]
//     #[cfg_attr(docsrs, doc(cfg(feature = "ws")))]
//     #[error("Sec-WebSocket-Protocol mismatched")]
//     SecWebSocketProtocolMismatch,
// }
