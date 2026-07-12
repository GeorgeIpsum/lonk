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
         created_at TEXT NOT NULL
       )",
      [],
    )?;
    Ok(Db(Mutex::new(conn)))
  }

  /// Returns Ok(false) if `id` already exists.
  pub fn insert(&self, id: &str, url: &str) -> rusqlite::Result<bool> {
    let conn = self.0.lock().unwrap();
    let n = conn.execute(
      "INSERT OR IGNORE INTO links (id, url, created_at) VALUES (?1, ?2, datetime('now'))",
      [id, url],
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
    assert!(db.insert("abc1234", "https://example.com/").unwrap());
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
    assert!(db.insert("abc1234", "https://example.com/").unwrap());
    assert!(!db.insert("abc1234", "https://other.example/").unwrap());
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
}
