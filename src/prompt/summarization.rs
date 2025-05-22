use crate::prompt::common::{
    current_date, global_context, DONT_TELL_ME, FORMAT_INSTRUCTIONS, WRITE_IN_CLEAR_ENGLISH,
};

/// Generate a prompt for creating an "Explain Like I'm 5" simplified explanation of an article
pub fn eli5_prompt(article_text: &str, pub_date: Option<&str>) -> String {
    format!(
        r#"{context}
## ARTICLE (FOR ELI5 EXPLANATION):
----------
{article}
----------

IMPORTANT INSTRUCTIONS:
* **Analyze ONLY the article above.**
* **IGNORE the global context unless explicitly mentioned in article.**
* **For non-English text, include translations of relevant quotes.**

### **Explain Like I'm 5 (ELI5)**
Create a simple explanation of this article that someone with no background knowledge could understand. The explanation should be clear, accessible, and use vocabulary and concepts that are widely familiar.

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

### Content Structure
* First paragraph: Introduce the main topic/event in simple terms
* Middle paragraphs: Explain important details using analogies and examples
* Final paragraph: Explain why this matters in everyday terms
* If the article concerns threatening or upsetting content, maintain factual accuracy but use measured, non-alarmist language

### Writing Approach
* Explain as if to someone intelligent but with no specialized knowledge in this field
* Focus on the "why" and "how" for better understanding
* Use specific examples over vague generalizations
* Connect to familiar experiences when possible
* Maintain appropriate source attribution when information comes from specific sources
* Balance thoroughness with simplicity - explain fully but with the simplest possible concepts

**SOURCE TYPE HANDLING:**
* For [OFFICIAL] sources: "The people at [Company/Organization] announced that..."
* For [NEWS] sources: "According to news reports from [Publication]..."
* For [RUMOR/LEAK] sources: "There are rumors that..." or "Some sources suggest that..."
* For [ANALYSIS] sources: "Experts who study this topic believe that..."

**SUCCESSFUL EXAMPLES:**

**Example 1: Technology Article (Original topic: Advanced AI Model Release)**
"OpenAI just made a new AI helper called GPT-4 that's much better at understanding both text and pictures. Think of AI as a computer brain that can learn to do tasks by looking at lots of examples.

This new AI is like upgrading from a toy car to a real one. The old version (GPT-3) was already pretty smart - it could write stories, answer questions, and explain complicated topics in simple ways. But it sometimes made silly mistakes or got confused easily.

The new GPT-4 is much better at solving tricky problems. Imagine if your calculator could not just add numbers but also help with your homework, write your book reports, and explain why the sky is blue - all just by typing questions to it.

One of the biggest improvements is that GPT-4 can now understand pictures. If you show it a photo of what's in your refrigerator, it could suggest recipes you can make with those ingredients. This is a big deal because earlier versions could only work with text.

This matters because AI tools like GPT-4 are becoming part of everyday life - they help doctors diagnose illnesses, assist customer service agents, create art, and even help students learn. As these tools get better, they'll change how we work, learn, and solve problems in the future."

**Example 2: Business News (Original topic: Semiconductor Manufacturing Supply Chain)**
"Computer chips (also called semiconductors) are like the brains inside all our electronic devices - phones, laptops, cars, and even refrigerators. Most of these chips are made in just a few places in the world, especially Taiwan and South Korea.

The article explains that there's a big problem happening right now because not enough chips are being made to meet everyone's needs. It's like if there was suddenly not enough bread for everyone who wants sandwiches.

This chip shortage happened for several reasons. First, when COVID-19 hit, companies thought people would buy fewer electronics, so they ordered fewer chips. But the opposite happened - people stuck at home bought MORE computers and gadgets, not less!

At the same time, chip factories (called 'fabs') are extremely complicated to build. Imagine the most advanced factory you can think of, then multiply that by 100. They cost billions of dollars and take years to construct. So manufacturers can't just quickly make more chips when demand increases.

The shortage affects many things we buy. Car companies have had to stop making some vehicles because they can't get the chips that run everything from engines to entertainment systems. That's why some car prices have gone up and why it's harder to find certain models.

This matters to everyday people because it means electronics might cost more or be harder to find in stores. It also shows how connected our global economy is - problems in one part of the world can affect products everywhere else."

**Example 3: Scientific Research (Original topic: CRISPR Gene Editing Breakthrough)**
"Scientists have found a better way to use a tool called CRISPR, which lets them change the instruction manual inside living cells. Every living thing has DNA, which is like a cookbook with recipes that tell cells how to grow and work.

Sometimes, there are mistakes in this cookbook that can cause diseases. CRISPR works like a very tiny pair of scissors combined with a search function - it can find specific recipes (genes) and make precise changes to fix problems.

The big news in this article is that scientists made CRISPR much more accurate. Earlier versions sometimes made changes in the wrong places - imagine trying to fix a typo in a cookbook but accidentally changing instructions on a different page too! The improved method reduces these mistakes by about 80%.

To understand how impressive this is, think about performing surgery with a butter knife versus a precise scalpel. Both can cut, but the scalpel lets you be much more careful and exact. This new CRISPR technique is like upgrading from an okay tool to an excellent one.

In their experiments, scientists successfully corrected a genetic mutation that causes a blood disease called sickle cell anemia. They took cells from patients, fixed the genetic mistake, then put the healthy cells back - and the cells started making proper blood components.

This matters because many diseases are caused by problems in our DNA. Better gene editing tools could eventually help treat or cure conditions like cystic fibrosis, certain types of blindness, and even some cancers. However, there are still many steps before these treatments would be widely available to patients."

**UNSUCCESSFUL EXAMPLES (AVOID):**

1. Too Technical: "The CRISPR-Cas9 system's off-target effects were mitigated through modification of the guide RNA scaffold, resulting in an 80% reduction in non-specific endonuclease activity as measured by genome-wide sequencing."

2. Too Vague: "Scientists made a thing that edits genes better. It's more accurate now and helps with diseases. This is important for medicine."

3. Too Condescending: "Imagine DNA is like a book, but a really really complicated book that most people wouldn't understand. The scientists, who are very smart, figured out how to change words in this book!"

4. Not Factually Accurate: "The new CRISPR technique can now cure all genetic diseases with no risks, and doctors will start using it in all hospitals next month."

5. Too Abstract: "The paradigm of genetic intervention has shifted toward a more deterministic methodology, centralizing accuracy over throughput in the evolving narrative of biomedical applications."

Now create a simple ELI5 explanation of this article:
{write_in_clear_english}
{dont_tell_me}"#,
        context = global_context(pub_date),
        article = article_text,
        write_in_clear_english = WRITE_IN_CLEAR_ENGLISH,
        dont_tell_me = DONT_TELL_ME
    )
}

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

