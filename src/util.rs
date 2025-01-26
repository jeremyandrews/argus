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

    // Get the file path from the environment variable
    let json_path = env::var(PLACES_JSON_PATH_ENV)
        .map_err(|_| format!("Environment variable {} is not set.", PLACES_JSON_PATH_ENV))?;

    // Check if the file exists
    if !Path::new(&json_path).exists() {
        return Err(format!(
            "The specified places.json file does not exist: {}",
            json_path
        ));
    }

    // Load the JSON data from the file
    let json_data = fs::read_to_string(&json_path)
        .map_err(|err| format!("Failed to read the places.json file: {}", err))?;

    // Parse the JSON data
    let places_data: Value = serde_json::from_str(&json_data)
        .map_err(|err| format!("Failed to parse places.json: {}", err))?;

    // Build the hierarchical structure
    let mut hierarchy = BTreeMap::new();

    if let Value::Array(continents) = places_data {
        for continent in continents {
            if let Value::Object(continent_obj) = continent {
                let continent_name = continent_obj
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                let mut countries_map = BTreeMap::new();

                if let Some(Value::Array(countries)) = continent_obj.get("countries") {
                    for country in countries {
                        if let Value::Object(country_obj) = country {
                            let country_name = country_obj
                                .get("name")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_string();
                            let mut regions_vec = Vec::new();

                            if let Some(Value::Array(regions)) = country_obj.get("regions") {
                                for region in regions {
                                    if let Value::Object(region_obj) = region {
                                        if let Some(region_name) =
                                            region_obj.get("name").and_then(Value::as_str)
                                        {
                                            regions_vec.push(region_name.to_string());
                                        }
                                    }
                                }
                            }

                            countries_map.insert(country_name, regions_vec);
                        }
                    }
                }

                hierarchy.insert(continent_name, countries_map);
            }
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

    // Get the file path from the environment variable
    let json_path = env::var(PLACES_JSON_PATH_ENV)
        .map_err(|_| format!("Environment variable {} is not set.", PLACES_JSON_PATH_ENV))?;

    // Check if the file exists
    if !Path::new(&json_path).exists() {
        return Err(format!(
            "The specified places.json file does not exist: {}",
            json_path
        ));
    }

    // Load the JSON data from the file
    let json_data = fs::read_to_string(&json_path)
        .map_err(|err| format!("Failed to read the places.json file: {}", err))?;

    // Parse the JSON data
    let places_data: Value = serde_json::from_str(&json_data)
        .map_err(|err| format!("Failed to parse places.json: {}", err))?;

    // Build the detailed hierarchical structure
    let mut detailed_hierarchy = BTreeMap::new();

    if let Value::Array(continents) = places_data {
        for continent in continents {
            if let Value::Object(continent_obj) = continent {
                let continent_name = continent_obj
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                let mut countries_map = BTreeMap::new();

                if let Some(Value::Array(countries)) = continent_obj.get("countries") {
                    for country in countries {
                        if let Value::Object(country_obj) = country {
                            let country_name = country_obj
                                .get("name")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_string();
                            let mut regions_map = BTreeMap::new();

                            if let Some(Value::Array(regions)) = country_obj.get("regions") {
                                for region in regions {
                                    if let Value::Object(region_obj) = region {
                                        let region_name = region_obj
                                            .get("name")
                                            .and_then(Value::as_str)
                                            .unwrap_or_default()
                                            .to_string();
                                        let mut cities_map = BTreeMap::new();

                                        if let Some(Value::Array(cities)) = region_obj.get("cities")
                                        {
                                            for city in cities {
                                                if let Value::Object(city_obj) = city {
                                                    let city_name = city_obj
                                                        .get("name")
                                                        .and_then(Value::as_str)
                                                        .unwrap_or_default()
                                                        .to_string();
                                                    let mut people_vec = Vec::new();

                                                    if let Some(Value::Array(people)) =
                                                        city_obj.get("people")
                                                    {
                                                        for person in people {
                                                            if let Some(person_name) =
                                                                person.as_str()
                                                            {
                                                                people_vec
                                                                    .push(person_name.to_string());
                                                            }
                                                        }
                                                    }

                                                    cities_map.insert(city_name, people_vec);
                                                }
                                            }
                                        }

                                        regions_map.insert(region_name, cities_map);
                                    }
                                }
                            }

                            countries_map.insert(country_name, regions_map);
                        }
                    }
                }

                detailed_hierarchy.insert(continent_name, countries_map);
            }
        }
    }

    Ok(detailed_hierarchy)
}
