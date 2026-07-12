use rand::Rng;
use rusqlite::{Connection, OptionalExtension};
use std::sync::Mutex;

pub struct Db(Mutex<Connection>);

impl Db {
  pub fn open(path: &str) -> rusqlite::Result<Self> {
    let conn = Connection::open(path)?;
    conn.execute(
      "CREATE TABLE IF NOT EXISTS links (
         id         TEXT PRIMARY KEY,
         url        TEXT NOT NULL,
         created_at TEXT NOT NULL,
         headers    TEXT NOT NULL DEFAULT '[]'
       )",
      [],
    )?;
    // Databases created before the headers feature lack the column; add it.
    let has_headers: bool = conn
      .prepare("SELECT 1 FROM pragma_table_info('links') WHERE name = 'headers'")?
      .exists([])?;
    if !has_headers {
      conn.execute(
        "ALTER TABLE links ADD COLUMN headers TEXT NOT NULL DEFAULT '[]'",
        [],
      )?;
    }
    Ok(Db(Mutex::new(conn)))
  }

  /// Returns Ok(false) if `id` already exists. `headers_json` is a JSON array
  /// of [name, value] pairs (already validated by the caller).
  pub fn insert(&self, id: &str, url: &str, headers_json: &str) -> rusqlite::Result<bool> {
    let conn = self.0.lock().unwrap();
    let n = conn.execute(
      "INSERT OR IGNORE INTO links (id, url, created_at, headers)
       VALUES (?1, ?2, datetime('now'), ?3)",
      [id, url, headers_json],
    )?;
    Ok(n == 1)
  }

  pub fn get_url(&self, id: &str) -> rusqlite::Result<Option<String>> {
    let conn = self.0.lock().unwrap();
    conn
      .query_row("SELECT url FROM links WHERE id = ?1", [id], |row| {
        row.get(0)
      })
      .optional()
  }

  pub fn get_link(&self, id: &str) -> rusqlite::Result<Option<(String, Vec<(String, String)>)>> {
    let conn = self.0.lock().unwrap();
    let row = conn
      .query_row(
        "SELECT url, headers FROM links WHERE id = ?1",
        [id],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
      )
      .optional()?;
    Ok(
      row.map(|(url, headers_json)| (url, serde_json::from_str(&headers_json).unwrap_or_default())),
    )
  }
}

const SLUG_CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";

pub fn gen_slug(len: usize) -> String {
  let mut rng = rand::thread_rng();
  (0..len)
    .map(|_| SLUG_CHARS[rng.gen_range(0..SLUG_CHARS.len())] as char)
    .collect()
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn insert_then_get_roundtrip() {
    let db = Db::open(":memory:").unwrap();
    assert!(db.insert("abc1234", "https://example.com/", "[]").unwrap());
    assert_eq!(
      db.get_url("abc1234").unwrap(),
      Some("https://example.com/".to_string())
    );
  }

  #[test]
  fn get_unknown_id_is_none() {
    let db = Db::open(":memory:").unwrap();
    assert_eq!(db.get_url("nope").unwrap(), None);
  }

  #[test]
  fn insert_duplicate_id_returns_false() {
    let db = Db::open(":memory:").unwrap();
    assert!(db.insert("abc1234", "https://example.com/", "[]").unwrap());
    assert!(!db
      .insert("abc1234", "https://other.example/", "[]")
      .unwrap());
    // original mapping is untouched
    assert_eq!(
      db.get_url("abc1234").unwrap(),
      Some("https://example.com/".to_string())
    );
  }

  #[test]
  fn gen_slug_has_len_and_charset() {
    let slug = gen_slug(7);
    assert_eq!(slug.len(), 7);
    assert!(slug.bytes().all(|b| SLUG_CHARS.contains(&b)));
  }

  #[test]
  fn get_link_returns_url_and_headers() {
    let db = Db::open(":memory:").unwrap();
    let headers = r#"[["Set-Cookie","a=1"],["Set-Cookie","b=2"],["X-Demo","x"]]"#;
    assert!(db
      .insert("abc1234", "https://example.com/", headers)
      .unwrap());
    let (url, parsed) = db.get_link("abc1234").unwrap().unwrap();
    assert_eq!(url, "https://example.com/");
    assert_eq!(
      parsed,
      vec![
        ("Set-Cookie".to_string(), "a=1".to_string()),
        ("Set-Cookie".to_string(), "b=2".to_string()),
        ("X-Demo".to_string(), "x".to_string()),
      ]
    );
  }

  #[test]
  fn get_link_unknown_id_is_none() {
    let db = Db::open(":memory:").unwrap();
    assert_eq!(db.get_link("nope").unwrap(), None);
  }

  #[test]
  fn open_migrates_pre_headers_schema() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("old.db");
    {
      let conn = rusqlite::Connection::open(&path).unwrap();
      conn
        .execute(
          "CREATE TABLE links (id TEXT PRIMARY KEY, url TEXT NOT NULL, created_at TEXT NOT NULL)",
          [],
        )
        .unwrap();
      conn
        .execute(
          "INSERT INTO links VALUES ('old1234', 'https://example.com/old', datetime('now'))",
          [],
        )
        .unwrap();
    }
    let db = Db::open(path.to_str().unwrap()).unwrap();
    assert_eq!(
      db.get_link("old1234").unwrap().unwrap(),
      ("https://example.com/old".to_string(), vec![])
    );
    assert!(db
      .insert("new1234", "https://example.com/new", "[]")
      .unwrap());
  }
}
