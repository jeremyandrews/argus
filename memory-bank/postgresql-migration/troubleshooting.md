# PostgreSQL Migration Troubleshooting Guide

Common issues and solutions for the Argus PostgreSQL migration.

## Migration Issues

### Migration Fails to Start

**Symptoms:**
- `migrate_to_postgres` exits immediately
- "Prerequisites check failed" error

**Solutions:**
```bash
# Check SQLite database exists
ls -la argus.db

# Check DATABASE_URL is set
echo $DATABASE_URL

# Test PostgreSQL connection manually
psql $DATABASE_URL -c "SELECT version();"

# Check prerequisites manually
./target/release/migrate_to_postgres --help
```

### Data Migration Errors

**Symptoms:**
- SQLite dump fails
- PostgreSQL import errors
- Row count mismatch

**Solutions:**
```bash
# Manual SQLite dump test
sqlite3 argus.db ".dump" > test_dump.sql
head -50 test_dump.sql

# Check SQLite integrity
sqlite3 argus.db "PRAGMA integrity_check;"

# Manual PostgreSQL import test
psql $DATABASE_URL -f test_dump.sql

# Check specific table migrations
psql $DATABASE_URL -c "SELECT COUNT(*) FROM articles;"
sqlite3 argus.db "SELECT COUNT(*) FROM articles;"
```

### Schema Creation Fails

**Symptoms:**
- "Permission denied" errors
- "Table already exists" errors
- Trigger creation failures

**Solutions:**
```bash
# Check PostgreSQL permissions
psql $DATABASE_URL -c "SELECT current_user, session_user;"

# Drop existing schema (CAUTION: Only in development)
psql $DATABASE_URL -c "DROP SCHEMA public CASCADE; CREATE SCHEMA public;"

# Manual schema creation
psql $DATABASE_URL -f memory-bank/postgresql-migration/schema.sql

# Check existing tables
psql $DATABASE_URL -c "\dt"
```

### Configuration Migration Issues

**Symptoms:**
- Environment variables not migrated
- Missing topics or RSS feeds
- Invalid configuration format

**Solutions:**
```bash
# Check current environment variables
env | grep -E "(TOPICS|URLS|SLACK)"

# Manual configuration migration
aa config set rust_log "info"
aa topics add "Test" "Test topic prompt"
aa rss add "test" "https://example.com/rss"

# Check configuration in database
psql $DATABASE_URL -c "SELECT * FROM configurations;"
```

## PostgreSQL Connection Issues

### Connection Refused

**Symptoms:**
- "Connection refused" errors
- "Could not connect to server" messages

**Solutions:**
```bash
# Check PostgreSQL service status
sudo systemctl status postgresql

# Start PostgreSQL if stopped
sudo systemctl start postgresql

# Check PostgreSQL is listening
sudo netstat -tlnp | grep 5432

# Test local connection
sudo -u postgres psql -c "SELECT version();"

# Check pg_hba.conf authentication
sudo tail -10 /etc/postgresql/*/main/pg_hba.conf
```

### Authentication Failed

**Symptoms:**
- "Authentication failed" errors
- "Password authentication failed" messages

**Solutions:**
```bash
# Reset user password
sudo -u postgres psql -c "ALTER USER argus_user PASSWORD 'newpassword';"

# Update DATABASE_URL with new password
export DATABASE_URL=postgresql://argus_user:newpassword@localhost/argus_prod

# Check user exists and has permissions
sudo -u postgres psql -c "SELECT usename, usecreatedb, usesuper FROM pg_user WHERE usename = 'argus_user';"

# Grant database permissions
sudo -u postgres psql -c "GRANT ALL PRIVILEGES ON DATABASE argus_prod TO argus_user;"
```

### Database Does Not Exist

**Symptoms:**
- "Database does not exist" errors
- Connection to non-existent database

**Solutions:**
```bash
# List existing databases
sudo -u postgres psql -c "\l"

# Create missing database
sudo -u postgres createdb argus_prod

# Ensure correct DATABASE_URL
echo $DATABASE_URL

# Test connection to correct database
psql $DATABASE_URL -c "SELECT current_database();"
```

## Performance Issues

### Slow Query Performance

**Symptoms:**
- Application responds slowly
- Database queries take > 1 second
- High CPU usage on database

**Solutions:**
```bash
# Check slow queries
psql $DATABASE_URL -c "
SELECT query, calls, total_time, mean_time 
FROM pg_stat_statements 
WHERE mean_time > 100 
ORDER BY mean_time DESC LIMIT 10;"

# Check missing indexes
psql $DATABASE_URL -c "
SELECT schemaname, tablename, attname, n_distinct, correlation 
FROM pg_stats 
WHERE schemaname = 'public' 
ORDER BY n_distinct DESC;"

# Analyze table statistics
psql $DATABASE_URL -c "ANALYZE;"

# Check index usage
psql $DATABASE_URL -c "
SELECT indexrelname, idx_tup_read, idx_tup_fetch 
FROM pg_stat_user_indexes 
ORDER BY idx_tup_read DESC;"
```

