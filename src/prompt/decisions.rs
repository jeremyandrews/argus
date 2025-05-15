/// Generate a prompt to determine if an article describes a life threat
pub fn threat_prompt(article_text: &str) -> String {
    format!(
        r#"
----------
{article}
----------
Is this article describing an **ongoing** or **imminent** event or situation that might pose
a threat to human life or health? Answer ONLY 'yes' or 'no'."#,
        article = article_text
    )
}

/// Generate a prompt to determine if a region is affected by a life threat
pub fn region_threat_prompt(
    article_text: &str,
    region: &str,
    country: &str,
    continent: &str,
) -> String {
    format!(
        r#"
----------
{article}
----------
This article mentions that people in {region}, {country}, {continent} may be affected by an ongoing or imminent life-threatening event. 
Please confirm if the article is indeed about such an event in this region. Answer yes or no, and explain briefly why."#,
        article = article_text,
        region = region,
        country = country,
        continent = continent
    )
}

/// Generate a prompt to determine if a city is affected by a life threat
pub fn city_threat_prompt(
    article_text: &str,
    city_name: &str,
    region: &str,
    country: &str,
    continent: &str,
) -> String {
    format!(
        r#"
----------
{article}
----------

This article mentions that people in or near {city}, {region}, {country}, {continent} may be affected by an ongoing or imminent life-threatening event. 
Please confirm if the article is indeed about such an event in this city. Answer yes or no, and explain briefly why."#,
        article = article_text,
        city = city_name,
        region = region,
        country = country,
        continent = continent
    )
}

/// Generate a prompt to determine if an article is about a specific topic
pub fn is_this_about(article_text: &str, topic_name: &str) -> String {
    format!(
        r#"
==========
{article}
----------

Question: Does this article primarily focus on and provide substantial information about {topic}?
Instructions:
1. First, identify the article's language. If not in English:
   - Look for topic-relevant terms in that language (e.g., "Toscana" for "Tuscany")
   - Consider regional variations and local terminology
2. Carefully read the article summary above.
3. Compare the main focus of the article to the topic: {topic}
4. Check if the article is primarily promotional:
   - REJECT articles mainly about price reductions, discounts, or sales
   - REJECT articles with titles or focuses like "Slashes Prices", "Offers Discounts", etc.
   - REJECT articles that exist mainly to promote a sale or special offer
   - ONLY accept articles that provide substantial information beyond price
5. Answer ONLY 'Yes' or 'No' based on the following criteria:
   - Answer 'Yes' if BOTH of these are true:
     * The article is specifically about {topic} AND contains enough content for analysis
     * The article is NOT primarily promotional or about price reductions/sales
   - Answer 'No' if ANY of these are true:
     * The article is not primarily about {topic}
     * The article only mentions it briefly
     * The article is unrelated
     * The article is PRIMARILY about price drops, sales, or discounts
6. Do not explain your reasoning - provide only a one-word answer: 'Yes' or 'No'.
Answer:"#,
        article = article_text,
        topic = topic_name
    )
}

/// Generate a prompt to confirm if a summary is about a specific topic
pub fn confirm_prompt(summary_response: &str, topic_name: &str) -> String {
    format!(
        r#"{summary}
Question: Confirm if this is a valid article about {topic}.
Instructions:
1. First validate the content quality regardless of language:
   a) Contains complete sentences and coherent paragraphs in any language
   b) Is not an error message, loading screen, or technical issue
   c) Is not just a headline or stub
   d) Is not primarily an advertisement

2. For non-English content:
   a) Consider local/native terms (e.g., "Toscana" for "Tuscany")
   b) Account for regional spelling variations
   c) Check for topic-specific local terminology
   d) Include regional subdivisions or administrative terms

3. Answer ONLY 'Yes' or 'No' based on these criteria:
   - Answer 'Yes' if ALL of these are true:
     a) Is valid article content (not an error/loading message)
     b) The article significantly discusses {topic} or applications of {topic} (in any language)
     c) Contains enough content for analysis
     d) Is not primarily a promotion or advertisement
   - Answer 'No' if ANY of these are true:
     a) Contains error messages or technical issues
     b) Is not complete article content
     c) The article only mentions {topic} in passing and provides no substantive information
     d) Is unrelated to {topic}
     e) Is primarily a promotion or advertisement

4. Do not explain your reasoning - provide only a one-word answer: 'Yes' or 'No'.
Answer:"#,
        summary = summary_response,
        topic = topic_name
    )
}

/// Generate a prompt to confirm if an article describes a life threat
pub fn confirm_threat_prompt(article_text: &str) -> String {
    format!(
        r#"
----------
{article}
----------

Question: Confirm if this article describes a current or imminent threat to human life or safety.
Instructions:
1. Carefully check if this describes an ACTUAL threat by verifying:
   a) Contains specific details about a current or imminent danger
   b) Is not an error message or technical issue
   c) Is not just a headline or stub
   d) Is not primarily an advertisement
2. Answer ONLY 'Yes' or 'No' based on these criteria:
   - Answer 'Yes' ONLY if ALL of these are true:
     a) Is valid article content (not an error/loading message)
     b) Describes a specific, current, or imminent threat
     c) The threat could affect human life or safety
     d) Contains enough details to understand the threat
   - Answer 'No' if ANY of these are true:
     a) Contains error messages or technical issues
     b) Is not complete article content
     c) Describes past events with no current threat
     d) Is speculative about future possibilities
     e) Is primarily promotional content
4. Do not explain your reasoning - provide only a one-word answer: 'Yes' or 'No'.
Answer:"#,
        article = article_text
    )
}

/// Generate a prompt to filter promotional content
pub fn filter_promotional_content(article_text: &str) -> String {
    format!(
        r#"
----------
{article}
----------

Question: Is this article primarily about a product price reduction, discount, sale, or special offer?
Instructions:
1. Carefully examine the article to determine if its primary purpose is to announce or promote:
   - Price reductions or discounts
   - Limited-time sales or offers
   - Special pricing events or promotions
   - Product deals or bargains

2. Look for these specific patterns:
   - Phrases like "slashes prices", "slashing prices", "price cut", "discount", "sale"
   - Focus on temporary price changes rather than product features
   - Primary emphasis on saving money rather than product information
   - Headlines emphasizing price reductions rather than product capabilities
   - Limited substantive information beyond pricing details

3. Examples of promotional articles to REJECT:
   - "Garmin Slashes Price on Forerunner 965 Smartwatch for Spring Sale"
   - "Hydrow Offers Discounts on Rowing Machines in April 2025"
   - "Amazon Slashes Prices on iPad Air, 11th Gen iPad"
   - "Sony Headphones at 40% Off During Memorial Day Weekend"

4. Answer ONLY 'Yes' or 'No' based on these criteria:
   - Answer 'Yes' if the article is PRIMARILY about a price reduction, sale, or discount
   - Answer 'No' if the article has substantial informational content beyond any mention of price

5. Do not explain your reasoning - provide only a one-word answer: 'Yes' or 'No'.
Answer:"#,
        article = article_text
    )
}
