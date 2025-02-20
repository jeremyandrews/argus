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
        r#"Below is the summary of an article between ~~~ markers:
~~~
{summary}
~~~

CREATE ONE SENTENCE:
* TARGET LENGTH: 200 characters
* ABSOLUTE MAXIMUM: 400 characters
* If your sentence reaches 400 characters, start over and prioritize better
* Never leave an incomplete thought or hanging sentence

REQUIREMENTS:
* Must fit in a tweet
* Must prioritize most important information
* Must drop less critical details
* Must use active voice
* Must be ONE complete, coherent sentence

For multi-topic articles:
* Use "In [timeframe], [main event]; [second event]; [third event]"
* Drop minor events to stay within length
* Keep only the most significant numbers/dates
* Plan your sentence before writing to ensure completion within limit

EXAMPLES OF CORRECT LENGTH:

**Perfect Length (178 chars):**
"In February 2025, Trump imposed 25% tariffs on cars and semiconductors, ordered mass deportations, and reversed climate policies in his first month as president."

**Acceptable Length (256 chars):**
"In his first month as president, Trump imposed 25% tariffs on foreign goods, initiated deportations of undocumented migrants, and reversed environmental policies while denying climate change evidence."

**INCORRECT APPROACH - DO NOT DO THIS:**
"In his first month, Trump issued numerous executive orders including tariffs, deportations, and environmental reversals, while also renaming geographic features, changing straw policies..." [Incomplete at 400 characters]

**CORRECT APPROACH - DO THIS INSTEAD:**
"In February 2025, Trump's first month as president saw three major actions: 25% tariffs on foreign goods, mass deportation orders, and environmental policy reversals."

{write_in_clear_english}
{dont_tell_me}"#,
        summary = summary_response,
        write_in_clear_english = WRITE_IN_CLEAR_ENGLISH,
        dont_tell_me = DONT_TELL_ME
    )
}

pub fn tiny_title_prompt(summary_response: &str) -> String {
    format!(
        r#"{summary}
Create ONE informational and accurate 3-5 word title that:
* Captures the main theme or themes
* For multi-topic articles:
  - Use broader encompassing terms (e.g., "Global Weekly Developments")
  - Focus on the common thread if exists
  - Indicate time period if relevant
* Maintains clarity and accuracy
* Avoids clickbait or sensationalism
* RETURN EXACTLY ONE TITLE, regardless of topic count

**EXAMPLE (Single Topic):**
"February 20th SpaceX Launch Success"

**EXAMPLE (Multi-Topic):**
"March Global Events Review"

IMPORTANT: Return ONLY one title, even if the article covers multiple topics or events.

{write_in_clear_english}
{dont_tell_me}"#,
        summary = summary_response,
        write_in_clear_english = WRITE_IN_CLEAR_ENGLISH,
        dont_tell_me = DONT_TELL_ME
    )
}

