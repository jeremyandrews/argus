use anyhow::Result;

fn fix_string_concatenation(values_part: &str) -> Result<String> {
    // Fix string concatenation issues in VALUES clause
    let mut result = values_part.to_string();

    // Handle the specific pattern from the error: URL || timestamp
    // Look for patterns like 'url' || 'timestamp' and merge them properly
    use regex::Regex;

    // Pattern: 'string1' || 'string2' -> 'string1string2'
    let concat_regex = Regex::new(r"'([^']*?)'\s*\|\|\s*'([^']*?)'").unwrap();

    // Keep applying the regex until no more matches (handles multiple concatenations)
    loop {
        let new_result = concat_regex.replace_all(&result, "'$1$2'").to_string();
        if new_result == result {
            break; // No more changes
        }
        result = new_result;
    }

    // Also handle unquoted concatenation patterns
    let unquoted_concat_regex = Regex::new(r"([^,\s]+)\s*\|\|\s*'([^']*?)'").unwrap();
    result = unquoted_concat_regex
        .replace_all(&result, "$1$2")
        .to_string();

    Ok(result)
}

fn main() -> Result<()> {
    println!("🧪 Testing string concatenation fix...");

    // Test case 1: The exact failing case from the error
    let test1 = "1,'https://science.slashdot.org/story/24/06/11/044219/early-morning-frost-spotted-on-some-of-mars-huge-mountains?utm_source=rss1.0mainlinkanon&utm_medium=feed' || '2024-06-11 09:36:57.000+0000',false,NULL,NULL,'science.slashdot.org:story:24:06:11:044219:early-morning-frost-spotted-on-some-of-mars-huge-mountains:',NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL);";

    let fixed1 = fix_string_concatenation(test1)?;
    println!("✅ Test 1 - Original failing case:");
    println!("   Input:  {}", &test1[..100]);
    println!("   Output: {}", &fixed1[..100]);

    // Verify the concatenation was fixed
    assert!(
        !fixed1.contains(" || "),
        "String concatenation should be removed"
    );
    assert!(fixed1.contains("'https://science.slashdot.org/story/24/06/11/044219/early-morning-frost-spotted-on-some-of-mars-huge-mountains?utm_source=rss1.0mainlinkanon&utm_medium=feed2024-06-11 09:36:57.000+0000'"), "URL and timestamp should be concatenated");

    // Test case 2: Multiple concatenations
    let test2 = "'part1' || 'part2' || 'part3',123,'another' || 'field'";
    let fixed2 = fix_string_concatenation(test2)?;
    println!("✅ Test 2 - Multiple concatenations:");
    println!("   Input:  {}", test2);
    println!("   Output: {}", fixed2);

    assert_eq!(fixed2, "'part1part2part3',123,'anotherfield'");

    // Test case 3: No concatenation (should remain unchanged)
    let test3 = "1,'normal_string',false,NULL,'another_string'";
    let fixed3 = fix_string_concatenation(test3)?;
    println!("✅ Test 3 - No concatenation:");
    println!("   Input:  {}", test3);
    println!("   Output: {}", fixed3);

    assert_eq!(fixed3, test3);

    // Test case 4: Mixed quoted and unquoted
    let test4 = "123 || 'timestamp',456,'normal'";
    let fixed4 = fix_string_concatenation(test4)?;
    println!("✅ Test 4 - Mixed quoted/unquoted:");
    println!("   Input:  {}", test4);
    println!("   Output: {}", fixed4);

    assert_eq!(fixed4, "123timestamp,456,'normal'");

    println!("🎉 All string concatenation tests passed!");
    Ok(())
}
