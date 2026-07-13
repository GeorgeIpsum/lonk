mod args;
mod config;

use args::{Cli, Cmd};
use clap::Parser;

const EXIT_OK: i32 = 0;
const EXIT_USAGE: i32 = 1;
const EXIT_NETWORK: i32 = 2;

fn main() {
  let cli = match Cli::try_parse() {
    Ok(cli) => cli,
    Err(e) => {
      let code = match e.kind() {
        clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion => EXIT_OK,
        _ => EXIT_USAGE,
      };
      let _ = e.print();
      std::process::exit(code);
    }
  };
  std::process::exit(run(cli));
}

fn run(cli: Cli) -> i32 {
  match &cli.cmd {
    Some(Cmd::Setup { base_url }) => {
      let profile = cli.profile.as_deref().unwrap_or("default");
      run_setup(base_url.clone(), profile)
    }
    Some(Cmd::Status { ref target }) => run_status(target, cli.profile.as_deref()),
    None if cli.valid => run_valid(&cli.urls),
    None => run_shorten(&cli),
  }
}

fn run_valid(urls: &[String]) -> i32 {
  let mut ok = true;
  for url in urls {
    match lonk_core::validate_url(url) {
      Ok(_) => println!("valid"),
      Err(e) => {
        println!("invalid: {e}");
        ok = false;
      }
    }
  }
  if ok {
    EXIT_OK
  } else {
    EXIT_USAGE
  }
}

use std::io::IsTerminal;
use std::io::Write;

/// Parse and validate repeatable -H "Name: value" flags.
fn parse_headers(raw: &[String]) -> Result<Vec<(String, String)>, String> {
  if raw.len() > lonk_core::MAX_HEADERS_PER_LINK {
    return Err(format!(
      "too many headers (max {})",
      lonk_core::MAX_HEADERS_PER_LINK
    ));
  }
  raw
    .iter()
    .map(|h| {
      let (name, value) = h
        .split_once(':')
        .ok_or_else(|| format!("invalid header {h:?}: expected \"Name: value\""))?;
      let (name, value) = (name.trim().to_string(), value.trim().to_string());
      lonk_core::validate_header(&name, &value).map_err(|e| e.to_string())?;
      Ok((name, value))
    })
    .collect()
}

/// Validate a server base URL and normalize it for path-appending:
/// endpoints are built as `{base}/api/links` etc., so a query string or
/// fragment would corrupt every request the profile ever makes.
fn normalize_base_url(raw: &str) -> Result<String, String> {
  let parsed = lonk_core::validate_url(raw.trim()).map_err(|e| e.to_string())?;
  if parsed.query().is_some() {
    return Err("base url must not contain a query string".to_string());
  }
  if parsed.fragment().is_some() {
    return Err("base url must not contain a fragment".to_string());
  }
  Ok(parsed.as_str().trim_end_matches('/').to_string())
}

fn run_setup(base_url: Option<String>, profile: &str) -> i32 {
  let raw = match base_url.or_else(prompt_base_url) {
    Some(url) => url,
    None => {
      eprintln!("no base url given and stdin is not a terminal");
      eprintln!("usage: lonk setup <base-url>   (e.g. lonk setup https://s.example.com)");
      return EXIT_USAGE;
    }
  };
  let base = match normalize_base_url(&raw) {
    Ok(base) => base,
    Err(e) => {
      eprintln!("{e}");
      return EXIT_USAGE;
    }
  };

  let path = config::config_path();
  let mut cfg = match config::Config::load(&path) {
    Ok(c) => c,
    Err(e) => {
      eprintln!("{e}");
      return EXIT_USAGE;
    }
  };
  cfg.profiles.insert(
    profile.to_string(),
    config::Profile {
      base_url: base.clone(),
    },
  );
  if let Err(e) = cfg.save(&path) {
    eprintln!("{e}");
    return EXIT_USAGE;
  }
  eprintln!("profile '{profile}' -> {base} saved to {}", path.display());
  EXIT_OK
}

/// Interactive base-url prompt. None when stdin is not a TTY or the line is empty.
fn prompt_base_url() -> Option<String> {
  if !std::io::stdin().is_terminal() {
    return None;
  }
  eprint!("Base URL of your lonk server: ");
  let _ = std::io::stderr().flush();
  let mut line = String::new();
  std::io::stdin().read_line(&mut line).ok()?;
  let line = line.trim().to_string();
  if line.is_empty() {
    None
  } else {
    Some(line)
  }
}

fn run_shorten(cli: &Cli) -> i32 {
  if cli.urls.is_empty() {
    eprintln!("no URL given; usage: lonk [OPTIONS] <URL>...");
    return EXIT_USAGE;
  }

  // 1. parse and validate headers, fail fast before any network traffic
  let headers = match parse_headers(&cli.headers) {
    Ok(h) => h,
    Err(e) => {
      eprintln!("{e}");
      return EXIT_USAGE;
    }
  };

  // 2. local validation, fail fast before any network traffic
  for url in &cli.urls {
    if let Err(e) = lonk_core::validate_url(url) {
      eprintln!("{url}: {e}");
      return EXIT_USAGE;
    }
  }
  if cli.qr_svg && cli.urls.len() != 1 {
    eprintln!("--qr-svg requires exactly one URL");
    return EXIT_USAGE;
  }

  // 3. resolve base url from profile
  let base = match resolve_base_url(cli.profile.as_deref()) {
    Ok(b) => b,
    Err(code) => return code,
  };

  // 4. shorten in input order, fail fast
  for (i, url) in cli.urls.iter().enumerate() {
    match shorten_one(&base, url, &headers) {
      Ok(short) => {
        if i > 0 && cli.qr {
          println!();
        }
        emit(&short, cli);
      }
      Err((code, msg)) => {
        eprintln!("{url}: {msg}");
        return code;
      }
    }
  }
  EXIT_OK
}

