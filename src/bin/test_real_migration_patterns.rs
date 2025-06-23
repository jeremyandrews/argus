use anyhow::Result;

fn fix_string_concatenation(values_part: &str) -> Result<String> {
    // Fix string concatenation issues in VALUES clause
    let mut result = values_part.to_string();

    // Handle the specific pattern from the error: URL || timestamp
    // Look for patterns like 'url' || 'timestamp' and merge them properly
    use regex::Regex;

    // More aggressive pattern matching to handle all concatenation cases
    // Pattern 1: 'string1' || 'string2' -> 'string1string2'
    let quoted_concat_regex = Regex::new(r"'([^']*)'\s*\|\|\s*'([^']*)'").unwrap();

    // Keep applying the regex until no more matches (handles multiple concatenations)
    loop {
        let new_result = quoted_concat_regex
            .replace_all(&result, "'$1$2'")
            .to_string();
        if new_result == result {
            break; // No more changes
        }
        result = new_result;
    }

    // Pattern 2: Handle mixed patterns like value || 'string'
    let mixed_concat_regex = Regex::new(r"([^,\s']+)\s*\|\|\s*'([^']*)'").unwrap();
    result = mixed_concat_regex
        .replace_all(&result, "'$1$2'")
        .to_string();

    // Pattern 3: Handle 'string' || value patterns
    let reverse_mixed_regex = Regex::new(r"'([^']*)'\s*\|\|\s*([^,\s']+)").unwrap();
    result = reverse_mixed_regex
        .replace_all(&result, "'$1$2'")
        .to_string();

    // Pattern 4: Handle unquoted || unquoted patterns
    let unquoted_concat_regex = Regex::new(r"([^,\s']+)\s*\|\|\s*([^,\s']+)").unwrap();
    result = unquoted_concat_regex
        .replace_all(&result, "'$1$2'")
        .to_string();

    Ok(result)
}

fn main() -> Result<()> {
    println!("🧪 Testing real migration patterns...");

    // Test the exact failing patterns from the migration output
    let real_patterns = vec![
        // Pattern 1: URL || timestamp (from line 1)
        "1,'https://science.slashdot.org/story/24/06/11/044219/early-morning-frost-spotted-on-some-of-mars-huge-mountains?utm_source=rss1.0mainlinkanon&utm_medium=feed' || '2024-06-11 09:36:57.000+0000',false,NULL,NULL,'science.slashdot.org:story:24:06:11:044219:early-morning-frost-spotted-on-some-of-mars-huge-mountains:',NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL);",

        // Pattern 2: URL || timestamp + hash || hash (from line 2)
        "40504,'https://www.aljazeera.com/features/2024/10/11/the-south-african-anti-apartheid-cathedral-now-a-pro-palestine-hub?traffic_source=rss' || '2024-10-11 11:07:55.000+0000',false,NULL,NULL,'aljazeera.com:features:2024:10:11:the-south-african-anti-apartheid-cathedral-now-a-pro-palestine-hub:traffic_source:rss:' || 'f116492ba7695b4a3ef310253b3951db013293f8e60186ab53235f81f7ca51dc',NULL,'36df4fc5bfab85fad95c5286b5e320f3a0b71c4eaa21bc56a8bfd14004f20c15',NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL);",

        // Pattern 3: Simple URL || timestamp (from line 3)
        "40505,'https://finance.yahoo.com/news/meet-unstoppable-growth-stock-could-080200237.html' || '2024-10-11 11:07:56.000+0000',false,NULL,NULL,'finance.yahoo.com:news:meet-unstoppable-growth-stock-could-080200237:' || '8c62ef37a3c1d38616cb2d837f6ae8becfaa7ce03df3875e1a62c8e0287da235',NULL,'5291f36333f185c864e98557e1d279ed35428764dfb35d43811ef00b95cfc570',NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL);",
    ];

    for (i, pattern) in real_patterns.iter().enumerate() {
        println!("\n✅ Test {} - Real migration pattern:", i + 1);
        println!(
            "   Input (first 100 chars): {}",
            &pattern[..100.min(pattern.len())]
        );

        let fixed = fix_string_concatenation(pattern)?;
        println!(
            "   Output (first 100 chars): {}",
            &fixed[..100.min(fixed.len())]
        );

        // Verify no || operators remain
        if fixed.contains(" || ") {
            println!("   ❌ FAILED: Still contains || operators");
            println!("   Full output: {}", fixed);
            return Err(anyhow::anyhow!(
                "String concatenation fix failed for pattern {}",
                i + 1
            ));
        } else {
            println!("   ✅ SUCCESS: No || operators found");
        }

        // Count the number of values to ensure we don't break the structure
        let input_commas = pattern.matches(',').count();
        let output_commas = fixed.matches(',').count();

        if input_commas != output_commas {
            println!(
                "   ⚠️  WARNING: Comma count changed from {} to {}",
                input_commas, output_commas
            );
        } else {
            println!("   ✅ Comma count preserved: {}", output_commas);
        }
    }

    println!("\n🎉 All real migration patterns processed successfully!");
    Ok(())
}
