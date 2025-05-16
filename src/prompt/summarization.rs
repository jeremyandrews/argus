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

3. **The EVENT Bullet Point MUST:**
   - This MUST be exactly ONE bullet point
   - Start with "EVENT:" followed by a concise description of the main event (who, what, when, where).
   - EXPLICITLY INDICATE information source type using one of these labels:
     * [OFFICIAL]: For confirmed facts from primary sources (company announcements, official statements, press releases)
       - Use this for PRESENT-TENSE announcements: "Company X announces," "Studio Y releases," "Artist Z publishes"
       - Use this for announcements about CONFIRMED future events: "Movie premiere scheduled for July 2025"
       - Use this for ANY release date announcement by an official source (studio, publisher, developer)
     * [NEWS]: For reporting from established news sources
       - Use this for factual reporting by journalists and news outlets
       - Use this when a news source is reporting on an announcement, but isn't the primary source
     * [RUMOR/LEAK]: For unconfirmed information, leaks, or rumors
       - Use this ONLY for genuinely unconfirmed information from unofficial sources
       - Information must lack official confirmation to be classified as [RUMOR/LEAK]
       - DO NOT use for official announcements about future plans/features/releases
     * [ANALYSIS]: For expert analysis or opinions
   - CRITICAL: Use precise verbs that accurately reflect certainty level:
     * For confirmed facts: "announced", "released", "launched", "confirmed"
     * For rumors/reports/leaks: "reportedly", "allegedly", "rumored to", "according to sources", "is said to"
   - Examples: 
     * Confirmed: "EVENT: [OFFICIAL] Microsoft announced acquisition of GitHub for $7.5 billion on June 4, 2018."
     * Unconfirmed: "EVENT: [RUMOR/LEAK] Apple reportedly plans to release a foldable iPhone in 2026, according to industry leaks."

4. **The CONTEXT Bullet Point MUST:**
   - This MUST be exactly ONE bullet point
   - Start with "CONTEXT:" followed by information that places the event in broader context
   - EXPLICITLY INDICATE the reliability/source of this contextual information using the same labels as EVENT
   - Example: "CONTEXT: [NEWS] This acquisition follows Microsoft's strategic shift toward open-source development under CEO Satya Nadella, reported by The Wall Street Journal."

5. **All Other Bullet Points (Summary Content) MUST:**
   - Start with a dash (-).
   - Include specific data points (numbers, dates, percentages).
   - Contain multiple related facts in a single coherent sentence.
   - Provide complete context for each point.
   - Use active voice.
   - Be substantial (15–35 words each).
   - Include full names of key entities (people, organizations, locations) on first mention.
   - Use as many bullets as needed based on the article length requirements in rule #2.

6. **Attribution and Certainty REQUIRED:**
   - Always maintain appropriate attribution for unconfirmed information
   - Clearly indicate when information comes from rumors, leaks, analysts, or unconfirmed sources
   - NEVER present rumors, leaks, or speculation as confirmed facts
   - Use specific attribution phrases: "according to sources", "reportedly", "allegedly", "rumored", "leaks suggest"
   - For Apple and other companies: explicitly distinguish between official announcements and unconfirmed reports/rumors
   - Maintain proper skepticism with phrases like "claimed to" or "purported to" for unverified claims

7. **DO NOT:**
   - Use headings or sections (except for the EVENT and CONTEXT prefixes).
   - Include nested bullets.
   - Include commentary or analysis.
   - Summarize the global context instead of the article.

**EXAMPLES (Correct):**

Confirmed Event (Political announcement):
- EVENT: [OFFICIAL] European Union approved new environmental regulations affecting 15 major industries across 3 continents on October 12, 2023.
- The regulations require a 45% reduction in carbon emissions by 2025, while providing $12 billion in transition funding for affected companies.
- CONTEXT: [NEWS] This legislation represents the EU's most aggressive climate action since the 2015 Paris Agreement, as reported by Reuters.

