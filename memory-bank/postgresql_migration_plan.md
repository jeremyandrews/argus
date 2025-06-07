# Argus SQLite to PostgreSQL Migration Plan

## Overview
This plan migrates Argus from SQLite to PostgreSQL to resolve database locking issues and improve concurrent performance. The migration is designed to be reversible and minimize downtime.

**Problem Statement**: Experiencing "database is locked" errors due to SQLite limitations under high concurrent load with multiple workers (RSS fetchers, decision workers, analysis workers) plus API server.

**Solution**: Migrate to PostgreSQL for better concurrent write handling, JSON support, and scalability.

**Estimated Timeline**: 6 days
**Risk Level**: Medium (well-isolated database layer)
**Expected Benefits**: Elimination of locking errors, 20-50% performance improvement, better concurrent handling

---

## Pre-Migration Checklist

### Environment Setup
1. **PostgreSQL Installation**
   ```bash
   # Ubuntu/Debian
   sudo apt-get install postgresql postgresql-contrib
   # macOS
   brew install postgresql
   # Start service
   sudo systemctl start postgresql
   ```

2. **Database Creation**
   ```sql
   CREATE DATABASE argus_prod;
   CREATE DATABASE argus_dev;
   CREATE USER argus_user WITH PASSWORD 'your_secure_password';
   GRANT ALL PRIVILEGES ON DATABASE argus_prod TO argus_user;
   GRANT ALL PRIVILEGES ON DATABASE argus_dev TO argus_user;
   ```

3. **Backup Current SQLite Database**
   ```bash
   cp argus.db argus.db.backup.$(date +%Y%m%d_%H%M%S)
   ```

---

## Phase 1: Code Preparation (Day 1)

### 1.1 Update Dependencies

**Cargo.toml changes:**
```toml
# Replace this line:
sqlx = { version = "0.8", features = ["sqlite", "runtime-tokio-rustls", "macros"] }

# With this (keep both during transition):
sqlx = { version = "0.8", features = ["sqlite", "postgres", "runtime-tokio-rustls", "macros", "uuid", "chrono"] }
```

### 1.2 Environment Configuration

**Update env.template:**
```bash
# Database Configuration
DATABASE_TYPE=postgres  # or sqlite for fallback
DATABASE_URL=postgresql://argus_user:your_secure_password@localhost/argus_prod
POSTGRES_HOST=localhost
POSTGRES_PORT=5432
POSTGRES_USER=argus_user
POSTGRES_PASSWORD=your_secure_password
POSTGRES_DB=argus_prod

# Keep existing for fallback
DATABASE_PATH=argus.db
```

### 1.3 Database Abstraction Layer

**Create src/db/config.rs:**
```rust
use serde::Deserialize;
use std::env;

#[derive(Debug, Clone)]
pub enum DatabaseConfig {
    Sqlite { path: String },
    Postgres { url: String },
}

impl DatabaseConfig {
    pub fn from_env() -> Self {
        let db_type = env::var("DATABASE_TYPE").unwrap_or_else(|_| "sqlite".to_string());
        
        match db_type.as_str() {
            "postgres" => {
                let url = env::var("DATABASE_URL")
                    .expect("DATABASE_URL must be set when using PostgreSQL");
                Self::Postgres { url }
            }
            "sqlite" | _ => {
                let path = env::var("DATABASE_PATH")
                    .unwrap_or_else(|_| "argus.db".to_string());
                Self::Sqlite { path }
            }
        }
    }
}
```

### 1.4 Update Database Core

