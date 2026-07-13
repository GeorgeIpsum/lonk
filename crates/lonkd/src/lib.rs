pub mod db;
pub mod routes;
pub mod web_assets;

use rocket::{Build, Rocket};

pub fn rocket_app(db_path: &str) -> Rocket<Build> {
  let db = db::Db::open(db_path).expect("failed to open database");
  rocket::build()
    .manage(db)
    .manage(web_assets::web_source_from_env())
    .mount(
      "/",
      rocket::routes![
        routes::create_link,
        routes::follow_link,
        routes::qr_svg,
        routes::valid_url,
        routes::link_status,
        web_assets::index,
        web_assets::assets
      ],
    )
    .register("/", rocket::catchers![routes::default_catcher])
}