Confirmed Event (Entertainment announcement):
- EVENT: [OFFICIAL] Paramount+ announced Star Trek: Strange New Worlds season 3 will premiere on July 17, 2025, with a two-episode debut.
- The new season will feature period-accurate 1960s elements, a murder-mystery on the Holodeck, and appearances by characters Kirk, Scotty, and Sybok.
- CONTEXT: [NEWS] This announcement highlights the continued success of the Star Trek franchise on streaming platforms, as covered in multiple entertainment publications.

Rumor/Leak Example:
- EVENT: [RUMOR/LEAK] Apple reportedly plans to release an augmented reality headset in 2025, according to supply chain sources cited in Bloomberg.
- The device is rumored to feature advanced eye-tracking technology and may be priced around $2,000, though specifications remain unconfirmed.
- Industry analysts suggest Apple has allegedly ordered specialized components from Taiwanese manufacturers for the initial production run.
- CONTEXT: [NEWS] This would represent Apple's first major new product category since the Apple Watch was introduced in 2015, as noted in multiple industry publications.

**EXAMPLES (Incorrect):**
- Summarizes unrelated global events mentioned in the context above.
- New environmental regulations were approved. (Too vague)
- There will be funding for companies. (Too vague)
- EVENT: Apple is releasing an AR headset in 2025. (WRONG - presents rumor as confirmed fact without [RUMOR/LEAK] label)
- EVENT: [OFFICIAL] The company plans to release a new product next year. (WRONG - labeled as official but describes future plans)
- EVENT: iPhone 16 will include advanced AI features. (WRONG - future event presented as definite without attribution and missing source label)
- CONTEXT: This is the latest in a series of developments. (WRONG - too vague and missing source label)

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

* The summary will include "EVENT:" and "CONTEXT:" bullet points with source labels like [OFFICIAL], [NEWS], [RUMOR/LEAK], or [ANALYSIS]
* You MUST REMOVE BOTH the "EVENT:" and "CONTEXT:" prefixes from your summary
* You MUST REMOVE the [OFFICIAL], [NEWS], [RUMOR/LEAK], or [ANALYSIS] source labels from your summary
* However, you MUST PRESERVE the level of certainty indicated by these source types in your language
* For [OFFICIAL] sources: Use confident, definitive language without qualifiers
  - BEFORE: "EVENT: [OFFICIAL] Microsoft announced new AI features..."
  - AFTER: "Microsoft announced new AI features..." (note: no "EVENT:" prefix, no [OFFICIAL] label)
* For [NEWS] sources: Include modest attribution when appropriate
  - BEFORE: "EVENT: [NEWS] The Wall Street Journal reports Apple plans..."
  - AFTER: "The Wall Street Journal reports Apple plans..." (note: no "EVENT:" prefix, no [NEWS] label)
* For [RUMOR/LEAK] sources: MUST include clear uncertainty qualifiers
  - BEFORE: "EVENT: [RUMOR/LEAK] Apple reportedly plans..."
  - AFTER: "Apple reportedly plans..." (note: no "EVENT:" prefix, no [RUMOR/LEAK] label)
* For [ANALYSIS] sources: Indicate these are opinions or predictions
  - BEFORE: "EVENT: [ANALYSIS] Market analysts predict Bitcoin will..."
  - AFTER: "Market analysts predict Bitcoin will..." (note: no "EVENT:" prefix, no [ANALYSIS] label)
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

