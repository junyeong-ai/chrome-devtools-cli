use std::process;

#[tokio::main]
async fn main() {
    // Initialize logging
    let verbose = std::env::args().any(|arg| arg == "--verbose" || arg == "-v");
    init_logging(verbose);

    // Parse CLI arguments
    let result = chrome_devtools_cli::cli::run().await;

    // Handle result
    if let Err(e) = result {
        eprintln!("Error: {}", e);
        process::exit(1);
    }
}

fn init_logging(verbose: bool) {
    use tracing_subscriber::{EnvFilter, fmt};

    let filter = if verbose {
        EnvFilter::new("debug").add_directive("chromiumoxide=info".parse().unwrap())
    } else {
        EnvFilter::from_default_env()
            .add_directive("warn".parse().unwrap())
            .add_directive("chromiumoxide=off".parse().unwrap())
    };

    fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_writer(std::io::stderr)
        .compact()
        .init();
}
