use crate::prompt::common::global_context;

/// Generate a prompt for extracting named entities from an article text to improve content matching
pub fn entity_extraction_prompt(article_text: &str, pub_date: Option<&str>) -> String {
    format!(
        r#"
{context}

ARTICLE TEXT (FOR ENTITY EXTRACTION):
----------
{article}
----------

TASK: Extract key entities from this article to enable accurate content matching and event tracking.

ENTITY EXTRACTION GUIDELINES:
1. Identify all important entities in the following categories:
   - PERSON: Individual people (politicians, executives, scientists, etc.)
   - ORGANIZATION: Companies, institutions, governments, agencies, etc.
   - LOCATION: Countries, cities, regions, landmarks, etc.
   - EVENT: Specific happenings including:
     * Political events (elections, inaugurations, summits)
     * Disasters/incidents (earthquakes, crashes, outages)
     * Corporate actions (product launches, mergers, announcements)
     * Planned occasions (conferences, ceremonies, celebrations)
     * Recurring events (annual meetings, quarterly earnings)
     * Military/security events (operations, attacks, deployments)
     * Scientific milestones (discoveries, experiments, breakthroughs)
   - PRODUCT: Products, services, technologies, etc.
   - OTHER: Any other significant entities not fitting above categories

2. For each entity:
   - Extract the canonical name as it appears in the article
   - Include a normalized version (lowercase, standardized format) 
   - Designate importance as one of: 
     * PRIMARY (central to the article)
     * SECONDARY (important but not central)
     * MENTIONED (mentioned but not focused on)

3. For EVENT entities, and ONLY for event entities:
   - Include a start_date if mentioned in ISO format (YYYY-MM-DD)
   - For ongoing events, include both start_date and end_date if available

4. Also extract an "event_date" field at the top level if the article refers to:
   - When a specific event occurred or will occur
   - A significant date mentioned in relation to the main topic
   - Format as ISO date string (YYYY-MM-DD), or empty string if no clear date

RETURN FORMAT (JSON):
{{
  "event_date": "YYYY-MM-DD", // Optional top-level date relating to main event
  "entities": [
    {{
      "name": "Exact name as it appears in text",
      "normalized_name": "standardized, lowercase version", 
      "type": "PERSON|ORGANIZATION|LOCATION|EVENT|PRODUCT|OTHER",
      "importance": "PRIMARY|SECONDARY|MENTIONED"
    }},
    ... additional entities ...
  ]
}}

RULES:
- Return a properly formatted JSON object
- Include 5-20 entities depending on article length
- Focus on entities directly mentioned in the article
- For people, include full names when available
- For organizations, use the most complete form mentioned
- For locations, include parent regions when relevant (city, state/province, country)
- For events, prioritize specificity (e.g., "2025 Paris Climate Conference" not just "conference")
- Do not include the same entity multiple times (use the most comprehensive mention)
- Rank importance accurately based on entity's role in the article

IMPORTANT RULES FOR EVENTS:
- When an article describes an action (launched, announced, released, introduced, etc.), 
  especially by a company or organization, you should ALWAYS create an EVENT entity
  that captures this action
  
- Format these action-oriented events as: "[Actor] [Action] [Object]"
  Examples: "Apple iPhone 15 Launch", "NASA Mars Mission Announcement", "Amazon Kuiper Satellite Deployment"
  
- Even for routine business activities like product launches, these should be classified as EVENTS,
  not just as mentions of the PRODUCT or ORGANIZATION

VALIDATION CHECK:
- If the article begins with words like "announced", "launched", "released", "introduced", "deployed", etc.,
  or contains phrases indicating something happened (e.g., "Amazon launched satellites"),
  make sure you have included at least one EVENT entity that captures this occurrence

EXAMPLES:

Example 1 - Tech News:
{{
  "event_date": "2024-05-15",
  "entities": [
    {{
      "name": "Apple Inc.",
      "normalized_name": "apple",
      "type": "ORGANIZATION",
      "importance": "PRIMARY"
    }},
    {{
      "name": "iPhone 16",
      "normalized_name": "iphone 16",
      "type": "PRODUCT",
      "importance": "PRIMARY"
    }},
    {{
      "name": "Tim Cook",
      "normalized_name": "tim cook",
      "type": "PERSON",
      "importance": "SECONDARY"
    }},
    {{
      "name": "WWDC 2024",
      "normalized_name": "wwdc 2024",
      "type": "EVENT",
      "importance": "SECONDARY"
    }},
    {{
      "name": "Cupertino",
      "normalized_name": "cupertino",
      "type": "LOCATION",
      "importance": "MENTIONED"
    }}
  ]
}}

Example 2 - Political News:
{{
  "event_date": "2025-01-20",
  "entities": [
    {{
      "name": "United States Presidential Inauguration",
      "normalized_name": "us presidential inauguration",
      "type": "EVENT",
      "importance": "PRIMARY"
    }},
    {{
      "name": "Donald Trump",
      "normalized_name": "donald trump",
      "type": "PERSON",
      "importance": "PRIMARY"
    }},
    {{
      "name": "Washington D.C.",
      "normalized_name": "washington dc",
      "type": "LOCATION",
      "importance": "SECONDARY"
    }},
    {{
      "name": "Republican Party",
      "normalized_name": "republican party",
      "type": "ORGANIZATION",
      "importance": "SECONDARY"
    }}
  ]
}}

Now, extract entities from the provided article:
"#,
        context = global_context(pub_date),
        article = article_text
    )
}
