use std::io;
use tracing_appender::rolling;
use tracing_subscriber::fmt;
use tracing_subscriber::prelude::*;
use tracing_subscriber::EnvFilter;

pub fn configure_logging() {
    let stdout_log = fmt::layer()
        .with_writer(io::stdout)
        .with_filter(EnvFilter::new(
            "info,llm_request=warn,web_request=warn,db=warn",
        ));

    let file_appender = rolling::daily("logs", "app.log");
    let file_log = fmt::layer()
        .with_writer(file_appender)
        .with_filter(EnvFilter::new("llm_request=debug,info"));

    tracing_subscriber::Registry::default()
        .with(stdout_log)
        .with(file_log)
        .init();
}
