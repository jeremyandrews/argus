use crate::prompt::common::{
    current_date, global_context, DONT_TELL_ME, FORMAT_INSTRUCTIONS, WRITE_IN_CLEAR_ENGLISH,
};

/// Generate a prompt for summarizing an article into a bullet-point summary
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

/// Generate a prompt for creating a tiny multi-sentence summary based on an existing summary
pub fn tiny_summary_prompt(summary_response: &str) -> String {
    format!(
        r#"Below is the summary of an article between ~~~ markers:
~~~
{summary}
~~~
CREATE A CONCISE SUMMARY:
* TARGET LENGTH: 200 characters total
* ABSOLUTE MAXIMUM: 400 characters total
* Use 2-3 short, complete sentences instead of one long sentence
* Each sentence should focus on a distinct aspect of the news
* If you reach 400 characters, start over and prioritize better

* The summary may include "EVENT:" and "CONTEXT:" prefixes in the bullet points
* Extract ONLY the information after these labels - DO NOT include the words "EVENT:" or "CONTEXT:" in your summary
* Use the information from the EVENT bullet point as the foundation of your first sentence
* Add the most important details from other bullet points in subsequent sentences
* You can incorporate relevant context if space allows

FORMAT REQUIREMENTS:
* All sentences MUST be in a SINGLE PARAGRAPH with NO line breaks between sentences
* Must fit in a tweet
* Must prioritize most important information
* Must drop less critical details
* Must use active voice
* Must be 2-3 complete, coherent sentences
* Must preserve source attribution and factual accuracy
* Must distinguish between confirmed facts vs rumors/leaks/reports
* Must NEVER convert "reportedly" or "according to leaks" into definitive statements
* Must maintain the same level of certainty as the original summary

CRITICAL: PRESERVE FACTUAL ACCURACY
* If original mentions "leaks" or "rumors", your summary MUST include this qualification
* If original says "reportedly" or "allegedly", your summary MUST maintain these qualifiers
* NEVER present unconfirmed information as confirmed fact
* NEVER convert phrases like "according to sources" into definitive statements
* NEVER suggest a company officially announced something when article only mentions leaks/rumors

PROPER ATTRIBUTION EXAMPLES:
* INCORRECT: "EVENT: Apple Details Foldable iPhone Specs." (includes label and implies official announcement)
* CORRECT: "Apple's foldable iPhone specs were detailed in recent leaks. The device may feature a 7.6-inch display." (no label, proper attribution)

* INCORRECT: "Google Reveals New Product Features. CONTEXT: This follows their previous release." (includes label and implies official reveal)
* CORRECT: "Google's new product features reportedly include voice control. The update is expected next month following their previous release." (no labels, maintains qualification)

TEMPORAL ACCURACY (CRITICAL):
* TODAY means {date} - the system's current date at the time of processing
* ALWAYS use appropriate tense to distinguish between past, present, and future events
* For PAST events (before today): Use past tense ("announced," "released," "discovered")
* For PRESENT events (happening now): Use present tense ("is announcing," "is rolling out")
* For FUTURE events (after today): Use future-indicating phrases ("will announce," "plans to release")
* NEVER describe future events as if they've already happened
* Check dates carefully and maintain temporal accuracy
* When a date is mentioned in the article, compare it to TODAY to determine proper tense

SENTENCE STRUCTURE:
* First sentence: Focus on the core event (who did what, when, where)
* Second sentence: Add important details, numbers, or implications
* Third sentence (if needed): Provide context or additional significance
* Keep each sentence under 150 characters when possible
* Each sentence should be complete on its own
* Avoid conjunctions that create run-on sentences

LEAD-IN VARIETY:
* DO NOT always start with "In [month/date/year]" unless the date is CRITICAL
* ONLY highlight the date when it adds significant value to the information
* Vary your opening approaches based on what's most important about the news

PROMOTIONAL CONTENT:
* Focus on substantive information, not promotions or sales
* If the article is primarily about price reductions, indicate this is a "price promotion article"

For multi-topic articles:
* Use one sentence per major topic
* Drop minor events to stay within length
* Keep only the most significant numbers/dates

EXAMPLES OF EFFECTIVE SUMMARIES:

**Technology Announcement (183 chars):**
"Drupal released its Views module allowing users to create organized content lists without SQL queries. The module offers both Page and Block display options with customizable formats."

**Discovery News (199 chars):**
"Researchers at MIT discovered a new quantum computing method that reduces error rates by 40%. This breakthrough requires fewer qubits and could accelerate practical applications by several years."

**Product Leak (170 chars):**
"Apple's upcoming iPhone will reportedly feature satellite connectivity for emergency calls. The technology would work in remote areas without cellular coverage, according to multiple sources."

**Climate Report (177 chars):**
"Arctic ice reached its lowest summer extent since records began. Scientists warn the region could experience ice-free summers by 2035 if current trends continue."

**Election News (198 chars):**
"Voters will decide on Proposition 37 on November 5, 2025. The measure would establish universal basic income for all state residents over 18, funded by a 2% wealth tax on assets exceeding $50 million."

**INCORRECT APPROACH - DO NOT DO THIS:**
"EVENT: Drupal released its Views module. The module helps organize content by creating custom displays without SQL queries. CONTEXT: This follows increased demand for no-code solutions." [Includes labels and has line breaks]

**CORRECT APPROACH - DO THIS INSTEAD:**
"Drupal released its Views module allowing users to create organized content lists without SQL queries. The module offers both Page and Block options with various formatting choices for easier website management."
{write_in_clear_english}
{dont_tell_me}"#,
        summary = summary_response,
        date = current_date(),
        write_in_clear_english = WRITE_IN_CLEAR_ENGLISH,
        dont_tell_me = DONT_TELL_ME
    )
}

/// Generate a prompt for creating a title from a summary
pub fn tiny_title_prompt(summary_response: &str) -> String {
    format!(
        r#"{summary}
Create ONE 3-5 word title that:

* Captures the main theme or themes
* For single-topic articles:
- The main thing in a headline is the fact. A headline should report an event and answer the questions "who?", "what?", and "where?". Make the headline as informative as possible.
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

* CRITICAL FACTUAL ACCURACY REQUIREMENTS:
- For rumors, leaks, or unconfirmed reports, your title MUST reflect this uncertainty
  - Good: *iPhone Specs Leaked Online*
  - Bad: *Apple Announces iPhone Specs* (when it's only a leak)
- Never present rumors or leaks as confirmed facts
  - Good: *Analyst Predicts Tesla Expansion*
  - Bad: *Tesla Expands to New Markets* (when it's just a prediction)
- Use verbs that accurately reflect the level of certainty
  - For confirmed actions: "Announces", "Launches", "Releases"
  - For rumors/leaks: "Reportedly", "Allegedly", "Rumored to", "Leaks Suggest"
- For articles about price drops, discounts, or sales:
  - Add "Sale:" prefix if the article is primarily about a promotional discount
  - Example: *Sale: iPad Prices Reduced*

* For multi-topic articles:
- Use broader encompassing terms (e.g., "Global Weekly Developments")
- Focus on the common thread if it exists
- Indicate time period if relevant
* Maintains clarity and accuracy
* Avoids clickbait or sensationalism
* RETURN EXACTLY ONE TITLE, regardless of topic count

**EXAMPLES (Single Topic):**
"Trump Called Zelensky a Dictator"

**EXAMPLES (With Attribution):**
"iPhone Features Reportedly Leaked" (for leaks)
"Apple Products' Prices Reduced" (for sales)

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
