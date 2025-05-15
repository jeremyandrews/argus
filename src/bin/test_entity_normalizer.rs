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

    // Test stemming normalization
    println!("\nStemming Normalization Tests:");
    let stem_test_names = [
        ("Rockets", EntityType::Product),
        ("Satellites", EntityType::Product),
        ("Engineering Teams", EntityType::Organization),
        ("Engineers", EntityType::Organization),
        ("Running", EntityType::Event),
    ];

    for (name, entity_type) in &stem_test_names {
        println!(
            "'{}' ({:?}) → '{}'",
            name,
            entity_type,
            normalizer.normalize(name, *entity_type)
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

    // Test substring matching
    println!("\nSubstring Matching Tests:");
    let substring_cases = [
        (
            "Atlas V",
            "United Launch Alliance Atlas V rocket",
            EntityType::Product,
        ),
        ("iPhone", "Apple iPhone 15", EntityType::Product),
        (
            "NASA",
            "NASA Goddard Space Flight Center",
            EntityType::Organization,
        ),
        ("John", "John Doe Smith", EntityType::Person), // Should NOT match
        ("App", "Apple", EntityType::Organization),     // Should NOT match
    ];

    for (name1, name2, entity_type) in &substring_cases {
        let matches = normalizer.names_match(name1, name2, *entity_type);
        println!(
            "'{}' vs '{}' ({:?}): {}",
            name1,
            name2,
            entity_type,
            if matches { "MATCH" } else { "NO MATCH" }
        );
    }

    // Test Levenshtein distance matching
    println!("\nLevenshtein Distance Matching Tests:");
    let levenshtein_cases = [
        ("Microsoft 356", "Microsoft 365", EntityType::Product), // Distance = 1
        ("Elon Muskk", "Elon Musk", EntityType::Person),         // Distance = 1
        ("MagSafe Chargr", "MagSafe Charger", EntityType::Product), // Distance = 1
        ("Microsoft Windows", "Microsoft Office", EntityType::Product), // Should NOT match
    ];

    for (name1, name2, entity_type) in &levenshtein_cases {
        let matches = normalizer.names_match(name1, name2, *entity_type);
        println!(
            "'{}' vs '{}' ({:?}): {}",
            name1,
            name2,
            entity_type,
            if matches { "MATCH" } else { "NO MATCH" }
        );
    }

    // Test cross-type entity matching
    println!("\nCross-Type Entity Matching Tests:");
    test_cross_type_matching();

    // Test our specific problem case
    println!("\nTesting Specific Problem Case:");
    test_problem_case();
}

/// Test cross-type compatibility between Products and Organizations
fn test_cross_type_matching() {
    let normalizer = EntityNormalizer::new();

    // Create an array of cross-type test cases
    let cross_type_cases = [
        (
            "Apple",
            EntityType::Organization,
            "iPhone",
            EntityType::Product,
        ),
        (
            "Microsoft",
            EntityType::Organization,
            "Windows",
            EntityType::Product,
        ),
        (
            "SpaceX",
            EntityType::Organization,
            "Starlink",
            EntityType::Product,
        ),
        (
            "Google",
            EntityType::Organization,
            "Android",
            EntityType::Product,
        ),
        // These should NOT match despite compatible types (different entities)
        (
            "Apple",
            EntityType::Organization,
            "Windows",
            EntityType::Product,
        ),
        (
            "Microsoft",
            EntityType::Organization,
            "MacBook",
            EntityType::Product,
        ),
    ];

    // Test each case for compatibility and name matching
    for (name1, type1, name2, type2) in &cross_type_cases {
        // Check if types are compatible
        let type_compatible = type1 == type2 || type1.is_compatible_with(type2);

        // Check if names match (this won't match for unrelated entities)
        let name_match = normalizer.names_match(name1, name2, *type1);

        println!(
            "Source: '{}' ({:?}), Target: '{}' ({:?}) => Types Compatible: {}, Name Match: {}",
            name1, type1, name2, type2, type_compatible, name_match
        );
    }
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

    // Test cross-type entity matching
    println!("\nCross-type entity matching:");
    let mut cross_type_overlap_count = 0;
    for source_entity in &source.entities {
        for target_entity in &target.entities {
            // Entities must be different types but compatible
            if source_entity.entity_type != target_entity.entity_type
                && source_entity
                    .entity_type
                    .is_compatible_with(&target_entity.entity_type)
            {
                if normalizer.names_match(
                    &source_entity.normalized_name,
                    &target_entity.normalized_name,
                    source_entity.entity_type, // Use source type for matching rules
                ) {
                    cross_type_overlap_count += 1;

                    println!(
                        "Cross-type match: '{}' ({:?}) vs '{}' ({:?})",
                        source_entity.name,
                        source_entity.entity_type,
                        target_entity.name,
                        target_entity.entity_type
                    );

                    break;
                }
            }
        }
    }

    println!(
        "Cross-type overlapping entities: {}",
        cross_type_overlap_count
    );

    // Create a case with Product and Organization that should match
    println!("\nSpecific cross-type test case:");
    let amazon_org = Entity::new(
        "Amazon",
        "amazon",
        EntityType::Organization,
        ImportanceLevel::Primary,
    );

    let amazon_products = [
        Entity::new(
            "Amazon Echo",
            "amazon echo",
            EntityType::Product,
            ImportanceLevel::Primary,
        ),
        Entity::new(
            "Amazon Kindle",
            "amazon kindle",
            EntityType::Product,
            ImportanceLevel::Primary,
        ),
        Entity::new(
            "Amazon Prime",
            "amazon prime",
            EntityType::Product,
            ImportanceLevel::Primary,
        ),
        Entity::new(
            "Microsoft Surface", // Should NOT match
            "microsoft surface",
            EntityType::Product,
            ImportanceLevel::Primary,
        ),
    ];

    for product in &amazon_products {
        let matches = normalizer.names_match(
            &amazon_org.normalized_name,
            &product.normalized_name,
            amazon_org.entity_type,
        );

        println!(
            "Org '{}' vs Product '{}': {} (Based on substring and token matching)",
            amazon_org.name,
            product.name,
            if matches { "MATCH" } else { "NO MATCH" }
        );

        // Also check type compatibility
        let type_compatible = amazon_org
            .entity_type
            .is_compatible_with(&product.entity_type);
        println!(
            "Type compatibility between {:?} and {:?}: {}",
            amazon_org.entity_type, product.entity_type, type_compatible
        );
    }
}
