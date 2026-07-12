use std::fmt;

pub use url::Url;

pub mod paths;
pub mod types;

#[derive(Debug, PartialEq, Eq)]
pub enum ValidateError {
  Parse(String),
  Scheme(String),
}

impl fmt::Display for ValidateError {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      ValidateError::Parse(msg) => write!(f, "invalid url: {msg}"),
      ValidateError::Scheme(s) => write!(f, "url scheme must be http or https (got {s})"),
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
}
