//! `conveyor-media-ffmpeg`: the ffmpeg image of `apps/media`.
//!
//! `--check-lock` proves the image covers every captured ffmpeg capability; `--list` prints the
//! kinds; `--run <kind>` executes one. The runner contract (issue #144) will call the same
//! dispatch, so nothing here has to change when it lands.

use clap::Parser;
use conveyor_media::common::{Config, entry};
use conveyor_media::ffmpeg;

fn main() -> std::process::ExitCode {
    let cli = entry::Cli::parse();
    if cli.check_lock {
        return entry::check(ffmpeg::CAPTURES, ffmpeg::KINDS);
    }
    if cli.list {
        entry::list(ffmpeg::KINDS);
        return std::process::ExitCode::SUCCESS;
    }
    let config = Config::from_env(ffmpeg::SERVICE);
    match cli.run.clone() {
        Some(kind) => entry::run(&kind, &cli, &config, &ffmpeg::dispatch),
        None => {
            eprintln!("{}: pass --run <kind>, --list or --check-lock", ffmpeg::SERVICE);
            std::process::ExitCode::FAILURE
        }
    }
}