CRITICAL: PRESERVE FACTUAL ACCURACY AND SOURCE LABELS
* ALWAYS maintain the level of certainty indicated by the source labels ([OFFICIAL], [NEWS], [RUMOR/LEAK], [ANALYSIS])
* For [OFFICIAL] sources, you may present information confidently as confirmed fact
* For [NEWS] sources, maintain modest attribution (e.g., "as reported by")
* For [RUMOR/LEAK] sources, you MUST include qualifiers like "reportedly", "allegedly", "rumored", or "according to leaks"
* For [ANALYSIS] sources, indicate these are opinions/predictions with appropriate qualifiers
* If original mentions "leaks" or "rumors", your summary MUST include this qualification
* If original says "reportedly" or "allegedly", your summary MUST maintain these qualifiers
* NEVER present unconfirmed information as confirmed fact
* NEVER convert phrases like "according to sources" into definitive statements
* NEVER suggest a company officially announced something when article only mentions leaks/rumors
* Pay SPECIAL ATTENTION to rumors about Apple, Google, Microsoft and other tech companies
* Distinguish clearly between:
  - Official announcements ("Apple announced new iPhone features")
  - Credible reporting ("The Wall Street Journal reports that Apple will announce...")
  - Rumors/leaks ("Apple is reportedly planning..." or "According to leaks...")
  - Analyst predictions ("Analysts predict Apple will release...")
* Be explicit about the source of information in your summary

PROPER ATTRIBUTION EXAMPLES:

* INCORRECT: "Apple's new AR headset will launch next month with a $1,999 price tag." (presents [RUMOR/LEAK] as confirmed fact)
* CORRECT: "Apple reportedly plans to launch an AR headset next month, with sources suggesting a $1,999 price tag, according to Bloomberg." (maintains attribution and uncertainty from [RUMOR/LEAK] label)

* INCORRECT: "Google's Pixel 8 includes advanced AI features for photo editing." (when based on leaks/rumors)
* CORRECT: "Google's upcoming Pixel 8 will reportedly include advanced AI features for photo editing, according to leaked specifications." (maintains attribution)

* INCORRECT: "Microsoft is releasing Windows updates to improve security." (when based on analyst speculation)
* CORRECT: "Security analysts expect Microsoft to release Windows updates addressing recent vulnerabilities, though no official announcement has been made." (proper attribution)

* INCORRECT: "EVENT: Apple details foldable iPhone specs." (includes "EVENT:" prefix)
* INCORRECT: "[OFFICIAL] Apple announced new features." (includes source label)
* INCORRECT: "EVENT: [RUMOR/LEAK] Apple reportedly plans..." (includes both prefix and label)
* CORRECT: "Apple's foldable iPhone specs were detailed in recent leaks. The device may feature a 7.6-inch display." (no "EVENT:" prefix, no source label, maintains uncertainty)

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

**SOURCE-SPECIFIC SUMMARY EXAMPLES:**

**[OFFICIAL] SOURCE EXAMPLE - ORIGINAL BULLETS:**
- EVENT: [OFFICIAL] Microsoft announced new AI features for Office 365 on March 15, 2025.
- The update includes integration with GPT-6, allowing real-time document summarization and smart content suggestions for users across all pricing tiers.
- CONTEXT: [NEWS] This release comes amid increasing competition in the productivity software market, as reported by CNBC.

**[OFFICIAL] SOURCE EXAMPLE - CORRECT TINY SUMMARY (note: no "EVENT:" or "CONTEXT:" prefixes, no source labels):**
"Microsoft announced new AI features for Office 365 on March 15, 2025, including GPT-6 integration for document summarization. The update offers smart content suggestions for users across all pricing tiers amid increasing competition in the productivity software market."

**[RUMOR/LEAK] SOURCE EXAMPLE - ORIGINAL BULLETS:**
- EVENT: [RUMOR/LEAK] Apple reportedly plans to release a foldable iPhone in 2026, according to supply chain sources cited by Bloomberg.
- The device is rumored to feature a 7.6-inch flexible display when unfolded and may be priced starting at $1,999.
- CONTEXT: [NEWS] This would represent Apple's response to Samsung's dominance in the foldable phone market, which currently holds 70% market share.

**[RUMOR/LEAK] SOURCE EXAMPLE - CORRECT TINY SUMMARY (note: no "EVENT:" or "CONTEXT:" prefixes, no source labels):**
"Apple reportedly plans to release a foldable iPhone in 2026, according to supply chain sources cited by Bloomberg. The device is rumored to feature a 7.6-inch flexible display and may be priced around $1,999 to compete with Samsung's 70% dominance in the foldable market."