/// Resolve the profile's base url; Err carries the exit code (already reported).
fn resolve_base_url(profile: Option<&str>) -> Result<String, i32> {
  let path = config::config_path();
  let mut cfg = config::Config::load(&path).map_err(|e| {
    eprintln!("{e}");
    EXIT_USAGE
  })?;
  let name = profile.unwrap_or("default");
  if let Some(base) = cfg.base_url(name) {
    return Ok(base.trim_end_matches('/').to_string());
  }
  if profile.is_some() {
    eprintln!("profile '{name}' not found; run: lonk setup --profile {name} <base-url>");
    return Err(EXIT_USAGE);
  }
  // default profile missing: one-time interactive setup on a TTY
  match prompt_base_url() {
    Some(raw) => match normalize_base_url(&raw) {
      Ok(base) => {
        cfg.profiles.insert(
          "default".into(),
          config::Profile {
            base_url: base.clone(),
          },
        );
        cfg.save(&path).map_err(|e| {
          eprintln!("{e}");
          EXIT_USAGE
        })?;
        Ok(base)
      }
      Err(e) => {
        eprintln!("{e}");
        Err(EXIT_USAGE)
      }
    },
    None => {
      eprintln!("no server configured; run: lonk setup <base-url>");
      Err(EXIT_USAGE)
    }
  }
}

/// A bare slug passes through; a full short URL contributes its last path segment.
fn extract_slug(target: &str) -> Result<String, String> {
  if !target.starts_with("http://") && !target.starts_with("https://") {
    return Ok(target.to_string());
  }
  let parsed = lonk_core::validate_url(target).map_err(|e| e.to_string())?;
  parsed
    .path_segments()
    .and_then(|segments| segments.filter(|s| !s.is_empty()).last())
    .map(String::from)
    .ok_or_else(|| format!("no slug in url {target:?}"))
}

fn run_status(target: &str, profile: Option<&str>) -> i32 {
  let slug = match extract_slug(target) {
    Ok(slug) => slug,
    Err(e) => {
      eprintln!("{e}");
      return EXIT_USAGE;
    }
  };
  let base = match resolve_base_url(profile) {
    Ok(base) => base,
    Err(code) => return code,
  };
  match ureq::get(&format!("{base}/{slug}/status")).call() {
    Ok(resp) => {
      let body: lonk_core::types::StatusResp = match resp.into_json() {
        Ok(body) => body,
        Err(e) => {
          eprintln!("bad response: {e}");
          return EXIT_NETWORK;
        }
      };
      if body.alive {
        println!("alive ({})", body.http_status.unwrap_or(0));
        EXIT_OK
      } else {
        match (body.http_status, body.error) {
          (Some(code), _) => println!("dead ({code})"),
          (None, Some(err)) => println!("dead ({err})"),
          (None, None) => println!("dead"),
        }
        EXIT_USAGE
      }
    }
    Err(ureq::Error::Status(404, _)) => {
      eprintln!("no such link: {slug}");
      EXIT_USAGE
    }
    Err(ureq::Error::Status(code, _)) => {
      eprintln!("server returned {code}");
      EXIT_NETWORK
    }
    Err(ureq::Error::Transport(t)) => {
      eprintln!("{t}");
      EXIT_NETWORK
    }
  }
}

/// POST one url; Ok(full short url), Err((exit code, message)).
fn shorten_one(
  base: &str,
  url: &str,
  headers: &[(String, String)],
) -> Result<String, (i32, String)> {
  let resp = ureq::post(&format!("{base}/api/links"))
    .send_json(&lonk_core::types::CreateLinkReq {
      url: url.to_string(),
      headers: headers.to_vec(),
    })
    .map_err(|e| match e {
      ureq::Error::Status(code, resp) => {
        let msg = resp
          .into_json::<serde_json::Value>()
          .ok()
          .and_then(|v| v["error"].as_str().map(String::from))
          .unwrap_or_else(|| format!("server returned {code}"));
        (EXIT_NETWORK, msg)
      }
      ureq::Error::Transport(t) => (EXIT_NETWORK, t.to_string()),
    })?;
  let body: lonk_core::types::LinkResp = resp
    .into_json()
    .map_err(|e| (EXIT_NETWORK, format!("bad response: {e}")))?;
  Ok(format!("{base}{}", body.short_url))
}

/// Print the short url, plus a QR code (unicode to stdout, or SVG to stdout
/// with the url on stderr) when `--qr`/`--qr-svg` was requested.
fn emit(short: &str, cli: &Cli) {
  if cli.qr_svg {
    let svg = match qr_svg(short) {
      Ok(s) => s,
      Err(e) => {
        eprintln!("{e}");
        std::process::exit(EXIT_USAGE);
      }
    };
    eprintln!("{short}");
    println!("{svg}");
    return;
  }
  println!("{short}");
  if cli.qr {
    match qr_unicode(short) {
      Ok(q) => println!("{q}"),
      Err(e) => {
        eprintln!("{e}");
        std::process::exit(EXIT_USAGE);
      }
    }
  }
}

fn qr_unicode(content: &str) -> Result<String, String> {
  let code =
    qrcode::QrCode::new(content.as_bytes()).map_err(|e| format!("qr encoding failed: {e}"))?;
  Ok(code.render::<qrcode::render::unicode::Dense1x2>().build())
}

fn qr_svg(content: &str) -> Result<String, String> {
  let code =
    qrcode::QrCode::new(content.as_bytes()).map_err(|e| format!("qr encoding failed: {e}"))?;
  Ok(
    code
      .render::<qrcode::render::svg::Color>()
      .min_dimensions(256, 256)
      .build(),
  )
}
