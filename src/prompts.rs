use chrono::Local;
use std::collections::BTreeMap;

const DONT_TELL_ME: &str = r#"
Important instructions for your responses:

1. Do not narrate or describe your actions.
2. Do not explain or mention that you're writing in American English.
3. Do not summarize or restate the instructions I've given you.
4. Do not preface your responses with phrases like "Here's a summary..." or "I will now..."
5. Do not acknowledge or confirm that you understand these instructions.
6. Simply proceed with the task or answer directly, without any meta-commentary.
7. If asked a question, answer it directly without restating the question.
8. Avoid phrases like "As an AI language model..." or similar self-referential statements.

Your responses should appear as if they're coming from a knowledgeable human expert who naturally follows these guidelines without needing to mention them.
"#;

const FORMAT_INSTRUCTIONS: &str = r#"
To ensure our conversation is easy to follow and understand, use the following Markdown formatting options when they enhance readability:

### Headings
Use headings to organize content hierarchically:
# H1 for main titles
## H2 for subtitles
### H3 for section headers

### Emphasis and Special Words
- Use **bold** text for important information, like **key takeaways** or **main points**.
- Use _italic_ formatting for special terms, like _technical jargon_ or _foreign words_.

### Quotes and Block Quotes
- For short, inline quotes, use quotation marks: "This is a short quote."
- For larger quotes or to set apart text, use block quotes on new lines:

> This is an example of a block quote.
> It can span multiple lines and is useful for
> quoting articles or emphasizing larger sections of text.

### Code and Technical Terms
- Use `inline code` formatting for short code snippets, commands, or technical terms.
- For larger code blocks, use triple backticks with an optional language specifier:

```python
def hello_world():
    print("Hello, World!")
```

### Lists
Use ordered (numbered) or unordered (bullet) lists as appropriate:

1. First item
2. Second item
3. Third item

- Bullet point one
- Bullet point two
- Bullet point three

### Links and Images
- Create links like this: [Link Text](URL)
- Insert images like this: ![Alt Text](Image URL)

### Horizontal Rule
Use three dashes to create a horizontal line for separating content:

---

### Tables
Use tables for organizing data:

| Header 1 | Header 2 |
|----------|----------|
| Cell 1   | Cell 2   |

### General Guidelines
- Use formatting to enhance readability, not for decoration.
- Avoid excessive formatting, as it can make the text harder to understand.
- Always start a new line for block elements like quotes, code blocks, and lists.
- Use appropriate spacing between elements for clarity.

By following these guidelines, you'll create clear, concise, and engaging text that is easy to read and understand.
"#;

const WRITE_IN_CLEAR_ENGLISH: &str = r#"
Regardless of the source language of the article or content being discussed:
1. Write all responses in clear, accessible American English.
2. Use standard American spelling and grammar conventions.
3. Translate any non-English terms, phrases, or quotes into American English.
4. If a non-English term is crucial and doesn't have a direct English equivalent, provide the original term followed by an explanation in parentheses.
5. Aim for a writing style that is easily understood by a general American audience.
6. Avoid idioms or cultural references that may not be familiar to all English speakers.
7. When discussing measurements, provide both metric and imperial units where applicable.

Your goal is to ensure that the output is consistently in American English and easily comprehensible to American English speakers, regardless of the original language of the source material.
"#;

const CONTEXT: &str = "
In Q1 2024, BRICS expanded, shifting global economic power, while record temperatures highlighted climate concerns. Japan's 7.6 earthquake and U.S. winter storms exposed vulnerabilities. France enshrined abortion rights, Sweden joined NATO, and the U.S. Supreme Court ruled on key legal precedents. Major wildfires and geopolitical tensions added to global challenges.
In Q2 2024, a solar eclipse captivated North America as record heatwaves and severe floods underscored climate urgency. Trump’s trial and free speech protests stirred U.S. discourse. Putin’s fifth term, Xi's European visit, and G7's $50B Ukraine aid shaped geopolitics. Apple’s AI integration marked tech innovation.
In Q3 2024, the Paris Olympics fostered unity amidst record-breaking heatwaves and escalating Gaza tensions. Biden withdrew from the presidential race, endorsing Kamala Harris. The UN's 'Pact for the Future' and a historic face transplant marked milestones. Hurricane Helene and mpox emphasized urgent global challenges.
In Q4 2024, Trump’s re-election and U.S. economic growth highlighted domestic shifts. Hurricane Helene devastated the Gulf Coast, while 2024 set a record as the hottest year. South Korea’s political turmoil and Assad’s overthrow reshaped global dynamics. The Notre-Dame reopening symbolized cultural resilience.
- In January 2025, Donald Trump is inaugurated as the 47th U.S. President, signaling a major political shift. Los Angeles faces its most destructive wildfires, causing significant damage and loss of life. A European report confirms 2024 as the hottest year on record, emphasizing climate change urgency. Ukraine halts Russian gas transit, affecting European energy dynamics. Canadian Prime Minister Justin Trudeau announces his resignation, indicating impending leadership changes.";