**[NEWS] SOURCE EXAMPLE - ORIGINAL BULLETS:**
- EVENT: [NEWS] The Wall Street Journal reports that Tesla is developing a new battery technology that could double vehicle range.
- According to the publication, the technology uses silicon-based anodes and could enter production within 18 months.
- CONTEXT: [ANALYSIS] Industry experts believe this advancement could significantly strengthen Tesla's competitive position against traditional automakers.

**[NEWS] SOURCE EXAMPLE - CORRECT TINY SUMMARY (note: no "EVENT:" or "CONTEXT:" prefixes, no source labels):**
"The Wall Street Journal reports that Tesla is developing a new battery technology with silicon-based anodes that could double vehicle range. According to the publication, this technology could enter production within 18 months, potentially strengthening Tesla's position against traditional automakers."

**[ANALYSIS] SOURCE EXAMPLE - ORIGINAL BULLETS:**
- EVENT: [ANALYSIS] Cryptocurrency analysts at Goldman Sachs predict Bitcoin will reach $100,000 by end of 2025.
- Their forecast is based on institutional adoption trends and decreasing volatility metrics observed over the past three quarters.
- CONTEXT: [NEWS] This projection comes as several major banks have launched Bitcoin ETF products, as reported by Financial Times.

**[ANALYSIS] SOURCE EXAMPLE - CORRECT TINY SUMMARY (note: no "EVENT:" or "CONTEXT:" prefixes, no source labels):**
"Cryptocurrency analysts at Goldman Sachs predict Bitcoin will reach $100,000 by the end of 2025, based on institutional adoption trends and decreasing volatility. This projection comes as several major banks have launched Bitcoin ETF products, according to Financial Times."

**INCORRECT CONVERSION EXAMPLES TO AVOID:**

* "EVENT: Apple reportedly plans to release a foldable iPhone." (WRONG - includes "EVENT:" prefix)
* "[RUMOR/LEAK] Apple reportedly plans to release a foldable iPhone." (WRONG - includes source label)
* "CONTEXT: This would represent Apple's response to Samsung's dominance." (WRONG - includes "CONTEXT:" prefix)
* "Apple will release a foldable iPhone in 2026 with a 7.6-inch display priced at $1,999." (WRONG - removes uncertainty qualifiers)
* "Tesla is developing battery technology that doubles vehicle range and will enter production within 18 months." (WRONG - removes attribution to WSJ)
* "Bitcoin will reach $100,000 by end of 2025 due to institutional adoption and decreasing volatility." (WRONG - presents prediction as fact)
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

* HIGHEST PRIORITY - RUMOR/LEAK ACCURACY:
  - If the summary contains [RUMOR/LEAK], your title MUST CLEARLY indicate this is unconfirmed information
  - NEVER use these verbs with [RUMOR/LEAK] content: "Unveils", "Announces", "Launches", "Reveals", "Introduces", "Releases", "Confirms"
  - MANDATORY: For [RUMOR/LEAK] source about future products/features, use ONLY these patterns:
    * "Rumored [Feature]" (e.g., "Rumored iPhone AI Features")
    * "[Feature] Reportedly Coming" (e.g., "Battery AI Feature Reportedly Coming")
    * "Leak Suggests [Feature]" (e.g., "Leak Suggests iPhone Battery AI")
    * "Report: [Company] Planning [Feature]" (e.g., "Report: Apple Planning Battery AI")
  
  - DIRECT EXAMPLES OF THE PROBLEM TO AVOID:
    * [RUMOR/LEAK] summary about "Apple reportedly working on AI battery feature"
      - BAD: "Apple Unveils AI Battery Feature" (WRONG - implies official announcement)
      - BAD: "New iPhone Battery Features Coming" (WRONG - presents as confirmed)
      - GOOD: "Rumored iPhone Battery AI Feature" (correct - indicates rumor status)
      - GOOD: "Apple Battery AI Feature Reportedly Coming" (correct - indicates rumor status)

