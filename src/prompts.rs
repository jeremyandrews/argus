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
- In January 2025, Donald Trump was inaugurated as the 47th U.S. President and issued significant executive orders affecting trade and international relations. The month also recorded the warmest January globally, highlighting climate concerns. A ceasefire was reached in the Israel-Hamas conflict, and Canadian Prime Minister Justin Trudeau resigned amid a political crisis. Trump's actions included imposing tariffs on Mexico, China, and Canada, withdrawing the U.S. from the World Health Organization, and defunding the UN agency for Palestinian refugees, signaling a shift toward protectionism and unilateral foreign policy.
- In early February 2025, President Trump imposed significant tariffs on Canada, Mexico, and China, escalating global trade tensions. The U.S. conducted airstrikes against Islamic State positions in Somalia, signaling intensified counterterrorism efforts. The administration announced the shutdown of USAID, merging it into the State Department, indicating a major shift in foreign aid policy. Additionally, the U.S. declared it would assume control over the Gaza Strip in agreement with Israel, and reinstated a maximum pressure policy against Iran, both actions with significant geopolitical implications.
";

fn current_date() -> String {
    let today = Local::now();
    format!(
        "{} {}, {}",
        today.format("%B"),
        today.format("%-d"),
        today.format("%Y")
    )
}

fn global_context() -> String {
    format!(
        "
This is a small sampling of events since January of 2023 between ~~~ markers:
~~~
{}
~~~
Today's date is: {}\n",
        CONTEXT,
        current_date()
    )
}

pub fn summary_prompt(article_text: &str) -> String {
    format!(
        r#"
{context}

Below is the text of an article between ~~~ markers:
~~~
{article}
~~~

First, carefully read and thoroughly understand the entire text.

Then, create a comprehensive bullet-point summary that follows these STRICT rules:
1. Format: Use ONLY simple bullet points with a single dash (-)
2. Length:
   - Very short texts (≤25 words): Quote verbatim
   - Short texts (26-100 words): 2-3 bullets
   - Medium texts (101-500 words): 3-4 bullets
   - Long texts (501-2000 words): 4-6 bullets
   - Very long texts (>2000 words): 6-8 bullets

3. Each bullet point MUST:
   - Start with a dash (-)
   - Include specific data points (numbers, dates, percentages)
   - Contain multiple related facts in a single coherent sentence
   - Provide complete context for each point
   - Use active voice
   - Be substantial (25-40 words each)

4. DO NOT:
   - Use headings or sections
   - Include nested bullets
   - Include commentary or analysis

Format example using a generic topic:
- Introduces new environmental regulations affecting 15 major industries across 3 continents, requiring a 45% reduction in carbon emissions by 2025, while providing $12 billion in transition funding for affected companies.

Now summarize the article text above using these rules:
{write_in_clear_english}

{dont_tell_me}

{format_instructions}"#,
        context = global_context(),
        article = article_text,
        write_in_clear_english = WRITE_IN_CLEAR_ENGLISH,
        dont_tell_me = DONT_TELL_ME,
        format_instructions = FORMAT_INSTRUCTIONS
    )
}

pub fn tiny_summary_prompt(summary_response: &str) -> String {
    format!(
        "Below is the summary of an article between ~~~ markers:
~~~
{summary}
~~~

Create a single sentence summary of maximum 400 characters that captures the most essential information. Focus on the main event or finding only.

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
        r#"
{context}

Below is the text of an article between ~~~ markers:
~~~
{article}
~~~

Provide a concise critical analysis with these specific points:

### Credibility Score: [1-10]
   Briefly justify (max 15 words)

### Style Score: [1-10]
   Briefly justify (max 15 words)

### Political Leaning: [Far Left | Left | Center Left | Center | Center Right | Right | Far Right | N/A]
   Briefly justify (max 15 words)

### Tone: [Neutral | Positive | Negative | Alarmist | Optimistic | Skeptical | Other]
   Briefly justify (max 15 words)

### Target Audience: (max 10 words)

### Critical Analysis: (2-3 bullet points)

### Key Takeaway: (1-2 bullet points)

{write_in_clear_english}
{dont_tell_me}"#,
        context = global_context(),
        article = article_text,
        write_in_clear_english = WRITE_IN_CLEAR_ENGLISH,
        dont_tell_me = DONT_TELL_ME
    )
}

