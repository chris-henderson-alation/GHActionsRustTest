use convert_case::{Case, Casing};
use rand::{thread_rng, Rng};
use uuid::Uuid;

/// rfc1035_label returns a lowercase, hexadecimal encoded, UUID that is also
/// guaranteed to be valid RFC 1035 label. Please see [RFC 1035](https://datatracker.ietf.org/doc/html/rfc1035)
/// for more information.
///
/// The need for this functionality stems from Kubernetes' own validation requirements for DNS
/// labels. Please see [validation.go](https://github.com/kubernetes/kubernetes/blob/f3b98a08b05257fbc3c19b52ced70ea67c546b1e/staging/src/k8s.io/apimachinery/pkg/util/validation/validation.go#L135)
/// for K8s own implementation of this check.
///
/// Specifically, failure to comply results in the following error message from the API server.
///
/// ```text
/// a DNS-1035 label must consist of lower case alphanumeric characters or '-', start with an alphabetic character, and end with an alphanumeric character (e.g. 'my-name',  or 'abc-123', regex used for validation is '[a-z]([-a-z0-9]*[a-z0-9])?'
/// ```
///
/// With regards to usages with Kubernetes, this is used to generate unique tags for connector images.
pub fn rfc1035_label() -> String {
    let mut name = uuid();
    if !name.starts_with(char::is_alphabetic) {
        name.remove(0);
        name.insert(0, thread_rng().gen_range('a'..='z'))
    }
    name
}

const DEFAULT_IF_INVALID_SUBDOMAIN: &str = "invalid-rfc1123-connector-name";

/// rfc1123_subdomain takes in a string which is a prefix, normalizes it, and suffixes it
/// with the contents of a UUID (where at minimum eight bytes of the UUID are used).
///
/// Normalization:
/// * 1. All non-alphanumeric characters are converted to a space character.
///     * 1a. E.G. "Oracle Connector v.1.2.3:latest" is converted to "Oracle Connector v 1 2 3 latest"
/// * 2. The result of #1 is converted to a lowercase "kebab".
///     * 2a. E.G "oracle-connector-v-1-2-3-latest".
///     * 2b. If the result of #2 is empty, then "invalid-rfc1123-connector-name" is used as the prefix.
/// * 3. A lowercase, hexadecimal, UUID is suffixed to the output of #2.
///     * 3a. If the prefix + suffix length is less than or equal to 63, then that string is returned.
///     * 3b. If the prefix is too long to accommodate at least 8 bytes worth of UUID, then the
///             prefix is truncated to 54 bytes and 8 bytes worth of UUID is suffixed and returned.
///     * 3c. Otherwise, the UUID is truncated such that prefix + suffix is 63 bytes long.
///
/// Please see the following from [RFC 1123](https://datatracker.ietf.org/doc/html/rfc1123#section-6.1.3.5) with regard to DNS names.
///
/// ```text
/// RFC 1123 Section 6.1.3.5  Extensibility
///     The DNS defines domain name syntax very generally -- a
///     string of labels each containing up to 63 8-bit octets,
///     separated by dots, and with a maximum total of 255
///     octets.  Particular applications of the DNS are
///     permitted to further constrain the syntax of the domain
///     names they use, although the DNS deployment has led to
///     some applications allowing more general names.  In
///     particular, Section 2.1 of this document liberalizes
///     slightly the syntax of a legal Internet host name that
///     was defined in RFC-952 [DNS:4].
/// ```
///
/// With regards to usages with Kubernetes, this is used as the name for pods and services
/// since those names must be valid subdomains.
pub fn rfc1123_subdomain<T: AsRef<str>>(prefix: T) -> String {
    let mut uuid = uuid();
    let mut prefix = prefix
        .as_ref()
        .chars()
        .into_iter()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect::<String>()
        .to_case(Case::Kebab);
    if prefix.is_empty() {
        prefix = DEFAULT_IF_INVALID_SUBDOMAIN.to_string();
    }
    // +1/9 because of the hyphen that separates {prefix}-{uuid}
    if uuid.len() + prefix.len() + 1 <= 63 {
        // Case 3.a
    } else if prefix.len() + 9 > 63 {
        // Case 3.b
        prefix.truncate(63 - 9);
        uuid.truncate(8);
    } else {
        // Case 3.c
        let ulen = 63 - 1 - prefix.len();
        uuid.truncate(ulen);
    }
    // These assertions are only compiled into debug (dev/test) builds.
    debug_assert!(prefix.len() + uuid.len() <= 63);
    debug_assert!(uuid.len() >= 8);
    return format!("{}-{}", prefix, uuid);
}

/// Returns a randomly generated, lowercase, hexadecimal encoded, UUID string.
pub fn uuid() -> String {
    Uuid::from_u128(thread_rng().gen()).to_simple().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use regex::Regex;

    #[test]
    fn fuzz_rfc1035() {
        let r = Regex::new("^[a-z]([-a-z0-9]*[a-z0-9])?$").unwrap();
        for _ in 0..100000 {
            assert!(r.is_match(rfc1035_label().as_str()));
        }
    }

    #[test]
    fn test_complex_name() {
        let domain = rfc1123_subdomain(
            "Alation's Oracle Connecfor (OCF:v.1.23) aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                .to_string(),
        );
        assert!(domain.starts_with("alation-s-oracle-connecfor-ocf-v-1-23"));
        assert!(domain.len() <= 63);
    }

    #[test]
    fn test_empty_rfc1123() {
        let domain = rfc1123_subdomain("");
        assert!(domain.starts_with(DEFAULT_IF_INVALID_SUBDOMAIN));
    }

    #[test]
    fn test_invalid_prefix_rfc1123() {
        let domain = rfc1123_subdomain("🤮🤮🤮");
        assert!(domain.starts_with(DEFAULT_IF_INVALID_SUBDOMAIN));
    }

    #[test]
    fn test_case_3a_rfc1123() {
        // Full prefix and full UUID fits.
        let domain = rfc1123_subdomain("super cool connector v1.2");
        assert!(domain.starts_with("super-cool-connector-v-1-2"));
        assert_eq!(domain.len(), "super-cool-connector-v-1-2-".len() + 32);
    }

    #[test]
    fn test_case_3b_rfc1123() {
        // The prefix is so long that we truncate the UUID a bit, but at minimum we need
        // eight bytes worth off UUID.
        let domain =
            rfc1123_subdomain("super cool connector v1.2.123456789123456789123456789123456789");
        assert_eq!(
            domain.len(),
            "super-cool-connector-v-1-2-123456789123456789123456789-".len() + 8
        );
    }

    #[test]
    fn test_case_3c_rfc1123() {
        // The prefix is so long that we truncate the UUID a bit.
        let domain = rfc1123_subdomain("super cool connector v1.2.123456789");
        assert_eq!(
            domain.len(),
            "super-cool-connector-v-1-2-123456789-".len() + 26
        );
    }

    #[test]
    fn fuzz_rfc1123() {
        let mut rng = thread_rng();
        for _ in 0..10000 {
            let length = rng.gen_range(0..200);
            let test: String = (0..length).map(|_| rng.gen_range(' '..='~')).collect();
            let got = rfc1123_subdomain(test);
            assert!(got.len() <= 63);
            assert!(got.len() > 33);
            assert!(got.starts_with(char::is_alphanumeric));
        }
    }
}