CORE PRINCIPLES:

1. PRESENT TENSE: Titles must be in present tense, the western tradition for headlines
   * Use "Apple Announces New iPhone" NOT "Apple Will Announce New iPhone" or "Apple Announced New iPhone"
   * For events that have already happened, still use present tense: "Russia Invades Ukraine" (even if it happened yesterday)

2. ACTIVE VOICE: Use active subject-verb construction whenever possible
   * "Company Launches Product" NOT "Product Launched by Company"

3. ATTRIBUTION CLARITY: Maintain the level of certainty indicated in the summaries
   * If the original summary shows [RUMOR/LEAK], your title MUST indicate uncertainty
   * If the original summary shows [OFFICIAL], you may use definitive language
   * If the tiny summary contains qualifiers like "reportedly" or "allegedly", preserve them

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

EXAMPLES:

Original summary contains [OFFICIAL]:
- EVENT: [OFFICIAL] Microsoft announced new AI features for Office 365 on March 15, 2025.
Tiny summary: "Microsoft announced new AI features for Office 365 on March 15, 2025, including GPT-6 integration."
TITLE: "Microsoft Announces Office AI" (definitive, present tense)

Original summary contains [RUMOR/LEAK]:
- EVENT: [RUMOR/LEAK] Apple reportedly plans to release a foldable iPhone in 2026, according to supply chain sources.
Tiny summary: "Apple reportedly plans to release a foldable iPhone in 2026, according to supply chain sources."
TITLE: "Apple Reportedly Planning Foldable" (preserves uncertainty)

Original summary contains [NEWS]:
- EVENT: [NEWS] The Wall Street Journal reports that Tesla will cut 10% of its workforce.
Tiny summary: "The Wall Street Journal reports Tesla plans to cut 10% of its workforce due to economic pressures."
TITLE: "WSJ Reports Tesla Layoffs" (attributes to source)

Original summary contains [ANALYSIS]:
- EVENT: [ANALYSIS] Cryptocurrency analysts predict Bitcoin will reach $100,000 by end of 2025.
Tiny summary: "Cryptocurrency analysts predict Bitcoin will reach $100,000 by the end of 2025, based on institutional adoption trends."
TITLE: "Analysts Predict Bitcoin Rise" (indicates prediction)

COMMON MISTAKES TO AVOID:

1. SOURCE TYPE ERRORS:
   • WRONG: "Apple Announces New Features" (when original summary has [RUMOR/LEAK])
   • CORRECT: "Apple Reportedly Planning Features" or "Rumored Apple Features"

2. TENSE ERRORS:
   • WRONG: "Company Will Launch Product" (future tense)
   • WRONG: "Company Launched Product" (past tense)
   • CORRECT: "Company Launches Product" (present tense)

3. ATTRIBUTION ERRORS:
   • WRONG: "Rumored Treaty Signing" (when original summary has [OFFICIAL] or [NEWS])
   • WRONG: "Apple Releases iPhone" (when original summary has [RUMOR/LEAK])
   • CORRECT: Match certainty level to the source type in original summary

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
        write_in_clear_english = WRITE_IN_CLEAR_ENGLISH,
        dont_tell_me = DONT_TELL_ME
    )
}
