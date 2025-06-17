# 09 - Post-Migration Optimization & Maintenance

**Effort**: XS (half day setup, ongoing)

## Immediate Post-Migration Tasks

### Configuration Cleanup
```bash
echo "🧹 Cleaning up post-migration..."

# Remove SQLite fallback after successful migration
unset SQLITE_PATH
unset DATABASE_PATH

# Update .env file to remove migrated variables
sed -i.bak '/^TOPICS=/d; /^URLS=/d; /^SLACK_TOKEN=/d; /^SLACK_CHANNEL=/d' .env

# Archive migration files
mkdir -p migration_archive
mv migration_data/ migration_archive/
mv argus.db.final_backup.* migration_archive/
mv env.backup.* migration_archive/

echo "✅ Cleanup completed"
```

### Remove SQLite Dependencies
```toml
# Update Cargo.toml - remove SQLite features
sqlx = { version = "0.8", features = ["postgres", "runtime-tokio-rustls", "macros", "uuid", "chrono", "json"] }
```

```bash
# Rebuild without SQLite
cargo build --release
```

## PostgreSQL Optimization

### Performance Monitoring Setup
```sql
-- Enable query statistics collection
CREATE EXTENSION IF NOT EXISTS pg_stat_statements;

-- Check slow queries
SELECT query, calls, total_time, mean_time, rows
FROM pg_stat_statements 
ORDER BY mean_time DESC 
LIMIT 10;

-- Monitor index usage
SELECT schemaname, tablename, attname, n_distinct, correlation
FROM pg_stats 
WHERE tablename IN ('articles', 'entities', 'article_entities');
```

### Index Optimization
```sql
-- Add missing indexes based on actual usage patterns
CREATE INDEX CONCURRENTLY idx_articles_seen_at_relevant 
ON articles (seen_at, is_relevant) WHERE is_relevant = true;

CREATE INDEX CONCURRENTLY idx_article_entities_importance 
ON article_entities (importance, entity_id);

-- Drop unused indexes (check first!)
-- DROP INDEX IF EXISTS old_unused_index;
```

### Vacuum and Analyze Schedule
```sql
-- Set up automatic vacuum and analyze
ALTER TABLE articles SET (
  autovacuum_vacuum_scale_factor = 0.1,
  autovacuum_analyze_scale_factor = 0.05
);

-- Manual analyze for immediate optimization
ANALYZE articles;
ANALYZE entities;
ANALYZE article_entities;
```

## Monitoring Setup

### Database Monitoring Script
**Create monitor_postgres.sh:**
```bash
#!/bin/bash

echo "📊 PostgreSQL Health Report - $(date)"
echo "========================================"

# Connection count
CONNECTIONS=$(psql $DATABASE_URL -t -c "SELECT count(*) FROM pg_stat_activity WHERE state = 'active';")
echo "Active connections: $CONNECTIONS"

# Database size
DB_SIZE=$(psql $DATABASE_URL -t -c "SELECT pg_size_pretty(pg_database_size(current_database()));")
echo "Database size: $DB_SIZE"

# Table sizes
echo -e "\nTable sizes:"
psql $DATABASE_URL -c "
SELECT 
  tablename,
  pg_size_pretty(pg_total_relation_size(tablename::regclass)) as size
FROM pg_tables 
WHERE schemaname = 'public' 
ORDER BY pg_total_relation_size(tablename::regclass) DESC 
LIMIT 10;"

# Query performance
echo -e "\nSlowest queries (if pg_stat_statements available):"
psql $DATABASE_URL -c "
SELECT 
  SUBSTRING(query, 1, 50) as query_start,
  calls,
  ROUND(mean_time::numeric, 2) as avg_time_ms
FROM pg_stat_statements 
ORDER BY mean_time DESC 
LIMIT 5;" 2>/dev/null || echo "pg_stat_statements not available"

# Lock monitoring
LOCKS=$(psql $DATABASE_URL -t -c "SELECT count(*) FROM pg_locks WHERE NOT granted;")
echo -e "\nBlocked queries: $LOCKS"

echo "========================================"
```

### Automated Monitoring with Cron
```bash
# Add to crontab for hourly monitoring
# crontab -e
0 * * * * /opt/argus/monitor_postgres.sh >> /var/log/argus/postgres_monitor.log 2>&1
```

## Configuration Management Enhancement

