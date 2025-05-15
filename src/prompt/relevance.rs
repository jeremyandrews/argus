use crate::prompt::common::{
    global_context, DONT_TELL_ME, FORMAT_INSTRUCTIONS, WRITE_IN_CLEAR_ENGLISH,
};
use std::collections::BTreeMap;

/// Generate a prompt for analyzing an article's relation to a specific topic
pub fn relation_to_topic_prompt(
    article_text: &str,
    topic_prompt: &str,
    pub_date: Option<&str>,
) -> String {
    format!(
        r#" {context}
## ARTICLE (FOR RELATION TO TOPIC ANALYSIS):
----------
{article}
----------

IMPORTANT INSTRUCTIONS:
* **Analyze ONLY the article above.**
* **IGNORE the global context unless explicitly mentioned in article.**
* **For non-English text, include translations of relevant quotes.**

### **Topic Relationship Analysis**
Determine how the article relates to: **{topic}**

* **Direct Relation:** Article explicitly discusses the topic
* **Indirect Relation:** Article touches on topic through related themes
* **No Relation:** Article has no meaningful connection to topic

### **Response Format**
Provide exactly two sentences that:

**First Sentence MUST:**
* Begin with one of these EXACT phrases:
  - For direct relation: "This article relates to {topic} because..."
  - For indirect relation: "This article indirectly relates to {topic} because..."
  - For no relation: "This article does not relate to {topic} because..."
* Clearly explain the connection (or lack thereof)
* Include specific evidence from the article
* Reference relevant quotes (with translations if non-English)

**Second Sentence MUST:**
* Provide additional supporting details
* Include specific data, examples, or events from the article
* Maintain focus on the article's content
* Avoid speculation or external information

**EXAMPLE (Direct Relation):**
"This article relates to climate change because it reports new data showing global temperatures rose 1.5°C in 2024, with detailed analysis from three independent research institutions. The findings specifically link this increase to a 12% rise in extreme weather events across 40 countries, resulting in $50 billion in economic damage."

**EXAMPLE (Indirect Relation):**
"This article indirectly relates to artificial intelligence because while focusing on semiconductor manufacturing, it discusses how 35% of chip production now serves AI-specific computing needs. The report details how TSMC's $20 billion factory expansion specifically targets AI processor production, indicating the technology's growing influence on hardware development."

**EXAMPLE (No Relation):**
"This article does not relate to healthcare because it exclusively covers changes in professional sports regulations and athlete compensation policies. The content focuses entirely on new salary cap rules affecting 32 teams, with no mention of health, medical care, or player wellness issues."

Now explain how the article relates to **{topic}** using these rules:
{write_in_clear_english}
{dont_tell_me}"#,
        context = global_context(pub_date),
        article = article_text,
        topic = topic_prompt,
        write_in_clear_english = WRITE_IN_CLEAR_ENGLISH,
        dont_tell_me = DONT_TELL_ME
    )
}

/// Generate a prompt for assessing how an article affects specific places
pub fn how_does_it_affect_prompt(article_text: &str, affected_places: &str) -> String {
    format!(
        r#" 
## ARTICLE (FOR IMPACT ANALYSIS):
----------
{article}
----------

IMPORTANT INSTRUCTIONS:
* **Analyze ONLY the article above.**
* **Assess impact on: {places}**
* **For non-English content, include translations of relevant quotes.**

### **Impact Assessment Guidelines**
Determine the article's effect on life and safety in the specified locations:

**Impact Levels:**
* **Direct Impact:** Immediate or near-term effects on life/safety
* **Indirect Impact:** Secondary or longer-term effects
* **Potential Impact:** Possible future effects if conditions continue
* **No Impact:** No significant effect on life/safety

**Response Format:**
Provide exactly two sentences that:

**First Sentence MUST:**
* Begin with one of these EXACT phrases:
  - For direct impact: "This article directly affects..."
  - For indirect impact: "This article indirectly affects..."
  - For potential impact: "This article could affect..."
  - For no impact: "This article does not affect..."
* Specify which locations are affected
* Explain the nature of the impact
* Include specific evidence from the article

**Second Sentence MUST:**
* Provide supporting details about:
  - Severity of impact
  - Timeline of effects
  - Scope of affected population
  - Specific measures or responses
* Include relevant data or quotes
* Focus only on life and safety implications

**EXAMPLE (Direct Impact):**
"This article directly affects residents of coastal Florida through immediate evacuation orders affecting 50,000 people due to the approaching Category 4 hurricane. Emergency services have established 15 shelters across three counties, with mandatory evacuation orders in effect for all areas below 10 feet elevation."

**EXAMPLE (Indirect Impact):**
"This article indirectly affects communities in northern Mexico through potential water shortages resulting from the new dam project in Arizona, which will reduce Colorado River flow by 15%. The reduced water access could impact agricultural operations supporting 200,000 residents within the next two years."

**EXAMPLE (No Impact):**
"This article does not affect the specified regions as the described policy changes only apply to European Union member states. The regulatory updates discussed have no jurisdiction or practical effect on operations or safety measures in these locations."

Now analyze the impact on {places} using these rules:
{write_in_clear_english}
{dont_tell_me}
{format_instructions}"#,
        article = article_text,
        places = affected_places,
        write_in_clear_english = WRITE_IN_CLEAR_ENGLISH,
        dont_tell_me = DONT_TELL_ME,
        format_instructions = FORMAT_INSTRUCTIONS
    )
}

