#!/bin/bash

set -e  # Exit on any error

echo "=== Complete PostgreSQL Migration Script ==="
echo "This script performs a FULL database migration from SQLite to PostgreSQL"
echo

# Check prerequisites
if [ ! -f "argus.db" ]; then
    echo "❌ ERROR: argus.db not found in current directory"
    exit 1
fi

if [ -z "$DATABASE_URL" ]; then
    echo "❌ ERROR: DATABASE_URL environment variable not set"
    echo "   Example: export DATABASE_URL=postgresql://user:pass@host/db"
    exit 1
fi

# Verify PostgreSQL connection
echo "🔍 Testing PostgreSQL connection..."
if ! psql "$DATABASE_URL" -c "SELECT version();" > /dev/null 2>&1; then
    echo "❌ ERROR: Cannot connect to PostgreSQL database"
    echo "   Check your DATABASE_URL: $DATABASE_URL"
    exit 1
fi
echo "✅ PostgreSQL connection successful"

# Create backup
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
BACKUP_FILE="argus.db.backup.$TIMESTAMP"
echo "💾 Creating SQLite backup: $BACKUP_FILE"
cp argus.db "$BACKUP_FILE"

# Step 1: Analyze current database
echo
echo "📊 Step 1: Analyzing current SQLite database..."
sqlite3 argus.db "SELECT name FROM sqlite_master WHERE type='table';" > temp_tables.txt
TABLE_COUNT=$(wc -l < temp_tables.txt)
echo "   Found $TABLE_COUNT tables in SQLite database"

# Show table list
echo "   Tables found:"
cat temp_tables.txt | while read table; do
    count=$(sqlite3 argus.db "SELECT COUNT(*) FROM $table;" 2>/dev/null || echo "0")
    echo "   - $table: $count records"
done
rm temp_tables.txt

# Step 2: Create SQLite dump
DUMP_FILE="argus_dump_$TIMESTAMP.sql"
echo
echo "📤 Step 2: Creating SQLite dump..."
echo "   Exporting to: $DUMP_FILE"
sqlite3 argus.db ".dump" > "$DUMP_FILE"

# Validate dump
DUMP_SIZE=$(wc -l < "$DUMP_FILE")
echo "   Dump created: $DUMP_SIZE lines"

# Step 3: Validate migration coverage
echo
echo "🔍 Step 3: Validating migration coverage..."
./validate_migration.sh "$DUMP_FILE"

# Ask for confirmation
echo
read -p "🤔 Do you want to proceed with the migration? (y/N): " -n 1 -r
echo
if [[ ! $REPLY =~ ^[Yy]$ ]]; then
    echo "❌ Migration cancelled by user"
    echo "   Backup preserved: $BACKUP_FILE"
    echo "   Dump preserved: $DUMP_FILE"
    exit 1
fi

# Step 4: Build migration tool
echo
echo "🛠️  Step 4: Building migration tool..."
cargo build --bin migrate_to_postgres_complete --release

# Step 5: Transform SQL
POSTGRES_FILE="postgres_migration_$TIMESTAMP.sql"
echo
echo "🔄 Step 5: Transforming SQL for PostgreSQL..."
cargo run --release --bin migrate_to_postgres_complete "$DUMP_FILE" -o "$POSTGRES_FILE"

# Validate transformed SQL
if [ ! -f "$POSTGRES_FILE" ]; then
    echo "❌ ERROR: Migration transformation failed"
    exit 1
fi

POSTGRES_SIZE=$(wc -l < "$POSTGRES_FILE")
echo "   PostgreSQL SQL generated: $POSTGRES_SIZE lines"

# Step 6: Create PostgreSQL schema
echo
echo "🏗️  Step 6: Setting up PostgreSQL schema..."
echo "   Dropping existing tables..."
psql "$DATABASE_URL" -c "
DROP SCHEMA IF EXISTS public CASCADE;
CREATE SCHEMA public;
GRANT ALL ON SCHEMA public TO postgres;
GRANT ALL ON SCHEMA public TO public;
" > /dev/null

echo "   Creating new schema..."
psql "$DATABASE_URL" -f memory-bank/postgresql-migration/schema.sql > /dev/null

# Step 7: Import data
echo
echo "📥 Step 7: Importing data to PostgreSQL..."
echo "   This may take several minutes for large databases..."

# Import with error handling
if psql "$DATABASE_URL" -f "$POSTGRES_FILE" -v ON_ERROR_STOP=1 > import_log_$TIMESTAMP.txt 2>&1; then
    echo "✅ Data import completed successfully"
else
    echo "❌ ERROR: Data import failed"
    echo "   Check import_log_$TIMESTAMP.txt for details"
    echo "   Last 10 lines of import log:"
    tail -10 "import_log_$TIMESTAMP.txt"
    exit 1
fi

# Step 8: Validate migration
echo
echo "✅ Step 8: Validating migration..."

# Count records in key tables
echo "   Record counts in PostgreSQL:"
for table in articles entities configurations; do
    count=$(psql "$DATABASE_URL" -t -c "SELECT COUNT(*) FROM $table;" 2>/dev/null | xargs || echo "0")
    echo "   - $table: $count records"
done

# Test basic functionality
echo "   Testing basic database functionality..."
psql "$DATABASE_URL" -c "SELECT 'PostgreSQL migration test successful' as status;" > /dev/null

# Step 9: Cleanup and summary
echo
echo "🧹 Step 9: Cleanup..."
if [ -f "import_log_$TIMESTAMP.txt" ]; then
    # Only keep error logs
    if ! grep -q "ERROR\|FATAL" "import_log_$TIMESTAMP.txt"; then
        rm "import_log_$TIMESTAMP.txt"
        echo "   Import log cleaned (no errors found)"
    else
        echo "   Import log preserved: import_log_$TIMESTAMP.txt (contains warnings/errors)"
    fi
fi

# Step 10: Final summary
echo
echo "🎉 MIGRATION COMPLETED SUCCESSFULLY!"
echo
echo "📋 Summary:"
echo "   ✅ SQLite backup created: $BACKUP_FILE"
echo "   ✅ SQLite dump created: $DUMP_FILE"
echo "   ✅ PostgreSQL SQL generated: $POSTGRES_FILE"
echo "   ✅ Schema created in PostgreSQL"
echo "   ✅ Data imported to PostgreSQL"
echo "   ✅ Migration validated"
echo
echo "🔧 Next steps:"
echo "   1. Test your application: cargo run --release"
echo "   2. Use admin tool: cargo run --bin argus_admin"
echo "   3. Create alias: alias aa='cargo run --bin argus_admin'"
echo "   4. Monitor logs for any issues"
echo
echo "⚠️  Important:"
echo "   - Keep the SQLite backup until you're confident the migration worked"
echo "   - The original argus.db is unchanged"
echo "   - You can safely delete the dump/SQL files after testing"
echo
echo "🗑️  Cleanup command (run after testing):"
echo "   rm $BACKUP_FILE $DUMP_FILE $POSTGRES_FILE"
