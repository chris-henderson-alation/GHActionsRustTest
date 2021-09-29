pub use httpcode_derive::*;
pub use rocket::http::Status;

/// A type that implements the HttpCode trait will be able to communicate what the
/// status should set to by the HTTP framework should an instance of the type be
/// returned to the caller.
///
/// The easiest way to implement HttpCode is via the derive macro.
///
/// ```
/// use httpcode::{HttpCode, Status};
///
/// #[derive(HttpCode)]
/// #[code(Status::ServiceUnavailable)]
/// struct ServerDown {}
/// ```
///
/// See <https://api.rocket.rs/v0.5-rc/rocket/http/struct.Status.html> for a full list of
/// available return code.
pub trait HttpCode {
    fn http_code(&self) -> Status;
}

#[cfg(test)]
mod tests {
    use crate as httpcode;
    use httpcode::*;

    #[derive(HttpCode)]
    #[code(httpcode::Status::BadGateway)]
    struct Struct {}

    #[derive(HttpCode)]
    enum Enum {
        #[code(httpcode::Status::NotFound)]
        Badness,
        #[code(httpcode::Status::Ok)]
        NotSoBadness,
        #[code(httpcode::Status::new(1000))]
        Custom,
    }

    #[test]
    fn smoke() {
        assert_eq!(httpcode::Status::BadGateway, Struct {}.http_code());
        assert_eq!(httpcode::Status::NotFound, Enum::Badness.http_code());
        assert_eq!(httpcode::Status::Ok, Enum::NotSoBadness.http_code());
        assert_eq!(httpcode::Status::new(1000), Enum::Custom.http_code());
    }
}
