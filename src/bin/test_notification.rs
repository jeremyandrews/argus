use anyhow::Result;

use argus::app;

#[tokio::main]
async fn main() -> Result<()> {
    app::send_to_app(
        "Rust background test",
        "This test is _backgrounded_ from a *Rust* program.",
    )
    .await;
    Ok(())
}
