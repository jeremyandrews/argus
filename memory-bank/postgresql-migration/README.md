# PostgreSQL Migration Plan

This directory contains the complete migration plan for moving Argus from SQLite to PostgreSQL with database-driven configuration management.

## Migration Overview

**Goal**: Eliminate SQLite concurrency issues and enable runtime configuration management  
**Effort**: L (few days to week)  
**Risk**: Medium (well-isolated database layer changes)

## File Structure

| File | Description | Effort | Status |
|------|-------------|--------|---------|
| [01-migration-overview.md](01-migration-overview.md) | Problem, solution, timeline, risks | - | 📋 Plan |
| [02-prerequisites.md](02-prerequisites.md) | PostgreSQL setup, dependencies, backups | S (1 day) | 🔧 Setup |
| [03-schema-migration.md](03-schema-migration.md) | PostgreSQL schema design and creation | M (2 days) | 🏗️ Build |
| [04-data-migration.md](04-data-migration.md) | Export/import procedures and validation | M (2 days) | 📥 Migrate |
| [05-code-changes.md](05-code-changes.md) | Database abstraction and integration | L (few days) | 💻 Code |
| [06-configuration-migration.md](06-configuration-migration.md) | Environment to database migration | S (1 day) | ⚙️ Config |
| [07-testing-validation.md](07-testing-validation.md) | Testing procedures and validation | S (1 day) | 🧪 Test |
| [08-deployment.md](08-deployment.md) | Production deployment and rollback | S (1 day) | 🚀 Deploy |
| [09-post-migration.md](09-post-migration.md) | Optimization and maintenance | XS (ongoing) | 🔧 Maintain |

## Quick Start

### Prerequisites
```bash
# Install PostgreSQL
sudo apt-get install postgresql postgresql-contrib  # Ubuntu
brew install postgresql                               # macOS

# Create database and user
sudo -u postgres createdb argus_prod
sudo -u postgres createuser argus_user
```

### Migration Process
```bash
# 1. Setup and preparation
./setup_prerequisites.sh

# 2. Create PostgreSQL schema  
cargo run --bin create_postgres_schema

# 3. Migrate data
./migrate_data.sh

# 4. Migrate configuration
./migrate_config.sh

# 5. Test everything
./test_migration.sh

# 6. Deploy to production
./deploy_production.sh
```

## Key Benefits

- **Eliminate SQLite locking errors** under concurrent load
- **20-50% performance improvement** for database operations
- **Runtime configuration management** without restarts
- **Scalable architecture** ready for public deployment

## Critical Files Created

### Migration Tools
- `src/bin/create_postgres_schema.rs` - Schema creation
- `src/bin/export_sqlite_data.rs` - Data export
- `src/bin/import_postgres_data.rs` - Data import
- `src/bin/migrate_env_to_db.rs` - Configuration migration
- `src/bin/validate_migration.rs` - Migration validation

### Runtime Management
- `src/bin/manage_topics.rs` - Topic management
- `src/bin/manage_feeds.rs` - RSS feed management
- `src/config/mod.rs` - Configuration manager
- `src/db/config.rs` - Database configuration

### Testing & Monitoring
- `src/bin/test_postgresql_functionality.rs` - Functionality tests
- `src/bin/test_postgresql_performance.rs` - Performance tests
- `monitor_postgres.sh` - Health monitoring
- `backup_config.sh` - Configuration backup

## Configuration Categories

| Category | Description | Examples |
|----------|-------------|----------|
| `topics` | Topic definitions and prompts | "Space: Space exploration news" |
| `rss_feeds` | RSS feed URLs | "https://hnrss.org/frontpage" |
| `system` | System settings | slack_token, rust_log |

## Runtime Management Commands

```bash
# Topic management
cargo run --bin manage_topics list
cargo run --bin manage_topics add "AI" "Artificial Intelligence news"
cargo run --bin manage_topics remove "OldTopic"

# RSS feed management  
cargo run --bin manage_feeds list
cargo run --bin manage_feeds add "hn" "https://hnrss.org/frontpage"
cargo run --bin manage_feeds remove "old_feed"

# Configuration backup
./backup_config.sh
```

## Rollback Plan

If issues occur during migration:

```bash
# Stop PostgreSQL services
systemctl stop argus-*

# Restore SQLite backup
cp argus.db.final_backup.YYYYMMDD_HHMMSS argus.db

# Restore environment variables
source env.backup.YYYYMMDD_HHMMSS

# Switch back to SQLite
export DATABASE_TYPE=sqlite

# Restart services
systemctl start argus-*
```

## Post-Migration Tasks

1. **Remove SQLite dependencies** from Cargo.toml
2. **Setup monitoring** with PostgreSQL statistics
3. **Configure automated backups** for database and configuration
4. **Optimize performance** based on usage patterns
5. **Update documentation** for new configuration management

## Support

For questions or issues during migration:
1. Check the specific file for detailed procedures
2. Review validation scripts for troubleshooting
3. Use rollback procedures if needed
4. Test in staging environment first

## Migration Checklist

- [ ] **Prerequisites**: PostgreSQL installed and configured
- [ ] **Schema**: PostgreSQL schema created successfully  
- [ ] **Data**: All data migrated and validated
- [ ] **Configuration**: Environment variables moved to database
- [ ] **Testing**: All tests passing
- [ ] **Deployment**: Production deployment successful
- [ ] **Validation**: Post-deployment checks completed
- [ ] **Monitoring**: Health checks and monitoring active
- [ ] **Cleanup**: SQLite dependencies removed
- [ ] **Documentation**: Team trained on new configuration management

---

**Status**: Ready for execution  
**Last Updated**: June 2025  
**Owner**: Engineering Team