### Connection Pool Exhaustion

**Symptoms:**
- "Too many connections" errors
- Application hangs on database operations
- Connection timeouts

**Solutions:**
```bash
# Check current connections
psql $DATABASE_URL -c "SELECT count(*) FROM pg_stat_activity;"

# Check connection limit
psql $DATABASE_URL -c "SHOW max_connections;"

# Kill idle connections
psql $DATABASE_URL -c "
SELECT pg_terminate_backend(pid) 
FROM pg_stat_activity 
WHERE state = 'idle' AND state_change < NOW() - INTERVAL '1 hour';"

# Increase connection limit (in postgresql.conf)
sudo sed -i 's/max_connections = 100/max_connections = 200/' /etc/postgresql/*/main/postgresql.conf
sudo systemctl restart postgresql
```

### High Memory Usage

**Symptoms:**
- PostgreSQL using excessive memory
- System running out of memory
- OOM killer terminating processes

**Solutions:**
```bash
# Check PostgreSQL memory settings
psql $DATABASE_URL -c "SHOW shared_buffers;"
psql $DATABASE_URL -c "SHOW work_mem;"

# Monitor memory usage
ps aux | grep postgres | awk '{print $4,$11}' | sort -nr

# Optimize memory settings (postgresql.conf)
shared_buffers = 128MB        # Reduce if needed
work_mem = 2MB               # Reduce for high connection counts
maintenance_work_mem = 32MB   # For maintenance operations

# Restart PostgreSQL after changes
sudo systemctl restart postgresql
```

## Admin Tool Issues

### Command Not Found

**Symptoms:**
- `aa` command not found
- `argus_admin` not found

**Solutions:**
```bash
# Check if binary exists
ls -la target/release/argus_admin

# Rebuild admin tool
cargo build --release --bin argus_admin

# Create alias manually
alias aa='./target/release/argus_admin'

# Add to shell profile
echo "alias aa='$(pwd)/target/release/argus_admin'" >> ~/.bashrc
source ~/.bashrc

# Use full path
./target/release/argus_admin health-check
```

### Database Connection Errors in Admin Tool

**Symptoms:**
- `aa health-check` fails
- "Cannot connect to database" in admin tool

**Solutions:**
```bash
# Check DATABASE_URL in admin tool context
aa status  # Should show connection error details

# Test database connection manually
psql $DATABASE_URL -c "SELECT 1;"

# Check environment variables
env | grep DATABASE_URL

# Use admin tool with explicit DATABASE_URL
DATABASE_URL=postgresql://user:pass@localhost/db aa health-check
```

### Configuration Commands Fail

**Symptoms:**
- `aa topics add` fails
- Configuration not persisting
- "Permission denied" on configuration changes

**Solutions:**
```bash
# Check database permissions
psql $DATABASE_URL -c "SELECT * FROM information_schema.table_privileges WHERE table_name = 'configurations';"

# Test manual configuration
psql $DATABASE_URL -c "INSERT INTO configurations (category, name, value) VALUES ('test', 'key', 'value');"

# Check configuration table exists
psql $DATABASE_URL -c "\d configurations"

# Grant necessary permissions
psql $DATABASE_URL -c "GRANT ALL ON configurations TO argus_user;"
```

## Application Issues

### Application Won't Start

**Symptoms:**
- Cargo run fails immediately
- "Database connection failed" errors
- Segmentation faults

**Solutions:**
```bash
# Check DATABASE_URL is set for application
echo $DATABASE_URL

# Test database connection before starting app
psql $DATABASE_URL -c "SELECT 1;"

# Check for missing dependencies
cargo check

# Run with verbose logging
RUST_LOG=debug cargo run --release

# Check for port conflicts
netstat -tlnp | grep 8080

# Check system resources
free -h
df -h
```

### Runtime Configuration Not Working

**Symptoms:**
- Changes via `aa` not reflected in application
- Application using old configuration
- Environment variables still being used

**Solutions:**
```bash
# Check application is using database configuration
aa config list

# Restart application after configuration changes
pkill -f "target/release/argus"
cargo run --release &

# Check application logs for configuration loading
tail -f logs/argus.log | grep config

# Verify configuration in database
psql $DATABASE_URL -c "SELECT category, name, value FROM configurations WHERE enabled = true;"
```

### Data Inconsistencies

