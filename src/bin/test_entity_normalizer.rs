use argus::entity::{
    normalizer::EntityNormalizer,
    types::{Entity, EntityType, ExtractedEntities, ImportanceLevel},
};

fn main() {
    println!("Entity Normalization Test Tool");
    println!("------------------------------");

    let normalizer = EntityNormalizer::new();

    // Test basic normalization
    println!("\nBasic Normalization Tests:");
    let test_names = [
        "Blue Origin",
        "Blue-Origin",
        "BLUE ORIGIN",
        " Blue  Origin ",
    ];

    for name in &test_names {
        println!(
            "'{}' → '{}'",
            name,
            normalizer.normalize(name, EntityType::Organization)
        );
    }

    // Test problematic cases
    println!("\nProblem Case Tests:");
    let problem_cases = [
        ("Project Kuiper", "Projekt Kuiper", EntityType::Product),
        ("Amazon", "Amazon.com", EntityType::Organization),
        ("SpaceX", "Space X", EntityType::Organization),
        ("SpaceX's Starlinks", "Starlink", EntityType::Product),
        (
            "OneWeb satellite constellation",
            "OneWeb",
            EntityType::Product,
        ),
    ];

    for (name1, name2, entity_type) in &problem_cases {
        let matches = normalizer.names_match(name1, name2, *entity_type);
        println!(
            "'{}' vs '{}' ({:?}): {}",
            name1,
            name2,
            entity_type,
            if matches { "MATCH" } else { "NO MATCH" }
        );
    }

    // Test our specific problem case
    println!("\nTesting Specific Problem Case:");
    test_problem_case();
}

/// Test the specific problem case from the user's test
fn test_problem_case() {
    // Create source entities
    let mut source = ExtractedEntities::new();
    source.add_entity(Entity::new(
        "Project Kuiper",
        "project kuiper",
        EntityType::Product,
        ImportanceLevel::Primary,
    ));
    source.add_entity(Entity::new(
        "Amazon",
        "amazon",
        EntityType::Organization,
        ImportanceLevel::Primary,
    ));
    source.add_entity(Entity::new(
        "Jeff Bezos",
        "jeff bezos",
        EntityType::Person,
        ImportanceLevel::Mentioned,
    ));

    // Create target entities
    let mut target = ExtractedEntities::new();
    target.add_entity(Entity::new(
        "Projekt Kuiper",
        "projekt kuiper",
        EntityType::Product,
        ImportanceLevel::Primary,
    ));
    target.add_entity(Entity::new(
        "Amazon",
        "amazon",
        EntityType::Organization,
        ImportanceLevel::Primary,
    ));
    target.add_entity(Entity::new(
        "Jeff Bezos",
        "jeff bezos",
        EntityType::Person,
        ImportanceLevel::Secondary,
    ));

    // Test with normalizer
    let normalizer = EntityNormalizer::new();

    // Check individual entity matches
    println!("\nEntity comparisons with normalization:");
    for source_entity in &source.entities {
        for target_entity in &target.entities {
            if source_entity.entity_type == target_entity.entity_type {
                let matches = normalizer.names_match(
                    &source_entity.normalized_name,
                    &target_entity.normalized_name,
                    source_entity.entity_type,
                );

                println!(
                    "'{}' ({:?}) vs '{}' ({:?}): {}",
                    source_entity.name,
                    source_entity.importance,
                    target_entity.name,
                    target_entity.importance,
                    if matches { "MATCH" } else { "NO MATCH" }
                );
            }
        }
    }

    // Count overlapping entities with normalization
    let mut overlap_count = 0;
    for source_entity in &source.entities {
        for target_entity in &target.entities {
            if source_entity.entity_type == target_entity.entity_type {
                if normalizer.names_match(
                    &source_entity.normalized_name,
                    &target_entity.normalized_name,
                    source_entity.entity_type,
                ) {
                    overlap_count += 1;
                    break; // Count each source entity only once
                }
            }
        }
    }

    println!(
        "\nOverlapping entities with normalization: {}",
        overlap_count
    );

    // Count overlapping entities without normalization (direct string comparison)
    let mut direct_overlap_count = 0;
    for source_entity in &source.entities {
        for target_entity in &target.entities {
            if source_entity.entity_type == target_entity.entity_type {
                if source_entity.normalized_name == target_entity.normalized_name {
                    direct_overlap_count += 1;
                    break;
                }
            }
        }
    }

    println!(
        "Overlapping entities without normalization: {}",
        direct_overlap_count
    );
}
