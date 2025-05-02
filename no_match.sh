#!/bin/bash

DEVICE_ID="d89d3f851a513be626f54a310f114b4567911e74174447922077040045344254"

# analyze-match.sh - Script for analyzing why two articles match or don't match
# Usage: ./analyze-match.sh <source_article_id> <target_article_id>

# Load environment variables
if [ -f .env ]; then
    source .env
else
    echo "Error: .env file not found" >&2
    exit 1
fi

# Check if DEVICE_ID is set (required for authentication)
if [ -z "$DEVICE_ID" ]; then
    echo "Error: DEVICE_ID environment variable not set in .env" >&2
    echo "Add a line like: DEVICE_ID=0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef" >&2
    exit 1
fi

# Check if API_URL is set, otherwise use default
API_URL=${API_URL:-"http://argus-server.pozza:8080"}

# Function to get an authentication token
get_auth_token() {
    # Make authentication request
    AUTH_RESPONSE=$(curl -s -X POST "${API_URL}/authenticate" \
      -H "Content-Type: application/json" \
      -d "{\"device_id\": \"$DEVICE_ID\"}")
    
    # Extract token from response
    TOKEN=$(echo "$AUTH_RESPONSE" | grep -o '"token":"[^"]*"' | cut -d':' -f2 | tr -d '"')
    
    if [ -z "$TOKEN" ]; then
        echo "Error: Failed to get authentication token" >&2
        echo "Response: $AUTH_RESPONSE" >&2
        exit 1
    fi
    
    echo "$TOKEN"
}

# Function to analyze match between two articles
analyze_match() {
    local source_id=$1
    local target_id=$2
    local token=$3
    
    # Make API call to analyze match
    RESPONSE=$(curl -s -X POST "${API_URL}/articles/analyze-match" \
      -H "Authorization: Bearer $token" \
      -H "Content-Type: application/json" \
      -d "{\"source_article_id\": $source_id, \"target_article_id\": $target_id}")
    
    # Check if response is empty
    if [ -z "$RESPONSE" ]; then
        echo "Error: Empty response from API" >&2
        exit 1
    fi
    
    # Check if response contains an error
    if echo "$RESPONSE" | grep -q '"error"'; then
        echo "Error in API response:" >&2
        echo "$RESPONSE" | jq 2>/dev/null || echo "$RESPONSE" >&2
        exit 1
    fi
    
    # Format and output result
    if command -v jq &>/dev/null; then
        # If jq is available, format the JSON nicely
        echo "$RESPONSE" | jq '.'
    else
        # Otherwise, just print the raw response
        echo "$RESPONSE"
    fi
}

# Check if both arguments are provided
if [ $# -ne 2 ]; then
    echo "Usage: $0 <source_article_id> <target_article_id>" >&2
    exit 1
fi

# Assign arguments to variables
SOURCE_ID=$1
TARGET_ID=$2

# Verify arguments are numbers
if ! [[ "$SOURCE_ID" =~ ^[0-9]+$ ]] || ! [[ "$TARGET_ID" =~ ^[0-9]+$ ]]; then
    echo "Error: Article IDs must be integers" >&2
    exit 1
fi

# Get authentication token
echo "Authenticating..." >&2
TOKEN=$(get_auth_token)
echo "Authentication successful!" >&2

# Analyze match
echo "Analyzing match between articles $SOURCE_ID and $TARGET_ID..." >&2
analyze_match "$SOURCE_ID" "$TARGET_ID" "$TOKEN"
