//! `conveyor-media-browser`: the Chromium image of `apps/media`.
//!
//! `--check-lock` proves the image covers every captured browser capability; `--list` prints the
//! kinds; `--run <kind>` executes one. The runner contract (issue #144) will call the same
//! dispatch, so nothing here has to change when it lands.

use clap::Parser;
use conveyor_media::browser;
use conveyor_media::common::{Config, entry};

fn main() -> std::process::ExitCode {
    let cli = entry::Cli::parse();
    if cli.check_lock {
        return entry::check(browser::CAPTURES, browser::KINDS);
    }
    if cli.list {
        entry::list(browser::KINDS);
        return std::process::ExitCode::SUCCESS;
    }
    let config = Config::from_env(browser::SERVICE);
    match cli.run.clone() {
        Some(kind) => entry::run(&kind, &cli, &config, &browser::dispatch),
        None => {
            eprintln!("{}: pass --run <kind>, --list or --check-lock", browser::SERVICE);
            std::process::ExitCode::FAILURE
        }
    }
}
