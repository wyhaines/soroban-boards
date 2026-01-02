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
UPGRADE_MAIN=false
UPGRADE_COMMUNITY=false
UPGRADE_VOTING=false
UPGRADE_CONFIG=false
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
            main)        UPGRADE_MAIN=true ;;
            community)   UPGRADE_COMMUNITY=true ;;
            voting)      UPGRADE_VOTING=true ;;
            config)      UPGRADE_CONFIG=true ;;
            board|boards) UPGRADE_BOARDS=true ;;
            all)         UPGRADE_ALL=true ;;
            *)
                echo -e "${RED}Unknown contract: $arg${NC}"
                echo "Valid options: registry, permissions, content, theme, admin, main, community, voting, config, board, all"
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
    UPGRADE_MAIN=true
    UPGRADE_COMMUNITY=true
    UPGRADE_VOTING=true
    UPGRADE_CONFIG=true
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

# Function to deploy a new contract
deploy_contract() {
    local name=$1
    local wasm_file="$WASM_DIR/${name}.wasm"

    if [ ! -f "$wasm_file" ]; then
        echo -e "${RED}Error: $wasm_file not found${NC}" >&2
        exit 1
    fi

    echo -e "${YELLOW}Deploying $name...${NC}" >&2

    local output=$(stellar contract deploy \
        --wasm "$wasm_file" \
        --source $DEPLOYER \
        --network $NETWORK 2>&1)

    # Extract the contract ID (line starting with C, 56 chars)
    local contract_id=$(echo "$output" | grep -E '^C[A-Z0-9]{55}$' | tail -1)

    if [ -z "$contract_id" ]; then
        echo -e "${RED}Error: Failed to deploy $name${NC}" >&2
        echo "$output" >&2
        exit 1
    fi

    echo -e "${GREEN}$name deployed: $contract_id${NC}" >&2
    echo "$contract_id"
}

# Function to upgrade the registry contract directly
upgrade_registry() {
    local wasm_hash=$1

    echo -e "${YELLOW}Upgrading Registry ($REGISTRY_ID)...${NC}"

    # Get deployer address for caller parameter
    local deployer_addr=$(stellar keys address $DEPLOYER)

    stellar contract invoke \
        --id "$REGISTRY_ID" \
        --source $DEPLOYER \
        --network $NETWORK \
        -- upgrade \
        --new_wasm_hash "$wasm_hash" \
        --caller "$deployer_addr" 2>&1 | grep -v "^ℹ" || true

    echo -e "${GREEN}Registry upgraded successfully${NC}"
}