### Advanced Topic Management
**Create src/bin/bulk_manage_topics.rs:**
```rust
use anyhow::Result;
use argus::config::ConfigManager;
use std::fs;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
struct TopicExport {
    topics: Vec<TopicEntry>,
}

#[derive(Deserialize, Serialize)]
struct TopicEntry {
    name: String,
    prompt: String,
    enabled: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    
    if args.len() < 2 {
        println!("Usage: bulk_manage_topics <command> [file]");
        println!("Commands:");
        println!("  export <file.json>  - Export topics to JSON");
        println!("  import <file.json>  - Import topics from JSON");
        return Ok(());
    }
    
    let config_manager = ConfigManager::new().await?;
    
    match args[1].as_str() {
        "export" => {
            if args.len() < 3 {
                println!("Usage: bulk_manage_topics export <file.json>");
                return Ok(());
            }
            
            let topics = config_manager.get_topics().await;
            let export = TopicExport {
                topics: topics.into_iter().map(|(name, prompt)| TopicEntry {
                    name,
                    prompt,
                    enabled: true,
                }).collect(),
            };
            
            let json = serde_json::to_string_pretty(&export)?;
            fs::write(&args[2], json)?;
            println!("✅ Exported {} topics to {}", export.topics.len(), args[2]);
        }
        
        "import" => {
            if args.len() < 3 {
                println!("Usage: bulk_manage_topics import <file.json>");
                return Ok(());
            }
            
            let content = fs::read_to_string(&args[2])?;
            let import: TopicExport = serde_json::from_str(&content)?;
            
            for topic in import.topics {
                if topic.enabled {
                    config_manager.add_topic(&topic.name, &topic.prompt).await?;
                    println!("✅ Imported topic: {}", topic.name);
                }
            }
            
            println!("✅ Import completed");
        }
        
        _ => {
            println!("Unknown command: {}", args[1]);
        }
    }
    
    Ok(())
}
```

### Configuration Backup System
**Create backup_config.sh:**
```bash
#!/bin/bash

BACKUP_DIR="/opt/argus/config_backups"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)

mkdir -p "$BACKUP_DIR"

echo "💾 Backing up configuration..."

# Export topics
cargo run --release --bin bulk_manage_topics export "$BACKUP_DIR/topics_$TIMESTAMP.json"

# Export RSS feeds (simple list)
cargo run --release --bin manage_feeds list > "$BACKUP_DIR/feeds_$TIMESTAMP.txt"

# Export system settings
psql $DATABASE_URL -c "
COPY (
  SELECT category, name, value 
  FROM configurations 
  WHERE category = 'system' AND enabled = true
) TO STDOUT WITH CSV HEADER
" > "$BACKUP_DIR/system_$TIMESTAMP.csv"

echo "✅ Configuration backed up to $BACKUP_DIR"

# Cleanup old backups (keep 30 days)
find "$BACKUP_DIR" -name "*.json" -o -name "*.txt" -o -name "*.csv" | \
  head -n -90 | xargs rm -f 2>/dev/null || true

echo "✅ Old backups cleaned up"
```

## Performance Optimization

### Connection Pooling with PgBouncer
```ini
# /etc/pgbouncer/pgbouncer.ini
[databases]
argus_prod = host=localhost port=5432 dbname=argus_prod

[pgbouncer]
listen_port = 6432
listen_addr = localhost
auth_type = md5
auth_file = /etc/pgbouncer/userlist.txt
pool_mode = transaction
max_client_conn = 100
default_pool_size = 20
min_pool_size = 5
reserve_pool_size = 5
max_db_connections = 50
```

### Update Database URL for Connection Pooling
```bash
# Use PgBouncer instead of direct connection
export DATABASE_URL=postgresql://argus_user:password@localhost:6432/argus_prod
```

### Query Optimization Examples
```sql
-- Optimize frequent article queries
CREATE INDEX CONCURRENTLY idx_articles_category_date 
ON articles (category, seen_at DESC) 
WHERE is_relevant = true;

-- Optimize entity search
CREATE INDEX CONCURRENTLY idx_entities_search 
ON entities USING gin(to_tsvector('english', name));

-- Optimize cluster queries
CREATE INDEX CONCURRENTLY idx_clusters_active_updated 
ON article_clusters (status, last_updated DESC) 
WHERE status = 'active';
```

## Maintenance Procedures

### Daily Maintenance Script
**Create daily_maintenance.sh:**
```bash
#!/bin/bash

echo "🔧 Starting daily maintenance - $(date)"

# Update statistics
psql $DATABASE_URL -c "ANALYZE;" > /dev/null

# Check for fragmentation
FRAGMENTATION=$(psql $DATABASE_URL -t -c "
SELECT ROUND(
  (COUNT(*) FILTER (WHERE n_dead_tup > 0) * 100.0 / COUNT(*))::numeric, 2
) FROM pg_stat_user_tables;")

echo "Table fragmentation: ${FRAGMENTATION}%"

if (( $(echo "$FRAGMENTATION > 10" | bc -l) )); then
    echo "⚠️  High fragmentation detected, consider running VACUUM"
fi

# Check index usage
UNUSED_INDEXES=$(psql $DATABASE_URL -t -c "
SELECT COUNT(*) FROM pg_stat_user_indexes 
WHERE idx_scan = 0 AND idx_tup_read = 0;")

if [ "$UNUSED_INDEXES" -gt 0 ]; then
    echo "⚠️  Found $UNUSED_INDEXES potentially unused indexes"
fi

# Configuration backup
/opt/argus/backup_config.sh > /dev/null

echo "✅ Daily maintenance completed"
```

