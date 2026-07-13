#[rocket::launch]
fn rocket() -> _ {
  let db_path = std::env::var("LONK_DB").unwrap_or_else(|_| {
    let dir = lonk_core::paths::config_dir();
    std::fs::create_dir_all(&dir).expect("failed to create config dir");
    dir.join("lonk.db").to_string_lossy().into_owned()
  });
  lonkd::rocket_app(&db_path)
}