pub fn logical_fallacies_prompt(article_text: &str) -> String {
    format!(
        r#"
{context}

Below is the text of an article between ~~~ markers:
~~~
{article}
~~~

Analyze for logical fallacies and argument strength:

### Logical Fallacies Found:
   - Name: [fallacy type]
     Briefly explain (max 15 words)
   (List up to 3 most significant fallacies, or state "No apparent logical fallacies detected")

### Argument Strength: [1-10]
   Briefly justify (max 20 words)

### Evidence Quality: [1-10]
   Briefly justify (max 20 words)

### Overall Assessment: (1-2 bullet points)

{write_in_clear_english}
{dont_tell_me}"#,
        context = global_context(),
        article = article_text,
        write_in_clear_english = WRITE_IN_CLEAR_ENGLISH,
        dont_tell_me = DONT_TELL_ME
    )
}

pub fn source_analysis_prompt(article_html: &str, source_url: &str) -> String {
    format!(
        r#"
{context}

Below is the article text and source URL between ~~~ markers:
~~~
{article}
Source URL: {source_url}
~~~

Please analyze the WEBSITE DOMAIN ITSELF as a publication source, not the specific article content. Begin by extracting just the domain from the URL, followed by the publication date (or best estimate based on content, then provide factual details about the website and its operating organization in exactly four bullet points. Skip any bullet points for which you do not know the answer.

Example response format:
businessnews.com
Published: February 4, 2024

- The domain is owned and operated by Global Media Holdings Corporation, which was acquired by Berkshire Hathaway in 2019 for $2.3 billion, with a trust structure ensuring editorial independence.

- This business news website reaches approximately 12 million monthly readers globally, primarily serving financial professionals and business leaders from its headquarters in Toronto with 15 international bureaus.

- The publication has earned six Pulitzer Prizes for financial reporting, faced a notable libel lawsuit in 2021 that was settled out of court, and maintains an A+ rating from NewsGuard's transparency index.

- The site publishes approximately 200 stories daily across its digital platforms, operates on a subscription model with 800,000 paid subscribers, and maintains a 24/7 newsroom with peak activity during US market hours.

Now please provide a similar institutional analysis for the domain {source_url}, focusing on the WEBSITE/ORGANIZATION itself rather than any specific article content. Use only verified information and omit any bullet points where information is uncertain.

{write_in_clear_english}
{dont_tell_me}"#,
        article = article_html,
        source_url = source_url,
        context = global_context(),
        write_in_clear_english = WRITE_IN_CLEAR_ENGLISH,
        dont_tell_me = DONT_TELL_ME
    )
}

pub fn relation_to_topic_prompt(article_text: &str, topic_prompt: &str) -> String {
    format!(
        r#"
{context}

Below is the text of an article between ~~~ markers:
~~~
{article}
~~~

Explain in exactly two sentences how this article relates to {topic}. 
First sentence must begin with "This article relates to {topic} because..."

{write_in_clear_english}
{dont_tell_me}"#,
        context = global_context(),
        article = article_text,
        topic = topic_prompt,
        write_in_clear_english = WRITE_IN_CLEAR_ENGLISH,
        dont_tell_me = DONT_TELL_ME
    )
}

pub fn how_does_it_affect_prompt(article_text: &str, affected_places: &str) -> String {
    format!(
        "
Article text:        
```
{article}
```
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
        "
Article text:        
```
{article}
```

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
1. First, identify the article's language. If not in English:
   - Look for topic-relevant terms in that language (e.g., "Toscana" for "Tuscany")
   - Consider regional variations and local terminology
2. Carefully read the article summary above.
3. Compare the main focus of the article to the topic: {topic}
4. Answer ONLY 'Yes' or 'No' based on the following criteria:
   - Answer 'Yes' if the article is specifically about {topic} AND contains enough content for analysis,
     regardless of the original language
   - Answer 'No' if the article is not primarily about {topic}, only mentions it briefly, or is unrelated
5. Do not explain your reasoning - provide only a one-word answer: 'Yes' or 'No'.
Answer:"#,
        article = article_text,
        topic = topic_name
    )
}

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
   - Answer 'Yes' ONLY if ALL of these are true:
     a) Is valid article content (not an error/loading message)
     b) The article is specifically about {topic} (in any language)
     c) Contains enough content for analysis
     d) Is not primarily a promotion or advertisement
   - Answer 'No' if ANY of these are true:
     a) Contains error messages or technical issues
     b) Is not complete article content
     c) The article is not primarily about {topic}
     d) Only mentions {topic} briefly
     e) Is unrelated to {topic}
     f) Is primarily a promotion or advertisement

4. Do not explain your reasoning - provide only a one-word answer: 'Yes' or 'No'.
Answer:"#,
        summary = summary_response,
        topic = topic_name
    )
}

pub fn confirm_threat_prompt(article_text: &str) -> String {
    format!(
        r#"{article}
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