**Modify src/db/core.rs:**
```rust
use sqlx::{
    postgres::{PgConnectOptions, PgPoolOptions},
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
    Pool, Postgres, Sqlite,
};
use std::str::FromStr;
use tokio::time::Duration;

use super::config::DatabaseConfig;

#[derive(Clone)]
pub enum DatabasePool {
    Sqlite(Pool<Sqlite>),
    Postgres(Pool<Postgres>),
}

#[derive(Clone)]
pub struct Database {
    pool: DatabasePool,
    config: DatabaseConfig,
}

impl Database {
    pub async fn new_from_config(config: DatabaseConfig) -> Result<Self, sqlx::Error> {
        let pool = match &config {
            DatabaseConfig::Sqlite { path } => {
                let connect_options = SqliteConnectOptions::from_str(&format!("sqlite://{}", path))?
                    .create_if_missing(true)
                    .journal_mode(SqliteJournalMode::Wal)
                    .busy_timeout(Duration::from_secs(5))
                    .synchronous(SqliteSynchronous::Normal);

                let pool = SqlitePoolOptions::new()
                    .max_connections(5)
                    .connect_with(connect_options)
                    .await?;
                
                DatabasePool::Sqlite(pool)
            }
            
            DatabaseConfig::Postgres { url } => {
                let connect_options = PgConnectOptions::from_str(url)?
                    .application_name("argus");

                let pool = PgPoolOptions::new()
                    .max_connections(20)  // Increased for better concurrency
                    .acquire_timeout(Duration::from_secs(10))
                    .connect_with(connect_options)
                    .await?;
                
                DatabasePool::Postgres(pool)
            }
        };

        let db = Database { pool, config };
        db.initialize_schema().await?;
        Ok(db)
    }

    pub async fn new(database_url: &str) -> Result<Self, sqlx::Error> {
        // Legacy method - determine type from URL
        if database_url.starts_with("postgresql://") || database_url.starts_with("postgres://") {
            let config = DatabaseConfig::Postgres { url: database_url.to_string() };
            Self::new_from_config(config).await
        } else {
            let config = DatabaseConfig::Sqlite { path: database_url.to_string() };
            Self::new_from_config(config).await
        }
    }
}
```

---

## Phase 2: Schema Migration (Day 2)

### 2.1 Create PostgreSQL Schema

