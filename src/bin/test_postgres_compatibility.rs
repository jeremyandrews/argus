use anyhow::Result;

// Copy the relevant functions from migrate_to_postgres.rs for testing
fn fix_postgres_compatibility(sql: &str) -> Result<String> {
    let mut result = sql.to_string();

    // Fix common PostgreSQL compatibility issues

    // 1. Handle char() function calls - PostgreSQL uses chr() instead of char()
    result = result.replace("char(10)", "chr(10)");
    result = result.replace("char(13)", "chr(13)");

    // 2. Handle problematic escape sequences in string literals
    // Look for patterns like ','\n',char(10)),' and fix them
    use regex::Regex;

    // Fix newline escape sequences that might be causing issues
    let newline_regex = Regex::new(r"'\\n'").unwrap();
    result = newline_regex.replace_all(&result, "E'\\n'").to_string();

    // Fix carriage return escape sequences
    let cr_regex = Regex::new(r"'\\r'").unwrap();
    result = cr_regex.replace_all(&result, "E'\\r'").to_string();

    // Fix tab escape sequences
    let tab_regex = Regex::new(r"'\\t'").unwrap();
    result = tab_regex.replace_all(&result, "E'\\t'").to_string();

    // 3. Handle problematic quote escaping
    // Replace sequences like '\'' with proper PostgreSQL escaping
    result = result.replace("\\'", "''");

    // 4. Handle NULL byte characters that might cause issues
    result = result.replace("\\0", "");

    // 5. Fix any remaining char() calls to chr()
    let char_regex = Regex::new(r"\bchar\((\d+)\)").unwrap();
    result = char_regex.replace_all(&result, "chr($1)").to_string();

    Ok(result)
}

fn main() -> Result<()> {
    println!("🧪 Testing PostgreSQL compatibility fixes...");

    // Test case 1: The problematic line from the error
    let test_sql_1 =
        "INSERT INTO articles VALUES(1,'Some text with char(10) and \\n newlines','data');";

    let result_1 = fix_postgres_compatibility(test_sql_1)?;
    println!("✅ Test 1 - char() function conversion:");
    println!("   Input:  {}", test_sql_1);
    println!("   Output: {}", result_1);

    // Verify the transformation worked
    if result_1.contains("chr(10)") && !result_1.contains("char(10)") {
        println!("   ✅ char() to chr() conversion successful!");
    } else {
        println!("   ❌ char() to chr() conversion failed!");
        return Err(anyhow::anyhow!("Test 1 failed"));
    }

    // Test case 2: Escape sequences
    let test_sql_2 = "INSERT INTO table VALUES('text with '\\n' newline and '\\t' tab');";

    let result_2 = fix_postgres_compatibility(test_sql_2)?;
    println!("\n✅ Test 2 - Escape sequences:");
    println!("   Input:  {}", test_sql_2);
    println!("   Output: {}", result_2);

    if result_2.contains("E'\\n'") && result_2.contains("E'\\t'") {
        println!("   ✅ Escape sequence conversion successful!");
    } else {
        println!("   ❌ Escape sequence conversion failed!");
        return Err(anyhow::anyhow!("Test 2 failed"));
    }

    // Test case 3: Quote escaping
    let test_sql_3 = "INSERT INTO table VALUES('text with \\' quote');";

    let result_3 = fix_postgres_compatibility(test_sql_3)?;
    println!("\n✅ Test 3 - Quote escaping:");
    println!("   Input:  {}", test_sql_3);
    println!("   Output: {}", result_3);

    if result_3.contains("''") && !result_3.contains("\\'") {
        println!("   ✅ Quote escaping conversion successful!");
    } else {
        println!("   ❌ Quote escaping conversion failed!");
        return Err(anyhow::anyhow!("Test 3 failed"));
    }

    // Test case 4: Complex case similar to the error
    let test_sql_4 =
        "INSERT INTO table VALUES('complex text with '\\n' and char(10) and \\' quotes');";

    let result_4 = fix_postgres_compatibility(test_sql_4)?;
    println!("\n✅ Test 4 - Complex case:");
    println!("   Input:  {}", test_sql_4);
    println!("   Output: {}", result_4);

    if result_4.contains("chr(10)") && result_4.contains("E'\\n'") && result_4.contains("''") {
        println!("   ✅ Complex transformation successful!");
    } else {
        println!("   ❌ Complex transformation failed!");
        return Err(anyhow::anyhow!("Test 4 failed"));
    }

    // Test case 5: NULL bytes
    let test_sql_5 = "INSERT INTO table VALUES('text with \\0 null byte');";

    let result_5 = fix_postgres_compatibility(test_sql_5)?;
    println!("\n✅ Test 5 - NULL byte handling:");
    println!("   Input:  {}", test_sql_5);
    println!("   Output: {}", result_5);

    if !result_5.contains("\\0") {
        println!("   ✅ NULL byte removal successful!");
    } else {
        println!("   ❌ NULL byte removal failed!");
        return Err(anyhow::anyhow!("Test 5 failed"));
    }

    println!("\n🎉 All tests passed! PostgreSQL compatibility fixes are working correctly.");
    println!("\n📋 Summary:");
    println!("   - Converts char() functions to chr() for PostgreSQL");
    println!("   - Handles escape sequences with E'' syntax");
    println!("   - Fixes quote escaping for PostgreSQL");
    println!("   - Removes problematic NULL bytes");
    println!("   - Handles complex combinations of all issues");
    println!("\n🚀 The migration should now handle special characters correctly!");

    Ok(())
}