pub fn summary_prompt(article_text: &str) -> String {
    format!(
        r#"{article}

Instructions for summarizing the provided text:

1. Carefully read and thoroughly understand the entire text.
2. Create a comprehensive summary in bullet points that:
   - Covers all main ideas and key points from the entire text
   - Maintains the original text's structure and flow
   - Uses clear, concise, and objective language
   - Avoids introducing new information or personal interpretation

3. Adjust the number of bullet points based on the text length:
   - For very short texts (up to 25 words): Simply quote the text verbatim
   - For short texts (26-100 words): Use 2-4 bullet points
   - For medium-length texts (101-500 words): Use 3-5 bullet points
   - For long texts (501-2000 words): Use 4-8 bullet points
   - For very long texts (over 2000 words): Use 6-10 bullet points

4. Ensure each bullet point is self-contained and meaningful on its own.
5. Use sub-bullets if necessary to organize related ideas under a main point.
6. Include any crucial statistics, dates, or figures mentioned in the text.
7. If the text contains distinct sections, reflect this structure in your summary.
8. For scientific or technical texts, maintain the precise terminology used in the original.

Remember: The goal is to provide a clear, accurate, and concise representation of the original text that allows readers to quickly grasp its essential content.

{write_in_clear_english}

{dont_tell_me}

{format_instructions}"#,
        article = article_text,
        write_in_clear_english = WRITE_IN_CLEAR_ENGLISH,
        dont_tell_me = DONT_TELL_ME,
        format_instructions = FORMAT_INSTRUCTIONS
    )
}

pub fn tiny_summary_prompt(summary_response: &str) -> String {
    format!(
        "{summary} | Please summarize down to 400 characters or less.

{write_in_clear_english}

{dont_tell_me}",
        summary = summary_response,
        write_in_clear_english = WRITE_IN_CLEAR_ENGLISH,
        dont_tell_me = DONT_TELL_ME
    )
}

pub fn tiny_title_prompt(summary_response: &str) -> String {
    format!(
        "{summary} | Please write an informational and accurate 3 to 5 word title for this text.

{write_in_clear_english}

{dont_tell_me}",
        summary = summary_response,
        write_in_clear_english = WRITE_IN_CLEAR_ENGLISH,
        dont_tell_me = DONT_TELL_ME
    )
}

pub fn critical_analysis_prompt(article_text: &str) -> String {
    format!(
        r#"{article}

Provide a critical analysis of the text above, addressing the following points:

1. Credibility Score:
   - Score: [1-10]
   - Explanation: (max 15 words)
   - Criteria: 1 = highly biased, 10 = unbiased and fact-based
   - Consider: sources cited, factual accuracy, balance of perspectives

2. Style Score:
   - Score: [1-10]
   - Explanation: (max 15 words)
   - Criteria: 1 = poorly written, 10 = eloquent and engaging
   - Consider: clarity, coherence, appropriate language, effective structure

3. Political Leaning:
   - Category: [Far Left | Left | Center Left | Center | Center Right | Right | Far Right | Not Applicable]
   - Explanation: (max 15 words)
   - Consider: language used, sources cited, topics emphasized, overall narrative

4. Tone:
   - Category: [Neutral | Positive | Negative | Alarmist | Optimistic | Skeptical | Other (specify)]
   - Explanation: (max 15 words)

5. Target Audience:
   - Identify the likely intended audience (max 10 words)

6. Critical Analysis:
   - Provide a concise 2-3 sentence critical analysis
   - Address strengths, weaknesses, potential biases, and overall effectiveness

7. Key Takeaway:
   - Summarize the most important point or insight in one sentence

{write_in_clear_english}
{dont_tell_me}
{format_instructions}"#,
        article = article_text,
        write_in_clear_english = WRITE_IN_CLEAR_ENGLISH,
        dont_tell_me = DONT_TELL_ME,
        format_instructions = FORMAT_INSTRUCTIONS
    )
}

