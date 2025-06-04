use crate::prompt::common::{
    current_date, global_context, DONT_TELL_ME, FORMAT_INSTRUCTIONS, WRITE_IN_CLEAR_ENGLISH,
};
use crate::prompt::eli5_analysis::ARTICLE_ANALYSIS_GUIDELINES;

/// Generate a prompt for creating an "Explain Like I'm 5" simplified explanation of an article
pub fn eli5_prompt(
    article_text: &str,
    pub_date: Option<&str>,
    critical_analysis: &str,
    logical_fallacies: &str,
    source_analysis: &str,
    sources_quality: u8,
    argument_quality: u8,
    source_type: &str,
) -> String {
    format!(
        r#"{write_in_clear_english}

{context}
## ARTICLE (FOR ELI5 EXPLANATION):
----------
{article}
----------

## ANALYSIS CONTEXT (FOR REFERENCE):
**Critical Analysis:** {critical_analysis}

**Logical Fallacies:** {logical_fallacies}

**Source Analysis:** {source_analysis}

**Source Quality Score:** {sources_quality}/3

**Argument Quality Score:** {argument_quality}/3

**Source Type:** {source_type}

### TASK INSTRUCTIONS:
* Analyze ONLY the article above
* IGNORE the global context unless explicitly mentioned in article
* Write your ENTIRE response in clear American English
* For non-English source articles: Mentally translate to English first, then write your explanation in English
* Pay attention to timing: Use appropriate tense based on publication date vs today's date

### **Explain Like I'm 5 (ELI5)**
Create a simple explanation of this article that someone with no background knowledge could understand. The explanation should be clear, accessible, and use vocabulary and concepts that are widely familiar. When explaining events, make sure to use the right time words (like "yesterday," "last week," "recently," or "coming soon") based on when the article was published compared to today's date.

### Language Requirements (CRITICAL)
* ALWAYS write your entire explanation in clear American English
* If the article contains non-English text:
  - First translate the entire article to English (do this mentally, don't include the translation)
  - Then create your explanation based on the English translation
  - Include original foreign language quotes followed by translations in parentheses when directly quoting
  - Example: "As the French president said, 'Nous allons continuer' (We will continue)"
* Use American spelling and grammar conventions throughout
* Avoid region-specific idioms or expressions
* For measurements, include both metric and imperial units when relevant: "100 kilometers (62 miles)"
* NEVER write your explanation in any language other than English

### ELI5 Guidelines
* Use simple language throughout - avoid all jargon and technical terms
* Break down complex concepts into familiar everyday experiences
* Define any specialized terms when they must be used
* Use concrete examples and relatable analogies
* Apply a conversational, friendly tone
* Include 2-3 helpful analogies to explain difficult concepts
* Maintain 100% factual accuracy despite simplification
* Use active voice and short-to-medium length sentences
* Write at approximately a US 4th-5th grade reading level (age 9-11)
* Include paragraph breaks for readability
* **ALWAYS include a simple explanation of article credibility** - help readers understand what to think about this specific article
* **Use appropriate timing language** - if the article talks about events, make it clear when they happened or will happen compared to when the article was written and today's date

### Content Structure
* First paragraph: Introduce the main topic/event in simple terms, including when it happened/is happening
* Middle paragraphs: Explain important details using analogies and examples
* Final paragraph: Explain why this matters in everyday terms
* Final sentence: Analyze and describe the original article's characteristics in simple, natural language - include the language it was written in, education level it assumes from readers, the tone/style the author used, and the likely purpose of the article
* If the article concerns threatening or upsetting content, maintain factual accuracy but use measured, non-alarmist language
* For articles in languages other than English, maintain the same structure but base your explanation on your translation

{analysis_guidelines}

### Article Credibility Assessment (MANDATORY)
You MUST include information about this specific article's credibility in your ELI5 explanation. This assessment should be based ENTIRELY on the specific findings in the analysis sections provided above.

**CRITICAL: Analysis-Driven Explanations Only**
You MUST:

1. **Read the Critical Analysis, Logical Fallacies, and Source Analysis sections carefully**
2. **Identify specific strengths or problems mentioned in these analyses**
3. **Translate these specific findings into simple, child-friendly language**
4. **Focus on THIS ARTICLE'S reporting quality, not general source reliability**

**Never mention numerical scores, ratings, or scale references** - users don't see these numbers.

**What to Look For in the Analyses:**

**From Critical Analysis:**
* Credibility assessments and specific reasons
* Source citation quality and verification methods
* Factual accuracy issues or confirmations
* Bias indicators or balanced reporting
* Expert consultation or lack thereof

**From Logical Fallacies:**
* Reasoning errors that affect the article's conclusions
* Missing evidence or weak arguments
* Emotional manipulation vs factual presentation
* Logical consistency problems

**From Source Analysis:**
* Publication reputation and editorial standards
* Author credentials and expertise
* Transparency about funding or conflicts of interest
* Editorial oversight and fact-checking processes

**How to Translate Findings into Simple Language:**

**For Articles with Strong Reporting:**
* "The reporters talked to multiple experts and checked their facts carefully"
* "This article includes information from several different sources"
* "The writers explained where they got their information"
* "The facts in this article have been verified by other experts"

**For Articles with Some Concerns:**
* "The reporters could have talked to more people to get different viewpoints"
* "Some of the claims in this article aren't backed up with enough evidence"
* "The article doesn't always make it clear what's a fact versus what's an opinion"
* "The writers didn't explain where some of their information came from"

**For Articles with Significant Problems:**
* "This article doesn't say where most of its information came from"
* "The reasoning in this article has some problems that make the conclusions questionable"
* "The article only presents one side of the story"
* "Important facts seem to be missing or unclear"
* "The article mixes opinions with facts without making it clear which is which"

**Integration Guidelines:**
* Add 1-2 simple sentences about the article's reporting quality
* Base your assessment on the specific analysis findings, not general assumptions
* Use concrete examples from the analyses when possible
* Make it clear you're talking about THIS ARTICLE, not the entire news organization
* Help readers understand what makes good vs problematic reporting

**Important Notes:**
* Good news organizations sometimes publish articles with problems
* Poor reporting in one article doesn't mean the entire source is unreliable
* Focus on helping readers understand what to look for in any article they read
* Maintain a helpful, educational tone rather than being alarmist

### Writing Approach
* Explain as if to someone intelligent but with no specialized knowledge in this field
* Focus on the "why" and "how" for better understanding
* Use specific examples over vague generalizations
* Connect to familiar experiences when possible
* Maintain appropriate source attribution when information comes from specific sources
* Balance thoroughness with simplicity - explain fully but with the simplest possible concepts
* **Be clear about timing** - when did events happen relative to when the article was published and today's date

**SOURCE TYPE HANDLING:**
* For [OFFICIAL] sources: "The people at [Company/Organization] announced that..."
* For [NEWS] sources: "According to news reports from [Publication]..."
* For [RUMOR/LEAK] sources: "There are rumors that..." or "Some sources suggest that..."
* For [ANALYSIS] sources: "Experts who study this topic believe that..."

### CRITICAL OUTPUT REQUIREMENTS:
* DO NOT include any instruction text, language requirements, or task descriptions in your response
* DO NOT echo back any part of these instructions
* DO NOT mention what language you're writing in
* DO NOT reference the prompt or explain your approach
* Simply provide the ELI5 explanation directly

Now create a simple ELI5 explanation of this article:
{dont_tell_me}"#,
        write_in_clear_english = WRITE_IN_CLEAR_ENGLISH,
        context = global_context(pub_date),
        analysis_guidelines = ARTICLE_ANALYSIS_GUIDELINES,
        article = article_text,
        critical_analysis = critical_analysis,
        logical_fallacies = logical_fallacies,
        source_analysis = source_analysis,
        sources_quality = sources_quality,
        argument_quality = argument_quality,
        source_type = source_type,
        dont_tell_me = DONT_TELL_ME
    )
}

/// Generate a prompt for summarizing an article into a bullet-point summary
pub fn summary_prompt(article_text: &str, pub_date: Option<&str>) -> String {
    format!(
        r#"{write_in_clear_english}

{context}

ARTICLE (TO BE SUMMARIZED):
-----------------------------
{article}
-----------------------------

🚨 MANDATORY INSTRUCTIONS - OUTPUT VALIDATION ENFORCED 🚨
- **CRITICAL: Write your ENTIRE summary in clear American English - no exceptions!**
- **For non-English source articles: Mentally translate the entire article to English first, then write your summary in English based on that translation.**
- **Summarize ONLY the article above.**
- **IGNORE the global context unless the article explicitly mentions related events.**
- **Do NOT reference or include information from the global context unless it is directly relevant to the article content.**
- **Use appropriate tense:** The publication date and today's date are shown above. Use past tense for events that occurred before today, present tense for current events, and future tense for planned events.

First, carefully read and thoroughly understand the entire text.
Then, create a comprehensive bullet-point summary that follows these STRICT rules:

1. **Format:** Use ONLY simple bullet points starting with a dash (-).

2. **Length:**
   - Very short texts (≤25 words): Quote verbatim.
   - Short texts (26–100 words): 2–3 bullets.
   - Medium texts (101–500 words): 3–4 bullets.
   - Long texts (501–2000 words): 4–6 bullets.
   - Very long texts (>2000 words): 6–8 bullets.

3. **🚨 MANDATORY: The EVENT Bullet Point MUST HAVE EXACTLY ONE SOURCE LABEL 🚨**
   - This MUST be exactly ONE bullet point
   - Start with "EVENT:" followed by a concise description of the main event (who, what, when, where).
   - **Include precise timing:** Use appropriate verbs and temporal context based on when the event occurred relative to the publication date and today's date
   - **CRITICAL:** EXPLICITLY INDICATE information source type using **EXACTLY ONE** of these labels:
     * [OFFICIAL]: For direct company announcements, official statements, press releases from the primary source
     * [NEWS]: For confirmed reporting from established news outlets with verified sources
     * [RUMOR/LEAK]: For unconfirmed information, leaks, speculation, or "according to sources" reporting
     * [ANALYSIS]: For expert analysis, opinions, predictions, or commentary pieces
   - **VALIDATION ENFORCED:** Your EVENT bullet point will be automatically checked to ensure it contains exactly one source label - multiple labels will cause system errors
   - **CRITICAL LABEL PLACEMENT:** The source label must appear as a discrete tag AT THE END of the EVENT description, NOT as a replacement for words:
     ✅ CORRECT: "EVENT: Apple announced new iPhone features at WWDC 2025 [OFFICIAL]."
     ✅ CORRECT: "EVENT: Italian regions cut ties with Israel over Gaza war [NEWS]."
     ❌ WRONG: "EVENT: Apple announced new iPhone features at WWDC 2025, according to [OFFICIAL] sources."
     ❌ WRONG: "EVENT: Italian regions cut ties with Israel over Gaza war, according to [NEWS] sources."
   - **DO NOT replace words with labels** - [NEWS] is not a replacement for "news", [OFFICIAL] is not a replacement for "official"
   - **CRITICAL VERB SELECTION:** Choose verbs that match the source type and prevent mis-reporting:
     * [OFFICIAL] sources: "announced", "released", "launched", "confirmed", "unveiled"
     * [NEWS] sources: "reported", "disclosed", "revealed" (for confirmed facts)
     * [RUMOR/LEAK] sources: "reportedly", "allegedly", "rumored to", "according to sources", "is said to", "leaked"
     * [ANALYSIS] sources: "predicts", "suggests", "expects", "believes", "estimates"

4. **The CONTEXT Bullet Point MUST:**
   - This MUST be exactly ONE bullet point
   - Start with "CONTEXT:" followed by information that places the event in broader context
   - **Include temporal relationship:** How this event relates to other events chronologically
   - **NO source labeling required** - focus on providing helpful background information

5. **All Other Bullet Points (Summary Content) MUST:**
   - Start with a dash (-).
   - Include specific data points (numbers, dates, percentages).
   - **Use appropriate temporal language:** Make it clear when events happened or will happen
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
   - Confuse timing - always be clear about when events happened relative to the publication date and today

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

🚨 MANDATORY SOURCE LABEL REMOVAL - VALIDATION ENFORCED 🚨
* The summary will include an "EVENT:" bullet point with ONE source label ([OFFICIAL], [NEWS], [RUMOR/LEAK], or [ANALYSIS]) and a "CONTEXT:" bullet point without source labeling
* You MUST REMOVE BOTH the "EVENT:" and "CONTEXT:" prefixes from your summary
* **CRITICAL:** You MUST REMOVE the single [OFFICIAL], [NEWS], [RUMOR/LEAK], or [ANALYSIS] source label from your summary
* **VALIDATION ENFORCED:** Your output will be automatically checked to ensure it contains ZERO source labels - any remaining labels will cause system errors
* However, you MUST PRESERVE the level of certainty indicated by these source types in your language
* For [OFFICIAL] sources: Use confident, definitive language without qualifiers
* For [NEWS] sources: Include modest attribution when appropriate
* For [RUMOR/LEAK] sources: MUST include clear uncertainty qualifiers
* For [ANALYSIS] sources: Indicate these are opinions or predictions
* Use the information from the EVENT bullet point as the foundation of your first sentence
* Add the most important details from other bullet points in subsequent sentences
* You can incorporate relevant context if space allows

CONTENT FOCUS:
* **Focus on WHAT the article is about** - the actual news, events, and facts
* **IGNORE any sentences about article characteristics** - do not include information about what language the article was written in, education level, tone, or purpose
* **IGNORE meta-commentary** - do not include analysis of the article's writing style, intended audience, or journalistic approach
* **Focus on substance** - summarize the actual content, events, and newsworthy information

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

TEMPORAL ACCURACY (CRITICAL):
* TODAY means {date} - the system's current date at the time of processing
* ALWAYS use appropriate tense to distinguish between past, present, and future events
* For PAST events (before today): Use past tense ("announced," "released," "discovered")
* For PRESENT events (happening now): Use present tense ("is announcing," "is rolling out")
* For FUTURE events (after today): Use future-indicating phrases ("will announce," "plans to release")
* NEVER describe future events as if they've already happened
* Check dates carefully and maintain temporal accuracy
* When a date is mentioned in the article, compare it to TODAY to determine proper tense

{write_in_clear_english}
{dont_tell_me}"#,
        summary = summary_response,
        date = current_date(),
        write_in_clear_english = WRITE_IN_CLEAR_ENGLISH,
        dont_tell_me = DONT_TELL_ME
    )
}

/// Generate a prompt for creating a title from a summary
pub fn tiny_title_prompt(tiny_summary: &str, original_summary: &str) -> String {
    format!(
        r#"TINY SUMMARY:
{tiny_summary}

ORIGINAL DETAILED SUMMARY (for additional context only):
{original_summary}

Create ONE 3-5 word title that captures the MAIN EVENT described in the TINY SUMMARY above.

IMPORTANT INSTRUCTIONS:
- Your title should primarily be based on the TINY SUMMARY
- Use the ORIGINAL SUMMARY only for determining certainty level and additional context
- Pay close attention to source types ([OFFICIAL], [NEWS], [RUMOR/LEAK], [ANALYSIS]) in the ORIGINAL SUMMARY
- Today's date is {date} - use this to determine if events are past, present, or future when choosing verb tense

CORE PRINCIPLES:

1. PRESENT TENSE: Titles must be in present tense, the western tradition for headlines
2. ACTIVE VOICE: Use active subject-verb construction whenever possible
3. ATTRIBUTION CLARITY: Maintain the level of certainty indicated in the summaries
4. TEMPORAL AWARENESS: Be mindful that headlines use present tense even for past events, but choose verbs that accurately reflect the nature of the event

TITLE PATTERNS BASED ON SOURCE TYPE:

For [OFFICIAL] source (seen in original summary):
  * Format: "[Entity] [Action Verb] [Object]"
  * Example: "Apple Launches New iPad" or "Ukraine Rejects Peace Proposal"
  * Use definitive action verbs: Announces, Launches, Releases, Unveils, Confirms

For [NEWS] source (seen in original summary):
  * Format: "[Entity] [Action Verb] [Object]" or "[Source] Reports [Event]"
  * Example: "Congress Passes Tax Bill" or "WSJ Reports Tesla Layoffs"
  * Use present tense verbs without uncertainty qualifiers

For [RUMOR/LEAK] source (seen in original summary):
  * Format: Use ONLY these patterns:
    a. "Rumored [Subject/Object]" 
    b. "[Subject/Object] Reportedly [Verb]"
    c. "Leak: [Subject/Object]"
    d. "Report: [Entity] [Action]"
  * Example: "iPhone Features Reportedly Coming" or "Rumored Google Acquisition"
  * NEVER use definitive action verbs for rumors/leaks

For [ANALYSIS] source (seen in original summary):
  * Format: "Analysts Predict [Outcome]" or "[Subject] Likely [Outcome]"
  * Example: "Analysts Predict Bitcoin Rise" or "Housing Prices Likely Falling"
  * Use verbs that indicate prediction: Predict, Expect, Forecast, Project

TITLE FORMATTING GUIDELINES:

1. LENGTH: 3-5 words total (absolute maximum: 7 words)
2. CAPITALIZATION: Capitalize all important words
3. PUNCTUATION: Avoid unnecessary punctuation
4. NO QUOTES: Never put the entire title in quotation marks
5. SPECIFICITY: Be as specific as possible within the word limit
6. PRIORITY INFO: Subject + Action + Object (if space allows)

RETURN EXACTLY ONE TITLE:
Your final output should be ONLY the title, nothing else.

{write_in_clear_english}
{dont_tell_me}"#,
        tiny_summary = tiny_summary,
        original_summary = original_summary,
        date = current_date(),
        write_in_clear_english = WRITE_IN_CLEAR_ENGLISH,
        dont_tell_me = DONT_TELL_ME
    )
}