**Create migrations/postgresql_schema.sql:**
```sql
-- PostgreSQL version of your schema
CREATE TABLE IF NOT EXISTS articles (
    id BIGSERIAL PRIMARY KEY,
    url TEXT NOT NULL,
    normalized_url TEXT NOT NULL UNIQUE,
    seen_at TIMESTAMPTZ NOT NULL,
    pub_date TIMESTAMPTZ,
    event_date TIMESTAMPTZ,
    title TEXT,
    source TEXT,
    is_relevant BOOLEAN NOT NULL,
    category TEXT,
    tiny_summary TEXT,
    analysis JSONB,  -- JSONB for better performance
    json_data JSONB,
    quality REAL,
    hash TEXT,
    title_domain_hash TEXT,
    r2_url TEXT,
    cluster_id BIGINT
);

-- Indexes
CREATE INDEX IF NOT EXISTS idx_relevant_category ON articles (is_relevant, category);
CREATE INDEX IF NOT EXISTS idx_hash ON articles (hash);
CREATE INDEX IF NOT EXISTS idx_title_domain_hash ON articles (title_domain_hash);
CREATE INDEX IF NOT EXISTS idx_r2_url ON articles (r2_url);
CREATE INDEX IF NOT EXISTS idx_seen_at_r2_url ON articles (seen_at, r2_url);
CREATE INDEX IF NOT EXISTS idx_seen_at_category_r2_url ON articles (seen_at, category, r2_url);
CREATE INDEX IF NOT EXISTS idx_articles_event_date ON articles (event_date);
CREATE INDEX IF NOT EXISTS idx_articles_pub_date ON articles (pub_date);
CREATE INDEX IF NOT EXISTS idx_articles_cluster_id ON articles (cluster_id);

-- JSONB indexes for analysis field
CREATE INDEX IF NOT EXISTS idx_articles_analysis_quality ON articles USING GIN ((analysis->'quality'));

-- Entity tables
CREATE TABLE IF NOT EXISTS entities (
    id BIGSERIAL PRIMARY KEY,
    name TEXT NOT NULL,
    type TEXT NOT NULL,
    normalized_name TEXT NOT NULL,
    parent_id BIGINT,
    UNIQUE(normalized_name, type),
    FOREIGN KEY (parent_id) REFERENCES entities (id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_entities_normalized_name ON entities (normalized_name);
CREATE INDEX IF NOT EXISTS idx_entities_type ON entities (type);
CREATE INDEX IF NOT EXISTS idx_entities_parent_id ON entities (parent_id);

-- Article-Entity relationships
CREATE TABLE IF NOT EXISTS article_entities (
    id BIGSERIAL PRIMARY KEY,
    article_id BIGINT NOT NULL,
    entity_id BIGINT NOT NULL,
    importance TEXT NOT NULL,
    context TEXT,
    FOREIGN KEY (article_id) REFERENCES articles (id) ON DELETE CASCADE,
    FOREIGN KEY (entity_id) REFERENCES entities (id) ON DELETE CASCADE,
    UNIQUE(article_id, entity_id)
);

CREATE INDEX IF NOT EXISTS idx_article_entities_article_id ON article_entities (article_id);
CREATE INDEX IF NOT EXISTS idx_article_entities_entity_id ON article_entities (entity_id);
CREATE INDEX IF NOT EXISTS idx_article_entities_importance ON article_entities (importance);

-- Article clusters
CREATE TABLE IF NOT EXISTS article_clusters (
    id BIGSERIAL PRIMARY KEY,
    name TEXT,
    creation_date TIMESTAMPTZ NOT NULL,
    last_updated TIMESTAMPTZ NOT NULL,
    primary_entity_ids JSONB NOT NULL DEFAULT '[]',
    article_count INTEGER NOT NULL DEFAULT 0,
    needs_summary_update INTEGER NOT NULL DEFAULT 1,
    summary TEXT,
    summary_version INTEGER NOT NULL DEFAULT 0,
    status TEXT NOT NULL DEFAULT 'active',
    importance_score REAL NOT NULL DEFAULT 0.0,
    has_timeline INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_article_clusters_last_updated ON article_clusters (last_updated);
CREATE INDEX IF NOT EXISTS idx_article_clusters_status ON article_clusters (status);
CREATE INDEX IF NOT EXISTS idx_article_clusters_importance ON article_clusters (importance_score);

-- [Continue with all other tables from schema.rs, converting types appropriately]
-- Queue tables, entity aliases, device tables, etc.

-- Queue tables
CREATE TABLE IF NOT EXISTS rss_queue (
    id BIGSERIAL PRIMARY KEY,
    url TEXT NOT NULL,
    normalized_url TEXT NOT NULL UNIQUE,
    title TEXT,
    seen_at TIMESTAMPTZ NOT NULL,
    pub_date TIMESTAMPTZ
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_seen_at_normalized_url ON rss_queue (seen_at, normalized_url);
CREATE UNIQUE INDEX IF NOT EXISTS idx_pub_date_normalized_url ON rss_queue (pub_date, normalized_url);

-- [Additional tables follow same pattern...]
```

### 2.2 Schema Conversion Script

**Create scripts/convert_schema.py:**
```python
#!/usr/bin/env python3
"""
Convert SQLite schema to PostgreSQL
"""
import re
import sys

def convert_sqlite_to_postgres(sqlite_sql):
    """Convert SQLite DDL to PostgreSQL DDL"""
    
    # Replace SQLite types with PostgreSQL equivalents
    conversions = [
        (r'\bINTEGER PRIMARY KEY AUTOINCREMENT\b', 'BIGSERIAL PRIMARY KEY'),
        (r'\bINTEGER\b', 'BIGINT'),
        (r'\bREAL\b', 'REAL'),
        (r'\bBOOLEAN\b', 'BOOLEAN'),
        (r'\bTEXT\b', 'TEXT'),
        # Convert timestamp fields to TIMESTAMPTZ
        (r'seen_at TEXT', 'seen_at TIMESTAMPTZ'),
        (r'pub_date TEXT', 'pub_date TIMESTAMPTZ'),
        (r'event_date TEXT', 'event_date TIMESTAMPTZ'),
        (r'creation_date TEXT', 'creation_date TIMESTAMPTZ'),
        (r'last_updated TEXT', 'last_updated TIMESTAMPTZ'),
        # JSON fields to JSONB
        (r'analysis TEXT', 'analysis JSONB'),
        (r'json_data TEXT', 'json_data JSONB'),
        (r'primary_entity_ids TEXT', 'primary_entity_ids JSONB'),
    ]
    
    result = sqlite_sql
    for pattern, replacement in conversions:
        result = re.sub(pattern, replacement, result, flags=re.IGNORECASE)
    
    return result

if __name__ == "__main__":
    with open('src/db/schema.rs', 'r') as f:
        content = f.read()
    
    # Extract SQL from the Rust file
    sql_match = re.search(r'r#"\s*(CREATE TABLE.*?)"\s*#', content, re.DOTALL)
    if sql_match:
        sqlite_sql = sql_match.group(1)
        postgres_sql = convert_sqlite_to_postgres(sqlite_sql)
        
        with open('migrations/postgresql_schema.sql', 'w') as f:
            f.write(postgres_sql)
        
        print("Schema converted and saved to migrations/postgresql_schema.sql")
    else:
        print("Could not extract SQL from schema.rs")
```

