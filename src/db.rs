use rusqlite::{params, Connection, Result};
use std::time::{SystemTime, UNIX_EPOCH};

pub struct Database {
    conn: Connection,
}

impl Database {
    pub fn new(database_url: &str) -> Result<Self> {
        let conn = Connection::open(database_url)?;

        conn.execute(
            r#"
            CREATE TABLE IF NOT EXISTS articles (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                url TEXT NOT NULL UNIQUE,
                seen_at TEXT NOT NULL,
                is_relevant BOOLEAN NOT NULL,
                category TEXT,
                analysis TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_relevant_category ON articles (is_relevant, category);
            "#,
            [],
        )?;

        Ok(Database { conn })
    }

    pub fn add_article(
        &self,
        url: &str,
        is_relevant: bool,
        category: Option<&str>,
        analysis: Option<&str>,
    ) -> Result<()> {
        let seen_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time travel)")
            .as_secs()
            .to_string();

        self.conn.execute(
            r#"
            INSERT INTO articles (url, seen_at, is_relevant, category, analysis)
            VALUES (?1, ?2, ?3, ?4, ?5)
            ON CONFLICT(url) DO UPDATE SET seen_at = excluded.seen_at, is_relevant = excluded.is_relevant, category = excluded.category, analysis = excluded.analysis
            "#,
            params![url, seen_at, is_relevant, category, analysis],
        )?;
        Ok(())
    }

    pub fn has_seen(&self, url: &str) -> Result<bool> {
        let mut stmt = self.conn.prepare("SELECT 1 FROM articles WHERE url = ?1")?;
        let mut rows = stmt.query(params![url])?;

        Ok(rows.next()?.is_some())
    }
}
