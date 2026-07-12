use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Default, Debug, PartialEq)]
pub struct Config {
  #[serde(default)]
  pub profiles: BTreeMap<String, Profile>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
pub struct Profile {
  pub base_url: String,
}

pub fn config_path() -> PathBuf {
  lonk_core::paths::config_dir().join("config.toml")
}

impl Config {
  pub fn load(path: &Path) -> Result<Self, String> {
    match std::fs::read_to_string(path) {
      Ok(s) => toml::from_str(&s).map_err(|e| format!("malformed config {}: {e}", path.display())),
      Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
      Err(e) => Err(format!("cannot read {}: {e}", path.display())),
    }
  }

  pub fn save(&self, path: &Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
      std::fs::create_dir_all(parent)
        .map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
    }
    let body = toml::to_string_pretty(self).map_err(|e| format!("cannot serialize config: {e}"))?;
    std::fs::write(path, body).map_err(|e| format!("cannot write {}: {e}", path.display()))
  }

  pub fn base_url(&self, profile: &str) -> Option<&str> {
    self.profiles.get(profile).map(|p| p.base_url.as_str())
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn missing_file_loads_empty_config() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = Config::load(&dir.path().join("config.toml")).unwrap();
    assert_eq!(cfg, Config::default());
  }

  #[test]
  fn save_then_load_roundtrip_with_profiles() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("nested").join("config.toml");
    let mut cfg = Config::default();
    cfg.profiles.insert(
      "default".into(),
      Profile {
        base_url: "https://s.example.com".into(),
      },
    );
    cfg.profiles.insert(
      "work".into(),
      Profile {
        base_url: "https://links.corp.example".into(),
      },
    );
    cfg.save(&path).unwrap();
    let loaded = Config::load(&path).unwrap();
    assert_eq!(loaded, cfg);
    assert_eq!(loaded.base_url("work"), Some("https://links.corp.example"));
    assert_eq!(loaded.base_url("nope"), None);
  }

  #[test]
  fn malformed_toml_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "profiles = 42").unwrap();
    assert!(Config::load(&path).is_err());
  }
}
