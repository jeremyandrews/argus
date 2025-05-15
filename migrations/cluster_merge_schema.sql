-- Add status column to article_clusters to track active/merged state
ALTER TABLE article_clusters ADD COLUMN status TEXT NOT NULL DEFAULT 'active';

-- Create table to track merge history
CREATE TABLE cluster_merge_history (
    original_cluster_id INTEGER NOT NULL,
    merged_into_cluster_id INTEGER NOT NULL,
    merge_date TEXT NOT NULL,
    merge_reason TEXT,
    PRIMARY KEY (original_cluster_id),
    FOREIGN KEY (original_cluster_id) REFERENCES article_clusters (id) ON DELETE CASCADE,
    FOREIGN KEY (merged_into_cluster_id) REFERENCES article_clusters (id) ON DELETE CASCADE
);

-- Create index for efficient lookups of clusters merged into a target
CREATE INDEX idx_cluster_merge_target ON cluster_merge_history (merged_into_cluster_id);
