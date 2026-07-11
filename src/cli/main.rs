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
  match cli.cmd {
    Some(Cmd::Setup { base_url }) => {
      let profile = cli.profile.as_deref().unwrap_or("default");
      run_setup(base_url, profile)
    }
    None if cli.valid => run_valid(&cli.urls),
    None => run_shorten(&cli),
  }
}

fn run_valid(urls: &[String]) -> i32 {
  let mut ok = true;
  for url in urls {
    match lonk_validate::validate_url(url) {
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

fn run_setup(base_url: Option<String>, profile: &str) -> i32 {
  let raw = match base_url.or_else(prompt_base_url) {
    Some(url) => url,
    None => {
      eprintln!("no base url given and stdin is not a terminal");
      eprintln!("usage: lonk setup <base-url>   (e.g. lonk setup https://s.example.com)");
      return EXIT_USAGE;
    }
  };
  let parsed = match lonk_validate::validate_url(raw.trim()) {
    Ok(u) => u,
    Err(e) => {
      eprintln!("{e}");
      return EXIT_USAGE;
    }
  };
  let base = parsed.as_str().trim_end_matches('/').to_string();

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
  // 1. local validation, fail fast before any network traffic
  for url in &cli.urls {
    if let Err(e) = lonk_validate::validate_url(url) {
      eprintln!("{url}: {e}");
      return EXIT_USAGE;
    }
  }
  if cli.qr_svg && cli.urls.len() != 1 {
    eprintln!("--qr-svg requires exactly one URL");
    return EXIT_USAGE;
  }

  // 2. resolve base url from profile
  let base = match resolve_base_url(cli.profile.as_deref()) {
    Ok(b) => b,
    Err(code) => return code,
  };

  // 3. shorten in input order, fail fast
  for (i, url) in cli.urls.iter().enumerate() {
    match shorten_one(&base, url) {
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
    return Ok(base.to_string());
  }
  if profile.is_some() {
    eprintln!("profile '{name}' not found; run: lonk setup --profile {name} <base-url>");
    return Err(EXIT_USAGE);
  }
  // default profile missing: one-time interactive setup on a TTY
  match prompt_base_url().and_then(|raw| lonk_validate::validate_url(raw.trim()).ok()) {
    Some(parsed) => {
      let base = parsed.as_str().trim_end_matches('/').to_string();
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
    None => {
      eprintln!("no server configured; run: lonk setup <base-url>");
      Err(EXIT_USAGE)
    }
  }
}

/// POST one url; Ok(full short url), Err((exit code, message)).
fn shorten_one(base: &str, url: &str) -> Result<String, (i32, String)> {
  let resp = ureq::post(&format!("{base}/api/links"))
    .send_json(serde_json::json!({ "url": url }))
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
  let body: serde_json::Value = resp
    .into_json()
    .map_err(|e| (EXIT_NETWORK, format!("bad response: {e}")))?;
  let short_path = body["short_url"]
    .as_str()
    .ok_or((EXIT_NETWORK, "response missing short_url".to_string()))?;
  Ok(format!("{base}{short_path}"))
}

/// Print one result. Task 9 extends this with --qr / --qr-svg rendering.
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
