use anyhow::Result;
use std::time::Duration;
use tokio::time::sleep;
use tracing::info;

pub async fn app_api_loop() -> Result<()> {
    loop {
        info!("Top of app_api_loop");
        sleep(Duration::from_secs(60)).await;
    }
}
