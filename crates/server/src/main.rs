mod api;
mod auth;
mod data;
mod error;
mod frontend;
mod library;
mod logging;

use crate::{api::AppState, auth::Auth, data::DataFile, library::ServerLibrary};
use anyhow::Context;
use axum::{Router, middleware};
use shelf_utility::timeutil::Time;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::{
    net::TcpListener,
    signal::unix::{SignalKind, signal},
};
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};
use tracing::{error, info};

const LIBRARY_ROOT: &str = "/books";
/// The server's own data (users), kept out of the library so it never syncs with it.
const DATA_DIR: &str = "/data";
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
    let data = DataFile::load(Path::new(DATA_DIR), Time::now())
        .await
        .with_context(|| format!("couldn't load the server data in {DATA_DIR}"))?;
    let auth = Auth::new(data);
    // Never log the users themselves: they hold passwords.
    info!(users = auth.user_count().await, "server data loaded");

    let library = ServerLibrary::open(PathBuf::from(LIBRARY_ROOT), || {})
        .await
        .with_context(|| format!("couldn't open the library at {LIBRARY_ROOT}"))?;

    let state = AppState {
        library: Arc::new(library),
        auth: Arc::new(auth),
    };
    let app = Router::new()
        .nest(
            "/api",
            // Outermost last: the id is set, then the access log runs with it.
            api::router(state.clone())
                .layer(middleware::from_fn(logging::access_log))
                .layer(PropagateRequestIdLayer::x_request_id())
                .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid)),
        )
        .fallback(frontend::serve)
        .with_state(state);

    let listener = TcpListener::bind(BIND)
        .await
        .with_context(|| format!("couldn't listen on {BIND}"))?;
    info!(address = BIND, "listening");
    axum::serve(listener, app)
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
