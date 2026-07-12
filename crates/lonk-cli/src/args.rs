use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
  name = "lonk",
  about = "Shorten URLs via a lonk server",
  arg_required_else_help = true
)]
pub struct Cli {
  /// URLs to shorten (http:// or https://)
  #[arg(value_name = "URL")]
  pub urls: Vec<String>,

  /// Print a scannable unicode QR code after each short link
  #[arg(long, conflicts_with = "qr_svg")]
  pub qr: bool,

  /// Print the QR code as SVG text to stdout (exactly one URL)
  #[arg(long = "qr-svg")]
  pub qr_svg: bool,

  /// Validate URLs locally and exit; no config or network needed
  #[arg(long)]
  pub valid: bool,

  /// Use a named profile from the config file (default: "default")
  #[arg(long, global = true, value_name = "NAME")]
  pub profile: Option<String>,

  /// Custom response header for the created link(s), "Name: value" (repeatable)
  #[arg(short = 'H', long = "header", value_name = "NAME: VALUE")]
  pub headers: Vec<String>,

  #[command(subcommand)]
  pub cmd: Option<Cmd>,
}

#[derive(Subcommand, Debug)]
pub enum Cmd {
  /// Configure the server base URL (first-time setup or reconfigure)
  Setup {
    /// e.g. https://s.example.com (prompts if omitted)
    base_url: Option<String>,
  },
  /// Check whether a short link's destination is alive
  Status {
    /// Slug or full short URL (e.g. Ab3dEf9 or https://s.example.com/Ab3dEf9)
    target: String,
  },
}

#[cfg(test)]
mod tests {
  use super::*;
  use clap::Parser;

  #[test]
  fn parses_urls_and_flags() {
    let cli =
      Cli::try_parse_from(["lonk", "--qr", "https://a.example", "https://b.example"]).unwrap();
    assert!(cli.qr);
    assert_eq!(cli.urls, vec!["https://a.example", "https://b.example"]);
    assert!(cli.cmd.is_none());
  }

  #[test]
  fn parses_setup_subcommand_with_profile() {
    let cli =
      Cli::try_parse_from(["lonk", "setup", "--profile", "work", "https://s.example"]).unwrap();
    match cli.cmd {
      Some(Cmd::Setup { base_url }) => assert_eq!(base_url.as_deref(), Some("https://s.example")),
      other => panic!("expected setup, got {other:?}"),
    }
    assert_eq!(cli.profile.as_deref(), Some("work"));
  }

  #[test]
  fn qr_and_qr_svg_conflict() {
    assert!(Cli::try_parse_from(["lonk", "--qr", "--qr-svg", "https://a.example"]).is_err());
  }

  #[test]
  fn zero_args_is_an_error_that_shows_help() {
    let err = Cli::try_parse_from(["lonk"]).unwrap_err();
    assert_eq!(
      err.kind(),
      clap::error::ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
    );
  }

  #[test]
  fn parses_repeated_headers() {
    let cli = Cli::try_parse_from([
      "lonk",
      "-H",
      "X-A: 1",
      "--header",
      "X-B: 2",
      "https://a.example",
    ])
    .unwrap();
    assert_eq!(cli.headers, vec!["X-A: 1", "X-B: 2"]);
  }

  #[test]
  fn parses_status_subcommand() {
    let cli = Cli::try_parse_from(["lonk", "status", "Ab3dEf9"]).unwrap();
    match cli.cmd {
      Some(Cmd::Status { target }) => assert_eq!(target, "Ab3dEf9"),
      other => panic!("expected status, got {other:?}"),
    }
  }
}
