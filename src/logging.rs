use std::io;
use tracing::Level;
use tracing_appender::rolling;
use tracing_subscriber::filter::FilterFn;
use tracing_subscriber::fmt;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::prelude::*;
use tracing_subscriber::EnvFilter;

pub fn configure_logging() {
    // Custom filter to ignore specific warnings
    let custom_filter = FilterFn::new(|metadata| {
        // Exclude specific warnings based on their target and message
        if metadata.level() == &Level::WARN && metadata.target() == "html5ever::serialize" {
            false
        } else {
            true
        }
    });

    let stdout_log = fmt::layer()
        .with_writer(io::stdout)
        .with_filter(EnvFilter::new(
            "info,llm_request=warn,web_request=warn,db=warn",
        ))
        .with_filter(custom_filter);

    let file_appender = rolling::daily("logs", "app.log");
    let file_log = fmt::layer()
        .with_writer(file_appender)
        .with_filter(EnvFilter::new("llm_request=debug,info"));

    tracing_subscriber::Registry::default()
        .with(stdout_log)
        .with(file_log)
        .init();
}
