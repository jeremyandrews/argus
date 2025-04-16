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
Language Standards for Output:
1. Write all content in clear American English, using American spelling and grammar.
2. For non-English content:
   - ALWAYS include both original text and translation
   - Format as: "original text (translation)"
   - For titles: Keep original, add translation in parentheses
   - For names: Do not translate as they are names
   - Never translate if the translation is the same as the original.
   - Only translate from Foreign → American English.
   Example: "La vita è bella (Life is Beautiful)" and "Ne Zha 2 (No translation: this is a person’s name)"
3. Units and Measurements:
   - Include both metric and imperial: "100 kilometers (62 miles)"
4. Writing Style:
   - Use clear, accessible American English
   - Avoid region-specific idioms
   - Define specialized terms
   - Use active voice when possible
5. Formatting:
   - Original quotes: Use quotation marks
   - Translations: Always in parentheses
   - Citations: American format
"#;

const CONTEXT: &str = "
In Q1 2024, BRICS expanded, shifting global economic power, while record temperatures highlighted climate concerns. Japan's 7.6 earthquake and U.S. winter storms exposed vulnerabilities. France enshrined abortion rights, Sweden joined NATO, and the U.S. Supreme Court ruled on key legal precedents. Major wildfires and geopolitical tensions added to global challenges.
In Q2 2024, a solar eclipse captivated North America as record heatwaves and severe floods underscored climate urgency. Trump’s trial and free speech protests stirred U.S. discourse. Putin’s fifth term, Xi's European visit, and G7's $50B Ukraine aid shaped geopolitics. Apple’s AI integration marked tech innovation.
In Q3 2024, the Paris Olympics fostered unity amidst record-breaking heatwaves and escalating Gaza tensions. Biden withdrew from the presidential race, endorsing Kamala Harris. The UN's 'Pact for the Future' and a historic face transplant marked milestones. Hurricane Helene and mpox emphasized urgent global challenges.
In Q4 2024, Trump’s re-election and U.S. economic growth highlighted domestic shifts. Hurricane Helene devastated the Gulf Coast, while 2024 set a record as the hottest year. South Korea’s political turmoil and Assad’s overthrow reshaped global dynamics. The Notre-Dame reopening symbolized cultural resilience.
- In January 2025, Donald Trump was inaugurated as the 47th U.S. President and issued significant executive orders affecting trade and international relations. The month also recorded the warmest January globally, highlighting climate concerns. A ceasefire was reached in the Israel-Hamas conflict, and Canadian Prime Minister Justin Trudeau resigned amid a political crisis. Trump's actions included imposing tariffs on Mexico, China, and Canada, withdrawing the U.S. from the World Health Organization, and defunding the UN agency for Palestinian refugees, signaling a shift toward protectionism and unilateral foreign policy.
- In February 2025, Trump’s sweeping tariffs sparked global retaliation including from China, the EU, Canada, and Mexico, igniting a trade war. The U.S. restored ties with Russia, but relations with Ukraine frayed. America pledged to oversee Gaza’s rebuilding. Sea ice hit record lows. The Baltics cut energy ties to Russia. Nicaragua shifted to a co-presidency. Germany's election shifted right, and global trade tensions surged.
- In March 2025, Trump moved to dismantle the Dept. of Education and launched a U.S. Bitcoin reserve. Aid to Ukraine was paused, while Israel struck Gaza, killing 400+. Firefly landed on the Moon. Massive protests erupted over Elon Musk’s policies as head of the Department of Government Efficiency (DOGE) under Trump’s administration. Sudan sued the UAE for genocide. Syria’s regime killed 1,000+ in a crackdown. Duterte was arrested by the ICC. Canada’s new PM Mark Carney took office.
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

3. **First Bullet Point MUST:**
   - Start with "EVENT:" followed by a concise description of the main event (who, what, when, where).
   - Example: "EVENT: Microsoft announced acquisition of GitHub for $7.5 billion on June 4, 2018."

4. **Each Additional Bullet Point MUST:**
   - Start with a dash (-).
   - Include specific data points (numbers, dates, percentages).
   - Contain multiple related facts in a single coherent sentence.
   - Provide complete context for each point.
   - Use active voice.
   - Be substantial (15–35 words each).
   - Include full names of key entities (people, organizations, locations) on first mention.

