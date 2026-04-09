//! go-modular entrypoint.
//!
//! At D-INFRA-11 `main()` delegates straight to `go_modular::serve()`.
//! D-CLI-1..5 replaces this with a clap subcommand dispatcher (serve,
//! migrate, seed, generate-config) that still routes `serve` through
//! the same `go_modular::serve()` entry.

use std::process::ExitCode;

use go_modular::serve;

#[tokio::main]
async fn main() -> ExitCode {
    if let Err(err) = serve().await {
        eprintln!("go-modular: fatal error: {err:#}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
