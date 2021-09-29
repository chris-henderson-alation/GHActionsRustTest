// use error::httpcode::HttpCode;
// use error::Status;
// use kind::Kind;
// use rocket::serde::de::Error;
// use rocket::serde::Deserializer;
// use serde::Serializer;
// use serde::{Deserialize, Serialize};
// use serde_json::json;
// use std::fmt::{Display, Formatter};
//
// pub trait AcmError: std::error::Error + HttpCode + Kind + Send + Sync {}
//
// #[derive(Debug, Deserialize)]
// struct GenericError {
//     kind: String,
//     message: String,
//     cause: Option<String>,
//     code: u16,
// }
//
// impl Display for GenericError {
//     fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
//         f.write_str(&self.message)?;
//         if let Some(cause) = self.cause.as_ref() {
//             f.write_str(", Cause: ")?;
//             f.write_str(cause)?;
//         }
//         Ok(())
//     }
// }
//
// impl std::error::Error for GenericError {}
//
// impl HttpCode for GenericError {
//     fn http_code(&self) -> Status {
//         Status::from_code(self.code).expect("bad HTTP code received")
//     }
// }
//
// impl Kind for GenericError {
//     fn kind(&self) -> String {
//         self.kind.clone()
//     }
// }
//
// unsafe impl Send for GenericError {}
// unsafe impl Sync for GenericError {}
//
// impl AcmError for GenericError {}
//
// impl Serialize for Box<dyn AcmError> {
//     fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
//     where
//         S: Serializer,
//     {
//         json!({
//             "kind": self.kind(),
//             "message": format!("{}", self),
//             "cause": self.source().map(|cause| format!("{}", cause)),
//             "code": self.http_code().code
//         })
//         .serialize(serializer)
//     }
// }
//
// struct Response<T, E> {
//     payload: Payload<T, E>,
// }
//
// impl<T: Serialize + Kind, E> From<T> for Response<T, E> {
//     fn from(object: T) -> Self {
//         Response {
//             payload: Payload::Object(object),
//         }
//     }
// }
//
// impl<T> From<Box<dyn AcmError>> for Response<T, Box<dyn AcmError>> {
//     fn from(error: Box<dyn AcmError>) -> Self {
//         Response {
//             payload: Payload::Error(error),
//         }
//     }
// }
//
// impl<T: Serialize + Kind, E: Serialize + Kind> Serialize for Response<T, E> {
//     fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
//     where
//         S: Serializer,
//     {
//         self.payload.serialize(serializer)
//     }
// }
//
// enum Payload<T, E> {
//     Object(T),
//     Error(E),
// }
//
// impl<T: Serialize + Kind, E: Serialize + Kind> Serialize for Payload<T, E> {
//     fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
//     where
//         S: Serializer,
//     {
//         match self {
//             Payload::Object(object) => json!({
//                 "payload": { "kind": object.kind(), "object": object}, "errors": {}
//             }),
//             Payload::Error(error) => json!({
//                 "payload": {}, "errors": error
//             }),
//         }
//         .serialize(serializer)
//     }
// }