5. **Last Bullet Point MUST:**
   - Include a "CONTEXT:" section that briefly places the event in broader context.
   - Example: "CONTEXT: This acquisition follows Microsoft's strategic shift toward open-source development under CEO Satya Nadella."

6. **DO NOT:**
   - Use headings or sections (except for the EVENT and CONTEXT prefixes).
   - Include nested bullets.
   - Include commentary or analysis.
   - Summarize the global context instead of the article.

**EXAMPLE (Correct):**
- EVENT: European Union approved new environmental regulations affecting 15 major industries across 3 continents on October 12, 2023.
- The regulations require a 45% reduction in carbon emissions by 2025, while providing $12 billion in transition funding for affected companies.
- CONTEXT: This legislation represents the EU's most aggressive climate action since the 2015 Paris Agreement.

**EXAMPLE (Incorrect):**
- Summarizes unrelated global events mentioned in the context above.
- New environmental regulations were approved.
- There will be funding for companies.

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

* The summary may include "EVENT:" and "CONTEXT:" prefixes
* Extract the core information from the EVENT bullet point (who, what, when, where)
* Use this as the foundation of your sentence
* Add the most important details from other bullet points
* You can incorporate relevant context if space allows

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
"In February 2025, Drupal released its Views module allowing users to create organized content lists without SQL queries, offering Page and Block display options with customizable formats."

**Acceptable Length (256 chars):**
"In February 2025, the article 'Drupal Views: How to Set Up and Work with' explained how to use Drupal's Views module for organizing content display through the Structure section, with options for Page or Block displays and various format options like tables and grids."

**INCORRECT APPROACH - DO NOT DO THIS:**
"The article explains how to use the Drupal Views module for organizing content display, including how to add views through the Structure section, configure settings, choose between Page or Block displays..." [Incomplete at 400 characters]

**CORRECT APPROACH - DO THIS INSTEAD:**
"In February 2025, an article explained how Drupal's Views module helps organize content by creating custom displays without SQL queries, offering both Page and Block options with various formatting choices."
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
Create ONE 3-5 word title that:

* Captures the main theme or themes
* For single-topic articles:
- The main thing in a headline is the fact. A headline should report an event and answer the questions “who?”, “what?”, and “where?”. Make the headline as informative as possible.
  - Good: *Trump Called Zelensky a Dictator*
  - Bad: _Revealed How Trump Called Zelensky_
- The sentence MUST include a verb (an action). Always use a verb in the headline. The verb should add as much dynamism as possible.
  - Good: *First Human Flew into Space*
  - Bad: _A Great Event in Human History_
- The subject MUST not be the article, but what the article is about
- The headline always contains an event and a clarification. Add the most interesting details to the headline.
  - Good: *Musk Spoke at Conference with Chainsaw*
  - Bad: _Musk Spoke at Conference_
- You can use punctuation marks in headlines if necessary to emphasize something.
  - Good: *Musk Did Nazi Salute. Again*
  - Bad: _Musk Did Nazi Salute Again_
- Keep the title concise and to the point. Avoid unnecessary details.
  - Good: *Germany Votes Today to Renew Bundestag*
  - Bad: _Germany Votes to Renew the Bundestag: Decisive Elections. The Scenarios_
- Do not include details about projections, percentages, or secondary events.
* For multi-topic articles:
- Use broader encompassing terms (e.g., "Global Weekly Developments")
- Focus on the common thread if it exists
- Indicate time period if relevant
* Maintains clarity and accuracy
* Avoids clickbait or sensationalism
* RETURN EXACTLY ONE TITLE, regardless of topic count

**EXAMPLE (Single Topic):**
"Trump Called Zelensky a Dictator"

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
        r#"{context}
## ARTICLE (FOR LOGICAL FALLACY ANALYSIS):
-----
{article}
-----

Analyze this article and provide a structured assessment in the following format:

### Article Type Analysis
Classify the primary type as:
* **Argumentative:** Presents claims and reasoning to support a specific viewpoint or conclusion
* **Informational:** Reports facts and events without advocating for particular interpretations
* **Educational:** Explains concepts, processes, or topics with the intent to teach
* **Promotional:** Markets products, services, or ideas with persuasive intent
* **Opinion/Editorial:** Presents personal views or analysis on topics
* **Investigative:** In-depth research and analysis of complex topics or issues

