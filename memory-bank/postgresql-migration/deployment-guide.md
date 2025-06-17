# PostgreSQL Migration Deployment Guide

This guide covers production deployment of the PostgreSQL migration for Argus.

## Pre-Deployment Checklist

### Environment Preparation
- [ ] PostgreSQL server installed and configured
- [ ] Database user and permissions set up
- [ ] Connection pooling configured (optional but recommended)
- [ ] Backup strategy in place
- [ ] All tests passed in staging environment

### Code Preparation
- [ ] Migration binaries built and tested
- [ ] Database layer updated for PostgreSQL
- [ ] Environment variables configured
- [ ] Rollback procedures documented and tested

## Deployment Strategy

### Low-Risk Deployment Process
1. **Current State**: SQLite-based system running
2. **Migration Window**: Brief downtime for data migration
3. **New State**: PostgreSQL-based system running
4. **Rollback Available**: Quick revert to SQLite if needed

## Production Deployment Steps

### Step 1: Environment Setup

```bash
# Install PostgreSQL (if not already installed)
sudo apt-get update
sudo apt-get install postgresql postgresql-contrib

# Start and enable PostgreSQL
sudo systemctl start postgresql
sudo systemctl enable postgresql

# Create database and user
sudo -u postgres createdb argus_prod
sudo -u postgres createuser argus_user --pwprompt
sudo -u postgres psql -c "GRANT ALL PRIVILEGES ON DATABASE argus_prod TO argus_user;"

# Set environment variable
export DATABASE_URL=postgresql://argus_user:password@localhost/argus_prod

# Verify connection
psql $DATABASE_URL -c "SELECT version();"
```

### Step 2: Build Migration Tools

```bash
# Build the migration binary
cargo build --release --bin migrate_to_postgres

# Build the admin tool
cargo build --release --bin argus_admin

# Verify builds
ls -la target/release/migrate_to_postgres
ls -la target/release/argus_admin

# Create convenience alias
echo "alias aa='./target/release/argus_admin'" >> ~/.bashrc
source ~/.bashrc
```

### Step 3: Stop Running Services

```bash
# Stop all Argus services gracefully
sudo systemctl stop argus-api 2>/dev/null || true
sudo systemctl stop argus-workers 2>/dev/null || true

# Kill any remaining processes
pkill -f "target/release/argus" 2>/dev/null || true
pkill -f "cargo run" 2>/dev/null || true

# Verify no Argus processes are running
ps aux | grep argus

echo "✅ Services stopped"
```

### Step 4: Execute Migration

```bash
# Run the complete migration
echo "🚀 Starting PostgreSQL migration..."
./target/release/migrate_to_postgres

# Check exit code
if [ $? -eq 0 ]; then
    echo "✅ Migration completed successfully"
else
    echo "❌ Migration failed - check logs and consider rollback"
    exit 1
fi
```

### Step 5: Validate Migration

```bash
# Quick validation
echo "🔍 Validating migration..."

# Check database connection
psql $DATABASE_URL -c "SELECT COUNT(*) FROM articles;" || exit 1

# Check configuration
aa config list

# Check topics and RSS feeds
aa topics list
aa rss list

# Run health check
aa health-check

echo "✅ Migration validation completed"
```

### Step 6: Start Services

```bash
# Update application to use PostgreSQL
export DATABASE_URL=postgresql://argus_user:password@localhost/argus_prod

# Start the application
cargo run --release &
APP_PID=$!

# Wait for startup
sleep 10

# Test basic functionality
if curl -f http://localhost:8080/status 2>/dev/null; then
    echo "✅ Application started successfully"
else
    echo "⚠️  Application may not be fully ready yet"
fi

echo "✅ Deployment completed"
```

## Complete Deployment Script

### Create `deploy_postgres.sh`

