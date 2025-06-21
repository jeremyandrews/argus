use anyhow::Result;

// Copy the relevant functions from migrate_to_postgres.rs for testing
fn get_boolean_columns(table_name: &str) -> Vec<usize> {
    match table_name {
        "articles" => vec![3],            // is_relevant is 4th column (0-indexed: 3)
        "configurations" => vec![4],      // enabled is 5th column (0-indexed: 4)
        "endpoint_alerts" => vec![9],     // is_resolved is 10th column (0-indexed: 9)
        "alias_pattern_stats" => vec![6], // enabled is 7th column (0-indexed: 6)
        _ => vec![],
    }
}

fn transform_boolean_values(sql: &str, table_name: &str) -> Result<String> {
    let boolean_positions = get_boolean_columns(table_name);
    if boolean_positions.is_empty() {
        return Ok(sql.to_string());
    }

    // Find the VALUES clause
    if let Some(values_start) = sql.find(" VALUES(") {
        let before_values = &sql[..values_start + 8]; // Include " VALUES("
        let values_content = &sql[values_start + 8..];

        // Find the closing parenthesis for the VALUES clause
        if let Some(values_end) = values_content.rfind(')') {
            let values_data = &values_content[..values_end];
            let after_values = &values_content[values_end..]; // Include closing paren and semicolon

            // Parse and transform the values
            let transformed_values = transform_values_data(values_data, &boolean_positions)?;

            return Ok(format!(
                "{}{}{}",
                before_values, transformed_values, after_values
            ));
        }
    }

    // Fallback: use simple string replacement for basic cases
    let mut result = sql.to_string();

    // Handle common boolean patterns
    result = result.replace(",0,", ",false,");
    result = result.replace(",1,", ",true,");
    result = result.replace("(0,", "(false,");
    result = result.replace("(1,", "(true,");
    result = result.replace(",0)", ",false)");
    result = result.replace(",1)", ",true)");

    // Handle quoted boolean values
    result = result.replace(",'0',", ",false,");
    result = result.replace(",'1',", ",true,");
    result = result.replace("('0',", "(false,");
    result = result.replace("('1',", "(true,");
    result = result.replace(",'0')", ",false)");
    result = result.replace(",'1')", ",true)");

    Ok(result)
}

fn transform_values_data(values_data: &str, boolean_positions: &[usize]) -> Result<String> {
    // Simple CSV parsing for VALUES data
    let mut result = String::new();
    let mut current_field = String::new();
    let mut field_index = 0;
    let mut in_quotes = false;
    let mut escape_next = false;

    for ch in values_data.chars() {
        if escape_next {
            current_field.push(ch);
            escape_next = false;
            continue;
        }

        match ch {
            '\\' if in_quotes => {
                escape_next = true;
                current_field.push(ch);
            }
            '\'' => {
                in_quotes = !in_quotes;
                current_field.push(ch);
            }
            ',' if !in_quotes => {
                // End of field
                let transformed_field =
                    transform_field_if_boolean(&current_field, field_index, boolean_positions);
                result.push_str(&transformed_field);
                result.push(',');

                current_field.clear();
                field_index += 1;
            }
            _ => {
                current_field.push(ch);
            }
        }
    }

    // Handle the last field
    if !current_field.is_empty() {
        let transformed_field =
            transform_field_if_boolean(&current_field, field_index, boolean_positions);
        result.push_str(&transformed_field);
    }

    Ok(result)
}

fn transform_field_if_boolean(
    field: &str,
    field_index: usize,
    boolean_positions: &[usize],
) -> String {
    if !boolean_positions.contains(&field_index) {
        return field.to_string();
    }

    let trimmed = field.trim();

    // Handle various boolean representations
    match trimmed {
        "0" | "'0'" => "false".to_string(),
        "1" | "'1'" => "true".to_string(),
        "NULL" | "null" => "NULL".to_string(),
        _ => {
            // If it's not a clear boolean value, keep it as-is
            // This handles cases where the field might already be transformed
            if trimmed == "true" || trimmed == "false" {
                trimmed.to_string()
            } else {
                // Log unexpected boolean value for debugging
                eprintln!(
                    "Warning: Unexpected boolean value '{}' at position {}",
                    trimmed, field_index
                );
                field.to_string()
            }
        }
    }
}

