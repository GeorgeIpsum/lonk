mod args;
#[allow(dead_code)]
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
    Some(Cmd::Setup { .. }) => {
      eprintln!("setup: not implemented yet");
      EXIT_USAGE
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
