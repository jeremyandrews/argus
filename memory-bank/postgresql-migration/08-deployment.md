# 08 - Production Deployment

**Effort**: S (1 day)

## Pre-Deployment Checklist

### Environment Preparation
- [ ] PostgreSQL server configured and running
- [ ] Database user and permissions set up
- [ ] Connection pooling configured (pgbouncer recommended)
- [ ] Backup strategy in place
- [ ] Monitoring setup ready
- [ ] All tests passed in staging environment

### Code Preparation
- [ ] All migration binaries built and tested
- [ ] Configuration migration tested
- [ ] Application code updated for PostgreSQL
- [ ] Environment variables configured
- [ ] Rollback procedures documented

## Deployment Strategy

### Blue-Green Deployment Approach
1. **Current (Blue)**: SQLite-based system running
2. **New (Green)**: PostgreSQL-based system prepared
3. **Cutover**: Switch traffic from Blue to Green
4. **Rollback**: Switch back to Blue if issues occur

## Production Deployment Steps

### Step 1: Stop Services
```bash
echo "🛑 Stopping Argus services..."

# Stop all running services
systemctl stop argus-api || true
systemctl stop argus-workers || true

# Kill any remaining processes
pkill -f "target/release/argus" || true
pkill -f "cargo run" || true

echo "✅ Services stopped"
```

### Step 2: Final Backup
```bash
echo "💾 Creating final backup..."

# Create timestamped backup
BACKUP_DATE=$(date +%Y%m%d_%H%M%S)
cp argus.db "argus.db.final_backup.$BACKUP_DATE"

# Verify backup
sqlite3 "argus.db.final_backup.$BACKUP_DATE" "SELECT COUNT(*) FROM articles;" > /dev/null
echo "✅ Backup created and verified: argus.db.final_backup.$BACKUP_DATE"

# Also backup current environment
env | grep -E "(TOPICS|URLS|SLACK)" > "env.backup.$BACKUP_DATE"
echo "✅ Environment variables backed up"
```

### Step 3: Deploy PostgreSQL Configuration
```bash
echo "🔧 Deploying PostgreSQL configuration..."

# Set environment variables
export DATABASE_TYPE=postgres
export DATABASE_URL=postgresql://argus_user:password@localhost/argus_prod

# Verify PostgreSQL connection
psql $DATABASE_URL -c "SELECT version();" > /dev/null
echo "✅ PostgreSQL connection verified"

# Create schema
cargo run --release --bin create_postgres_schema
echo "✅ PostgreSQL schema created"
```

### Step 4: Migrate Data
```bash
echo "📥 Migrating data to PostgreSQL..."

# Run data migration
./migrate_data.sh

# Validate migration
cargo run --release --bin validate_migration
echo "✅ Data migration completed and validated"
```

### Step 5: Migrate Configuration
```bash
echo "⚙️  Migrating configuration..."

# Run configuration migration
./migrate_config.sh

# Validate configuration
cargo run --release --bin validate_config
echo "✅ Configuration migration completed and validated"
```

### Step 6: Deploy Updated Application
```bash
echo "🚀 Deploying updated application..."

# Build production binary
cargo build --release

# Update systemd services (if using systemd)
sudo systemctl daemon-reload

# Update environment file
cat > /etc/argus/environment << EOF
DATABASE_TYPE=postgres
DATABASE_URL=postgresql://argus_user:password@localhost/argus_prod
EOF

echo "✅ Application deployed"
```

### Step 7: Start Services
```bash
echo "▶️  Starting PostgreSQL-based services..."

# Start services
systemctl start argus-api
systemctl start argus-workers

# Wait for startup
sleep 10

# Verify services are running
systemctl is-active argus-api
systemctl is-active argus-workers

echo "✅ Services started"
```

