#[macro_use]
extern crate rocket;

#[get("/api")]
fn index() -> &'static str {
  "Hello, world!"
}

#[get("/links/create")]
fn create_get() -> &'static str {
  "Create endpoint"
}

#[post("/links/create")]
fn create_post() -> &'static str {
  "Create POST endpoint"
}

#[get("/links/<id>")]
async fn get_links_by_id(id: &str) -> String {
  format!("Get short link by ID: {}", id)
}

#[launch]
fn rocket() -> _ {
  rocket::build()
    .mount(
      "/",
      routes![index, create_get, create_post, get_links_by_id],
    )
    .mount("/", rocket::fs::FileServer::from("./static"))
}
