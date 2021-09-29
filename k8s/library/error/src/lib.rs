pub use error_derive::AcmError;
pub use httpcode;
pub use httpcode::{HttpCode, Status};
pub use kind::Kind;
use rocket::request::Request;
use rocket::response::Responder;
use serde::{Serialize, Serializer};
use serde_json::{json, to_string_pretty};
pub use thiserror;
pub use thiserror::Error;

/// An AcmError is the trait by which all errors returned by any ACM component
/// MUST adhere.
///
/// The easiest way to implement this error type is to utilize the derive
/// macros re-exported by this library. Notably, [Error](thiserror::Error),
/// [AcmError](error-derive::AcmError), [HttpCode](httpcode::HttpCode), and
/// [Kind](kind::Kind). [Debug](std::fmt::Debug) is required to fulfill the
/// standard library [Error](std::error::Error).
///
/// ```
/// use error::*;
///
/// #[derive(Error, AcmError, HttpCode, Kind, Debug)]
/// #[error(
/// "This is the string that will show up in the 'message' key of the resulting JSON. \
/// You can also (and should) interpolate date members into this string. For example, \
/// her is some {info} about this {action}!"
/// )]
/// #[code(Status::BadRequest)]
/// struct MyError {
///     action: String,
///     info: String,
///     // Any aggregated error type that is annotated as a source will be formatted
///     // and serialized into the 'cause' key of the resulting JSON.
///     #[source]
///     cause: std::io::Error,
/// }
/// ```
pub trait AcmError: std::error::Error + HttpCode + Kind + Send + Sync {}

/// This conversion supports the automatic boxing of any type that
/// implements [AcmError](crate::AcmError).
///
/// Note that this conversion results in an heap allocated error type with
/// dynamic dispatch (that is, it behaves more like an interface
/// object would in Java or Go).
impl<T: 'static + AcmError> From<T> for Box<dyn AcmError> {
    fn from(err: T) -> Self {
        Box::new(err)
    }
}

/// The [Serialize](serde::Serialize) trait implementation for an [AcmError](crate::AcmError)
/// is a JSON object. Give the following struct definition...
///
/// ```
/// use error::*;
///
/// #[derive(Error, AcmError, HttpCode, Kind, Debug)]
/// #[error(
/// "This is the string that will show up in the 'message' key of the resulting JSON."
/// )]
/// #[code(Status::BadRequest)]
/// struct MyError {
///     #[source]
///     cause: std::io::Error,
/// }
/// ```
///
/// ...the following JSON will be emitted.
///
/// ```ignore
/// {
///     "kind": "MyError",
///     "message": "This is the string that will show up in the 'message' key of the resulting JSON.",
///     "cause": "Failed to open file because of reasons."
/// }
/// ```
impl Serialize for Box<dyn AcmError> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        json!({
            "kind": self.kind(),
            "message": format!("{}", self),
            "cause": self.source().map(|cause| format!("{}", cause)),
        })
        .serialize(serializer)
    }
}

/// The [Responder](rocket::response::Responder) implementation for an [AcmError](crate::AcmError)
/// does three things:
///
/// 1. Sets the content type to JSON.
/// 2. Sets the HTTP status to the status declared in the error's `#[code(..)]` annotation.
/// 3. Serializes the error and sends the resulting bytes over the wire.
///
/// The resulting serialization is the following schema.
///
/// ```ignore
/// {
///     "payload": null,
///     "error": <See [AcmError::serialize](crate::AcmError::serialize)>
/// }
/// ```
impl<'r, 'o: 'r> Responder<'r, 'o> for Box<dyn AcmError> {
    fn respond_to(self, _: &'r Request<'_>) -> rocket::response::Result<'o> {
        let mut response = rocket::Response::build();
        response.header(rocket::http::ContentType::JSON);
        response.status(self.http_code());
        let json = json!({
            "payload": null,
            "error": self,
        });
        // @TODO it MIGHT be possible to fail here? No idea how. If so, can read the error here
        // and return that instead. I just have no idea what could ever cause it.
        let json =
            to_string_pretty(&json).unwrap_or_else(|_| panic!("failed to pretty print {}", json));
        response.sized_body(json.len(), std::io::Cursor::new(json));
        Ok(response.finalize())
    }
}

/// A `StringError` is a convenient way to convert a raw String type into a first class
/// AcmError. This is especially useful when you would like to embed a raw string as
/// [source](std::error::Error::source) for a higher AcmError.
///
/// The raw string, as is, is used as the [display](std::fmt::Display) for this type.
///
/// ```
/// use error::*;
///
/// #[derive(Error, AcmError, Kind, HttpCode, Debug)]
/// #[code(Status::InternalServerError)]
/// #[error("Something rather bad happened, and this is the gentle explanation.")]
/// struct HigherError {
///     #[source]
///     cause: StringError
/// }
///
/// fn do_work() -> Result<(), HigherError> {
///     Err(HigherError{cause: "and this is the gnarly explanation".into()})
/// }
/// ```
#[derive(Error, AcmError, Kind, HttpCode, Debug)]
#[code(Status::InternalServerError)]
#[error("{inner}")]
pub struct StringError {
    inner: String,
}

impl<T: AsRef<str>> From<T> for StringError {
    fn from(inner: T) -> Self {
        Self {
            inner: inner.as_ref().to_string(),
        }
    }
}

impl From<Box<dyn AcmError>> for StringError {
    fn from(inner: Box<dyn AcmError>) -> Self {
        Self {
            inner: format!("{:?}", inner),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket::get;
    use rocket::local::blocking::Client;
    use rocket::routes;

    #[derive(AcmError, Error, Kind, HttpCode, Debug)]
    #[error("Nice catch Blanco Niño")]
    #[code(rocket::http::Status::BadGateway)]
    struct TooBad {}

    #[get("/")]
    async fn fail_without_cause() -> std::result::Result<(), Box<dyn AcmError>> {
        Err(TooBad {}.into())
    }

    #[test]
    fn without_cause() {
        let client =
            Client::tracked(rocket::build().mount("/", routes![fail_without_cause])).unwrap();
        let response = client.get("/").dispatch();
        assert_eq!(response.status(), rocket::http::Status::BadGateway);
        let got: serde_json::Value = response.into_json().unwrap();
        let want = serde_json::json!({
            "payload": null,
            "error": {
                "kind": "TooBad",
                "message": "Nice catch Blanco Niño",
                "cause": null
            }
        });
        assert_eq!(got, want)
    }

    #[derive(AcmError, Error, Kind, HttpCode, Debug)]
    #[error("You got sacked")]
    #[code(rocket::http::Status::NotFound)]
    struct TooBadWithCause {
        #[from]
        bad_guy: TooBad,
    }

    #[get("/")]
    async fn fail_with_cause() -> std::result::Result<(), Box<dyn AcmError>> {
        Err(TooBadWithCause::from(TooBad {}).into())
    }

    #[test]
    fn with_cause() {
        let client = Client::tracked(rocket::build().mount("/", routes![fail_with_cause])).unwrap();
        let response = client.get("/").dispatch();
        assert_eq!(response.status(), rocket::http::Status::NotFound);
        let got: serde_json::Value = response.into_json().unwrap();
        let want = serde_json::json!({
            "payload": null,
            "error": {
                "kind": "TooBadWithCause",
                "message": "You got sacked",
                "cause": "Nice catch Blanco Niño"
            }
        });
        assert_eq!(got, want)
    }
}
