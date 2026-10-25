mod api;
mod error;
mod frontend;
mod logging;

use std::{net::SocketAddr, path::PathBuf, sync::Arc};

use anyhow::Context;
use axum::{Router, middleware};
use shelf_core::library::BookLibrary;
use tokio::{
    net::TcpListener,
    signal::unix::{SignalKind, signal},
};
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};
use tracing::{error, info};

const LIBRARY_ROOT: &str = "/books";
const BIND: &str = "0.0.0.0:8080";

#[tokio::main]
async fn main() {
    logging::init();
    info!(version = env!("CARGO_PKG_VERSION"), "starting");
    // Logged here rather than returned, so even a fatal error is a JSON line.
    if let Err(e) = run().await {
        error!(error = %format!("{e:#}"), "server failed");
        std::process::exit(1);
    }
}

async fn run() -> anyhow::Result<()> {
    // Also starts the watcher, which keeps the library's metadata current while the server runs.
    let library = BookLibrary::open(PathBuf::from(LIBRARY_ROOT), || {})
        .await
        .with_context(|| format!("couldn't open the library at {LIBRARY_ROOT}"))?;

    let app = Router::new()
        .nest(
            "/api",
            // Outermost last: the id is set, then the access log runs with it.
            api::router()
                .layer(middleware::from_fn(logging::access_log))
                .layer(PropagateRequestIdLayer::x_request_id())
                .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid)),
        )
        .fallback(frontend::serve)
        .with_state(Arc::new(library));

    let listener = TcpListener::bind(BIND)
        .await
        .with_context(|| format!("couldn't listen on {BIND}"))?;
    info!(address = BIND, "listening");
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await?;
    info!("stopped");
    Ok(())
}

/// Ctrl-C, or SIGTERM from `docker stop`.
async fn shutdown_signal() {
    let mut terminate = signal(SignalKind::terminate()).expect("can install a SIGTERM handler");
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        _ = terminate.recv() => {}
    }
}