---

## Phase 3: Data Migration (Day 3)

### 3.1 Export Script

**Create scripts/export_sqlite_data.py:**
```python
#!/usr/bin/env python3
"""
Export data from SQLite to CSV files for PostgreSQL import
"""
import sqlite3
import csv
import os
import json
from datetime import datetime

def export_table_to_csv(db_path, table_name, output_dir):
    """Export a table to CSV"""
    conn = sqlite3.connect(db_path)
    cursor = conn.cursor()
    
    # Get table schema
    cursor.execute(f"PRAGMA table_info({table_name})")
    columns = [col[1] for col in cursor.fetchall()]
    
    # Export data
    cursor.execute(f"SELECT * FROM {table_name}")
    
    output_file = os.path.join(output_dir, f"{table_name}.csv")
    with open(output_file, 'w', newline='', encoding='utf-8') as csvfile:
        writer = csv.writer(csvfile)
        writer.writerow(columns)
        
        for row in cursor.fetchall():
            # Convert timestamps and handle JSON
            converted_row = []
            for i, value in enumerate(row):
                if value is None:
                    converted_row.append('')
                elif columns[i] in ['seen_at', 'pub_date', 'event_date', 'creation_date', 'last_updated']:
                    # Convert to ISO format if it's a timestamp
                    if value and not value.startswith('20'):  # Simple heuristic
                        converted_row.append(value)
                    else:
                        try:
                            # Convert to PostgreSQL timestamp format
                            dt = datetime.fromisoformat(value.replace('Z', '+00:00'))
                            converted_row.append(dt.isoformat())
                        except:
                            converted_row.append(value)
                else:
                    converted_row.append(value)
            
            writer.writerow(converted_row)
    
    conn.close()
    print(f"Exported {table_name} to {output_file}")

def main():
    db_path = 'argus.db'
    output_dir = 'migration_data'
    
    if not os.path.exists(output_dir):
        os.makedirs(output_dir)
    
    # Get all table names
    conn = sqlite3.connect(db_path)
    cursor = conn.cursor()
    cursor.execute("SELECT name FROM sqlite_master WHERE type='table'")
    tables = [row[0] for row in cursor.fetchall()]
    conn.close()
    
    # Export each table
    for table in tables:
        try:
            export_table_to_csv(db_path, table, output_dir)
        except Exception as e:
            print(f"Error exporting {table}: {e}")

if __name__ == "__main__":
    main()
```

### 3.2 Import Script