pub fn logical_fallacies_prompt(article_text: &str) -> String {
    format!(
        r#"{article}

Analyze the provided text for logical fallacies and argument strength:

1. Logical Fallacies:
   - Identify any logical fallacies or biases present in the text.
   - For each fallacy found:
     a) Name the fallacy
     b) Provide a brief explanation (max 15 words)
     c) Quote or paraphrase the relevant part of the text
   - If no fallacies are found, state: "No apparent logical fallacies detected."

2. Argument Strength:
   - Evaluate the overall strength of arguments on a scale of 1-10
     (1 = very weak, 10 = very strong)
   - Provide a brief explanation for the score (max 20 words)
   - Consider:
     a) Quality and relevance of evidence presented
     b) Logical consistency of arguments
     c) Consideration of counterarguments
     d) Use of credible sources (if any)

3. Evidence Quality:
   - Rate the quality of evidence on a scale of 1-10
     (1 = poor/no evidence, 10 = excellent evidence)
   - Provide a brief explanation for the score (max 20 words)
   - Consider:
     a) Relevance to the main arguments
     b) Credibility of sources
     c) Comprehensiveness of data presented

4. Overall Assessment:
   - Summarize the logical strength and evidence quality in 1-2 sentences.
   - Highlight any particularly strong or weak aspects of the argumentation.

{write_in_clear_english}
{dont_tell_me}
{format_instructions}"#,
        article = article_text,
        write_in_clear_english = WRITE_IN_CLEAR_ENGLISH,
        dont_tell_me = DONT_TELL_ME,
        format_instructions = FORMAT_INSTRUCTIONS
    )
}

pub fn relation_to_topic_prompt(article_text: &str, topic_prompt: &str) -> String {
    format!( "{article} |
Briefly explain in one or two sentences how this relates to {topic}, starting with 'This relates to {topic}.'.

{write_in_clear_english}

{dont_tell_me}

{format_instructions}",
        article = article_text,
        topic = topic_prompt,
        write_in_clear_english = WRITE_IN_CLEAR_ENGLISH,
        dont_tell_me = DONT_TELL_ME,
        format_instructions = FORMAT_INSTRUCTIONS
    )
}

pub fn how_does_it_affect_prompt(article_text: &str, affected_places: &str) -> String {
    format!(
        "{article} |
How does this article affect the life and safety of people in the following places: {places}?
Answer in no more than two sentences.

{write_in_clear_english}

{dont_tell_me}

{format_instructions}",
        article = article_text,
        places = affected_places,
        write_in_clear_english = WRITE_IN_CLEAR_ENGLISH,
        dont_tell_me = DONT_TELL_ME,
        format_instructions = FORMAT_INSTRUCTIONS
    )
}

pub fn why_not_affect_prompt(article_text: &str, non_affected_places: &str) -> String {
    format!(
        "{article} |
Why does this article not affect the life and safety of people in the following places:
{places}? Answer in no more than two sentences.

{write_in_clear_english}

{dont_tell_me}

{format_instructions}",
        article = article_text,
        places = non_affected_places,
        write_in_clear_english = WRITE_IN_CLEAR_ENGLISH,
        dont_tell_me = DONT_TELL_ME,
        format_instructions = FORMAT_INSTRUCTIONS
    )
}

