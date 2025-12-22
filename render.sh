#!/bin/bash
# Render a Soroban Boards page
# Usage: ./render.sh /path/to/page [viewer_address]

set -e

# Load contract IDs
if [ -f .deployed-contracts.env ]; then
    source .deployed-contracts.env
else
    echo "Error: .deployed-contracts.env not found. Run ./deploy-local.sh first."
    exit 1
fi

PATH_ARG="${1:-/}"
VIEWER_ARG="${2:-}"

# Build the path argument
if [ -z "$PATH_ARG" ] || [ "$PATH_ARG" = "/" ]; then
    PATH_JSON='""'
else
    PATH_JSON="\"$PATH_ARG\""
fi

# Build viewer argument (needs JSON quoting)
if [ -n "$VIEWER_ARG" ]; then
    VIEWER_OPT="--viewer \"$VIEWER_ARG\""
else
    VIEWER_OPT=""
fi

# Invoke render and decode the hex output
OUTPUT=$(stellar contract invoke \
    --id $THEME_ID \
    --source local-deployer \
    --network local \
    -- render \
    --path "$PATH_JSON" \
    $VIEWER_OPT 2>&1 | grep -v "Simulation")

# Remove quotes and decode hex
echo "$OUTPUT" | tr -d '"' | xxd -r -p