**Create scripts/import_to_postgres.py:**
```python
#!/usr/bin/env python3
"""
Import CSV data into PostgreSQL
"""
import psycopg2
import csv
import os
import sys

def import_csv_to_table(conn, table_name, csv_file):
    """Import CSV file to PostgreSQL table"""
    cursor = conn.cursor()
    
    with open(csv_file, 'r', encoding='utf-8') as f:
        reader = csv.reader(f)
        columns = next(reader)  # Skip header
        
        # Create COPY command
        copy_sql = f"COPY {table_name} ({','.join(columns)}) FROM STDIN WITH CSV"
        
        # Reset file pointer and skip header
        f.seek(0)
        next(reader)
        
        cursor.copy_expert(copy_sql, f)
        conn.commit()
        
        print(f"Imported {csv_file} to {table_name}")

def main():
    # Database connection
    conn = psycopg2.connect(
        host=os.getenv('POSTGRES_HOST', 'localhost'),
        database=os.getenv('POSTGRES_DB', 'argus_prod'),
        user=os.getenv('POSTGRES_USER', 'argus_user'),
        password=os.getenv('POSTGRES_PASSWORD')
    )
    
    migration_dir = 'migration_data'
    
    # Import order (respecting foreign keys)
    import_order = [
        'entities',
        'articles',
        'article_entities',
        'article_clusters',
        'article_cluster_mappings',
        'cluster_merge_history',
        'entity_aliases',
        'rss_queue',
        'matched_topics_queue',
        'life_safety_queue',
        'devices',
        'device_subscriptions',
        'ip_logs',
        'endpoint_timeout_events',
        'endpoint_alerts',
        # Add other tables in dependency order
    ]
    
    for table in import_order:
        csv_file = os.path.join(migration_dir, f"{table}.csv")
        if os.path.exists(csv_file):
            try:
                import_csv_to_table(conn, table, csv_file)
            except Exception as e:
                print(f"Error importing {table}: {e}")
                conn.rollback()
        else:
            print(f"Warning: {csv_file} not found, skipping {table}")
    
    # Update sequences
    cursor = conn.cursor()
    for table in import_order:
        try:
            cursor.execute(f"SELECT setval('{table}_id_seq', (SELECT MAX(id) FROM {table}));")
            conn.commit()
        except:
            pass  # Some tables might not have sequences
    
    conn.close()
    print("Data migration completed")

if __name__ == "__main__":
    main()
```

---

## Phase 4: Code Updates (Day 4)

### 4.1 Query Adaptations

**Update src/db/schema.rs for PostgreSQL compatibility:**
```rust
impl Database {
    pub(crate) async fn initialize_schema(&self) -> Result<(), sqlx::Error> {
        match &self.pool {
            DatabasePool::Postgres(pool) => {
                // Read and execute PostgreSQL schema
                let schema_sql = include_str!("../../migrations/postgresql_schema.sql");
                sqlx::query(schema_sql).execute(pool).await?;
                info!(target: TARGET_DB, "PostgreSQL schema initialized");
            }
            DatabasePool::Sqlite(pool) => {
                // Existing SQLite schema code
                // ... (keep existing code)
            }
        }
        Ok(())
    }
}
```

### 4.2 Update Database Methods

**Modify methods in src/db/core.rs that use database-specific features:**
```rust
impl Database {
    pub async fn get_article_text(&self, article_id: i64) -> Result<String, sqlx::Error> {
        match &self.pool {
            DatabasePool::Postgres(pool) => {
                let analysis = sqlx::query_scalar::<_, Option<serde_json::Value>>(
                    "SELECT analysis FROM articles WHERE id = $1"
                )
                .bind(article_id)
                .fetch_one(pool)
                .await?;

                if let Some(analysis_json) = analysis {
                    if let Some(body) = analysis_json.get("article_body").and_then(|v| v.as_str()) {
                        return Ok(body.to_string());
                    }
                    return Ok(analysis_json.to_string());
                }

                // Fallback to tiny_summary
                let tiny_summary = sqlx::query_scalar::<_, Option<String>>(
                    "SELECT tiny_summary FROM articles WHERE id = $1",
                )
                .bind(article_id)
                .fetch_one(pool)
                .await?;

                Ok(tiny_summary.unwrap_or_default())
            }
            DatabasePool::Sqlite(pool) => {
                // Existing SQLite code with ? placeholders
                // ... (keep existing implementation)
            }
        }
    }

    // Update all methods to handle both database types
    // Replace ? with $1, $2, etc. for PostgreSQL
    // Use JSONB operators for JSON queries in PostgreSQL
}
```

### 4.3 Create Database Factory

**Create src/db/factory.rs:**
```rust
use super::{Database, config::DatabaseConfig};

pub struct DatabaseFactory;

impl DatabaseFactory {
    pub async fn create() -> Result<Database, sqlx::Error> {
        let config = DatabaseConfig::from_env();
        Database::new_from_config(config).await
    }
    
    pub async fn create_with_config(config: DatabaseConfig) -> Result<Database, sqlx::Error> {
        Database::new_from_config(config).await
    }
}
```

