use std::path::PathBuf;

/// Resolution order: $LONK_CONFIG_DIR, else $XDG_CONFIG_HOME/lonk, else ~/.config/lonk.
pub fn config_dir() -> PathBuf {
  if let Ok(dir) = std::env::var("LONK_CONFIG_DIR") {
    return PathBuf::from(dir);
  }
  if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
    return PathBuf::from(xdg).join("lonk");
  }
  PathBuf::from(std::env::var("HOME").expect("HOME is not set"))
    .join(".config")
    .join("lonk")
}

#[cfg(test)]
mod tests {
  use super::*;

  // One test exercising all branches sequentially: env vars are process-global,
  // so splitting into parallel #[test]s would race.
  #[test]
  fn resolution_order() {
    std::env::set_var("LONK_CONFIG_DIR", "/explicit/dir");
    std::env::set_var("XDG_CONFIG_HOME", "/xdg");
    assert_eq!(config_dir(), PathBuf::from("/explicit/dir"));

    std::env::remove_var("LONK_CONFIG_DIR");
    assert_eq!(config_dir(), PathBuf::from("/xdg/lonk"));

    std::env::remove_var("XDG_CONFIG_HOME");
    let home = std::env::var("HOME").unwrap();
    assert_eq!(
      config_dir(),
      PathBuf::from(home).join(".config").join("lonk")
    );
  }
}