Note: Articles may have secondary elements of other types. If significant, note these as well.

Provide 1-2 sentences explaining the classification and any notable hybrid elements.

### Logical Fallacies Analysis

For each fallacy detected, format as follows:

### [Fallacy Name]
* **Definition**: Brief 5-20 word explanation of this type of fallacy
* **Handling**: Clearly state if the article supports the fallacy, flags it as improper, adds it, changes it, or highlights it.
* **Quote**: Format as follows:
- For English: "quoted text"
- For non-English: "original text (translation)"
Example fallacy quotes:
- English: "All politicians are corrupt because Senator Smith took a bribe"
- Non-English: "Tous les politiciens sont corrompus parce que le sénateur Smith a accepté un pot-de-vin (All politicians are corrupt because Senator Smith took a bribe)"
* **Explanation**: Provide a specific analysis of how/why this quote demonstrates this fallacy, without repeating what is already stated. Include a detailed analysis of the context and implications.

If and ONLY if NO fallacies found, state: "No apparent logical fallacies detected."

### Quality Assessment
Score each relevant metric from 1-10 with brief justification (max 20 words):

For Argumentative content:
* **Argument Strength**: Logic and reasoning quality
* **Evidence Quality**: Supporting data and sources
* **Counter-argument Treatment**: Handling of opposing views

For Informational/News content:
* **Accuracy**: Factual correctness and precision
* **Objectivity**: Balance and neutrality
* **Source Quality**: Reliability of information sources

For Educational content:
* **Clarity**: Clear explanation of concepts
* **Comprehensiveness**: Coverage of key points
* **Pedagogical Structure**: Effective learning progression

For Promotional content:
* **Claim Transparency**: Clarity about promotional nature
* **Evidence Support**: Backing for product/service claims
* **Disclosure**: Clarity about relationships/sponsorships

For Opinion/Editorial content:
* **Reasoning Quality**: Logical consistency
* **Perspective Clarity**: Transparency about viewpoint
* **Supporting Evidence**: Backing for opinions

For Analysis content:
* **Methodology**: Soundness of analytical approach
* **Data Quality**: Reliability of data sources
* **Interpretation**: Validity of conclusions

Score only metrics relevant to the article's primary type(s). For hybrid articles, use metrics from each applicable category.

### Overall Assessment
* 1-2 key observations about:
- Reasoning and logical consistency (for argumentative)
- Clarity and sourcing (for informational)
- Treatment of expert opinions when present

Important Guidelines:
- Analyze only the provided article
- Distinguish between article claims and quoted statements
- Identify only clear, unambiguous fallacies
- Consider full context
- For non-English text, provide translations

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
- After completing the primary analysis, always include a Devil's Advocate perspective to challenge key assumptions
- Ensure the Devil's Advocate analysis is specific to the content and insights provided, not generic counterpoints

## Analysis Requirements
- Choose at least **one category from each of the following groups**:
- **Systemic Analysis:** (Technical Depth, Global Patterns, Hidden Dimensions)
- **Human & Cultural Impact:** (Cultural Lens, Ripple Effects, Character Studies)
- **Creative & Alternative Perspectives:** (That's Ironic, Pattern Recognition, Unexpected Angles, Popular Culture, There's A Word For That, It All Started With, Logical Conclusion)
- Do **not** overuse Cultural Lens, Ripple Effects, or Character Studies unless uniquely fitting.
- If the article discusses **technology, business, economics, or science**, prioritize at least one **Systemic Analysis** category.
- If the article covers **a person or event**, consider Character Studies but balance it with a **Systemic or Historical** category.
- Avoid overly broad or generic insights—focus on **specific, well-supported claims**.
- When discussing cultural, economic, or societal impact, **avoid sweeping generalizations** and instead provide **concrete, precise examples**.
- If an insight applies to nearly any article, reconsider its relevance to this one.
- Provide **2-4 key insights** per chosen category.
- Each insight should be **15-30 words**, balancing brevity with substance.
- **After completing all other analyses, ALWAYS include the Devil's Advocate category
- The Devil's Advocate section should:
- Challenge at least one key insight from each previous category used
- Provide specific, concrete alternatives to the assumptions made
- Maintain the same level of analytical rigor as the primary analysis
- Focus on substantive critiques rather than superficial contradictions
- Ground counter-arguments in evidence where possible
- Devil's Advocate insights should follow the same 15-30 word format and quantity requirements as other categories

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