---

## Phase 5: Testing (Day 5)

### 5.1 Unit Tests

**Create tests/postgres_migration_test.rs:**
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{Database, config::DatabaseConfig};
    
    #[tokio::test]
    async fn test_postgres_connection() {
        let config = DatabaseConfig::Postgres {
            url: "postgresql://test_user:test_pass@localhost/test_db".to_string()
        };
        
        let db = Database::new_from_config(config).await;
        assert!(db.is_ok());
    }
    
    #[tokio::test]
    async fn test_schema_creation() {
        // Test schema initialization
    }
    
    #[tokio::test]
    async fn test_data_migration() {
        // Test that migrated data is accessible
    }
}
```

### 5.2 Integration Testing

**Create scripts/test_migration.sh:**
```bash
#!/bin/bash

# Test script for validating migration
set -e

echo "Setting up test environment..."

# Create test database
createdb argus_test || true

# Set test environment
export DATABASE_TYPE=postgres
export DATABASE_URL=postgresql://argus_user:password@localhost/argus_test

# Run schema creation
cargo run --bin migrate_postgresql_schema

# Run basic functionality tests
echo "Testing basic database operations..."
cargo test --test postgres_migration_test

# Test with some sample data
echo "Testing with sample data..."
# Add sample data insertion and verification

echo "Migration test completed successfully!"
```

### 5.3 Performance Comparison

**Create scripts/benchmark_comparison.py:**
```python
#!/usr/bin/env python3
"""
Compare performance between SQLite and PostgreSQL
"""
import time
import subprocess
import statistics
import os

def run_benchmark(db_type, operation):
    """Run a benchmark operation"""
    env = os.environ.copy()
    env['DATABASE_TYPE'] = db_type
    
    start_time = time.time()
    result = subprocess.run([
        'cargo', 'run', '--bin', f'benchmark_{operation}'
    ], env=env, capture_output=True, text=True)
    end_time = time.time()
    
    return end_time - start_time, result.returncode == 0

def main():
    operations = ['insert_articles', 'concurrent_reads', 'entity_queries']
    results = {}
    
    for operation in operations:
        print(f"Benchmarking {operation}...")
        
        # SQLite benchmark
        sqlite_times = []
        for _ in range(5):
            time_taken, success = run_benchmark('sqlite', operation)
            if success:
                sqlite_times.append(time_taken)
        
        # PostgreSQL benchmark
        postgres_times = []
        for _ in range(5):
            time_taken, success = run_benchmark('postgres', operation)
            if success:
                postgres_times.append(time_taken)
        
        results[operation] = {
            'sqlite': statistics.mean(sqlite_times) if sqlite_times else None,
            'postgres': statistics.mean(postgres_times) if postgres_times else None
        }
    
    # Print results
    print("\nBenchmark Results:")
    print("-" * 50)
    for operation, times in results.items():
        print(f"{operation}:")
        if times['sqlite']:
            print(f"  SQLite:     {times['sqlite']:.3f}s")
        if times['postgres']:
            print(f"  PostgreSQL: {times['postgres']:.3f}s")
        if times['sqlite'] and times['postgres']:
            improvement = ((times['sqlite'] - times['postgres']) / times['sqlite']) * 100
            print(f"  Improvement: {improvement:.1f}%")
        print()

if __name__ == "__main__":
    main()