* CRITICAL DISTINCTION - NEWS vs. RUMORS:
  - ONLY use "Rumored", "Reportedly", "Unconfirmed", or "Leak Suggests" when the summary EXPLICITLY contains [RUMOR/LEAK]
  - NEVER add "Rumored" or "Unconfirmed" to news from [NEWS] or [OFFICIAL] sources
  - [NEWS] source means the information is reported by credible news outlets, NOT that it's a rumor
  - News about future plans from [NEWS] sources is still NEWS, not rumors
  
  - DIRECT EXAMPLES OF [NEWS] vs [RUMOR/LEAK]:
    * [NEWS] "Russian President Putin approved delegation for Ukraine negotiations"
      - GOOD: "Russia-Ukraine Talks Set" (correct - treated as news)
      - BAD: "Rumored Russia-Ukraine Talks" (WRONG - falsely implies uncertainty)
    
    * [NEWS] "Donald Trump met with Saudi Crown Prince in Riyadh"
      - GOOD: "Trump Saudi Meeting Held" (correct - treated as news)
      - BAD: "Rumored Trump Saudi Meeting" (WRONG - falsely implies uncertainty)
      
    * [NEWS] "Rep. Thanedar introduced impeachment articles, according to AP"
      - GOOD: "Impeachment Articles Introduced" (correct - treated as news)
      - BAD: "Impeachment Effort Unconfirmed" (WRONG - falsely implies uncertainty)