### Step 8: Validation
```bash
echo "✅ Running post-deployment validation..."

# Run comprehensive tests
./test_migration.sh

# Test API endpoints (if applicable)
curl -f http://localhost:8080/status || echo "❌ API health check failed"

# Test configuration management
cargo run --release --bin manage_topics list > /dev/null
cargo run --release --bin manage_feeds list > /dev/null

echo "✅ Post-deployment validation completed"
```

## Complete Deployment Script

### Create deploy_production.sh
```bash
#!/bin/bash
set -e

echo "🚀 Starting PostgreSQL migration deployment..."

# Pre-deployment checks
if [ ! -f "argus.db" ]; then
    echo "❌ SQLite database not found"
    exit 1
fi

if ! psql $DATABASE_URL -c "SELECT version();" > /dev/null 2>&1; then
    echo "❌ Cannot connect to PostgreSQL"
    exit 1
fi

# Step 1: Stop services
echo "🛑 Stopping services..."
systemctl stop argus-api || true
systemctl stop argus-workers || true
pkill -f "target/release/argus" || true

# Step 2: Backup
echo "💾 Creating backups..."
BACKUP_DATE=$(date +%Y%m%d_%H%M%S)
cp argus.db "argus.db.final_backup.$BACKUP_DATE"
env | grep -E "(TOPICS|URLS|SLACK)" > "env.backup.$BACKUP_DATE"

# Step 3: Build and deploy
echo "🔨 Building application..."
cargo build --release

# Step 4: Set environment
echo "🔧 Configuring environment..."
export DATABASE_TYPE=postgres

# Step 5: Create schema
echo "🏗️  Creating PostgreSQL schema..."
cargo run --release --bin create_postgres_schema

# Step 6: Migrate data
echo "📥 Migrating data..."
mkdir -p migration_data
cargo run --release --bin export_sqlite_data
cargo run --release --bin import_postgres_data

# Step 7: Migrate configuration
echo "⚙️  Migrating configuration..."
cargo run --release --bin migrate_env_to_db

# Step 8: Validate migration
echo "✅ Validating migration..."
cargo run --release --bin validate_migration
cargo run --release --bin validate_config

# Step 9: Start services
echo "▶️  Starting services..."
systemctl start argus-api
systemctl start argus-workers

# Step 10: Final validation
echo "🧪 Running final tests..."
sleep 10
systemctl is-active argus-api
systemctl is-active argus-workers

# Test basic functionality
curl -f http://localhost:8080/status || echo "⚠️  API health check failed"

echo "🎉 PostgreSQL migration deployment completed successfully!"
echo ""
echo "Deployment summary:"
echo "  - SQLite backup: argus.db.final_backup.$BACKUP_DATE"
echo "  - Environment backup: env.backup.$BACKUP_DATE"
echo "  - Database: PostgreSQL"
echo "  - Configuration: Database-driven"
echo ""
echo "Post-deployment commands:"
echo "  - View topics: cargo run --release --bin manage_topics list"
echo "  - View feeds: cargo run --release --bin manage_feeds list"
echo "  - Add topic: cargo run --release --bin manage_topics add \"Name\" \"Prompt\""
```

## Rollback Procedures

### Immediate Rollback (if deployment fails)
```bash
#!/bin/bash
echo "🔄 Rolling back to SQLite..."

# Stop PostgreSQL services
systemctl stop argus-api || true
systemctl stop argus-workers || true

# Restore environment
export DATABASE_TYPE=sqlite
export SQLITE_PATH=argus.db

# Find latest backup
LATEST_BACKUP=$(ls -t argus.db.final_backup.* | head -n1)
if [ -z "$LATEST_BACKUP" ]; then
    echo "❌ No backup found!"
    exit 1
fi

# Restore SQLite database
cp "$LATEST_BACKUP" argus.db
echo "✅ Restored SQLite database from $LATEST_BACKUP"

# Restore environment variables
LATEST_ENV_BACKUP=$(ls -t env.backup.* | head -n1)
if [ -n "$LATEST_ENV_BACKUP" ]; then
    source "$LATEST_ENV_BACKUP"
    echo "✅ Restored environment variables"
fi

# Build SQLite version
cargo build --release

# Start services
systemctl start argus-api
systemctl start argus-workers

echo "✅ Rollback completed - running on SQLite"
```