```bash
#!/bin/bash
set -e

echo "🚀 Starting PostgreSQL migration deployment..."

# Configuration
POSTGRES_DB="argus_prod"
POSTGRES_USER="argus_user"

# Pre-deployment checks
if [ ! -f "argus.db" ]; then
    echo "❌ SQLite database not found"
    exit 1
fi

if [ -z "$DATABASE_URL" ]; then
    echo "❌ DATABASE_URL environment variable not set"
    echo "Set it with: export DATABASE_URL=postgresql://user:pass@localhost/dbname"
    exit 1
fi

# Test PostgreSQL connection
if ! psql $DATABASE_URL -c "SELECT version();" > /dev/null 2>&1; then
    echo "❌ Cannot connect to PostgreSQL database"
    echo "Check your DATABASE_URL and ensure PostgreSQL is running"
    exit 1
fi

echo "✅ Pre-deployment checks passed"

# Build migration tools
echo "🔨 Building migration tools..."
cargo build --release --bin migrate_to_postgres --bin argus_admin

# Stop existing services
echo "🛑 Stopping existing services..."
sudo systemctl stop argus-api 2>/dev/null || true
sudo systemctl stop argus-workers 2>/dev/null || true
pkill -f "target/release/argus" 2>/dev/null || true

# Execute migration
echo "📦 Executing migration..."
./target/release/migrate_to_postgres

if [ $? -ne 0 ]; then
    echo "❌ Migration failed"
    exit 1
fi

# Validate migration
echo "✅ Validating migration..."
./target/release/argus_admin health-check

# Start application with PostgreSQL
echo "▶️  Starting application..."
cargo run --release &
APP_PID=$!

# Wait for startup and test
sleep 15
if ps -p $APP_PID > /dev/null; then
    echo "✅ Application started successfully (PID: $APP_PID)"
else
    echo "❌ Application failed to start"
    exit 1
fi

echo "🎉 PostgreSQL migration deployment completed successfully!"
echo ""
echo "Next steps:"
echo "  - Monitor application logs"
echo "  - Test functionality thoroughly"
echo "  - Use 'aa' command for administration"
echo ""
echo "Administration commands:"
echo "  ./target/release/argus_admin topics list"
echo "  ./target/release/argus_admin rss list"
echo "  ./target/release/argus_admin health-check"
```

### Make script executable

```bash
chmod +x deploy_postgres.sh
```

## Post-Deployment Validation

### Quick Health Check

```bash
# Database connectivity
aa health-check

# Configuration validation
aa config list
aa topics list
aa rss list

# Application status
curl -f http://localhost:8080/status || echo "API not available"

# Check logs for errors
tail -f logs/argus.log | grep ERROR
```

### Extended Validation

```bash
# Test configuration changes
aa topics add "TestTopic" "Test topic for validation"
aa topics list | grep TestTopic
aa topics remove "TestTopic"

# Test RSS feed management
aa rss add "test_feed" "https://example.com/feed.xml"
aa rss list | grep test_feed
aa rss remove "test_feed"

# Monitor performance
echo "Monitoring database queries..."
psql $DATABASE_URL -c "SELECT COUNT(*) FROM articles;"
psql $DATABASE_URL -c "SELECT COUNT(*) FROM configurations;"
```

## Rollback Procedures

### Immediate Rollback (if deployment fails)

```bash
#!/bin/bash
# rollback_to_sqlite.sh

echo "🔄 Rolling back to SQLite..."

# Stop PostgreSQL services
pkill -f "target/release/argus" 2>/dev/null || true

# Find latest backup
LATEST_BACKUP=$(ls -t argus.db.backup.* 2>/dev/null | head -n1)
if [ -z "$LATEST_BACKUP" ]; then
    echo "❌ No SQLite backup found!"
    exit 1
fi

# Restore SQLite database
cp "$LATEST_BACKUP" argus.db
echo "✅ Restored SQLite database from $LATEST_BACKUP"

# Restore environment variables
LATEST_ENV_BACKUP=$(ls -t env.backup.* 2>/dev/null | head -n1)
if [ -n "$LATEST_ENV_BACKUP" ]; then
    source "$LATEST_ENV_BACKUP"
    echo "✅ Restored environment variables"
fi

# Remove PostgreSQL environment
unset DATABASE_URL

# Start with SQLite
cargo run --release &
echo "✅ Rollback completed - running on SQLite"
```

### Make rollback script executable

```bash
chmod +x rollback_to_sqlite.sh
```

## Production Environment Configuration

### PostgreSQL Optimization

Create `/etc/postgresql/13/main/postgresql.conf` optimizations:

```ini
# Memory settings
shared_buffers = 256MB                    # 25% of RAM
work_mem = 4MB                           # Per operation
maintenance_work_mem = 64MB              # For maintenance ops

# Connection settings
max_connections = 100                    # Based on expected load
listen_addresses = 'localhost'          # Security: only local connections

# Performance settings
wal_buffers = 16MB                       # WAL buffering
checkpoint_completion_target = 0.9       # Smooth checkpoints
random_page_cost = 1.1                   # For SSD storage
effective_cache_size = 1GB               # Available for caching

# Logging (for monitoring)
log_statement = 'mod'                    # Log data-modifying statements
log_min_duration_statement = 1000       # Log slow queries (>1s)
```

Restart PostgreSQL after configuration changes:
```bash
sudo systemctl restart postgresql
```

### Environment Variables

Create `/etc/environment` or systemd environment file:

```bash
DATABASE_URL=postgresql://argus_user:password@localhost/argus_prod
RUST_LOG=info
```

### Systemd Service Configuration

Create `/etc/systemd/system/argus.service`:

