#!/bin/bash
# Test script for verifying alias system functionality after removing static maps
# Exit on error
set -e  

echo "===== TESTING ORGANIZATION ALIASES ====="
./manage_aliases.sh test -1 "Apple Inc" -2 "Apple" -e organization
./manage_aliases.sh test -1 "Microsoft Corporation" -2 "Microsoft" -e organization
./manage_aliases.sh test -1 "International Business Machines Corporation" -2 "IBM" -e organization
./manage_aliases.sh test -1 "Meta Platforms Inc" -2 "Facebook" -e organization
./manage_aliases.sh test -1 "Meta" -2 "FB" -e organization
./manage_aliases.sh test -1 "J.P. Morgan" -2 "JP Morgan" -e organization

echo "===== TESTING PERSON ALIASES ====="
./manage_aliases.sh test -1 "Jeffrey P. Bezos" -2 "Jeff Bezos" -e person
./manage_aliases.sh test -1 "Timothy D. Cook" -2 "Tim Cook" -e person

echo "===== TESTING LOCATION ALIASES ====="
./manage_aliases.sh test -1 "NYC" -2 "New York City" -e location
./manage_aliases.sh test -1 "United States of America" -2 "USA" -e location

echo "===== TESTING PRODUCT ALIASES ====="
./manage_aliases.sh test -1 "Apple iPhone" -2 "iPhone" -e product
./manage_aliases.sh test -1 "PlayStation 5" -2 "PS5" -e product

echo "===== TESTING NON-MATCHES ====="
./manage_aliases.sh test -1 "Apple" -2 "Google" -e organization
./manage_aliases.sh test -1 "Microsoft" -2 "Sony" -e organization

echo "===== TESTING CASE SENSITIVITY ====="
./manage_aliases.sh test -1 "apple inc." -2 "APPLE" -e organization
