-- Drop existing article_clusters table and recreate with enhanced schema
DROP TABLE IF EXISTS article_clusters;
CREATE TABLE article_clusters (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    creation_date TEXT NOT NULL,
    last_updated TEXT NOT NULL,
    primary_entity_ids TEXT NOT NULL, -- JSON array of entity IDs
    summary TEXT,
    summary_version INTEGER NOT NULL DEFAULT 0,
    article_count INTEGER NOT NULL DEFAULT 0,
    importance_score REAL NOT NULL DEFAULT 0.0,
    timeline_events TEXT, -- JSON array of timeline events
    has_timeline INTEGER NOT NULL DEFAULT 0,
    needs_summary_update INTEGER NOT NULL DEFAULT 0
);

-- Create index on last_updated
CREATE INDEX IF NOT EXISTS idx_article_clusters_updated_at ON article_clusters (last_updated);
CREATE INDEX IF NOT EXISTS idx_article_clusters_importance ON article_clusters (importance_score);
CREATE INDEX IF NOT EXISTS idx_article_clusters_needs_update ON article_clusters (needs_summary_update);

-- Create article_cluster_mappings table if it doesn't exist
CREATE TABLE IF NOT EXISTS article_cluster_mappings (
    article_id INTEGER NOT NULL,
    cluster_id INTEGER NOT NULL,
    added_date TEXT NOT NULL,
    similarity_score REAL NOT NULL,
    PRIMARY KEY (article_id, cluster_id),
    FOREIGN KEY (article_id) REFERENCES articles (id) ON DELETE CASCADE,
    FOREIGN KEY (cluster_id) REFERENCES article_clusters (id) ON DELETE CASCADE
);

-- Create indexes for article_cluster_mappings
CREATE INDEX IF NOT EXISTS idx_article_cluster_mappings_article_id ON article_cluster_mappings (article_id);
CREATE INDEX IF NOT EXISTS idx_article_cluster_mappings_cluster_id ON article_cluster_mappings (cluster_id);

-- Create user_cluster_preferences table
CREATE TABLE IF NOT EXISTS user_cluster_preferences (
    user_id INTEGER NOT NULL,
    cluster_id INTEGER NOT NULL,
    silenced INTEGER NOT NULL DEFAULT 0,
    followed INTEGER NOT NULL DEFAULT 1,
    last_seen_version INTEGER NOT NULL DEFAULT 0,
    last_interaction TEXT,
    PRIMARY KEY (user_id, cluster_id),
    FOREIGN KEY (cluster_id) REFERENCES article_clusters (id) ON DELETE CASCADE
);

-- Create indexes for user_cluster_preferences
CREATE INDEX IF NOT EXISTS idx_user_cluster_prefs_user_id ON user_cluster_preferences (user_id);
CREATE INDEX IF NOT EXISTS idx_user_cluster_prefs_cluster_id ON user_cluster_preferences (cluster_id);
CREATE INDEX IF NOT EXISTS idx_user_cluster_prefs_silenced ON user_cluster_preferences (silenced);