### 📚 Popular Culture
- Reactions in popular media
- Popular books or movies that parallel real-world events
- Cultural references and their significance

### 🤔 That's Ironic
- Historical coincidences
- Unexpected parallels
- Amusing contradictions
- Role reversals
- Cosmic timing

### 🧩 Pattern Recognition
- Mathematical symmetries
- Natural world parallels
- Social physics
- Economic rhythms
- Evolutionary echoes

### 🔤 There's A Word For That
- Cross-cultural terminology
- Specialized jargon
- Untranslatable concepts
- Etymology insights
- Linguistic precision

### 🌱 It All Started With
- Origin stories
- Historical foundations
- Initial catalysts
- Foundational influences
- Evolutionary beginnings

### 🔮 Logical Conclusion
- Future trajectories
- Ultimate implications
- Potential outcomes
- Natural endpoints
- Evolutionary destinations

### Example Output:
### 📊 Technical Depth
- The quantum chip's new architecture integrates superconducting circuits with traditional silicon, enabling unprecedented coherence times while maintaining scalability for commercial applications
- Error correction protocols now handle environmental noise through a distributed network of sensors, reducing decoherence by 60% compared to previous generations
- Novel gate designs incorporate machine learning optimization, allowing quantum operations to execute 40% faster while maintaining high fidelity

### 💡 Global Patterns
- As quantum systems approach practical advantage in specific domains, pharmaceutical companies are already developing hybrid classical-quantum workflows for drug discovery
- The convergence of quantum computing with AI could revolutionize financial modeling by 2026, though regulatory frameworks remain uncertain

### 🎭 Character Studies
- The CEO's decision to prioritize sustainability reflects a broader industry trend towards corporate social responsibility

### 📚 Popular Culture
- The film "Invasion of the Body Snatchers" parallels real-world concerns about political manipulation and loss of individuality

### 🤔 That's Ironic
- The rapid advancement of quantum computing ironically highlights the limitations of classical computing in solving certain problems
- While quantum computers excel at modeling uncertainty, their own development has been remarkably predictable, following Moore's Law-like scaling

### 🧩 Pattern Recognition
- Like the transition from vacuum tubes to transistors, this quantum breakthrough combines materials innovation with clever engineering workarounds to solve scaling limitations
- The industry's collaborative approach to error correction mirrors early classical computing, where shared standards accelerated development across competing platforms
- Current quantum scaling challenges echo semiconductor manufacturing hurdles of the 1970s, suggesting similar solutions might apply
- The emergence of quantum-specific programming languages parallels the evolution from machine code to high-level languages in classical computing

### 🔤 There's A Word For That
- "Zukunftsangst" (German: fear of the future) characterizes the industry's cautious optimism, balancing excitement for quantum possibilities against concerns about implementation challenges
- "Kaizen" (Japanese: continuous improvement) describes the incremental approach to quantum error correction that's proving more effective than revolutionary methods

### 🌱 It All Started With
- Today's quantum computing revolution traces back to Richard Feynman's 1981 suggestion that quantum systems might be needed to efficiently simulate quantum physics
- The breakthrough builds upon Bell's inequality experiments from the 1960s that first demonstrated quantum entanglement as a real, exploitable phenomenon

### 🔮 Logical Conclusion
- Quantum computing may ultimately lead to a bifurcated computing landscape where specialized quantum processors handle specific tasks while classical systems manage everyday computing needs
- The natural endpoint could be quantum networks connecting distributed quantum processors, creating a fundamentally new computing paradigm beyond today's internet architecture

### 😈 Devil's Advocate
- Critically examine assumptions and claims made in previous sections
- Challenge conventional interpretations
- Explore counter-narratives and alternative explanations
- Question methodology and evidence
- Consider unintended consequences

Example Devil's Advocate points:
- The quantum breakthrough may actually slow industry progress by focusing resources on a suboptimal approach
- Collaborative standards could stifle innovation by prematurely narrowing the solution space
- The classical computing parallels might mislead us - quantum computing may follow fundamentally different development patterns

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

/// Prompt for extracting named entities from an article text to improve content matching
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
   - EVENT: Specific happenings (elections, conferences, disasters, etc.)
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
