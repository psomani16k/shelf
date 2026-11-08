// Must come first: macro_rules! macros are only visible in code after their definition.
#[macro_use]
mod caller;
mod commands;

use std::process::ExitCode;

use tracing_subscriber::EnvFilter;

struct Caller {}

fn usage() {
    eprintln!("usage: demo <function> [args...]\n\nfunctions:");
    for (name, params, ret) in Caller::FUNCTIONS {
        eprintln!("  {name} {} {ret}", params.join(" "));
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,shelf_core=debug"));
    tracing_subscriber::fmt()
        .json()
        .flatten_event(true)
        .with_current_span(true)
        .with_span_list(false)
        .with_env_filter(filter)
        .init();

    let mut args = std::env::args().skip(1);
    let Some(name) = args.next() else {
        usage();
        return ExitCode::from(2);
    };
    let args: Vec<String> = args.collect();

    let caller = Caller {};
    match caller.call(&name, &args).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}\n");
            usage();
            ExitCode::FAILURE
        }
    }
}
