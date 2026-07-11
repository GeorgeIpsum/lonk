pub mod db;
pub mod routes;

use rocket::fs::{relative, FileServer};
use rocket::{Build, Rocket};

pub fn rocket_app(db_path: &str) -> Rocket<Build> {
  let db = db::Db::open(db_path).expect("failed to open database");
  rocket::build()
    .manage(db)
    .mount("/", rocket::routes![routes::create_link])
    .mount("/", FileServer::from(relative!("static")))
    .register("/", rocket::catchers![routes::default_catcher])
}
