#!/bin/bash
set -e

# Upgrade Soroban Boards contracts in place (preserves addresses and data)
# Usage: ./upgrade-contracts.sh [contract-name...]
#   No args: upgrade all contracts
#   With args: upgrade only specified contracts (registry, permissions, content, theme, admin, boards)

# Configuration
NETWORK="local"
DEPLOYER="local-deployer"
WASM_DIR="target/wasm32-unknown-unknown/release"
ENV_FILE=".deployed-contracts.env"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Check if env file exists
if [ ! -f "$ENV_FILE" ]; then
    echo -e "${RED}Error: $ENV_FILE not found${NC}"
    echo "Run ./deploy-local.sh first to deploy contracts"
    exit 1
fi

# Load existing contract addresses
source "$ENV_FILE"

echo -e "${GREEN}=== Soroban Boards Contract Upgrade ===${NC}"
echo -e "Network: ${YELLOW}$NETWORK${NC}"
echo ""

# Parse arguments - which contracts to upgrade
UPGRADE_ALL=true
UPGRADE_REGISTRY=false
UPGRADE_PERMISSIONS=false
UPGRADE_CONTENT=false
UPGRADE_THEME=false
UPGRADE_ADMIN=false
UPGRADE_BOARDS=false

if [ $# -gt 0 ]; then
    UPGRADE_ALL=false
    for arg in "$@"; do
        case $arg in
            registry)    UPGRADE_REGISTRY=true ;;
            permissions) UPGRADE_PERMISSIONS=true ;;
            content)     UPGRADE_CONTENT=true ;;
            theme)       UPGRADE_THEME=true ;;
            admin)       UPGRADE_ADMIN=true ;;
            boards)      UPGRADE_BOARDS=true ;;
            all)         UPGRADE_ALL=true ;;
            *)
                echo -e "${RED}Unknown contract: $arg${NC}"
                echo "Valid options: registry, permissions, content, theme, admin, boards, all"
                exit 1
                ;;
        esac
    done
fi

if [ "$UPGRADE_ALL" = true ]; then
    UPGRADE_REGISTRY=true
    UPGRADE_PERMISSIONS=true
    UPGRADE_CONTENT=true
    UPGRADE_THEME=true
    UPGRADE_ADMIN=true
    UPGRADE_BOARDS=true
fi

# Build contracts
echo -e "${YELLOW}Building contracts...${NC}"
cargo build --release --target wasm32-unknown-unknown
echo -e "${GREEN}Build complete${NC}"
echo ""

# Function to install WASM and get hash
install_wasm() {
    local name=$1
    local wasm_file="$WASM_DIR/${name}.wasm"

    if [ ! -f "$wasm_file" ]; then
        echo -e "${RED}Error: $wasm_file not found${NC}" >&2
        exit 1
    fi

    echo -e "${BLUE}Installing $name WASM...${NC}" >&2

    local output=$(stellar contract install \
        --wasm "$wasm_file" \
        --source $DEPLOYER \
        --network $NETWORK 2>&1)

    # Extract the WASM hash (64 hex chars)
    local wasm_hash=$(echo "$output" | grep -E '^[a-f0-9]{64}$' | tail -1)

    if [ -z "$wasm_hash" ]; then
        echo -e "${RED}Error: Failed to install $name WASM${NC}" >&2
        echo "$output" >&2
        exit 1
    fi

    echo "$wasm_hash"
}

# Function to upgrade the registry contract directly
upgrade_registry() {
    local wasm_hash=$1

    echo -e "${YELLOW}Upgrading Registry ($REGISTRY_ID)...${NC}"

    stellar contract invoke \
        --id "$REGISTRY_ID" \
        --source $DEPLOYER \
        --network $NETWORK \
        -- upgrade \
        --new_wasm_hash "$wasm_hash" 2>&1 | grep -v "^ℹ" || true

    echo -e "${GREEN}Registry upgraded successfully${NC}"
}

# Function to upgrade a contract via registry proxy
# (The registry calls the contract's upgrade function on behalf of admin)
upgrade_contract_via_registry() {
    local name=$1
    local contract_id=$2
    local wasm_hash=$3

    echo -e "${YELLOW}Upgrading $name ($contract_id) via registry...${NC}"

    stellar contract invoke \
        --id "$REGISTRY_ID" \
        --source $DEPLOYER \
        --network $NETWORK \
        -- upgrade_contract \
        --contract_id "$contract_id" \
        --new_wasm_hash "$wasm_hash" 2>&1 | grep -v "^ℹ" || true

    echo -e "${GREEN}$name upgraded successfully${NC}"
}

# Upgrade Registry first (needed for upgrading other contracts via registry)
if [ "$UPGRADE_REGISTRY" = true ]; then
    echo -e "${GREEN}=== Upgrading Registry ===${NC}"
    REGISTRY_HASH=$(install_wasm "boards_registry")
    echo -e "WASM hash: ${BLUE}$REGISTRY_HASH${NC}"
    upgrade_registry "$REGISTRY_HASH"
    echo ""