fn main() -> Result<()> {
    println!("🧪 Testing boolean transformation logic...");

    // Test case 1: The exact failing case from the error
    let test_sql_1 = "INSERT INTO articles (id, url, seen_at, is_relevant, category, analysis, normalized_url, hash, tiny_summary, title_domain_hash, r2_url, pub_date, event_date, cluster_id, title, json_data, quality, source) VALUES(1,'https://science.slashdot.org/story/24/06/11/044219/early-morning-frost-spotted-on-some-of-mars-huge-mountains?utm_source=rss1.0mainlinkanon&utm_medium=feed','2024-06-11 09:36:57.000+0000',0,NULL,NULL,'science.slashdot.org:story:24:06:11:044219:early-morning-frost-spotted-on-some-of-mars-huge-mountains:',NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL);";

    let result_1 = transform_boolean_values(test_sql_1, "articles")?;
    println!("✅ Test 1 - Original failing case:");
    println!("   Input:  {}", test_sql_1);
    println!("   Output: {}", result_1);

    // Verify the transformation worked
    if result_1.contains(",false,") && !result_1.contains(",0,") {
        println!("   ✅ Boolean transformation successful!");
    } else {
        println!("   ❌ Boolean transformation failed!");
        return Err(anyhow::anyhow!("Test 1 failed"));
    }

    // Test case 2: Multiple boolean values
    let test_sql_2 = "INSERT INTO configurations (id, category, name, value, enabled, created_at, updated_at) VALUES(1,'topics','test','value',1,'2024-01-01','2024-01-01');";

    let result_2 = transform_boolean_values(test_sql_2, "configurations")?;
    println!("\n✅ Test 2 - Configuration table:");
    println!("   Input:  {}", test_sql_2);
    println!("   Output: {}", result_2);

    if result_2.contains(",true,") && !result_2.contains(",1,") {
        println!("   ✅ Boolean transformation successful!");
    } else {
        println!("   ❌ Boolean transformation failed!");
        return Err(anyhow::anyhow!("Test 2 failed"));
    }

    // Test case 3: Quoted boolean values
    let test_sql_3 = "INSERT INTO articles (id, url, seen_at, is_relevant, category, analysis, normalized_url, hash, tiny_summary, title_domain_hash, r2_url, pub_date, event_date, cluster_id, title, json_data, quality, source) VALUES(2,'https://example.com','2024-01-01','1',NULL,NULL,'example.com',NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL);";

    let result_3 = transform_boolean_values(test_sql_3, "articles")?;
    println!("\n✅ Test 3 - Quoted boolean values:");
    println!("   Input:  {}", test_sql_3);
    println!("   Output: {}", result_3);

    if result_3.contains(",true,") && !result_3.contains(",'1',") {
        println!("   ✅ Boolean transformation successful!");
    } else {
        println!("   ❌ Boolean transformation failed!");
        return Err(anyhow::anyhow!("Test 3 failed"));
    }

    // Test case 4: Table with no boolean columns
    let test_sql_4 = "INSERT INTO entities (id, name, type, normalized_name, parent_id) VALUES(1,'Test Entity','PERSON','test entity',NULL);";

    let result_4 = transform_boolean_values(test_sql_4, "entities")?;
    println!("\n✅ Test 4 - No boolean columns:");
    println!("   Input:  {}", test_sql_4);
    println!("   Output: {}", result_4);

    if result_4 == test_sql_4 {
        println!("   ✅ No transformation applied (correct)!");
    } else {
        println!("   ❌ Unexpected transformation!");
        return Err(anyhow::anyhow!("Test 4 failed"));
    }

    println!("\n🎉 All tests passed! Boolean transformation logic is working correctly.");
    println!("\n📋 Summary:");
    println!("   - Handles unquoted boolean values (0 → false, 1 → true)");
    println!("   - Handles quoted boolean values ('0' → false, '1' → true)");
    println!("   - Only transforms columns identified as boolean");
    println!("   - Leaves non-boolean tables unchanged");
    println!("\n🚀 The migration should now work correctly!");

    Ok(())
}
