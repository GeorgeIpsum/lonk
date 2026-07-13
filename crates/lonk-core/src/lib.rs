use std::fmt;

pub use url::Url;

pub mod paths;
pub mod types;

#[derive(Debug, PartialEq, Eq)]
pub enum ValidateError {
  Parse(String),
  Scheme(String),
  Header(String),
}

impl fmt::Display for ValidateError {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      ValidateError::Parse(msg) => write!(f, "invalid url: {msg}"),
      ValidateError::Scheme(s) => write!(f, "url scheme must be http or https (got {s})"),
      ValidateError::Header(msg) => write!(f, "{msg}"),
    }
  }
}

impl std::error::Error for ValidateError {}

/// The single source of truth for what lonk accepts as a shortenable URL.
pub fn validate_url(input: &str) -> Result<Url, ValidateError> {
  let parsed = Url::parse(input).map_err(|e| ValidateError::Parse(e.to_string()))?;
  if !matches!(parsed.scheme(), "http" | "https") {
    return Err(ValidateError::Scheme(parsed.scheme().to_string()));
  }
  Ok(parsed)
}

/// Maximum header pairs a single link may carry (enforced by API and CLI).
pub const MAX_HEADERS_PER_LINK: usize = 16;

const HEADER_DENYLIST: &[&str] = &[
  "location",
  "content-length",
  "transfer-encoding",
  "connection",
  "content-type",
  "content-encoding",
];

fn is_token_byte(b: u8) -> bool {
  b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b)
}

/// Validate one custom response-header pair for a link.
pub fn validate_header(name: &str, value: &str) -> Result<(), ValidateError> {
  if name.is_empty() || name.len() > 128 {
    return Err(ValidateError::Header(format!(
      "header name must be 1-128 bytes (got {})",
      name.len()
    )));
  }
  if !name.bytes().all(is_token_byte) {
    return Err(ValidateError::Header(format!(
      "invalid header name {name:?}"
    )));
  }
  if HEADER_DENYLIST.contains(&name.to_ascii_lowercase().as_str()) {
    return Err(ValidateError::Header(format!("header not allowed: {name}")));
  }
  if value.len() > 1024 {
    return Err(ValidateError::Header(format!(
      "header value too long ({} bytes, max 1024)",
      value.len()
    )));
  }
  if !value
    .bytes()
    .all(|b| b == b'\t' || (b' '..=b'~').contains(&b))
  {
    return Err(ValidateError::Header(
      "header value contains control or non-ASCII bytes".to_string(),
    ));
  }
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn accepts_http_and_https() {
    assert!(validate_url("http://example.com/a").is_ok());
    assert!(validate_url("https://example.com/a?b=c&d=e").is_ok());
  }

  #[test]
  fn rejects_unparseable() {
    match validate_url("not a url") {
      Err(ValidateError::Parse(msg)) => assert!(!msg.is_empty()),
      other => panic!("expected Parse error, got {other:?}"),
    }
  }

  #[test]
  fn rejects_non_http_scheme() {
    match validate_url("ftp://example.com/f") {
      Err(ValidateError::Scheme(s)) => assert_eq!(s, "ftp"),
      other => panic!("expected Scheme error, got {other:?}"),
    }
  }

  #[test]
  fn error_messages_are_human_readable() {
    let e = validate_url("ftp://x.example").unwrap_err();
    assert_eq!(e.to_string(), "url scheme must be http or https (got ftp)");
  }

  #[test]
  fn header_accepts_common_cases() {
    assert!(validate_header("X-Custom-Header", "hello world").is_ok());
    assert!(validate_header("Set-Cookie", "a=1; Path=/; HttpOnly").is_ok());
    assert!(validate_header("Cache-Control", "no-store").is_ok());
    assert!(validate_header("X-Empty", "").is_ok());
    assert!(validate_header("X-Tab", "a\tb").is_ok());
  }

  #[test]
  fn header_rejects_bad_names() {
    for name in ["", "Bad Name", "Bad:Name", "Bad\nName", "Béader"] {
      assert!(
        validate_header(name, "v").is_err(),
        "accepted name {name:?}"
      );
    }
    assert!(validate_header(&"a".repeat(129), "v").is_err());
    assert!(validate_header(&"a".repeat(128), "v").is_ok());
  }

  #[test]
  fn header_rejects_injection_in_value() {
    for value in ["a\r\nSet-Cookie: evil=1", "a\rb", "a\nb", "a\0b", "café"] {
      assert!(
        validate_header("X-Test", value).is_err(),
        "accepted value {value:?}"
      );
    }
    assert!(validate_header("X-Test", &"v".repeat(1025)).is_err());
    assert!(validate_header("X-Test", &"v".repeat(1024)).is_ok());
  }

  #[test]
  fn header_denylist_is_case_insensitive() {
    for name in [
      "Location",
      "location",
      "CONTENT-LENGTH",
      "Transfer-Encoding",
      "connection",
      "Content-Type",
      "content-encoding",
    ] {
      assert!(
        validate_header(name, "v").is_err(),
        "accepted denylisted {name}"
      );
    }
  }
}