```ini
[Unit]
Description=Argus News Analysis System
After=postgresql.service
Requires=postgresql.service

[Service]
Type=simple
User=argus
WorkingDirectory=/opt/argus
Environment="DATABASE_URL=postgresql://argus_user:password@localhost/argus_prod"
Environment="RUST_LOG=info"
ExecStart=/opt/argus/target/release/argus
Restart=always
RestartSec=10

[Install]
WantedBy=multi-user.target
```

Enable and start the service:

```bash
sudo systemctl daemon-reload
sudo systemctl enable argus
sudo systemctl start argus
```

## Monitoring and Maintenance

### Health Monitoring Script

Create `monitor_postgres.sh`:

```bash
#!/bin/bash
# Simple health monitoring for PostgreSQL Argus deployment

echo "🏥 Argus PostgreSQL Health Check - $(date)"

# Check PostgreSQL service
if systemctl is-active postgresql > /dev/null; then
    echo "✅ PostgreSQL service: Running"
else
    echo "❌ PostgreSQL service: Not running"
fi

# Check database connectivity
if psql $DATABASE_URL -c "SELECT 1;" > /dev/null 2>&1; then
    echo "✅ Database connectivity: OK"
else
    echo "❌ Database connectivity: Failed"
fi

# Check application
if curl -f http://localhost:8080/status > /dev/null 2>&1; then
    echo "✅ Application API: Responding"
else
    echo "❌ Application API: Not responding"
fi

# Check data counts
ARTICLE_COUNT=$(psql $DATABASE_URL -t -c "SELECT COUNT(*) FROM articles;" 2>/dev/null | tr -d ' ')
CONFIG_COUNT=$(psql $DATABASE_URL -t -c "SELECT COUNT(*) FROM configurations;" 2>/dev/null | tr -d ' ')

echo "📊 Articles: ${ARTICLE_COUNT:-0}"
echo "📊 Configurations: ${CONFIG_COUNT:-0}"

# Check for recent errors (last 10 minutes)
RECENT_ERRORS=$(psql $DATABASE_URL -t -c "
    SELECT COUNT(*) FROM endpoint_alerts 
    WHERE created_at > NOW() - INTERVAL '10 minutes' 
    AND is_resolved = false;" 2>/dev/null | tr -d ' ')

if [ "${RECENT_ERRORS:-0}" -gt 0 ]; then
    echo "⚠️  Recent alerts: $RECENT_ERRORS"
else
    echo "✅ No recent alerts"
fi

echo "---"
```

### Backup Strategy

Create daily backup cron job:

```bash
# Add to crontab (crontab -e)
0 2 * * * /usr/bin/pg_dump $DATABASE_URL > /backup/argus_$(date +\%Y\%m\%d).sql
```

### Performance Monitoring

```sql
-- Monitor slow queries
SELECT query, calls, total_time, mean_time
FROM pg_stat_statements
WHERE mean_time > 100
ORDER BY mean_time DESC;

-- Monitor database size
SELECT pg_size_pretty(pg_database_size('argus_prod'));

-- Monitor table sizes
SELECT schemaname, tablename, pg_size_pretty(pg_total_relation_size(schemaname||'.'||tablename))
FROM pg_tables
WHERE schemaname = 'public'
ORDER BY pg_total_relation_size(schemaname||'.'||tablename) DESC;
```

## Success Criteria

### Deployment Success Indicators
- [ ] Migration completed without errors
- [ ] All data transferred successfully
- [ ] Application starts and responds to requests
- [ ] Configuration management working
- [ ] Admin tool (`aa`) functional
- [ ] No database connection errors in logs

### Performance Validation
- [ ] Query response times acceptable (< 1s for typical queries)
- [ ] No PostgreSQL connection pool exhaustion
- [ ] Memory usage within expected ranges
- [ ] No SQLite locking errors

### Operational Readiness
- [ ] Health monitoring active
- [ ] Backup procedures working
- [ ] Rollback procedures tested and ready
- [ ] Team trained on new admin commands

## Troubleshooting Quick Reference

### Common Issues

**Connection Refused**
```bash
# Check PostgreSQL status
sudo systemctl status postgresql
# Check connection settings
psql $DATABASE_URL -c "SELECT version();"
```

**Migration Fails**
```bash
# Check prerequisites
./target/release/migrate_to_postgres --dry-run  # if implemented
# Check logs and run rollback
./rollback_to_sqlite.sh
```

**Performance Issues**
```bash
# Check database connections
aa health-check
# Monitor query performance
psql $DATABASE_URL -c "SELECT * FROM pg_stat_activity;"
```

**Admin Tool Issues**
```bash
# Rebuild admin tool
cargo build --release --bin argus_admin
# Check database connectivity
./target/release/argus_admin health-check
```

For detailed troubleshooting, see `troubleshooting.md`.

---

**Deployment Timeline**: 1-2 hours  
**Downtime Window**: 15-30 minutes during migration  
**Rollback Time**: 5-10 minutes if needed