* CRITICAL - PRESERVING CRITICISM CORRECTLY:
  - When the summary mentions criticism about the "lack of" something positive (depth, quality, originality, etc.):
    * NEVER drop the "lack of" qualifier in the title
    * ALWAYS preserve the negative framing in the title
    * Use phrases like "Lacks Depth" or "Criticized for Lacking Depth" instead of just "Criticized for Depth"
    
  - DIRECT EXAMPLES OF CRITICISM PHRASING:
    * Summary: "criticized for lack of artistic depth"
      - BAD: "Criticized for Depth" (WRONG - this inverts the meaning to suggest having TOO MUCH depth)
      - GOOD: "Criticized for Lacking Depth" (correct - preserves negative framing)
      - GOOD: "Tour Lacks Depth, Critics Say" (correct - clearly indicates the missing quality)
    
    * Summary: "review noted poor choreography"
      - BAD: "Noted for Choreography" (WRONG - sounds positive)
      - GOOD: "Poor Choreography in Tour" (correct - preserves the negative assessment)
      
  - OTHER NEGATION PHRASES TO PRESERVE:
    * "insufficient", "poor", "weak", "inadequate", "deficient", "missing", "absence of"
    * Never drop these qualifiers when they modify criticized elements

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
- The summary will contain source labels ([OFFICIAL], [NEWS], [RUMOR/LEAK], [ANALYSIS])
- DO NOT include "EVENT:" or "CONTEXT:" prefixes in your title
- DO NOT include [OFFICIAL], [NEWS], [RUMOR/LEAK], or [ANALYSIS] labels in your title
- However, your title MUST reflect the appropriate level of certainty based on these source labels
- For [OFFICIAL] sources, you may use definitive verbs like "Announces", "Launches", "Releases"
- For [RUMOR/LEAK] sources:
  - Your title MUST use explicit rumor-indicating terms such as "Rumored", "Reportedly", "Leak Suggests"
  - NEVER use action verbs that imply confirmation like "Unveils", "Announces", "Launches"
  - ALWAYS make it clear the information is unconfirmed
  - Good: *iPhone Specs Leaked Online* or *Rumored iPhone Specs Surface* or *Report: Apple AI Feature Coming*
  - Bad: *Apple Announces iPhone Specs* (when it's only a [RUMOR/LEAK])
  - Bad: *Apple Unveils New Feature* (when it's only a [RUMOR/LEAK])
- For [NEWS] sources, indicate it's reporting if not a direct confirmation
  - Good: *WSJ Reports Tesla Expansion*
  - Bad: *Tesla Expands to New Markets* (when it's just a news report)
- For [ANALYSIS] sources, indicate it's an opinion or prediction
  - Good: *Analyst Predicts Tesla Expansion*
  - Bad: *Tesla Expands to New Markets* (when it's just an analysis)
- Use verbs that accurately reflect the level of certainty
  - For confirmed [OFFICIAL] actions: "Announces", "Launches", "Releases"
  - For [RUMOR/LEAK]: "Reportedly", "Allegedly", "Rumored to", "Leaks Suggest"
- Pay SPECIAL ATTENTION to tech companies (Apple, Google, Microsoft, etc.):
  - For [OFFICIAL] sources:
    * Good: *Apple Launches AR Headset*
    * Bad: *Apple Rumored to Launch AR Headset* (when it's an official announcement)
  - For [RUMOR/LEAK] sources:
    * Good: *Apple AR Headset Rumored*
    * Bad: *Apple Launches AR Headset* (when only rumored)
  - For [NEWS] sources:
    * Good: *Publication Reports iPhone Features*
    * Bad: *iPhone Gets New Features* (when just reported, not announced)
  - For [ANALYSIS] sources:
    * Good: *Analysts Predict iPhone Features*
    * Bad: *iPhone Gets New Features* (when just predicted)
- ALWAYS check the source label in the summary to determine certainty level:
  - [OFFICIAL]: Direct announcements from the company, confirmed future events with specific dates, product release announcements from official sources
  - [NEWS]: Credible reporting from established publications
  - [RUMOR/LEAK]: ONLY genuinely unconfirmed information from unofficial sources, information lacking official confirmation
  - [ANALYSIS]: Expert opinions, predictions, and analysis
- For articles about price drops, discounts, or sales:
  - Add "Sale:" prefix if the article is primarily about a promotional discount
  - Example: *Sale: iPad Prices Reduced*

* For multi-topic articles:
- Use broader encompassing terms (e.g., "Global Weekly Developments")
- Focus on the common thread if it exists
- Indicate time period if relevant
* Maintains clarity and accuracy
* Avoids clickbait or sensationalism
* NEVER use quotation marks around the entire title
* NEVER put the title in quotes for style or emphasis
* Only use quotation marks for actual quotes within a title
* RETURN EXACTLY ONE TITLE, regardless of topic count

**EXAMPLES (Single Topic by Source Type):**
"Trump Called Zelensky a Dictator" (for [OFFICIAL] source - uses confident language, no source label)
"WSJ Reports Border Agreement" (for [NEWS] source - attributes to publication, no source label)
"iPhone Features Reportedly Leaked" (for [RUMOR/LEAK] source - includes uncertainty qualifier, no source label)
"Rumored iPhone Battery Feature" (for [RUMOR/LEAK] source - clearly indicates rumor status)
"Leak Suggests Apple AI Plans" (for [RUMOR/LEAK] source - clearly indicates leak status)
"Analysts Predict Market Downturn" (for [ANALYSIS] source - indicates it's a prediction, no source label)
"Apple Products' Prices Reduced" (for sales)

**INCORRECT TITLE EXAMPLES TO AVOID:**
"EVENT: Trump Called Zelensky" (WRONG - includes "EVENT:" prefix)
"[OFFICIAL] Trump Called Zelensky" (WRONG - includes source label)
"CONTEXT: Tensions Between Countries" (WRONG - includes "CONTEXT:" prefix)
"iPhone 16 Will Have AI" (WRONG - presents [RUMOR/LEAK] as definite fact)
"Apple Unveils AI Battery Feature" (WRONG - presents [RUMOR/LEAK] as confirmed announcement)
"New iPhone Features Coming" (WRONG - presents [RUMOR/LEAK] as confirmed fact without indicating uncertainty)
"Rumored Official Announcement" (WRONG - contradicts itself by labeling confirmed news as a rumor)
"Unconfirmed AP Report" (WRONG - contradicts itself by labeling a credible news report as unconfirmed)
"\"Trump Saudi Meeting\"" (WRONG - puts entire title in quotation marks)

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
