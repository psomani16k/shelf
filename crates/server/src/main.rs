use shelf_server::Config;
use tracing::{error, info};

#[tokio::main]
async fn main() {
    shelf_server::init_logging();
    info!(version = env!("CARGO_PKG_VERSION"), "starting");
    // Logged here rather than returned, so even a fatal error is a JSON line.
    if let Err(e) = shelf_server::serve(Config::default()).await {
        error!(error = %format!("{e:#}"), "server failed");
        std::process::exit(1);
    }
}
