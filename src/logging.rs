use std::io;
use tracing::Level;
use tracing_appender::rolling;
use tracing_subscriber::filter::FilterFn;
use tracing_subscriber::fmt;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::prelude::*;
use tracing_subscriber::EnvFilter;

/// Sets up logging with a specific name and level.
///
/// # Arguments
///
/// * `name` - A string slice that holds the name of the program.
/// * `level` - The level of logging to enable.
///
/// # Returns
///
/// * `Result<(), anyhow::Error>` - Ok if successful, Err otherwise.
pub fn setup_logging(_name: &str, _level: Level) -> anyhow::Result<()> {
    // Ensure log directory exists
    std::fs::create_dir_all("logs").ok();

    // Configure logging
    configure_logging();

    Ok(())
}

pub fn configure_logging() {
    // Custom filter to ignore specific warnings
    let custom_filter = FilterFn::new(|metadata| {
        // Exclude specific warnings based on their target and message
        !(metadata.level() == &Level::WARN && metadata.target() == "html5ever::serialize")
    });

    // Stdout log configuration
    let stdout_log = fmt::layer()
        .with_writer(io::stdout)
        .with_filter(EnvFilter::new("info,db=warn,sqlx=off,html5ever=error"))
        .with_filter(custom_filter);

    // File log configuration
    let file_appender = rolling::daily("logs", "app.log");
    let file_log = fmt::layer()
        .with_writer(file_appender)
        .with_filter(EnvFilter::new("llm_request=debug,info,sqlx=info"));

    tracing_subscriber::Registry::default()
        .with(stdout_log)
        .with(file_log)
        .init();
}
