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

fn global_context(pub_date: Option<&str>) -> String {
    let publication_date = match pub_date {
        Some(date) => format!("Publication date: {}", date),
        None => String::new(),
    };

    format!(
        r#"
GLOBAL CONTEXT (FOR REFERENCE ONLY):
=============================
This section provides background information on significant global events from January 2023 to the present. 
**IMPORTANT:** This context is for reference ONLY. **DO NOT summarize, analyze, or reference it unless the article explicitly mentions related events.**

~~~
{context}
~~~

{publication_date}
Today's date: {date}
=============================
"#,
        context = CONTEXT,
        publication_date = publication_date,
        date = current_date()
    )
}

pub fn summary_prompt(article_text: &str, pub_date: Option<&str>) -> String {
    format!(
        r#"
{context}

ARTICLE (TO BE SUMMARIZED):
-----------------------------
{article}
-----------------------------

IMPORTANT INSTRUCTIONS:
- **Summarize ONLY the article above.** 
- **IGNORE the global context unless the article explicitly mentions related events.**
- **Do NOT reference or include information from the global context unless it is directly relevant to the article content.**

First, carefully read and thoroughly understand the entire text.

Then, create a comprehensive bullet-point summary that follows these STRICT rules:

1. **Format:** Use ONLY simple bullet points starting with a dash (-).
2. **Length:**
   - Very short texts (≤25 words): Quote verbatim.
   - Short texts (26–100 words): 2–3 bullets.
   - Medium texts (101–500 words): 3–4 bullets.
   - Long texts (501–2000 words): 4–6 bullets.
   - Very long texts (>2000 words): 6–8 bullets.

3. **Each Bullet Point MUST:**
   - Start with a dash (-).
   - Include specific data points (numbers, dates, percentages).
   - Contain multiple related facts in a single coherent sentence.
   - Provide complete context for each point.
   - Use active voice.
   - Be substantial (15–35 words each).

4. **DO NOT:**
   - Use headings or sections.
   - Include nested bullets.
   - Include commentary or analysis.
   - Summarize the global context instead of the article.

**EXAMPLE (Correct):**
- Introduces new environmental regulations affecting 15 major industries across 3 continents, requiring a 45% reduction in carbon emissions by 2025, while providing $12 billion in transition funding for affected companies.

**EXAMPLE (Incorrect):**
- Summarizes unrelated global events mentioned in the context above.

Now summarize the article text above using these rules:

{write_in_clear_english}

{dont_tell_me}

{format_instructions}"#,
        context = global_context(pub_date),
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

pub fn critical_analysis_prompt(article_text: &str, pub_date: Option<&str>) -> String {
    format!(
        r#"
{context}

ARTICLE (FOR CRITICAL ANALYSIS):
-----------------------------
{article}
-----------------------------

IMPORTANT INSTRUCTIONS:
- **Analyze ONLY the article above.** 
- **IGNORE the global context unless the article explicitly mentions related events.**
- **Do NOT reference or include information from the global context unless it is directly relevant to the article content.**

TASK:
Carefully read and understand the entire article.

Then, provide a concise critical analysis following these STRICT guidelines:

### **Credibility Score:** [1–10]
   - Briefly explain in no more than 15 words.

### **Style Score:** [1–10]
   - Briefly explain in no more than 15 words.

### **Political Leaning:** [Far Left | Left | Center Left | Center | Center Right | Right | Far Right | N/A]
   - Briefly explain in no more than 15 words.

### **Tone:** [Neutral | Positive | Negative | Alarmist | Optimistic | Skeptical | Other]
   - Briefly explain in no more than 15 words.

### **Target Audience:** 
   - Identify the intended audience in no more than 10 words.

### **Critical Analysis:** 
   - Provide 2–3 bullet points focusing on content, logic, and evidence quality.
   - Each bullet should highlight key observations about the article’s arguments, strengths, or weaknesses.

### **Key Takeaway:** 
   - Provide 1–2 bullet points summarizing the article’s most significant points or conclusions.

**EXAMPLE (Correct):**
- Highlights biased language favoring one political perspective despite factual accuracy, with inconsistent source citations affecting credibility.

**EXAMPLE (Incorrect):**
- Focuses on unrelated global events instead of analyzing the article content.

Now perform the critical analysis using these rules:

{write_in_clear_english}

{dont_tell_me}"#,
        context = global_context(pub_date),
        article = article_text,
        write_in_clear_english = WRITE_IN_CLEAR_ENGLISH,
        dont_tell_me = DONT_TELL_ME
    )
}

pub fn logical_fallacies_prompt(article_text: &str, pub_date: Option<&str>) -> String {
    format!(
        r#"
{context}

ARTICLE (FOR LOGICAL FALLACY ANALYSIS):
-----------------------------
{article}
-----------------------------

IMPORTANT INSTRUCTIONS:
- **Analyze ONLY the article above.** 
- **IGNORE the global context unless the article explicitly mentions related events.**
- **Do NOT reference or include information from the global context unless it is directly relevant to the article content.**

TASK:
Carefully read and understand the entire article.

Then, analyze for logical fallacies and argument strength following these STRICT guidelines:

### **Logical Fallacies Found:**
- **Format:** 
  #. [fallacy type]  
    Briefly explain in no more than 15 words.  
- **Instructions:** List up to 4 of the most significant fallacies found, or state:
  - *"No apparent logical fallacies detected."*

### **Argument Strength:** [1–10]
- **Justification:** Briefly explain in no more than 20 words.

### **Evidence Quality:** [1–10]
- **Justification:** Briefly explain in no more than 20 words.

### **Overall Assessment:** 
- Provide 1–2 bullet points summarizing key observations about the article’s reasoning and logical consistency.

**EXAMPLE (Correct):**
1. Strawman Fallacy  
  Misrepresents opposing argument to make it easier to refute.  

2. Appeal to Feat  
  Uses fear of actions to push for result.  

- **Argument Strength:** 6  
  Uses some evidence but relies heavily on assumptions without support.  

- **Evidence Quality:** 4  
  Relies on anecdotal evidence with no verifiable data.  

- **Overall Assessment:**  
  - Relies on emotional appeals over factual evidence, undermining the argument's logical foundation.

**EXAMPLE (Incorrect):**
- Focuses on summarizing unrelated global events or providing subjective opinions without evaluating logical structure.

Now perform the logical fallacy analysis using these rules:

{write_in_clear_english}

{dont_tell_me}"#,
        context = global_context(pub_date),
        article = article_text,
        write_in_clear_english = WRITE_IN_CLEAR_ENGLISH,
        dont_tell_me = DONT_TELL_ME
    )
}

pub fn source_analysis_prompt(
    article_html: &str,
    source_url: &str,
    pub_date: Option<&str>,
) -> String {
    format!(
        r#"
{context}

ARTICLE AND SOURCE URL (FOR SOURCE ANALYSIS):
-----------------------------
{article}
Source URL: {source_url}
-----------------------------

IMPORTANT INSTRUCTIONS:
- **Analyze ONLY the WEBSITE DOMAIN ITSELF, not the specific article content.** 
- **IGNORE the global context unless the article explicitly mentions related events.**
- **Focus on the publication source, its ownership, reputation, and background.**

TASK:
Carefully extract the domain from the URL, then analyze the publication source following these STRICT guidelines:

### **Domain Name:** 
- Extract and list the domain (e.g., `example.com`).

### **Publication Date:** 
- Identify the publication date of the article, or provide the best estimate based on content.

### **Institutional Analysis:** 
- Provide up to **five bullet points** about the WEBSITE/ORGANIZATION, focusing on the following (skip any bullet if the information is unavailable):
  - **Ownership and Management:** Who owns and operates the website? Include corporate affiliations if known.
  - **Audience and Reach:** Describe the website's target audience, monthly readership, or geographical influence.
  - **Reputation and History:** Mention any notable awards, controversies, or credibility ratings from reliable sources.
  - **Publishing Practices:** Frequency of publication, editorial policies, or any relevant operational details.
  - **Comparison:** Compare the publication to other similar sources.

**EXAMPLE (Correct):**

businessnews.com Published: February 4, 2024

    The domain is owned by Global Media Holdings, acquired by Berkshire Hathaway in 2019, maintaining editorial independence through a trust structure.
    Reaches 12 million monthly readers, primarily financial professionals globally, with headquarters in Toronto and 15 international bureaus.
    Earned six Pulitzer Prizes for financial reporting, faced a libel lawsuit in 2021, and holds an A+ NewsGuard rating.
    Publishes ~200 stories daily, operates on a subscription model with 800,000 paid subscribers, and maintains a 24/7 newsroom.


**EXAMPLE (Incorrect):**
- Focuses on summarizing article content instead of analyzing the publication source.

Now analyze the publication source using these rules:

{write_in_clear_english}

{dont_tell_me}"#,
        article = article_html,
        source_url = source_url,
        context = global_context(pub_date),
        write_in_clear_english = WRITE_IN_CLEAR_ENGLISH,
        dont_tell_me = DONT_TELL_ME
    )
}

pub fn relation_to_topic_prompt(
    article_text: &str,
    topic_prompt: &str,
    pub_date: Option<&str>,
) -> String {
    format!(
        r#"
{context}

ARTICLE (FOR RELATION TO TOPIC ANALYSIS):
-----------------------------
{article}
-----------------------------

IMPORTANT INSTRUCTIONS:
- **Analyze ONLY the article above to determine its relation to the topic.** 
- **IGNORE the global context unless the article explicitly mentions related events.**
- **Do NOT reference or include information from the global context unless it is directly relevant to the article content.**

TASK:
Carefully read and understand the entire article.

Then, explain in exactly **two sentences** how this article relates to **{topic}** following these STRICT guidelines:

1. **Sentence 1 MUST begin with:**  
   - "This article relates to {topic} because..."  
   - Clearly explain the direct connection between the article and the topic.

2. **Sentence 2 SHOULD:**  
   - Add any relevant details that further clarify the relationship, such as specific examples, events, or data from the article.

**EXAMPLE (Correct):**
- "This article relates to climate change because it discusses rising sea levels caused by global warming in coastal cities. It highlights how recent floods in Miami are linked to increasing ocean temperatures and polar ice melt."

**EXAMPLE (Incorrect):**
- "This article is about politics, which is often related to climate change." (Too vague, lacks specific connection)

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

pub fn sources_quality_prompt(critical_analysis: &str) -> String {
    format!(
        r#"Based on this critical analysis:
~~~
{analysis}
~~~

Return a single number (1, 2, or 3) representing the overall quality of the source:
1 = Poor (red) - Major credibility issues, unreliable source, or significant bias
2 = Moderate (yellow) - Some concerns but generally acceptable
3 = Excellent (green) - Highly credible, reliable source with minimal bias

Base your assessment primarily on:
- Credibility Score
- Source reputation and reliability
- Use of reliable sources
- Professional standards
- Editorial oversight

Return ONLY the number 1, 2, or 3.
{dont_tell_me}"#,
        analysis = critical_analysis,
        dont_tell_me = DONT_TELL_ME
    )
}

pub fn argument_quality_prompt(logical_fallacies: &str) -> String {
    format!(
        r#"Based on this logical fallacy analysis:
~~~
{analysis}
~~~

Return a single number (1, 2, or 3) representing the overall quality of the arguments:
1 = Poor (red) - Multiple serious fallacies or very weak arguments
2 = Moderate (yellow) - Some fallacies but generally sound reasoning
3 = Excellent (green) - Strong arguments with minimal or no fallacies

Base your assessment primarily on:
- Number and severity of logical fallacies
- Argument strength score
- Evidence quality score
- Overall logical consistency

Return ONLY the number 1, 2, or 3.
{dont_tell_me}"#,
        analysis = logical_fallacies,
        dont_tell_me = DONT_TELL_ME
    )
}

pub fn source_type_prompt(source_analysis: &str, article_url: &str) -> String {
    format!(
        r#"Based on this source analysis and URL:
~~~
{analysis}
URL: {url}
~~~

Return a single word describing the source type if it matches one of these categories, otherwise return "none":

"official" - Government websites (.gov), recognized international organizations, or primary sources for their domain (e.g., drupal.org for Drupal news)
"academic" - University or research institution websites (.edu, established research centers)
"questionable" - Known disinformation sources, sites with severe credibility issues, or extreme bias
"corporate" - Official company websites for relevant industry news
"nonprofit" - Recognized nonprofit or NGO websites
"press" - Established press organizations with professional standards

Return ONLY one of these exact words: official, academic, questionable, corporate, nonprofit, press, none
{dont_tell_me}"#,
        analysis = source_analysis,
        url = article_url,
        dont_tell_me = DONT_TELL_ME
    )
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