### Weekly Maintenance Script
**Create weekly_maintenance.sh:**
```bash
#!/bin/bash

echo "🔧 Starting weekly maintenance - $(date)"

# Full vacuum on smaller tables
psql $DATABASE_URL -c "VACUUM FULL configurations;"
psql $DATABASE_URL -c "VACUUM FULL entity_aliases;"

# Reindex critical indexes
psql $DATABASE_URL -c "REINDEX INDEX CONCURRENTLY idx_relevant_category;"
psql $DATABASE_URL -c "REINDEX INDEX CONCURRENTLY idx_entities_normalized_name;"

# Update table statistics
psql $DATABASE_URL -c "ANALYZE VERBOSE;" | grep -E "(ANALYZE|INFO)"

# Check database integrity
psql $DATABASE_URL -c "
SELECT schemaname, tablename, 
       n_tup_ins, n_tup_upd, n_tup_del,
       ROUND((n_dead_tup * 100.0 / GREATEST(n_live_tup + n_dead_tup, 1))::numeric, 2) as dead_pct
FROM pg_stat_user_tables
WHERE n_live_tup + n_dead_tup > 0
ORDER BY dead_pct DESC;"

echo "✅ Weekly maintenance completed"
```

## Security Hardening

### Database Security
```sql
-- Revoke public permissions
REVOKE ALL ON SCHEMA public FROM public;
GRANT USAGE ON SCHEMA public TO argus_user;

-- Limit connection permissions
ALTER USER argus_user CONNECTION LIMIT 50;

-- Enable row level security on sensitive tables if needed
-- ALTER TABLE configurations ENABLE ROW LEVEL SECURITY;
```

### Application Security
```bash
# Use read-only database user for certain operations
CREATE USER argus_readonly WITH PASSWORD 'readonly_password';
GRANT CONNECT ON DATABASE argus_prod TO argus_readonly;
GRANT USAGE ON SCHEMA public TO argus_readonly;
GRANT SELECT ON ALL TABLES IN SCHEMA public TO argus_readonly;
```

## Disaster Recovery

### Backup Strategy
```bash
# Daily PostgreSQL backup
#!/bin/bash
# backup_postgres.sh

BACKUP_DIR="/opt/argus/db_backups"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)

mkdir -p "$BACKUP_DIR"

# Full database backup
pg_dump $DATABASE_URL | gzip > "$BACKUP_DIR/argus_backup_$TIMESTAMP.sql.gz"

# Configuration-only backup
pg_dump $DATABASE_URL --table=configurations --table=configuration_changes | \
  gzip > "$BACKUP_DIR/config_backup_$TIMESTAMP.sql.gz"

# Cleanup old backups (keep 7 days)
find "$BACKUP_DIR" -name "*.sql.gz" -mtime +7 -delete

echo "✅ Database backup completed: $TIMESTAMP"
```

### Recovery Testing
```bash
# Test restore procedure (on test database)
#!/bin/bash
# test_restore.sh

echo "🧪 Testing backup restore..."

# Create test database
createdb argus_test_restore

# Restore from latest backup
LATEST_BACKUP=$(ls -t /opt/argus/db_backups/argus_backup_*.sql.gz | head -n1)
gunzip -c "$LATEST_BACKUP" | psql postgresql://argus_user:password@localhost/argus_test_restore

# Validate restore
ARTICLE_COUNT=$(psql postgresql://argus_user:password@localhost/argus_test_restore -t -c "SELECT COUNT(*) FROM articles;")
CONFIG_COUNT=$(psql postgresql://argus_user:password@localhost/argus_test_restore -t -c "SELECT COUNT(*) FROM configurations;")

echo "Restored data:"
echo "  Articles: $ARTICLE_COUNT"
echo "  Configurations: $CONFIG_COUNT"

# Cleanup
dropdb argus_test_restore

echo "✅ Restore test completed"
```

## Long-term Optimization

### Partitioning Strategy (for large datasets)
```sql
-- Consider partitioning articles table by date if it grows large
-- CREATE TABLE articles_2024 PARTITION OF articles
-- FOR VALUES FROM ('2024-01-01') TO ('2025-01-01');
```

### Archive Old Data
```sql
-- Archive articles older than 2 years
CREATE TABLE articles_archive AS 
SELECT * FROM articles 
WHERE seen_at < NOW() - INTERVAL '2 years';

-- Delete archived articles (after verification)
-- DELETE FROM articles WHERE seen_at < NOW() - INTERVAL '2 years';
```

## Success Metrics

### Performance Benchmarks
- Query response times under 100ms for common operations
- No more than 5% slow queries
- Database size growth under control
- Connection pool utilization optimal

### Operational Metrics
- Zero database locking errors
- 99.9% uptime
- Successful backups every day
- Configuration changes tracked and audited

## Maintenance Schedule

### Daily (Automated)
- Health checks
- Performance monitoring
- Configuration backup
- Log rotation

### Weekly (Semi-automated)
- Full vacuum of small tables
- Index maintenance
- Security audit
- Backup verification

### Monthly (Manual)
- Review slow queries
- Optimize indexes
- Update PostgreSQL (if needed)
- Disaster recovery testing

### Quarterly (Planned)
- Full database optimization review
- Capacity planning
- Security update review
- Architecture review