### Gradual Rollback (for performance issues)
If performance issues are discovered later:

1. **Monitor and assess**: Use PostgreSQL monitoring to identify issues
2. **Attempt fixes**: Apply PostgreSQL optimizations
3. **Rollback if needed**: Use immediate rollback procedure
4. **Analyze**: Review migration and fix issues
5. **Re-attempt**: Plan another migration attempt

## Production Environment Configuration

### PostgreSQL Tuning
```sql
-- postgresql.conf optimizations for production
shared_buffers = 256MB                    -- 25% of RAM
work_mem = 4MB                           -- Per operation
maintenance_work_mem = 64MB              -- For maintenance ops
max_connections = 100                    -- Based on load
wal_buffers = 16MB                       -- WAL buffering
checkpoint_completion_target = 0.9       -- Smooth checkpoints
random_page_cost = 1.1                   -- For SSD storage
effective_cache_size = 1GB               -- Available for caching
```

### Systemd Service Updates
```ini
# /etc/systemd/system/argus-api.service
[Unit]
Description=Argus API Server
After=postgresql.service
Requires=postgresql.service

[Service]
Type=simple
User=argus
WorkingDirectory=/opt/argus
Environment="DATABASE_TYPE=postgres"
Environment="DATABASE_URL=postgresql://argus_user:password@localhost/argus_prod"
ExecStart=/opt/argus/target/release/argus-api
Restart=always

[Install]
WantedBy=multi-user.target
```

## Monitoring and Health Checks

### Health Check Script
```bash
#!/bin/bash
# health_check.sh

echo "🏥 Running health checks..."

# Check PostgreSQL connection
if ! psql $DATABASE_URL -c "SELECT 1;" > /dev/null 2>&1; then
    echo "❌ PostgreSQL connection failed"
    exit 1
fi

# Check service status
if ! systemctl is-active argus-api > /dev/null; then
    echo "❌ API service not running"
    exit 1
fi

if ! systemctl is-active argus-workers > /dev/null; then
    echo "❌ Workers service not running"
    exit 1
fi

# Check API endpoint
if ! curl -f http://localhost:8080/status > /dev/null 2>&1; then
    echo "❌ API endpoint not responding"
    exit 1
fi

# Check database queries
ARTICLE_COUNT=$(psql $DATABASE_URL -t -c "SELECT COUNT(*) FROM articles;")
if [ -z "$ARTICLE_COUNT" ] || [ "$ARTICLE_COUNT" -eq 0 ]; then
    echo "❌ No articles found in database"
    exit 1
fi

echo "✅ All health checks passed"
echo "  - PostgreSQL: Connected"
echo "  - API Service: Running"
echo "  - Workers: Running"
echo "  - Articles: $ARTICLE_COUNT"
```

## Success Criteria

### Deployment Success
- [ ] All services started successfully
- [ ] Database connections working
- [ ] No data loss detected
- [ ] API endpoints responding
- [ ] Configuration management functional
- [ ] Performance acceptable
- [ ] No error logs

### Ready for Operation
- [ ] Runtime topic management working
- [ ] RSS feed management working  
- [ ] Background workers processing
- [ ] Monitoring in place
- [ ] Backup procedures validated
- [ ] Rollback procedures tested

## Execution Timeline

1. **T-60 min**: Final preparation and testing
2. **T-30 min**: Team briefing and readiness check
3. **T-15 min**: Begin service shutdown
4. **T-0**: Execute deployment script
5. **T+30 min**: Complete validation
6. **T+60 min**: Monitor for issues
7. **T+120 min**: Deployment complete or rollback decision
