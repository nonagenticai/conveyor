//! `conveyor-media-ffmpeg`: the ffmpeg image of `apps/media`.
//!
//! `--check-lock` proves the image covers every captured ffmpeg capability; `--list` prints the
//! kinds; `--run <kind>` executes one locally; `--instruction <path>` executes one `runner/v1`
//! instruction through conveyor's job routes (ADR 0007 §3–4), which is how the pod runs.

use clap::Parser;
use conveyor_media::common::{Config, entry};
use conveyor_media::ffmpeg;

#[tokio::main]
async fn main() -> std::process::ExitCode {
    let cli = entry::Cli::parse();
    if cli.check_lock {
        return entry::check(ffmpeg::CAPTURES, ffmpeg::KINDS);
    }
    if cli.list {
        entry::list(ffmpeg::KINDS);
        return std::process::ExitCode::SUCCESS;
    }
    let config = Config::from_env(ffmpeg::SERVICE);
    let dispatch = entry::Dispatch {
        kinds: ffmpeg::KINDS,
        call: ffmpeg::dispatch,
    };
    if cli.instruction.is_some() {
        return entry::run_instruction(&cli, &config, dispatch).await;
    }
    match cli.run.clone() {
        Some(kind) => entry::run(&kind, &cli, &config, dispatch).await,
        None => {
            eprintln!(
                "{}: pass --run <kind>, --list or --check-lock",
                ffmpeg::SERVICE
            );
            std::process::ExitCode::FAILURE
        }
    }
}
