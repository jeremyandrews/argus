# 02 - Prerequisites & Setup

**Effort**: S (1 day)

## PostgreSQL Installation

### Ubuntu/Debian
```bash
sudo apt-get update
sudo apt-get install postgresql postgresql-contrib
sudo systemctl start postgresql
sudo systemctl enable postgresql
```

### macOS
```bash
brew install postgresql
brew services start postgresql
```

### Docker (Development)
```bash
docker run --name argus-postgres \
  -e POSTGRES_DB=argus_dev \
  -e POSTGRES_USER=argus_user \
  -e POSTGRES_PASSWORD=your_secure_password \
  -p 5432:5432 \
  -d postgres:13
```

## Database Setup

### Create Databases
```sql
-- Connect as postgres user
sudo -u postgres psql

-- Create databases
CREATE DATABASE argus_prod;
CREATE DATABASE argus_dev;
CREATE DATABASE argus_test;

-- Create user
CREATE USER argus_user WITH PASSWORD 'your_secure_password';

-- Grant permissions
GRANT ALL PRIVILEGES ON DATABASE argus_prod TO argus_user;
GRANT ALL PRIVILEGES ON DATABASE argus_dev TO argus_user;
GRANT ALL PRIVILEGES ON DATABASE argus_test TO argus_user;

-- Exit
\q
```

### Test Connection
```bash
psql postgresql://argus_user:your_secure_password@localhost/argus_dev -c "SELECT version();"
```

## Backup Strategy

### SQLite Backup
```bash
# Create timestamped backup
cp argus.db argus.db.backup.$(date +%Y%m%d_%H%M%S)

# Verify backup
sqlite3 argus.db.backup.* "SELECT COUNT(*) FROM articles;"
```

### Environment Variables Backup
```bash
# Save current environment configuration
env | grep -E "(TOPICS|URLS|SLACK|OLLAMA)" > env.backup.$(date +%Y%m%d_%H%M%S)
```

## Update Dependencies

### Cargo.toml Changes
```toml
# Add PostgreSQL support (keep SQLite during transition)
sqlx = { version = "0.8", features = ["sqlite", "postgres", "runtime-tokio-rustls", "macros", "uuid", "chrono", "json"] }

# Additional dependencies
serde_json = "1.0"
tokio = { version = "1.0", features = ["full"] }
```

### Environment Configuration
```bash
# Add to .env file
DATABASE_TYPE=postgres  # or sqlite for fallback
DATABASE_URL=postgresql://argus_user:your_secure_password@localhost/argus_prod

# PostgreSQL specific
POSTGRES_HOST=localhost  
POSTGRES_PORT=5432
POSTGRES_USER=argus_user
POSTGRES_PASSWORD=your_secure_password
POSTGRES_DB=argus_prod

# Keep SQLite for fallback
SQLITE_PATH=argus.db
```

## Verification Checklist
- [ ] PostgreSQL server running
- [ ] Databases created successfully
- [ ] User permissions configured
- [ ] Connection test successful
- [ ] SQLite backup created
- [ ] Environment variables saved
- [ ] Dependencies updated
- [ ] Development environment ready