```

---

## Phase 6: Deployment (Day 6)

### 6.1 Production Deployment Checklist

**Pre-deployment:**
- [ ] PostgreSQL server configured with appropriate resources
- [ ] Database user created with proper permissions
- [ ] Connection pooling configured (pgbouncer recommended)
- [ ] Backup strategy in place
- [ ] Monitoring setup (pg_stat_statements, etc.)

**Deployment Steps:**
1. **Stop all Argus services**
   ```bash
   systemctl stop argus-*
   ```

2. **Final SQLite backup**
   ```bash
   cp argus.db argus.db.final_backup.$(date +%Y%m%d_%H%M%S)
   ```

3. **Run data migration**
   ```bash
   python3 scripts/export_sqlite_data.py
   python3 scripts/import_to_postgres.py
   ```

4. **Update environment configuration**
   ```bash
   # Update production environment
   export DATABASE_TYPE=postgres
   export DATABASE_URL=postgresql://argus_user:password@localhost/argus_prod
   ```

5. **Start services with PostgreSQL**
   ```bash
   systemctl start argus-api
   systemctl start argus-workers
   ```

6. **Validate operation**
   ```bash
   # Check service status
   systemctl status argus-*
   
   # Test API endpoints
   curl http://localhost:8080/health
   
   # Monitor logs
   journalctl -f -u argus-api
   ```

### 6.2 Rollback Plan

**If issues occur:**
1. **Stop PostgreSQL services**
   ```bash
   systemctl stop argus-*
   ```

2. **Switch back to SQLite**
   ```bash
   export DATABASE_TYPE=sqlite
   export DATABASE_PATH=argus.db
   ```

3. **Restore from backup if needed**
   ```bash
   cp argus.db.final_backup.YYYYMMDD_HHMMSS argus.db
   ```

4. **Restart services**
   ```bash
   systemctl start argus-*
   ```

---

## Post-Migration Optimization

### Performance Tuning
1. **PostgreSQL Configuration** (`postgresql.conf`):
   ```
   # Memory settings
   shared_buffers = 256MB
   work_mem = 4MB
   maintenance_work_mem = 64MB
   
   # Connection settings
   max_connections = 100
   
   # Write performance
   wal_buffers = 16MB
   checkpoint_completion_target = 0.9
   
   # Query performance
   random_page_cost = 1.1
   effective_cache_size = 1GB
   ```

2. **Index Optimization**:
   ```sql
   -- Analyze table statistics
   ANALYZE;
   
   -- Add composite indexes for common queries
   CREATE INDEX idx_articles_relevance_category_date 
   ON articles (is_relevant, category, seen_at);
   
   -- JSON indexes for analysis field
   CREATE INDEX idx_articles_analysis_quality 
   ON articles USING GIN ((analysis->'quality'));
   ```

3. **Connection Pooling** (pgbouncer):
   ```ini
   [databases]
   argus_prod = host=localhost port=5432 dbname=argus_prod
   
   [pgbouncer]
   pool_mode = transaction
   max_client_conn = 100
   default_pool_size = 20
   ```

### Monitoring Setup
1. **Enable query statistics**:
   ```sql
   CREATE EXTENSION IF NOT EXISTS pg_stat_statements;
   ```

2. **Create monitoring queries**:
   ```sql
   -- Top slow queries
   SELECT query, calls, total_time, mean_time 
   FROM pg_stat_statements 
   ORDER BY mean_time DESC LIMIT 10;
   
   -- Lock monitoring
   SELECT * FROM pg_locks WHERE granted = false;
   ```

---

## Success Metrics

After migration, expect to see:
- **Elimination of "database is locked" errors**
- **20-50% improvement in concurrent operation performance**
- **Better handling of high-frequency RSS updates**
- **Improved API response times under load**
- **More reliable worker processing**

---

## Key Implementation Notes

### Current Database Usage Patterns
Based on analysis of the codebase:
- **High concurrency**: Multiple worker types + API server
- **Complex schema**: 15+ tables with extensive indexing
- **JSON storage**: Analysis results stored as JSON (will benefit from JSONB)
- **Entity relationships**: Complex foreign key relationships
- **Vector operations**: Integrated with existing vector storage

### Critical Files to Update
- `src/db/core.rs` - Main database abstraction
- `src/db/schema.rs` - Schema initialization
- `src/db/mod.rs` - Module exports
- All query methods using `?` placeholders (convert to `$1`, `$2`, etc.)

### Migration Risks & Mitigations
- **Risk**: Data loss during migration
  - **Mitigation**: Multiple backups and tested rollback procedures
- **Risk**: Performance degradation
  - **Mitigation**: Comprehensive benchmarking and optimization
- **Risk**: Application downtime
  - **Mitigation**: Phased deployment with fallback options

This plan provides a comprehensive roadmap for migrating from SQLite to PostgreSQL while maintaining system reliability and minimizing downtime.
