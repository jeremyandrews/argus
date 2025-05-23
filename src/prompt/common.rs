use chrono::Local;

// Common text blocks for all prompts
pub const DONT_TELL_ME: &str = r#"
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

pub const FORMAT_INSTRUCTIONS: &str = r#"
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

pub const WRITE_IN_CLEAR_ENGLISH: &str = r#"
Language Standards for Output:
1. Write all content in clear American English, using American spelling and grammar.
2. For non-English content:
   - ALWAYS include both original text and translation
   - Format as: "original text (translation)"
   - For titles: Keep original, add translation in parentheses
   - For names: Do not translate as they are names
   - Never translate if the translation is the same as the original.
   - Only translate from Foreign → American English.
   Example: "La vita è bella (Life is Beautiful)" and "Ne Zha 2 (No translation: this is a person's name)"
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

pub const CONTEXT: &str = "
In Q1 2024, BRICS expanded, shifting global economic power, while record temperatures highlighted climate concerns. Japan's 7.6 earthquake and U.S. winter storms exposed vulnerabilities. France enshrined abortion rights, Sweden joined NATO, and the U.S. Supreme Court ruled on key legal precedents. Major wildfires and geopolitical tensions added to global challenges.
In Q2 2024, a solar eclipse captivated North America as record heatwaves and severe floods underscored climate urgency. Trump's trial and free speech protests stirred U.S. discourse. Putin's fifth term, Xi's European visit, and G7's $50B Ukraine aid shaped geopolitics. Apple's AI integration marked tech innovation.
In Q3 2024, the Paris Olympics fostered unity amidst record-breaking heatwaves and escalating Gaza tensions. Biden withdrew from the presidential race, endorsing Kamala Harris. The UN's 'Pact for the Future' and a historic face transplant marked milestones. Hurricane Helene and mpox emphasized urgent global challenges.
In Q4 2024, Trump's re-election and U.S. economic growth highlighted domestic shifts. Hurricane Helene devastated the Gulf Coast, while 2024 set a record as the hottest year. South Korea's political turmoil and Assad's overthrow reshaped global dynamics. The Notre-Dame reopening symbolized cultural resilience.
- In January 2025, Donald Trump was inaugurated as the 47th U.S. President and issued significant executive orders affecting trade and international relations. The month also recorded the warmest January globally, highlighting climate concerns. A ceasefire was reached in the Israel-Hamas conflict, and Canadian Prime Minister Justin Trudeau resigned amid a political crisis. Trump's actions included imposing tariffs on Mexico, China, and Canada, withdrawing the U.S. from the World Health Organization, and defunding the UN agency for Palestinian refugees, signaling a shift toward protectionism and unilateral foreign policy.
- In February 2025, Trump's sweeping tariffs sparked global retaliation including from China, the EU, Canada, and Mexico, igniting a trade war. The U.S. restored ties with Russia, but relations with Ukraine frayed. America pledged to oversee Gaza's rebuilding. Sea ice hit record lows. The Baltics cut energy ties to Russia. Nicaragua shifted to a co-presidency. Germany's election shifted right, and global trade tensions surged.
- In March 2025, the Syrian civil war intensified with mass civilian casualties, while Ukraine and Russia engaged in Black Sea ceasefire talks. The U.S. faced a historic tornado outbreak causing significant damage. Europe experienced a marine heatwave threatening ecosystems, and President Trump in the U.S. imposed new trade tariffs, escalating global economic tensions.
- In April 2025, Pope Francis's death prompted global mourning. India and Pakistan's conflict escalated post-Kashmir attack. Gaza faced a dire humanitarian crisis. Trump's abrupt tariffs shook global markets. A massive blackout hit Spain and Portugal, causing widespread disruption.
- In May 2025, global attention focused on escalating conflicts in Gaza and Syria's challenging transition post-Assad. Germany's NATO deployment marked a significant shift in European defense. Domestically, the U.S. grappled with the implications of a sweeping tax bill, Trump's 'One Big Beautiful Bill Act'. Additionally, the Catholic Church witnessed a historic moment with the election of Pope Leo XIV, the first American pontiff, signaling a new era in its global leadership.
";

/// Utility function to get the current date in a human-readable format
pub fn current_date() -> String {
    let today = Local::now();
    format!(
        "{} {}, {}",
        today.format("%B"),
        today.format("%-d"),
        today.format("%Y")
    )
}

/// Utility function to generate global context with optional publication date
pub fn global_context(pub_date: Option<&str>) -> String {
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
