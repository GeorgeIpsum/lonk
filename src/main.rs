#[rocket::launch]
fn rocket() -> _ {
  let db_path = std::env::var("LONK_DB").unwrap_or_else(|_| "lonk.db".to_string());
  lonk::rocket_app(&db_path)
}