**Symptoms:**
- Missing articles after migration
- Duplicate data
- Foreign key constraint violations

**Solutions:**
```bash
# Check data integrity
psql $DATABASE_URL -c "
SELECT 'article_entities' as table_name, COUNT(*) as orphaned_records
FROM article_entities ae 
LEFT JOIN articles a ON ae.article_id = a.id 
WHERE a.id IS NULL;"

# Fix orphaned records
psql $DATABASE_URL -c "
DELETE FROM article_entities 
WHERE article_id NOT IN (SELECT id FROM articles);"

# Reset sequences if needed
psql $DATABASE_URL -c "
SELECT setval('articles_id_seq', COALESCE((SELECT MAX(id) FROM articles), 1));"

# Check for duplicates
psql $DATABASE_URL -c "
SELECT normalized_url, COUNT(*) 
FROM articles 
GROUP BY normalized_url 
HAVING COUNT(*) > 1;"
```

## General Debugging

### Enable Detailed Logging

```bash
# PostgreSQL logging (postgresql.conf)
log_statement = 'all'                    # Log all statements
log_min_duration_statement = 0           # Log all queries with duration
log_line_prefix = '%t [%p]: [%l-1] '    # Add timestamp and process ID

# Application logging
RUST_LOG=debug cargo run --release

# Admin tool logging
RUST_LOG=debug aa health-check
```

### Database Health Check

```bash
# Complete health check
psql $DATABASE_URL << EOF
SELECT 'Database' as check_type, current_database() as result;
SELECT 'User' as check_type, current_user as result;
SELECT 'Tables' as check_type, count(*)::text as result FROM information_schema.tables WHERE table_schema = 'public';
SELECT 'Articles' as check_type, count(*)::text as result FROM articles;
SELECT 'Configurations' as check_type, count(*)::text as result FROM configurations;
EOF
```

### Emergency Recovery

```bash
# If all else fails, rollback to SQLite
./rollback_to_sqlite.sh

# Or restore from PostgreSQL backup
pg_restore -d $DATABASE_URL backup_file.sql

# Or restart migration from scratch
psql $DATABASE_URL -c "DROP SCHEMA public CASCADE; CREATE SCHEMA public;"
./target/release/migrate_to_postgres
```

## Prevention and Monitoring

### Setup Monitoring

```bash
# Create monitoring script
cat > check_argus.sh << 'EOF'
#!/bin/bash
echo "=== Argus Health Check ===" 
date
echo "PostgreSQL: $(systemctl is-active postgresql)"
echo "Database: $(psql $DATABASE_URL -t -c 'SELECT 1' 2>/dev/null && echo 'OK' || echo 'FAIL')"
echo "Articles: $(psql $DATABASE_URL -t -c 'SELECT COUNT(*) FROM articles' 2>/dev/null)"
echo "App Status: $(curl -s http://localhost:8080/status > /dev/null && echo 'OK' || echo 'FAIL')"
echo "=========================="
EOF

chmod +x check_argus.sh

# Run every 5 minutes
echo "*/5 * * * * /path/to/check_argus.sh >> /var/log/argus_health.log" | crontab -
```

### Regular Maintenance

```bash
# Weekly database maintenance
psql $DATABASE_URL -c "VACUUM ANALYZE;"

# Monthly backup
pg_dump $DATABASE_URL > /backup/argus_$(date +%Y%m%d).sql

# Log rotation
sudo logrotate /etc/logrotate.d/postgresql-common
```

## Quick Reference Commands

```bash
# Essential debugging commands
psql $DATABASE_URL -c "SELECT version();"                    # Test connection
aa health-check                                              # Admin tool health
./target/release/migrate_to_postgres                         # Re-run migration
./rollback_to_sqlite.sh                                      # Emergency rollback
systemctl status postgresql                                  # PostgreSQL status
tail -f /var/log/postgresql/postgresql-*-main.log           # PostgreSQL logs
RUST_LOG=debug cargo run --release                          # Debug application

# Performance monitoring
psql $DATABASE_URL -c "SELECT * FROM pg_stat_activity;"     # Active connections
psql $DATABASE_URL -c "SELECT * FROM pg_stat_user_tables;"  # Table statistics
htop                                                         # System resources
iotop                                                        # Disk I/O

# Configuration management
aa topics list                                               # List topics
aa config list                                               # List configuration
psql $DATABASE_URL -c "SELECT * FROM configurations;"       # Database config
```

For additional help, refer to:
- PostgreSQL documentation: https://www.postgresql.org/docs/
- Argus project README.md
- Migration implementation details in `migration-implementation.md`

---

**Remember**: Always test fixes in a development environment before applying to production. When in doubt, use the rollback procedures to restore stability.
