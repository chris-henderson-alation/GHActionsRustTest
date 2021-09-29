use kind::Kind;
use rocket::request::Request;
use rocket::response::Responder;
use serde::Serialize;
use serde_json::{json, to_string_pretty};

/// A Response may be constructed from any type that implements both
/// [Serialize](serde::Serialize) and [Kind](kind::Kind).
///
/// Once constructed, the resulting Response may be returned to the
/// HTTP framework where it will properly handle tasks such as
/// setting content headers, serializing your data, setting HTTP
/// response codes, and so on.
///
/// The following is an example usage.
///
/// ```
/// use serde::Serialize;
/// use response::Response;
/// use result::Result;
/// use kind::Kind;
/// use rocket::get;
///
/// #[derive(Serialize, Kind)]
/// struct Pod {}
///
/// #[get("/")]
/// async fn deploy() -> Result<Response<Pod>> {
///     Ok(Pod{}.into())
/// }
/// ```
pub struct Response<T> {
    payload: T,
}

/// A Response may be constructed from any type that implements both
/// [Serialize](serde::Serialize) and [Kind](kind::Kind) due to
/// this blanket implementation.
impl<T: Serialize + Kind> From<T> for Response<T> {
    fn from(payload: T) -> Self {
        Self { payload }
    }
}

/// The [Responder](rocket::response::Responder) implementation for a [Response](crate::Response)
/// does three things:
///
/// 1. Sets the content type to JSON.
/// 2. Sets the HTTP status to 200 (OK).
/// 3. Serializes the aggregated data and sends the resulting bytes over the wire.
///
/// The resulting serialization is the following schema.
///
/// ```ignore
/// {
///     "payload": {<object>},
///     "error": null
/// }
/// ```
impl<'r, 'o: 'r, T: Serialize + Kind> Responder<'r, 'o> for Response<T> {
    fn respond_to(self, _: &'r Request<'_>) -> rocket::response::Result<'o> {
        let mut response = rocket::Response::build();
        response.header(rocket::http::ContentType::JSON);
        response.status(rocket::http::Status::Ok);
        let json = json!({
            "payload": {
                "kind": self.payload.kind(),
                "object": self.payload
            },
            "error": null,
        });
        // @TODO it MIGHT be possible to fail here? No idea how. If so, can read the error here
        // and return that instead. I just have no idea what could ever cause it.
        let json = to_string_pretty(&json).expect(&format!("failed to pretty print {}", json));
        response.sized_body(json.len(), std::io::Cursor::new(json));
        Ok(response.finalize())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use result::Result;
    use rocket::get;
    use rocket::local::blocking::Client;
    use rocket::routes;

    #[get("/")]
    async fn greet() -> Result<Response<String>> {
        Ok("Hello, Alation!".to_string().into())
    }

    #[test]
    fn test_string() {
        let client = Client::tracked(rocket::build().mount("/", routes![greet])).unwrap();
        let response = client.get("/").dispatch();
        assert_eq!(response.status(), rocket::http::Status::Ok);
        let got: serde_json::Value = response.into_json().unwrap();
        let want = serde_json::json!({
            "payload": {
                "kind": "String",
                "object": "Hello, Alation!"
            },
            "error": null
        });
        assert_eq!(got, want)
    }

    #[derive(Serialize, Kind)]
    struct Pod {
        name: String,
        metadata: Metadata,
    }

    #[derive(Serialize)]
    struct Metadata {
        number: u32,
        bool: bool,
        arr: Vec<String>,
    }

    #[get("/")]
    async fn get_pod() -> Result<Response<Pod>> {
        Ok(Pod {
            name: "Bob".to_string(),
            metadata: Metadata {
                number: 1,
                bool: true,
                arr: vec!["this".to_string(), "and".to_string(), "that".to_string()],
            },
        }
        .into())
    }

    #[test]
    fn test_struct() {
        let client = Client::tracked(rocket::build().mount("/", routes![get_pod])).unwrap();
        let response = client.get("/").dispatch();
        assert_eq!(response.status(), rocket::http::Status::Ok);
        let got: serde_json::Value = response.into_json().unwrap();
        let want = serde_json::json!({
            "payload": {
                "kind": "Pod",
                "object": {
                    "name": "Bob",
                    "metadata": {
                        "number": 1,
                        "bool": true,
                        "arr": ["this", "and", "that"]
                    }
                }
            },
            "error": null
        });
        assert_eq!(got, want)
    }
}