/// Generate a prompt for explaining why an article does not affect specific places
pub fn why_not_affect_prompt(article_text: &str, non_affected_places: &str) -> String {
    format!(
        r#" 
## ARTICLE (FOR NON-IMPACT ANALYSIS):
----------
{article}
----------

IMPORTANT INSTRUCTIONS:
* **Analyze ONLY the article above.**
* **Explain lack of impact on: {places}**
* **For non-English content, include translations of relevant quotes.**

### **Non-Impact Assessment Guidelines**
Explain why the article does not affect life and safety in the specified locations:

**Non-Impact Categories:**
* **Geographic Exclusion:** Events/effects limited to other regions
* **Jurisdictional Limitation:** Laws/policies don't apply to these areas
* **Scope Restriction:** Effects don't extend to these locations
* **Time Limitation:** Past events with no current impact
* **Domain Difference:** Subject matter doesn't affect these areas

**Response Format:**
Provide exactly two sentences that:

**First Sentence MUST:**
* Begin with one of these EXACT phrases:
  - For geographic exclusion: "This article's effects are limited to..."
  - For jurisdictional limitation: "The policies/laws discussed only apply to..."
  - For scope restriction: "The impact is contained within..."
  - For time limitation: "The described events occurred in..."
  - For domain difference: "The subject matter exclusively concerns..."
* Explain why the specified locations are unaffected
* Reference specific evidence from the article

**Second Sentence MUST:**
* Provide supporting details about:
  - Specific boundaries of impact
  - Relevant jurisdictions
  - Temporal limitations
  - Domain restrictions
* Include relevant data or quotes
* Confirm absence of indirect effects

**EXAMPLE (Geographic Exclusion):**
"This article's effects are limited to Southeast Asian markets, specifically the ASEAN member states implementing the new trade regulations. The described policy changes have no jurisdiction or practical impact on {places}, as they fall outside the specified trading bloc's boundaries."

**EXAMPLE (Jurisdictional Limitation):**
"The policies discussed only apply to European Union member states implementing the new digital privacy framework affecting 450 million EU residents. These regulations have no legal authority or practical effect in {places}, which operate under different jurisdictional frameworks."

**EXAMPLE (Domain Difference):**
"The subject matter exclusively concerns changes to Antarctic research station protocols affecting 200 scientists across 12 research bases. The operational changes at these remote facilities have no connection to or impact on daily life and safety in {places}."

Now explain why there is no impact on {places} using these rules:
{write_in_clear_english}
{dont_tell_me}
{format_instructions}"#,
        article = article_text,
        places = non_affected_places,
        write_in_clear_english = WRITE_IN_CLEAR_ENGLISH,
        dont_tell_me = DONT_TELL_ME,
        format_instructions = FORMAT_INSTRUCTIONS
    )
}

/// Generates a prompt for the model to extract impacted regions from an article, using parsed place data.
pub fn threat_locations(
    article: &str,
    places_hierarchy: &BTreeMap<String, BTreeMap<String, Vec<String>>>,
) -> String {
    let mut prompt = String::from("You are analyzing a news article to determine the geographical regions impacted by the events described. ");

    // Instructions for the model
    prompt.push_str(
        "Your task is to list the impacted regions in a hierarchical JSON format based on the provided structure. ",
    );
    prompt.push_str("For each impacted region, provide the continent, country, and region name. ");
    prompt.push_str("If a region is not mentioned in or directly impacted by the text of the article, do not include it in the output. ");
    prompt.push_str("The JSON format should be:\n\n");
    prompt.push_str("{\n  \"impacted_regions\": [\n    {\n      \"continent\": \"<continent_name>\",\n      \"country\": \"<country_name>\",\n      \"region\": \"<region_name>\"\n    },\n    ...\n  ]\n}\n\n");

    // Add the hierarchical data for reference
    prompt.push_str("Here is the list of geographical regions for reference:\n\n");
    for (continent, countries) in places_hierarchy {
        prompt.push_str(&format!("- {}\n", continent));
        for (country, regions) in countries {
            prompt.push_str(&format!("  - {}\n", country));
            for region in regions {
                prompt.push_str(&format!("    - {}\n", region));
            }
        }
    }

    prompt.push_str("\n---\n\n");
    prompt.push_str("Here is the article:\n\n");
    prompt.push_str(article);
    prompt.push_str("\n\n---\n\n");
    prompt.push_str(
        "Based on the article, extract the impacted regions using the hierarchical JSON format specified above.",
    );

    prompt
}
