use error::AcmError;

/// A Result is an alias of [std::result::Result](std::result::Result) with its error variant
/// pre-populated with a `Box<dyn AcmError>`. This allows for shorter
/// notation throughout the codebase.
///
/// For example, instead of writing...
///
/// ```
/// use error::AcmError;
///
/// fn greet() -> Result<&'static str, Box<dyn AcmError>> {
///     Ok("Hello, Alation!")
/// }
/// ```
///
/// ...you can simply say...
///
/// ```
/// use error::AcmError;
/// use result::Result;
///
/// fn greet() -> Result<&'static str> {
///     Ok("Hello, Alation!")
/// }
/// ```
///
/// It also helps in easily identifying if any functions are returning errors
/// BEFORE converting them into project native [AcmError](error::AcmError)s.
pub type Result<T> = std::result::Result<T, Box<dyn AcmError>>;
