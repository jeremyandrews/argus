use regex::Regex;

fn main() {
    // Sample text containing various entity alias patterns
    let samples = [
        "Apple Inc., also known as Apple Computer, reported record earnings.",
        "Microsoft Corporation (formerly Micro-Soft) was founded by Bill Gates.",
        "Alphabet Inc., which is the parent company of Google, has diversified its business.",
        "Meta (aka Facebook) has rebranded itself to focus on the metaverse.",
        "Tim Cook, the CEO of Apple Inc., announced new products today.",
        "Twitter, now known as X, has undergone significant changes.",
        "IBM, which acquired Red Hat in 2019, continues to focus on cloud computing.",
        "JK (full name Joanne Kathleen Rowling) is the author of Harry Potter.",
        "SpaceX, which was founded by Elon Musk, launched another rocket today.",
        "Warner Bros Discovery, which is the parent company of HBO, reported quarterly earnings.",
        "Tesla, which was created by Elon Musk, is a leading electric vehicle manufacturer.",
        // Problem cases that shouldn't match
        "The weather was nice today and the temperature was 75 degrees.",
        "Body: NEW YORK CITY - The mayor announced a new policy today that will affect residents.",
        "at risk because of the Trump administration's broad cuts to USAID",
    ];

    // Define our patterns
    let patterns = [
        // Entity X, also known as Y - limits entity to 100 chars without newlines, prefers capitalized words
        r#"(?i)(?P<canonical>[A-Z][^,\.\(\)\n]{1,98}[^,\.\(\)\s\n]),?\s+(?:also\s+)?(?:known|called|referred\s+to)\s+as\s+["']?(?P<alias>[A-Z][^,\.\(\)\n]{0,98}[^,\.\(\)\s\n])["']?(?:[,\.\)]|$)"#,
        // Entity X (aka/formerly Y) - stricter boundary conditions
        r#"(?i)(?P<canonical>[A-Z][^,\.\(\)\n]{1,98}[^,\.\(\)\s\n])\s+\((?:a\.?k\.?a\.?|formerly|previously|originally|né[e]?)\s+["']?(?P<alias>[^,\.\(\)\n]{2,100}?)["']?\)"#,
        // Y, now known as X
        r#"(?i)["']?(?P<alias>[A-Z][^,\.\(\)\n]{1,98}[^,\.\(\)\s\n])["']?,?\s+now\s+(?:known\s+as\s+)?["']?(?P<canonical>[^,\.\(\)\n]{2,100}?)["']?(?:[,\.\)]|$)"#,
        // X, which rebranded as Y
        r#"(?i)(?P<canonical>[A-Z][^,\.\(\)\n]{1,98}[^,\.\(\)\s\n]),?\s+which\s+(?:rebranded|renamed)\s+(?:itself\s+)?(?:as|to)\s+["']?(?P<alias>[^,\.\(\)\n]{2,100}?)["']?(?:[,\.\)]|$)"#,
        // X (full name Y)
        r#"(?i)(?P<alias>[A-Z][^,\.\(\)\n]{0,20}[^,\.\(\)\s\n])\s+\((?:full\s+name|real\s+name|birth\s+name)\s+["']?(?P<canonical>[^,\.\(\)\n]{2,100}?)["']?\)"#,
        // Company acquisition pattern
        r#"(?i)(?P<canonical>[A-Z][^,\.\(\)\n]{1,98}[^,\.\(\)\s\n]),?\s+(?:which|that)\s+(?:acquired|bought|purchased)\s+["']?(?P<alias>[A-Z][^,\.\(\)\n]{1,98}[^,\.\(\)\s\n])["']?(?:[,\.\)]|$)"#,
        // Person title pattern (more specific for people)
        r#"(?i)(?P<canonical>[A-Z][a-zA-Z\-\'\s]{2,50}),?\s+(?:(?:the|a)\s+)?(?:CEO|founder|president|director|chairman|head|leader)\s+of\s+["']?(?P<alias>[A-Z][^,\.\(\)\n]{1,98}[^,\.\(\)\s\n])["']?(?:[,\.\)]|$)"#,
        // Parent company relationship pattern
        r#"(?i)(?P<canonical>[A-Z][^,\.\(\)\n]{1,98}[^,\.\(\)\s\n]),?\s+(?:which|that)\s+is\s+(?:the\s+)?(?:parent|holding)\s+company\s+of\s+["']?(?P<alias>[A-Z][^,\.\(\)\n]{1,98}[^,\.\(\)\s\n])["']?(?:[,\.\)]|$)"#,
        // Founder/created by pattern
        r#"(?i)(?P<alias>[A-Z][^,\.\(\)\n]{1,98}[^,\.\(\)\s\n]),?\s+(?:which|that)\s+was\s+(?:founded|created|started)\s+by\s+["']?(?P<canonical>[A-Z][a-zA-Z\-\'\s]{2,50})["']?(?:[,\.\)]|$)"#,
    ];

    // Compile the patterns
    let compiled_patterns: Vec<Regex> =
        patterns.iter().filter_map(|p| Regex::new(p).ok()).collect();

    println!(
        "Testing {} patterns against {} sample texts\n",
        patterns.len(),
        samples.len()
    );

    // For each sample, try to match all patterns
    for (i, sample) in samples.iter().enumerate() {
        println!("Sample {}: \"{}\"", i + 1, sample);
        let mut found_match = false;

        for (pattern_idx, pattern) in compiled_patterns.iter().enumerate() {
            for cap in pattern.captures_iter(sample) {
                if let (Some(canonical_match), Some(alias_match)) =
                    (cap.name("canonical"), cap.name("alias"))
                {
                    let canonical = canonical_match.as_str().trim();
                    let alias = alias_match.as_str().trim();

                    println!(
                        "  ✅ Match using pattern {}: '{}' ↔ '{}'",
                        pattern_idx + 1,
                        canonical,
                        alias
                    );
                    found_match = true;
                }
            }
        }

        if !found_match {
            println!("  ❌ No matches found");
        }
        println!();
    }
}
