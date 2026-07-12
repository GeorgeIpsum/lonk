use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct CreateLinkReq {
  pub url: String,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct LinkResp {
  pub id: String,
  pub url: String,
  pub short_url: String,
  pub qr_url: String,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct ValidResp {
  pub valid: bool,
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
    };
    let json = serde_json::to_string(&resp).unwrap();
    assert_eq!(serde_json::from_str::<LinkResp>(&json).unwrap(), resp);
  }
}