# Function to upgrade a contract via registry proxy
# (The registry calls the contract's upgrade function on behalf of admin)
upgrade_contract_via_registry() {
    local name=$1
    local contract_id=$2
    local wasm_hash=$3

    echo -e "${YELLOW}Upgrading $name ($contract_id) via registry...${NC}"

    # Get deployer address for caller parameter
    local deployer_addr=$(stellar keys address $DEPLOYER)

    stellar contract invoke \
        --id "$REGISTRY_ID" \
        --source $DEPLOYER \
        --network $NETWORK \
        -- upgrade_contract \
        --contract_id "$contract_id" \
        --new_wasm_hash "$wasm_hash" \
        --caller "$deployer_addr" 2>&1 | grep -v "^ℹ" || true

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

# Upgrade Main (via registry proxy)
if [ "$UPGRADE_MAIN" = true ]; then
    echo -e "${GREEN}=== Upgrading Main ===${NC}"
    MAIN_HASH=$(install_wasm "boards_main")
    echo -e "WASM hash: ${BLUE}$MAIN_HASH${NC}"
    upgrade_contract_via_registry "Main" "$MAIN_ID" "$MAIN_HASH"
    echo ""
fi

# Upgrade or Deploy Community contract
if [ "$UPGRADE_COMMUNITY" = true ]; then
    echo -e "${GREEN}=== Community Contract ===${NC}"

    # Check if COMMUNITY_ID exists in env
    if [ -z "$COMMUNITY_ID" ]; then
        echo -e "${YELLOW}Community contract not found - deploying new...${NC}"
        COMMUNITY_ID=$(deploy_contract "boards_community")

        # Initialize Community
        echo -e "${YELLOW}Initializing Community...${NC}"
        stellar contract invoke \
            --id "$COMMUNITY_ID" \
            --source $DEPLOYER \
            --network $NETWORK \
            -- init \
            --registry "$REGISTRY_ID" \
            --permissions "$PERMISSIONS_ID" \
            --theme "$THEME_ID" 2>&1 | grep -v "^ℹ" || true
        echo -e "${GREEN}Community initialized${NC}"

        # Register with registry using generic set_contract
        echo -e "${YELLOW}Registering Community with Registry...${NC}"
        stellar contract invoke \
            --id "$REGISTRY_ID" \
            --source $DEPLOYER \
            --network $NETWORK \
            -- set_contract \
            --alias community \
            --address "$COMMUNITY_ID" \
            --caller "$DEPLOYER_ADDR" 2>&1 | grep -v "^ℹ" || true
        echo -e "${GREEN}Community registered${NC}"

        # Update Main contract with community address
        echo -e "${YELLOW}Setting Community in Main contract...${NC}"
        stellar contract invoke \
            --id "$MAIN_ID" \
            --source $DEPLOYER \
            --network $NETWORK \
            -- set_community \
            --community "$COMMUNITY_ID" \
            --caller "$DEPLOYER_ADDR" 2>&1 | grep -v "^ℹ" || true
        echo -e "${GREEN}Main updated with Community${NC}"

        # Update env file
        echo "COMMUNITY_ID=$COMMUNITY_ID" >> "$ENV_FILE"
    else
        echo -e "Upgrading existing Community ($COMMUNITY_ID)"
        COMMUNITY_HASH=$(install_wasm "boards_community")
        echo -e "WASM hash: ${BLUE}$COMMUNITY_HASH${NC}"
        upgrade_contract_via_registry "Community" "$COMMUNITY_ID" "$COMMUNITY_HASH"
    fi
    echo ""
fi

# Upgrade or Deploy Voting contract
if [ "$UPGRADE_VOTING" = true ]; then
    echo -e "${GREEN}=== Voting Contract ===${NC}"

    # Check if VOTING_ID exists in env
    if [ -z "$VOTING_ID" ]; then
        echo -e "${YELLOW}Voting contract not found - deploying new...${NC}"
        VOTING_ID=$(deploy_contract "boards_voting")

        # Initialize Voting
        echo -e "${YELLOW}Initializing Voting...${NC}"
        stellar contract invoke \
            --id "$VOTING_ID" \
            --source $DEPLOYER \
            --network $NETWORK \
            -- init \
            --registry "$REGISTRY_ID" \
            --permissions "$PERMISSIONS_ID" 2>&1 | grep -v "^ℹ" || true
        echo -e "${GREEN}Voting initialized${NC}"

        # Register with registry using generic set_contract
        echo -e "${YELLOW}Registering Voting with Registry...${NC}"
        stellar contract invoke \
            --id "$REGISTRY_ID" \
            --source $DEPLOYER \
            --network $NETWORK \
            -- set_contract \
            --alias voting \
            --address "$VOTING_ID" \
            --caller "$DEPLOYER_ADDR" 2>&1 | grep -v "^ℹ" || true
        echo -e "${GREEN}Voting registered${NC}"

        # Update env file
        echo "VOTING_ID=$VOTING_ID" >> "$ENV_FILE"
    else
        echo -e "Upgrading existing Voting ($VOTING_ID)"
        VOTING_HASH=$(install_wasm "boards_voting")
        echo -e "WASM hash: ${BLUE}$VOTING_HASH${NC}"
        upgrade_contract_via_registry "Voting" "$VOTING_ID" "$VOTING_HASH"
    fi
    echo ""
fi

# Function to upgrade config contract directly (handles old 2-param and new 1-param signatures)
upgrade_config_direct() {
    local contract_id=$1
    local wasm_hash=$2

    echo -e "${YELLOW}Upgrading Config ($contract_id) directly...${NC}"

    local deployer_addr=$(stellar keys address $DEPLOYER)

    # Try the new 1-param signature first (via registry), fall back to old 2-param signature
    local result=$(stellar contract invoke \
        --id "$REGISTRY_ID" \
        --source $DEPLOYER \
        --network $NETWORK \
        -- upgrade_contract \
        --contract_id "$contract_id" \
        --new_wasm_hash "$wasm_hash" \
        --caller "$deployer_addr" 2>&1) || true

    if echo "$result" | grep -q "MismatchingParameterLen"; then
        echo -e "${YELLOW}Old config contract detected, using direct 2-param upgrade...${NC}"
        # Old contract has upgrade(wasm_hash, caller) - call directly
        stellar contract invoke \
            --id "$contract_id" \
            --source $DEPLOYER \
            --network $NETWORK \
            -- upgrade \
            --new_wasm_hash "$wasm_hash" \
            --caller "$deployer_addr" 2>&1 | grep -v "^ℹ" || true
    fi

    echo -e "${GREEN}Config upgraded successfully${NC}"
}

# Upgrade or Deploy Config contract
if [ "$UPGRADE_CONFIG" = true ]; then
    echo -e "${GREEN}=== Config Contract ===${NC}"

    # Check if CONFIG_ID exists in env
    if [ -z "$CONFIG_ID" ]; then
        echo -e "${YELLOW}Config contract not found - deploying new...${NC}"
        CONFIG_ID=$(deploy_contract "boards_config")

        # Initialize Config
        echo -e "${YELLOW}Initializing Config...${NC}"
        stellar contract invoke \
            --id "$CONFIG_ID" \
            --source $DEPLOYER \
            --network $NETWORK \
            -- init \
            --registry "$REGISTRY_ID" 2>&1 | grep -v "^ℹ" || true
        echo -e "${GREEN}Config initialized${NC}"

        # Register with registry
        echo -e "${YELLOW}Registering Config with Registry...${NC}"
        stellar contract invoke \
            --id "$REGISTRY_ID" \
            --source $DEPLOYER \
            --network $NETWORK \
            -- set_contract \
            --alias config \
            --address "$CONFIG_ID" \
            --caller "$DEPLOYER_ADDR" 2>&1 | grep -v "^ℹ" || true
        echo -e "${GREEN}Config registered as @config${NC}"

        # Update Main contract with config address
        echo -e "${YELLOW}Setting Config in Main contract...${NC}"
        stellar contract invoke \
            --id "$MAIN_ID" \
            --source $DEPLOYER \
            --network $NETWORK \
            -- set_config \
            --config "$CONFIG_ID" \
            --caller "$DEPLOYER_ADDR" 2>&1 | grep -v "^ℹ" || true
        echo -e "${GREEN}Main updated with Config${NC}"

        # Update Theme contract with config address
        echo -e "${YELLOW}Setting Config in Theme contract...${NC}"
        stellar contract invoke \
            --id "$THEME_ID" \
            --source $DEPLOYER \
            --network $NETWORK \
            -- set_config \
            --config "$CONFIG_ID" \
            --caller "$REGISTRY_ID" 2>&1 | grep -v "^ℹ" || true
        echo -e "${GREEN}Theme updated with Config${NC}"

        # Update env file
        echo "CONFIG_ID=$CONFIG_ID" >> "$ENV_FILE"
    else
        echo -e "Upgrading existing Config ($CONFIG_ID)"
        CONFIG_HASH=$(install_wasm "boards_config")
        echo -e "WASM hash: ${BLUE}$CONFIG_HASH${NC}"
        upgrade_config_direct "$CONFIG_ID" "$CONFIG_HASH"
    fi
    echo ""
fi

# Upgrade Board contract (single contract storing all boards)
if [ "$UPGRADE_BOARDS" = true ]; then
    echo -e "${GREEN}=== Upgrading Board Contract ===${NC}"

    # Check if BOARD_ID exists in env (new single-contract architecture)
    if [ -n "$BOARD_ID" ]; then
        echo -e "Upgrading single Board contract ($BOARD_ID)"
        BOARD_HASH=$(install_wasm "boards_board")
        echo -e "WASM hash: ${BLUE}$BOARD_HASH${NC}"
        upgrade_contract_via_registry "Board" "$BOARD_ID" "$BOARD_HASH"
    else
        # Fallback: old multi-contract architecture
        echo -e "${YELLOW}No BOARD_ID found, checking for old multi-board architecture...${NC}"

        # Install board WASM
        BOARD_HASH=$(install_wasm "boards_board")
        echo -e "WASM hash: ${BLUE}$BOARD_HASH${NC}"

        # Get board count
        BOARD_COUNT_OUTPUT=$(stellar contract invoke \
            --id "$REGISTRY_ID" \
            --source $DEPLOYER \
            --network $NETWORK \
            -- board_contract_count 2>&1)
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
    fi
    echo ""
fi

echo -e "${GREEN}=== Upgrade Complete! ===${NC}"
echo ""
echo "Contract addresses:"
echo "  Main (entry): $MAIN_ID"
echo "  Registry:     $REGISTRY_ID"
echo "  Permissions:  $PERMISSIONS_ID"
echo "  Content:      $CONTENT_ID"
echo "  Theme:        $THEME_ID"
echo "  Admin:        $ADMIN_CONTRACT_ID"
if [ -n "$BOARD_ID" ]; then
    echo "  Board:        $BOARD_ID"
fi
if [ -n "$COMMUNITY_ID" ]; then
    echo "  Community:    $COMMUNITY_ID"
fi
if [ -n "$VOTING_ID" ]; then
    echo "  Voting:       $VOTING_ID"
fi
if [ -n "$CONFIG_ID" ]; then
    echo "  Config:       $CONFIG_ID"
fi
echo ""
echo "All existing data has been preserved."