pub fn critical_analysis_prompt(article_text: &str, pub_date: Option<&str>) -> String {
    format!(
        r#" {context}
## ARTICLE (FOR CRITICAL ANALYSIS):
----------
 {article}
----------

IMPORTANT INSTRUCTIONS:
* **Analyze ONLY the article above.**
* **Distinguish between:**
  - **Article voice:** The author's/publication's direct statements
  - **Source quotes:** Statements from interviews, documents, or other sources
* **When evaluating credibility:**
  - Consider how effectively the article contextualizes its quotes
  - Assess the balance between direct reporting and quoted material
  - Note whether controversial quotes are properly attributed and explained

### **Content Analysis**
**Quote Integration:** [1-10]
* How effectively does the article:
  - Introduce and contextualize quotes
  - Balance direct reporting with quoted material
  - Handle controversial or incorrect quoted statements
  - Maintain clarity between article voice and quoted content

### **Credibility Analysis**
**Credibility Score:** [1-10]
* Provide specific reasons (max 20 words)
* Consider:
  - Source reliability
  - Citation quality
  - Expert consultation
  - Fact verification
  - Methodology transparency

### **Writing Style Analysis**
**Style Score:** [1-10]
* Provide specific reasons (max 20 words)
* Consider:
  - Clarity of expression
  - Organization
  - Technical accuracy
  - Language appropriateness
  - Engagement level

### **Political Analysis**
**Political Leaning:** [Far Left | Left | Center Left | Center | Center Right | Right | Far Right | N/A]
* Provide specific evidence (max 20 words)
* Include:
  - Word choice analysis
  - Source selection
  - Topic framing
  - Quote selection
  - Context presentation

### **Tone Assessment**
**Primary Tone:** [Neutral | Positive | Negative | Alarmist | Optimistic | Skeptical | Other]
* Support with specific examples (max 20 words)
* Secondary tones if present
* Quote relevant passages (with translations if needed)

### **Audience Analysis**
**Target Audience:**
* Demographics (max 10 words)
* Expertise level required
* Geographic focus
* Professional/General
* Cultural context

### **Critical Analysis**
2-3 bullet points examining:
* Argument structure
* Evidence quality
* Logical consistency
* Potential biases
* Information gaps
* Cultural/contextual factors

### **Key Takeaways**
1-2 bullet points covering:
* Main conclusions
* Significance
* Broader implications
* Notable limitations

**EXAMPLE OUTPUT:**
### Credibility Analysis
**Credibility Score:** 8/10
- Article clearly distinguishes between factual reporting and quoted opinions
- Provides context for controversial quotes
- Maintains accuracy while including diverse perspectives

### Quote Integration: 9/10
Article voice: "9 is the number after 8"
- States mathematical fact directly

Quoted content: "Because 7 8 9, we skip from 8 to 10"
- Clearly attributed as humorous reference
- Doesn't compromise article's factual accuracy
- Adds engaging cultural context

### Writing Style Analysis
**Style Score:** 7/10
- Clear technical explanations with appropriate jargon, well-structured arguments, engaging narrative

### Political Analysis
**Political Leaning:** Center-Right
- Emphasizes market-based solutions, quotes business leaders predominantly, focuses on economic impact

### Tone Assessment
**Primary Tone:** Skeptical
Text: "Les preuves ne sont pas concluantes"
Translation: "The evidence is not conclusive"
- Consistently questions assumptions and demands stronger evidence

### Audience Analysis
**Target Audience:** Financial professionals and policy makers in European markets
- Assumes familiarity with economic terms and regulatory framework

### Critical Analysis
- Strong empirical evidence supports main arguments, but overlooks potential alternative interpretations
- Comprehensive data presentation, though some regional comparisons lack context

### Key Takeaways
- Policy implications are well-supported by data but could benefit from more diverse expert perspectives
- Analysis provides valuable insights while acknowledging limitations of current research

Now, perform the analysis with these guidelines:
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
        r#" {context}
## ARTICLE (FOR LOGICAL FALLACY ANALYSIS):
-----
{article}
-----

IMPORTANT INSTRUCTIONS:
* **Analyze ONLY the article above.**
* **Distinguish between:**
  - **Article assertions:** Claims made directly by the article/author
  - **Quoted content:** Statements attributed to sources or subjects
* **When analyzing fallacies:**
  - Identify whether the fallacy is in the article's reasoning or in quoted content
  - Consider how the article presents and contextualizes quotes
  - Don't penalize articles for including fallacious quotes if they're properly contextualized
* **For non-English text, provide both the original and translation in American English.**

### **Article Type Analysis**
* Classify as either:
  - **Argumentative:** Presents claims, reasoning, or conclusions supporting a viewpoint
  - **Informational:** Primarily factual without arguing for a specific view
* For informational articles, focus on clarity and factual reliability rather than argument strength
* Authoritative opinions in professional contexts should be evaluated for relevance, not dismissed

### **Logical Fallacies Analysis**
* For each fallacy found:
  1. **Name of Fallacy**
  2. **Source of Fallacy:**
     - "Article:" for author's own reasoning
     - "Quoted:" for statements from sources
  3. Quote the relevant text (if non-English, include translation)
  4. Explain why it's fallacious (max 20 words)
  5. Note how the article handles the fallacy (if in quoted content)

* If and ONLY if NO fallacies are found:
  - Write: _"No apparent logical fallacies detected."_
* Otherwise:
  - List all fallacies found
  - Do NOT include "No apparent logical fallacies detected" after listing fallacies

### **Quality Assessment**
For Argumentative Articles:
* **Argument Strength:** [1-10]
  - Justification (max 20 words)
* **Evidence Quality:** [1-10]
  - Justification (max 20 words)

For Informational Articles:
* **Clarity & Coherence:** [1-10]
  - Justification (max 20 words)
* **Factual Reliability:** [1-10]
  - Justification (max 20 words)

### **Overall Assessment**
* 1-2 bullet points summarizing:
  - Key observations about reasoning and logical consistency
  - For informational articles: clarity and sourcing quality
  - Reliability of authoritative opinions when present

**EXAMPLE OUTPUT:**
### Article Type Analysis
 - Informational: Reports on parliamentary proceedings without advocating for a position

### Logical Fallacies Analysis
**Appeal to Emotion**
 - Source: Quoted
 - Text: "Because 7 8 9, we skip from 8 to 10"
 - Context: Article includes this humorous quote while correctly stating "9 is the number after 8"
 - Handling: Article appropriately presents this as a playful reference while maintaining factual accuracy

**False Dichotomy**
 - Source: Article
 - Text: "We must either count to 9 or skip it entirely"
 - Problem: Article itself presents a false choice, ignoring other numerical approaches

### Quality Assessment
**Clarity & Coherence:** 7/10
- Clear reporting of events but lacks contextual background

**Factual Reliability:** 6/10
- Presents verifiable statements but needs more supporting detail

### Overall Assessment
- Reporting is straightforward but could benefit from deeper investigation of claims
- Would be strengthened by including more diverse sources and historical context

Now, perform the analysis with these guidelines:
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
        r#"Based on this logical fallacy and article quality analysis:
~~~
{analysis}
~~~
Return a single number (1, 2, or 3) representing the overall quality of the article:
1 = Poor (red) - Multiple serious fallacies, weak arguments, or unreliable information
2 = Moderate (yellow) - Some fallacies, weaknesses in reasoning, or minor factual issues
3 = Excellent (green) - Strong arguments OR well-sourced, factually reliable information

Base your assessment on:
- If the article is **Argumentative**:
  - Number and severity of logical fallacies
  - Argument strength score
  - Evidence quality score
  - Overall logical consistency
- If the article is **Informational**:
  - Clarity & coherence score
  - Factual reliability score
  - Overall professionalism and objectivity

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

pub fn additional_insights_prompt(article_text: &str, pub_date: Option<&str>) -> String {
    format!(
        r#"# Analysis Framework
## Global Context (Reference Only)
{context}
_Note: Only reference global context if directly relevant to analyzing the article_

## Source Material
----------
{article}
----------

## Core Requirements 
- Analyze the article's content, not the global context
- Reveal deeper context and connections beyond the article's surface details
- Illuminate cultural nuances and regional perspectives specific to the article
- Draw unexpected parallels and insights that enhance understanding of the article
- Ground claims in concrete examples from or related to the article's content

## Analysis Requirements
- Choose at least **one category from each of the following groups**:
  - **Systemic Analysis:** (Technical Depth, Global Patterns, Hidden Dimensions)
  - **Human & Cultural Impact:** (Cultural Lens, Ripple Effects, Character Studies)
  - **Creative & Alternative Perspectives:** (Delightful Ironies, Pattern Recognition, Unexpected Angles, etc.)
- Do **not** overuse Cultural Lens, Ripple Effects, or Character Studies unless uniquely fitting.
- If the article discusses **technology, business, economics, or science**, prioritize at least one **Systemic Analysis** category.
- If the article covers **a person or event**, consider Character Studies but balance it with a **Systemic or Historical** category.
- Avoid overly broad or generic insights—focus on **specific, well-supported claims**.
- When discussing cultural, economic, or societal impact, **avoid sweeping generalizations** and instead provide **concrete, precise examples**.
- If an insight applies to nearly any article, reconsider its relevance to this one.
- Provide **2-4 key insights** per chosen category.
- Each insight should be **15-30 words**, balancing brevity with substance.
- **Ground insights in specific examples**.

### 🌍 Cultural Lens
- Power dynamics and hierarchies
- Local traditions and values
- Historical patterns
- Social structures and relationships
- Language and communication styles

### 📊 Technical Depth
- Core principles and mechanisms
- Hidden complexities
- System interactions
- Engineering challenges
- Implementation details

### 💡 Global Patterns
- Cross-cultural parallels
- Regional adaptations
- Universal principles
- Contrasting approaches
- Historical echoes

### 📈 Ripple Effects
- Industry transformations
- Societal shifts
- Economic impacts
- Political implications
- Cultural evolution

### 🤔 Hidden Dimensions
- Unspoken assumptions
- Alternative frameworks
- Overlooked factors
- Competing narratives
- Cultural blind spots

### 🔮 Future Threads
- Emerging patterns
- Potential disruptions
- Cultural evolution
- Technological convergence
- Societal adaptation

### 😄 Delightful Ironies
- Historical coincidences
- Unexpected parallels
- Amusing contradictions
- Role reversals
- Cosmic timing

### 📅 Time's Echo
- Events on this date
- Cyclical patterns
- Historical rhymes
- Forgotten precedents
- Anniversary insights

### 👁️ Unexpected Angles
- Nature's perspective
- Future archaeologists' view
- Children's understanding
- Alien anthropologist's report
- Ordinary objects' stories

### 🎭 Character Studies
- Key personalities
- Hidden influencers
- Unlikely heroes
- Silent catalysts
- Generational contrasts

### 🎨 Creative Connections
- Art world parallels
- Literary echoes
- Musical metaphors
- Architectural analogies
- Gaming dynamics

### 🧩 Pattern Recognition
- Mathematical symmetries
- Natural world parallels
- Social physics
- Economic rhythms
- Evolutionary echoes

### 🎬 Scene Shifts
- Behind the curtain
- Alternative endings
- Untold beginnings
- Parallel universes
- What-if scenarios

### 🌱 Seeds of Change
- Small triggers
- Butterfly effects
- Hidden catalysts
- Quiet revolutions
- Gradual transformations

### 🎯 Precision Focus
- Crucial details
- Pivotal moments
- Key decisions
- Critical junctures
- Defining elements

## Example Output:

### 📊 Technical Depth
- The quantum chip's new architecture integrates superconducting circuits with traditional silicon, enabling unprecedented coherence times while maintaining scalability for commercial applications
- Error correction protocols now handle environmental noise through a distributed network of sensors, reducing decoherence by 60% compared to previous generations
- Novel gate designs incorporate machine learning optimization, allowing quantum operations to execute 40% faster while maintaining high fidelity

### 🔮 Future Threads
- As quantum systems approach practical advantage in specific domains, pharmaceutical companies are already developing hybrid classical-quantum workflows for drug discovery
- The convergence of quantum computing with AI could revolutionize financial modeling by 2026, though regulatory frameworks remain uncertain

### 🧩 Pattern Recognition
- Like the transition from vacuum tubes to transistors, this quantum breakthrough combines materials innovation with clever engineering workarounds to solve scaling limitations
- The industry's collaborative approach to error correction mirrors early classical computing, where shared standards accelerated development across competing platforms
- Current quantum scaling challenges echo semiconductor manufacturing hurdles of the 1970s, suggesting similar solutions might apply
- The emergence of quantum-specific programming languages parallels the evolution from machine code to high-level languages in classical computing

### 😄 Delightful Ironies
- While quantum computers excel at modeling uncertainty, their own development has been remarkably predictable, following Moore's Law-like scaling
- The same Copenhagen interpretation that puzzled Einstein now enables practical quantum computing advances
- The quest for absolute precision in quantum gates ironically depends on carefully managed randomness

{write_in_clear_english}
{dont_tell_me}"#,
        context = global_context(pub_date),
        article = article_text,
        write_in_clear_english = WRITE_IN_CLEAR_ENGLISH,
        dont_tell_me = DONT_TELL_ME
    )
}

/** The following prompts expect a 'yes' or 'no' answer. */

pub fn threat_prompt(article_text: &str) -> String {
    format!(
        "
----------
{article}
----------
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
    format!("
----------
{article}
----------
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
    format!("
----------
{article}
----------

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