pub fn source_analysis_prompt(article_html: &str, source_url: &str) -> String {
    let today = Local::now();
    let day = today.format("%-d").to_string();
    let month = today.format("%B").to_string();
    let year = today.format("%Y").to_string();

    format!(
        r#"Context: A small sampling of events since your knowledge cutoff in 2022: `{context}`

Article to review: `{article}`
Source URL: `{source_url}`
Current date: {month} {day}, {year}

Provide a comprehensive source analysis addressing the following points:

1. Source Background:
   - Ownership and affiliation
   - Purpose and target audience
   - Founding date and brief history

2. Credibility Assessment:
   - Notable achievements or awards
   - Controversies or scandals
   - Overall reputation in the media landscape

3. Content Analysis:
   - Primary focus areas or topics covered
   - Political leaning or ideological stance, if any
   - Quality of reporting and fact-checking practices

4. Timeliness:
   - Assess whether the content appears recent or potentially outdated
   - Identify any time-sensitive information or references

5. Comparison:
   - Briefly compare to similar sources in the same niche
   - Highlight any unique features or approaches

6. Overall Evaluation:
   - Summarize the source's strengths and weaknesses
   - Provide a general recommendation for readers (e.g., reliable for certain topics, approach with caution, etc.)

Please provide your analysis in 4-6 concise sentences, focusing on the most relevant and insightful points from the above categories.

{write_in_clear_english}
{dont_tell_me}
{format_instructions}"#,
        context = CONTEXT,
        article = article_html,
        source_url = source_url,
        day = day,
        month = month,
        year = year,
        write_in_clear_english = WRITE_IN_CLEAR_ENGLISH,
        dont_tell_me = DONT_TELL_ME,
        format_instructions = FORMAT_INSTRUCTIONS
    )
}

/// Generates a prompt for the model to extract impacted regions from an article, using parsed place data.
///
/// - `article`: The text of the article to analyze.
/// - `places_hierarchy`: A hierarchical map of continents, countries, and regions derived from places.json.
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

/** The following prompts expect a 'yes' or 'no' answer. */

pub fn threat_prompt(article_text: &str) -> String {
    format!(
        "{article} |
Is this article describing an **ongoing** or **imminent** event or situation that might pose
a threat to human life or health? Answer ONLY 'yes' or 'no'.",
        article = article_text
    )
}

pub fn region_threat_prompt(
    article_text: &str,
    region: &str,
    country: &str,
    continent: &str,
) -> String {
    format!(
        "{article} |
This article mentions that people in {region}, {country}, {continent} may be affected by an ongoing or imminent life-threatening event. 
Please confirm if the article is indeed about such an event in this region. Answer yes or no, and explain briefly why.",
        article = article_text,
        region = region,
        country = country,
        continent = continent
    )
}

pub fn city_threat_prompt(
    article_text: &str,
    city_name: &str,
    region: &str,
    country: &str,
    continent: &str,
) -> String {
    format!(
        "{article} |
This article mentions that people in or near {city}, {region}, {country}, {continent} may be affected by an ongoing or imminent life-threatening event. 
Please confirm if the article is indeed about such an event in this city. Answer yes or no, and explain briefly why.",
        article = article_text,
        city = city_name,
        region = region,
        country = country,
        continent = continent
    )
}

pub fn is_this_about(article_text: &str, topic_name: &str) -> String {
    format!(
        r#"{article}

Question: Does this article primarily focus on and provide substantial information about {topic}?

Instructions:
1. Carefully read the article summary above.
2. Compare the main focus of the article to the topic: {topic}
3. Answer ONLY 'Yes' or 'No' based on the following criteria:
   - Answer 'Yes' if the article is specifically about {topic} AND contains enough content for analysis.
   - Answer 'No' if the article is not primarily about {topic}, only mentions it briefly, or is unrelated.
4. Do not explain your reasoning - provide only a one-word answer: 'Yes' or 'No'.

Answer:"#,
        article = article_text,
        topic = topic_name
    )
}

pub fn confirm_prompt(summary_response: &str, topic_name: &str) -> String {
    format!(
        r#"{summary}

Question: Confirm if this article is specifically about {topic} and not a promotion or advertisement.

Instructions:
1. Carefully re-read the article summary above.
2. Compare the main focus of the article to the topic: {topic}
3. Check if the article provides substantial, analytical content about {topic}.
4. Verify that the article is not primarily promotional or advertorial.
5. Answer ONLY 'Yes' or 'No' based on the following criteria:
   - Answer 'Yes' ONLY if ALL of these are true:
     a) The article is specifically about {topic}
     b) It contains enough content for analysis
     c) It is not primarily a promotion or advertisement
   - Answer 'No' if ANY of these are true:
     a) The article is not primarily about {topic}
     b) It only mentions {topic} briefly
     c) It is unrelated to {topic}
     d) It is primarily a promotion or advertisement
     e) It is an error message, not an article
6. Do not explain your reasoning - provide only a one-word answer: 'Yes' or 'No'.

Answer:"#,
        summary = summary_response,
        topic = topic_name
    )
}
