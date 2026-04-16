use std::env;
use std::sync::Once;

use tracing_subscriber::{EnvFilter, fmt, prelude::*};

static INIT: Once = Once::new();

pub fn init_tracing() {
    INIT.call_once(|| {
        let debug_enabled = env_truthy("WARMLITE_DEBUG");
        let default_level = if debug_enabled { "debug" } else { "info" };

        let filter = EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new(format!("warmlite={default_level},info")));

        let subscriber = tracing_subscriber::registry()
            .with(filter)
            .with(fmt::layer().with_target(true).with_line_number(true));

        if let Err(error) = tracing::subscriber::set_global_default(subscriber) {
            eprintln!("Failed to initialize tracing subscriber: {error}");
            return;
        }

        tracing::info!(
            debug_enabled,
            "tracing initialized (set WARMLITE_DEBUG=1 or RUST_LOG=warmlite=debug to increase verbosity)"
        );
    });
}

fn env_truthy(name: &str) -> bool {
    matches!(
        env::var(name).ok().as_deref(),
        Some("1" | "true" | "TRUE" | "yes" | "YES" | "on" | "ON")
    )
}