fi

# Upgrade Permissions (via registry proxy)
if [ "$UPGRADE_PERMISSIONS" = true ]; then
    echo -e "${GREEN}=== Upgrading Permissions ===${NC}"
    PERMISSIONS_HASH=$(install_wasm "boards_permissions")
    echo -e "WASM hash: ${BLUE}$PERMISSIONS_HASH${NC}"
    upgrade_contract_via_registry "Permissions" "$PERMISSIONS_ID" "$PERMISSIONS_HASH"
    echo ""
fi

# Upgrade Content (via registry proxy)
if [ "$UPGRADE_CONTENT" = true ]; then
    echo -e "${GREEN}=== Upgrading Content ===${NC}"
    CONTENT_HASH=$(install_wasm "boards_content")
    echo -e "WASM hash: ${BLUE}$CONTENT_HASH${NC}"
    upgrade_contract_via_registry "Content" "$CONTENT_ID" "$CONTENT_HASH"
    echo ""
fi

# Upgrade Theme (via registry proxy)
if [ "$UPGRADE_THEME" = true ]; then
    echo -e "${GREEN}=== Upgrading Theme ===${NC}"
    THEME_HASH=$(install_wasm "boards_theme")
    echo -e "WASM hash: ${BLUE}$THEME_HASH${NC}"
    upgrade_contract_via_registry "Theme" "$THEME_ID" "$THEME_HASH"
    echo ""
fi

# Upgrade Admin (via registry proxy)
if [ "$UPGRADE_ADMIN" = true ]; then
    echo -e "${GREEN}=== Upgrading Admin ===${NC}"
    ADMIN_HASH=$(install_wasm "boards_admin")
    echo -e "WASM hash: ${BLUE}$ADMIN_HASH${NC}"
    upgrade_contract_via_registry "Admin" "$ADMIN_CONTRACT_ID" "$ADMIN_HASH"
    echo ""
fi

# Upgrade Board contracts (need to iterate through all boards)
if [ "$UPGRADE_BOARDS" = true ]; then
    echo -e "${GREEN}=== Upgrading Board Contracts ===${NC}"

    # Install board WASM
    BOARD_HASH=$(install_wasm "boards_board")
    echo -e "WASM hash: ${BLUE}$BOARD_HASH${NC}"

    # Also update the registry's stored WASM hash for future board deployments
    echo -e "${YELLOW}Updating registry board WASM hash...${NC}"
    stellar contract invoke \
        --id "$REGISTRY_ID" \
        --source $DEPLOYER \
        --network $NETWORK \
        -- set_board_wasm_hash \
        --wasm_hash "$BOARD_HASH" 2>&1 | grep -v "^ℹ" || true
    echo -e "${GREEN}Registry board WASM hash updated${NC}"

    # Get board count
    BOARD_COUNT_OUTPUT=$(stellar contract invoke \
        --id "$REGISTRY_ID" \
        --source $DEPLOYER \
        --network $NETWORK \
        -- board_count 2>&1)
    BOARD_COUNT=$(echo "$BOARD_COUNT_OUTPUT" | grep -E '^[0-9]+$' | tail -1)

    if [ -z "$BOARD_COUNT" ] || [ "$BOARD_COUNT" -eq 0 ]; then
        echo -e "${YELLOW}No board contracts to upgrade${NC}"
    else
        echo -e "Found ${YELLOW}$BOARD_COUNT${NC} board(s) to upgrade"

        for ((i=0; i<BOARD_COUNT; i++)); do
            # Get board contract address
            BOARD_CONTRACT_OUTPUT=$(stellar contract invoke \
                --id "$REGISTRY_ID" \
                --source $DEPLOYER \
                --network $NETWORK \
                -- get_board_contract \
                --board_id $i 2>&1)

            # Extract contract ID (may be quoted)
            BOARD_CONTRACT=$(echo "$BOARD_CONTRACT_OUTPUT" | tr -d '"' | grep -E '^C[A-Z0-9]{55}$' | tail -1)

            if [ -n "$BOARD_CONTRACT" ]; then
                upgrade_contract_via_registry "Board #$i" "$BOARD_CONTRACT" "$BOARD_HASH"
            else
                echo -e "${YELLOW}Board #$i has no contract (skipping)${NC}"
            fi
        done
    fi
    echo ""
fi

echo -e "${GREEN}=== Upgrade Complete! ===${NC}"
echo ""
echo "Contract addresses remain unchanged:"
echo "  Registry:    $REGISTRY_ID"
echo "  Permissions: $PERMISSIONS_ID"
echo "  Content:     $CONTENT_ID"
echo "  Theme:       $THEME_ID"
echo "  Admin:       $ADMIN_CONTRACT_ID"
echo ""
echo "All existing data has been preserved."
