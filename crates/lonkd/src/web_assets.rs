use rocket::http::ContentType;
use rocket::State;
use std::path::{Path, PathBuf};

#[derive(rust_embed::RustEmbed)]
#[folder = "../../web/dist"]
struct Assets;

/// Where UI bytes come from, resolved once at startup.
pub enum WebSource {
  Embedded,
  Dir(PathBuf),
}

/// LONK_WEB_DIR set and non-empty -> serve that directory; else embedded.
pub fn web_source_from_env() -> WebSource {
  match std::env::var("LONK_WEB_DIR") {
    Ok(dir) if !dir.is_empty() => WebSource::Dir(PathBuf::from(dir)),
    _ => WebSource::Embedded,
  }
}

fn content_type_for(rel: &str) -> ContentType {
  Path::new(rel)
    .extension()
    .and_then(|ext| ext.to_str())
    .and_then(ContentType::from_extension)
    .unwrap_or(ContentType::Bytes)
}

fn load(source: &WebSource, rel: &str) -> Option<Vec<u8>> {
  match source {
    WebSource::Embedded => Assets::get(rel).map(|file| file.data.into_owned()),
    WebSource::Dir(root) => {
      let root = root.canonicalize().ok()?;
      let full = root.join(rel).canonicalize().ok()?;
      if !full.starts_with(&root) {
        return None; // path escaped the override root
      }
      std::fs::read(full).ok()
    }
  }
}

#[rocket::get("/")]
pub fn index(source: &State<WebSource>) -> Option<(ContentType, Vec<u8>)> {
  load(source, "index.html").map(|bytes| (ContentType::HTML, bytes))
}

/// Multi-segment asset paths (e.g. assets/index-<hash>.js). Single segments
/// still match /<id> first, exactly as with the old FileServer.
#[rocket::get("/<path..>", rank = 20)]
pub fn assets(path: PathBuf, source: &State<WebSource>) -> Option<(ContentType, Vec<u8>)> {
  let rel = path.to_str()?;
  load(source, rel).map(|bytes| (content_type_for(rel), bytes))
}
