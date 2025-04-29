use super::types::EntityType;
use lazy_static::lazy_static;
use std::collections::HashMap;

// Common cross-language and spelling variations
pub const COMMON_VARIATIONS: &[(&str, &str)] = &[
    ("project", "projekt"),
    ("center", "centre"),
    ("defense", "defence"),
    ("program", "programme"),
    ("color", "colour"),
    ("theater", "theatre"),
    ("organization", "organisation"),
    ("analyzer", "analyser"),
];

lazy_static! {
    // Person aliases
    static ref PERSON_ALIASES: HashMap<String, String> = {
        let mut map = HashMap::new();
        map.insert("jeff bezos".to_string(), "jeff bezos".to_string());
        map.insert("jeffrey bezos".to_string(), "jeff bezos".to_string());
        map.insert("jeffrey p bezos".to_string(), "jeff bezos".to_string());

        map.insert("elon musk".to_string(), "elon musk".to_string());
        map.insert("elon r musk".to_string(), "elon musk".to_string());
        // Add more common person aliases
        map
    };

    // Organization aliases
    static ref ORG_ALIASES: HashMap<String, String> = {
        let mut map = HashMap::new();
        map.insert("blue origin".to_string(), "blue origin".to_string());
        map.insert("blueorigin".to_string(), "blue origin".to_string());

        map.insert("spacex".to_string(), "spacex".to_string());
        map.insert("space x".to_string(), "spacex".to_string());
        map.insert("space exploration technologies".to_string(), "spacex".to_string());

        map.insert("ula".to_string(), "united launch alliance".to_string());
        map.insert("united launch alliance".to_string(), "united launch alliance".to_string());
        // Add more organization aliases
        map
    };

    // Product aliases
    static ref PRODUCT_ALIASES: HashMap<String, String> = {
        let mut map = HashMap::new();
        map.insert("project kuiper".to_string(), "project kuiper".to_string());
        map.insert("projekt kuiper".to_string(), "project kuiper".to_string());

        map.insert("starlink".to_string(), "starlink".to_string());
        map.insert("spacexs starlinks".to_string(), "starlink".to_string());
        map.insert("spacex starlink".to_string(), "starlink".to_string());
        map.insert("spacex's starlinks".to_string(), "starlink".to_string());

        map.insert("atlas v".to_string(), "atlas v".to_string());
        map.insert("atlas 5".to_string(), "atlas v".to_string());
        // Add more product aliases
        map
    };

    // Location aliases
    static ref LOCATION_ALIASES: HashMap<String, String> = {
        let mut map = HashMap::new();
        map.insert("usa".to_string(), "united states".to_string());
        map.insert("united states".to_string(), "united states".to_string());
        map.insert("united states of america".to_string(), "united states".to_string());

        // Add more location aliases
        map
    };
}

pub fn get_aliases_for_type(entity_type: EntityType) -> HashMap<String, String> {
    match entity_type {
        EntityType::Person => PERSON_ALIASES.clone(),
        EntityType::Organization => ORG_ALIASES.clone(),
        EntityType::Product => PRODUCT_ALIASES.clone(),
        EntityType::Location => LOCATION_ALIASES.clone(),
        _ => HashMap::new(),
    }
}
