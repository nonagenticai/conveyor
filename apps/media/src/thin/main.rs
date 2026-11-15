//! `conveyor-media-thin`: the `tts_gateway` and `youtube_upload_service` halves of `apps/media`.
//!
//! `serve` runs the binary; `--check-lock` compares its routed operations with the captured
//! contracts and exits non-zero while any captured operation has no route.

use clap::Parser;
use conveyor_media::common::{Config, entry};
use conveyor_media::thin;

#[tokio::main]
async fn main() -> std::process::ExitCode {
    let cli = entry::Cli::parse();

    if cli.check_lock {
        return entry::check(thin::LOCKS, thin::SERVED);
    }
    if cli.print_openapi {
        let document = entry::openapi(thin::SERVICE, thin::SERVED);
        println!("{}", serde_json::to_string_pretty(&document).unwrap_or_default());
        return std::process::ExitCode::SUCCESS;
    }

    let mut config = Config::from_env(thin::SERVICE, thin::DEFAULT_PORT);
    entry::apply(&mut config, &cli);
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    entry::serve(&config, thin::router()).await
}
