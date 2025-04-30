#!/bin/bash

# This script adds missing aliases that were identified during testing
# Exit on error
set -e

# Function to add alias pair with specified entity type
add_alias() {
    local entity_type=$1
    local canonical=$2
    local alias=$3
    echo "Adding $entity_type alias: $canonical ↔ $alias"
    cargo run --bin manage_aliases -- add --canonical "$canonical" --alias "$alias" --entity-type "$entity_type" --source "fix" --confidence 0.95
}

# Add relationships for Meta/Facebook with canonical form (this is better)
# We set Meta as the canonical name for all forms
add_alias "organization" "Meta" "Facebook"
add_alias "organization" "Meta" "FB"
add_alias "organization" "Meta" "Meta Platforms Inc"

# Add missing case with periods
add_alias "organization" "J.P. Morgan" "JP Morgan"
add_alias "organization" "JP Morgan" "J.P. Morgan"

# Make sure these are approved
echo "Running SQL to approve all new aliases..."
sqlite3 argus.db "UPDATE entity_aliases SET status = 'APPROVED', approved_by = 'bulk_approve', approved_at = datetime('now') WHERE status = 'PENDING';"

echo "Completed adding missing aliases"
