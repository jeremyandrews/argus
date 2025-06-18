use argus::entity::types::{Entity, EntityType, ExtractedEntities, ImportanceLevel};

fn main() {
    println!("Testing entity JSON serialization...");

    // Create test entities
    let entity1 = Entity::new(
        "Apple Inc.",
        "apple inc",
        EntityType::Organization,
        ImportanceLevel::Primary,
    );

    let entity2 = Entity::new(
        "Tim Cook",
        "tim cook",
        EntityType::Person,
        ImportanceLevel::Secondary,
    );

    let entity3 = Entity::new(
        "iPhone 15",
        "iphone 15",
        EntityType::Product,
        ImportanceLevel::Primary,
    );

    // Test individual entity JSON conversion
    println!("Individual entity JSON:");
    println!(
        "{}",
        serde_json::to_string_pretty(&entity1.to_frontend_json()).unwrap()
    );

    // Create ExtractedEntities collection
    let mut extracted = ExtractedEntities::new();
    extracted.add_entity(entity1);
    extracted.add_entity(entity2);
    extracted.add_entity(entity3);

    // Test collection JSON conversion
    println!("\nAll entities as frontend JSON array:");
    let entities_array = extracted.to_frontend_json_array();
    println!("{}", serde_json::to_string_pretty(&entities_array).unwrap());

    // Simulate what would be added to response_json
    let mut response_json = serde_json::json!({
        "topic": "Test Article",
        "title": "Apple Announces New iPhone",
        "summary": "Test summary...",
    });

    // Add entities using our new functionality
    response_json["entities"] = serde_json::json!(extracted.to_frontend_json_array());

    println!("\nComplete response JSON with entities:");
    println!("{}", serde_json::to_string_pretty(&response_json).unwrap());

    println!("\n✅ Entity JSON serialization test completed successfully!");
}
