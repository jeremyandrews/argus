#!/bin/bash

# Validate migration by checking what tables exist in your SQLite dump
echo "=== Migration Validation Script ==="

if [ "$#" -ne 1 ]; then
    echo "Usage: $0 <sqlite_dump_file>"
    echo ""
    echo "To create a dump file: sqlite3 argus.db .dump > argus_dump.sql"
    exit 1
fi

DUMP_FILE="$1"

if [ ! -f "$DUMP_FILE" ]; then
    echo "Error: Dump file $DUMP_FILE not found"
    exit 1
fi

echo "Analyzing SQLite dump file: $DUMP_FILE"
echo ""

# Extract all table names from CREATE TABLE statements
echo "=== Tables found in dump ==="
grep "CREATE TABLE " "$DUMP_FILE" | sed 's/CREATE TABLE \([^(]*\).*/\1/' | sort | while read table; do
    echo "- $table"
done

echo ""

# Check INSERT statement patterns
echo "=== INSERT statement patterns ==="
grep "INSERT INTO " "$DUMP_FILE" | sed 's/INSERT INTO \([^ ]*\) .*/\1/' | sort | uniq -c | sort -nr | while read count table; do
    echo "$table: $count INSERT statements"
done

echo ""

# Check for specific tables that our migration handles
echo "=== Migration coverage check ==="
# All tables from the PostgreSQL schema that our complete migration handles
handled_tables="articles entities article_entities configurations entity_aliases article_clusters article_cluster_mappings cluster_merge_history entity_negative_matches alias_pattern_stats alias_review_batches alias_review_items alias_cache_stats rss_queue matched_topics_queue life_safety_queue devices device_subscriptions ip_logs endpoint_timeout_events endpoint_alerts migration_metadata"

for table in $handled_tables; do
    count=$(grep -c "INSERT INTO $table " "$DUMP_FILE" 2>/dev/null || echo "0")
    if [ "$count" -gt 0 ]; then
        echo "✓ $table: $count records (handled by migration)"
    else
        echo "- $table: not found in dump"
    fi
done

echo ""

# Check for tables NOT handled by our migration
echo "=== Unhandled tables (may need migration updates) ==="
grep "INSERT INTO " "$DUMP_FILE" | sed 's/INSERT INTO \([^ ]*\) .*/\1/' | sort | uniq | while read table; do
    case "$table" in
        articles|entities|article_entities|configurations|entity_aliases|article_clusters|article_cluster_mappings|cluster_merge_history|entity_negative_matches|alias_pattern_stats|alias_review_batches|alias_review_items|alias_cache_stats|rss_queue|matched_topics_queue|life_safety_queue|devices|device_subscriptions|ip_logs|endpoint_timeout_events|endpoint_alerts|migration_metadata)
            # These are handled
            ;;
        *)
            count=$(grep -c "INSERT INTO $table " "$DUMP_FILE")
            echo "⚠️  $table: $count records (NOT handled - may need migration code)"
            ;;
    esac
done

echo ""
echo "=== Articles Table Analysis ==="
# Check if we have the simple 7-column version
articles_sample=$(grep "INSERT INTO articles " "$DUMP_FILE" | head -1)
if [ -n "$articles_sample" ]; then
    echo "Sample articles INSERT:"
    echo "$articles_sample" | cut -c1-200
    echo "..."
    
    # Count commas in the VALUES section to estimate column count
    values_part=$(echo "$articles_sample" | sed 's/.*VALUES(\([^)]*\)).*/\1/')
    comma_count=$(echo "$values_part" | tr -cd ',' | wc -c)
    column_count=$((comma_count + 1))
    echo "Estimated articles table column count: $column_count"
    
    if [ "$column_count" -eq 7 ]; then
        echo "✅ Appears to be 7-column articles table (compatible with complete migration)"
    elif [ "$column_count" -eq 18 ]; then
        echo "⚠️  Appears to be 18-column articles table (may need schema adjustment)"
    else
        echo "❓ Unexpected column count: $column_count"
    fi
fi

echo ""
echo "=== Next Steps ==="
echo "1. Run the complete migration: cargo run --bin migrate_to_postgres_complete argus_dump_*.sql"
echo "2. If any tables show as 'NOT handled', we can add them to the migration code"
echo "3. Test the output SQL file against your PostgreSQL schema"
echo "4. Import to PostgreSQL: psql \$DATABASE_URL -f postgres_migration_*.sql"
