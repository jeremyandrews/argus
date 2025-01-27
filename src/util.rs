use rand::{distributions::WeightedIndex, prelude::*, rngs::StdRng, SeedableRng};
use serde_json::Value;
use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::Path;
use tokio::time::{sleep, Duration};
use tracing::debug;

// Sleep for 0 to 2 seconds, favoring shorter sleeps.
pub async fn weighted_sleep() {
    // Retrieve the worker number
    let worker_id = format!("{:?}", std::thread::current().id());

    // Weights for sleeping durations from 0 to 1 seconds
    let weights = vec![1, 0];

    // Create a weighted index based on the defined weights
    let dist = WeightedIndex::new(&weights).unwrap();

    // Create a random number generator that is `Send`
    let mut rng = StdRng::from_entropy();

    // Select a duration based on the weighted distribution
    let duration_index = dist.sample(&mut rng);

    // Convert index to actual duration in seconds
    let sleep_duration = Duration::from_secs((duration_index + 1) as u64);

    // Log the sleep duration
    debug!("Worker {}: Sleeping for {:?}", worker_id, sleep_duration);

    // Sleep for the selected duration
    sleep(sleep_duration).await;
}

/// Loads places.json from the disk and parses it into a hierarchical structure.
pub fn parse_places_data_hierarchical(
) -> Result<BTreeMap<String, BTreeMap<String, Vec<String>>>, String> {
    const PLACES_JSON_PATH_ENV: &str = "PLACES_JSON_PATH";
    let json_path = env::var(PLACES_JSON_PATH_ENV)
        .map_err(|_| format!("Environment variable {} is not set.", PLACES_JSON_PATH_ENV))?;

    if !Path::new(&json_path).exists() {
        return Err(format!(
            "The specified places.json file does not exist: {}",
            json_path
        ));
    }

    let json_data = fs::read_to_string(&json_path)
        .map_err(|err| format!("Failed to read the places.json file: {}", err))?;

    let places_data: Value = serde_json::from_str(&json_data)
        .map_err(|err| format!("Failed to parse places.json: {}", err))?;

    let mut hierarchy = BTreeMap::new();

    if let Value::Object(continents) = places_data {
        for (continent_name, continent_value) in continents {
            let mut countries_map = BTreeMap::new();
            if let Value::Object(countries) = continent_value {
                for (country_name, country_value) in countries {
                    let mut regions_vec = Vec::new();
                    if let Value::Object(regions) = country_value {
                        for region_name in regions.keys() {
                            regions_vec.push(region_name.clone());
                        }
                    }
                    countries_map.insert(country_name, regions_vec);
                }
            }
            hierarchy.insert(continent_name, countries_map);
        }
    }

    Ok(hierarchy)
}

/// Loads places.json from the disk and parses it into a detailed hierarchical structure
/// that includes continents, countries, regions, cities, and people.
pub fn parse_places_data_detailed() -> Result<
    BTreeMap<String, BTreeMap<String, BTreeMap<String, BTreeMap<String, Vec<String>>>>>,
    String,
> {
    const PLACES_JSON_PATH_ENV: &str = "PLACES_JSON_PATH";
    let json_path = env::var(PLACES_JSON_PATH_ENV)
        .map_err(|_| format!("Environment variable {} is not set.", PLACES_JSON_PATH_ENV))?;

    if !Path::new(&json_path).exists() {
        return Err(format!(
            "The specified places.json file does not exist: {}",
            json_path
        ));
    }

    let json_data = fs::read_to_string(&json_path)
        .map_err(|err| format!("Failed to read the places.json file: {}", err))?;

    let places_data: Value = serde_json::from_str(&json_data)
        .map_err(|err| format!("Failed to parse places.json: {}", err))?;

    let mut detailed_hierarchy = BTreeMap::new();

    if let Value::Object(continents) = places_data {
        for (continent_name, continent_value) in continents {
            let mut countries_map = BTreeMap::new();
            if let Value::Object(countries) = continent_value {
                for (country_name, country_value) in countries {
                    let mut regions_map = BTreeMap::new();
                    if let Value::Object(regions) = country_value {
                        for (region_name, region_value) in regions {
                            let mut cities_map = BTreeMap::new();
                            if let Value::Array(people_list) = region_value {
                                for person in people_list {
                                    if let Some(person_str) = person.as_str() {
                                        let parts: Vec<&str> = person_str.split(", ").collect();
                                        if parts.len() >= 3 {
                                            let city = parts[2].to_string();
                                            cities_map
                                                .entry(city)
                                                .or_insert_with(Vec::new)
                                                .push(person_str.to_string());
                                        }
                                    }
                                }
                            }
                            regions_map.insert(region_name, cities_map);
                        }
                    }
                    countries_map.insert(country_name, regions_map);
                }
            }
            detailed_hierarchy.insert(continent_name, countries_map);
        }
    }

    Ok(detailed_hierarchy)
}
