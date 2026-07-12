use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct CreateLinkReq {
  pub url: String,
  #[serde(default)]
  pub headers: Vec<(String, String)>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct LinkResp {
  pub id: String,
  pub url: String,
  pub short_url: String,
  pub qr_url: String,
  // default keeps a newer CLI compatible with pre-headers lonkd responses
  #[serde(default)]
  pub headers: Vec<(String, String)>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct ValidResp {
  pub valid: bool,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub error: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct StatusResp {
  pub id: String,
  pub url: String,
  pub alive: bool,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub http_status: Option<u16>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub error: Option<String>,
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn valid_resp_omits_error_key_when_none() {
    let json = serde_json::to_string(&ValidResp {
      valid: true,
      error: None,
    })
    .unwrap();
    assert_eq!(json, r#"{"valid":true}"#);
  }

  #[test]
  fn link_resp_roundtrip() {
    let resp = LinkResp {
      id: "Ab3dEf9".into(),
      url: "https://example.com/x".into(),
      short_url: "/Ab3dEf9".into(),
      qr_url: "/Ab3dEf9/qr".into(),
      headers: vec![("X-A".into(), "1".into())],
    };
    let json = serde_json::to_string(&resp).unwrap();
    assert_eq!(serde_json::from_str::<LinkResp>(&json).unwrap(), resp);
  }

  #[test]
  fn link_resp_headers_default_when_absent() {
    // a pre-headers lonkd omits the headers key entirely
    let resp: LinkResp = serde_json::from_str(
      r#"{"id":"Ab3dEf9","url":"https://e.com/x","short_url":"/Ab3dEf9","qr_url":"/Ab3dEf9/qr"}"#,
    )
    .unwrap();
    assert_eq!(resp.headers, vec![]);
  }

  #[test]
  fn create_req_headers_default_to_empty() {
    let req: CreateLinkReq = serde_json::from_str(r#"{"url":"https://example.com/x"}"#).unwrap();
    assert_eq!(req.headers, vec![]);
    let req: CreateLinkReq =
      serde_json::from_str(r#"{"url":"https://e.com/x","headers":[["Set-Cookie","a=1"]]}"#)
        .unwrap();
    assert_eq!(
      req.headers,
      vec![("Set-Cookie".to_string(), "a=1".to_string())]
    );
  }

  #[test]
  fn status_resp_omits_absent_fields() {
    let json = serde_json::to_string(&StatusResp {
      id: "a".into(),
      url: "https://e.com/".into(),
      alive: false,
      http_status: None,
      error: Some("connect timeout".into()),
    })
    .unwrap();
    assert!(!json.contains("http_status"));
    assert!(json.contains("connect timeout"));
  }
}
