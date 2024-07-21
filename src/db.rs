use rusqlite::{params, Connection, Result};
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{error, info, instrument};

use crate::TARGET_DB;

pub struct Database {
    conn: Connection,
}

impl Database {
    #[instrument(target = "db", level = "info", skip(database_url))]
    pub fn new(database_url: &str) -> Result<Self> {
        info!(target: TARGET_DB, "Opening connection to database: {}", database_url);
        match Connection::open(database_url) {
            Ok(conn) => {
                info!(target: TARGET_DB, "Creating articles table if not exists");
                if let Err(e) = conn.execute(
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
                ) {
                    error!(target: TARGET_DB, "Failed to create articles table: {:?}", e);
                    return Err(e);
                }
                info!(target: TARGET_DB, "Database setup complete");
                Ok(Database { conn })
            }
            Err(e) => {
                error!(target: TARGET_DB, "Failed to open connection to database: {:?}", e);
                Err(e)
            }
        }
    }

    #[instrument(target = "db", level = "info", skip(self, url, category, analysis))]
    pub fn add_article(
        &mut self,
        url: &str,
        is_relevant: bool,
        category: Option<&str>,
        analysis: Option<&str>,
    ) -> Result<()> {
        let seen_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time travel")
            .as_secs()
            .to_string();

        info!(target: TARGET_DB, "Starting transaction to add/update article: {}", url);
        let tx = match self.conn.transaction() {
            Ok(tx) => tx,
            Err(e) => {
                error!(target: TARGET_DB, "Failed to start transaction: {:?}", e);
                return Err(e);
            }
        };

        if let Err(e) = tx.execute(
            r#"
            INSERT INTO articles (url, seen_at, is_relevant, category, analysis)
            VALUES (?1, ?2, ?3, ?4, ?5)
            ON CONFLICT(url) DO UPDATE SET seen_at = excluded.seen_at, is_relevant = excluded.is_relevant, category = excluded.category, analysis = excluded.analysis
            "#,
            params![url, seen_at, is_relevant, category, analysis],
        ) {
            error!(target: TARGET_DB, "Failed to execute insert/update: {:?}", e);
            return Err(e);
        }

        if let Err(e) = tx.commit() {
            error!(target: TARGET_DB, "Failed to commit transaction: {:?}", e);
            return Err(e);
        }

        info!(target: TARGET_DB, "Transaction committed for article: {}", url);
        Ok(())
    }

    #[instrument(target = "db", level = "info", skip(self))]
    pub fn has_seen(&self, url: &str) -> Result<bool> {
        info!(target: TARGET_DB, "Checking if article has been seen: {}", url);
        let mut stmt = match self.conn.prepare("SELECT 1 FROM articles WHERE url = ?1") {
            Ok(stmt) => stmt,
            Err(e) => {
                error!(target: TARGET_DB, "Failed to prepare statement: {:?}", e);
                return Err(e);
            }
        };

        let mut rows = match stmt.query(params![url]) {
            Ok(rows) => rows,
            Err(e) => {
                error!(target: TARGET_DB, "Failed to execute query: {:?}", e);
                return Err(e);
            }
        };

        let seen = rows.next()?.is_some();
        info!(target: TARGET_DB, "Article seen status for {}: {}", url, seen);
        Ok(seen)
    }
}

#[instrument(target = "db", level = "info", skip(db))]
pub fn process_article(
    db: &mut Database,
    url: &str,
    is_relevant: bool,
    category: Option<&str>,
    analysis: Option<&str>,
) -> Result<()> {
    if !db.has_seen(url)? {
        info!(target: TARGET_DB, "Article not seen before, adding to database: {}", url);
        db.add_article(url, is_relevant, category, analysis)?;
    } else {
        info!(target: TARGET_DB, "Article already seen, skipping: {}", url);
    }
    Ok(())
}
