mod args;
mod config;

use args::{Cli, Cmd};
use clap::Parser;

const EXIT_OK: i32 = 0;
const EXIT_USAGE: i32 = 1;
#[allow(dead_code)] // used from Task 8 on
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
    None => {
      eprintln!("shorten: not implemented yet");
      EXIT_USAGE
    }
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
