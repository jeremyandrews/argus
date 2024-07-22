use rand::{distributions::WeightedIndex, prelude::*, rngs::StdRng, SeedableRng};
use tokio::time::{sleep, Duration};
use tracing::debug;

// Sleep for 0 to 2 seconds, favoring shorter sleeps.
pub async fn weighted_sleep() {
    // Weights for sleeping durations from 0 to 2 seconds
    let weights = vec![2, 1, 0];

    // Create a weighted index based on the defined weights
    let dist = WeightedIndex::new(&weights).unwrap();

    // Create a random number generator that is `Send`
    let mut rng = StdRng::from_entropy();

    // Select a duration based on the weighted distribution
    let duration_index = dist.sample(&mut rng);

    // Convert index to actual duration in seconds
    let sleep_duration = Duration::from_secs((duration_index + 1) as u64);

    // Sleep for the selected duration
    debug!(" zzz - sleeping {:?} ...", sleep_duration);
    sleep(sleep_duration).await;
}
