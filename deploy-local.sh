#!/bin/bash
set -e

# Configuration
NETWORK="local"
RPC_URL="http://localhost:8000/soroban/rpc"
DEPLOYER="local-deployer"
WASM_DIR="target/wasm32-unknown-unknown/release"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${GREEN}=== Soroban Boards Local Deployment ===${NC}"

# Check if friendbot is available for funding
fund_account() {
    local addr=$1
    echo -e "${YELLOW}Funding account $addr...${NC}"
    curl -s "http://localhost:8000/friendbot?addr=$addr" > /dev/null 2>&1 || true
}

# Get deployer address
DEPLOYER_ADDR=$(stellar keys address $DEPLOYER 2>/dev/null || true)
if [ -z "$DEPLOYER_ADDR" ]; then
    echo -e "${YELLOW}Creating deployer identity...${NC}"
    stellar keys generate $DEPLOYER --network $NETWORK 2>/dev/null || true
    DEPLOYER_ADDR=$(stellar keys address $DEPLOYER)
fi

# Additional admin (your wallet address) - can be set via environment variable
EXTRA_ADMIN="${EXTRA_ADMIN:-GCPM76A3NUGYF6F3H4KM5K72JLXHBWONIJYQEHJU6DZOIWR7YBDJ7KKP}"

echo -e "Deployer Admin: ${YELLOW}$DEPLOYER_ADDR${NC}"
echo -e "Extra Admin:    ${YELLOW}$EXTRA_ADMIN${NC}"

# Fund the deployer account
fund_account $DEPLOYER_ADDR

# Function to deploy a contract
deploy_contract() {
    local name=$1
    local wasm_file="$WASM_DIR/${name}.wasm"

    if [ ! -f "$wasm_file" ]; then
        echo -e "${RED}Error: $wasm_file not found${NC}" >&2
        exit 1
    fi

    echo -e "${YELLOW}Deploying $name...${NC}" >&2

    # Deploy and capture only the contract ID (last line, starts with C)
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

# Build contracts first
echo -e "${YELLOW}Building contracts...${NC}"
cargo build --release --target wasm32-unknown-unknown

# Deploy shared contracts (no dependencies on each other)
echo ""
echo -e "${GREEN}=== Deploying Shared Contracts ===${NC}"

PERMISSIONS_ID=$(deploy_contract "boards_permissions")
CONTENT_ID=$(deploy_contract "boards_content")
THEME_ID=$(deploy_contract "boards_theme")
ADMIN_CONTRACT_ID=$(deploy_contract "boards_admin")
REGISTRY_ID=$(deploy_contract "boards_registry")
COMMUNITY_ID=$(deploy_contract "boards_community")
VOTING_ID=$(deploy_contract "boards_voting")
CONFIG_ID=$(deploy_contract "boards_config")
MAIN_ID=$(deploy_contract "boards_main")

# Save contract IDs to file (board contracts are auto-deployed)
echo ""
echo -e "${GREEN}=== Saving Contract IDs ===${NC}"
rm -f .deployed-contracts.env
cat > .deployed-contracts.env << EOF
# Soroban Boards - Local Deployment
# Generated: $(date)

NETWORK=$NETWORK
RPC_URL=$RPC_URL
DEPLOYER_ADDR=$DEPLOYER_ADDR
EXTRA_ADMIN=$EXTRA_ADMIN

# Main entry point contract (use this as VITE_CONTRACT_ID)
MAIN_ID=$MAIN_ID

REGISTRY_ID=$REGISTRY_ID
PERMISSIONS_ID=$PERMISSIONS_ID
CONTENT_ID=$CONTENT_ID
THEME_ID=$THEME_ID
ADMIN_CONTRACT_ID=$ADMIN_CONTRACT_ID
COMMUNITY_ID=$COMMUNITY_ID
VOTING_ID=$VOTING_ID
CONFIG_ID=$CONFIG_ID

# Note: Board contracts are auto-deployed when boards are created.
# Use: stellar contract invoke --id \$REGISTRY_ID ... -- get_board_contract --board_id <id>
EOF

echo -e "${GREEN}Contract IDs saved to .deployed-contracts.env${NC}"

# Initialize Registry
echo ""
echo -e "${GREEN}=== Initializing Registry ===${NC}"
stellar contract invoke \
    --id $REGISTRY_ID \
    --source $DEPLOYER \
    --network $NETWORK \
    -- init \
    --admins "[\"$DEPLOYER_ADDR\", \"$EXTRA_ADMIN\"]" \
    --permissions $PERMISSIONS_ID \
    --content $CONTENT_ID \
    --theme $THEME_ID \
    --admin_contract $ADMIN_CONTRACT_ID

echo -e "${GREEN}Registry initialized with admins: $DEPLOYER_ADDR, $EXTRA_ADMIN${NC}"

# Initialize Config (needs registry for admin verification)
echo ""
echo -e "${GREEN}=== Initializing Config ===${NC}"
stellar contract invoke \
    --id $CONFIG_ID \
    --source $DEPLOYER \
    --network $NETWORK \
    -- init \
    --registry $REGISTRY_ID

echo -e "${GREEN}Config initialized${NC}"

# Install board WASM hash for auto-deployment of board contracts
echo ""
echo -e "${GREEN}=== Installing Board WASM Hash ===${NC}"
BOARD_WASM_OUTPUT=$(stellar contract install \
    --wasm "$WASM_DIR/boards_board.wasm" \
    --source $DEPLOYER \
    --network $NETWORK 2>&1)
# Extract the WASM hash (64 hex chars)
BOARD_WASM_HASH=$(echo "$BOARD_WASM_OUTPUT" | grep -E '^[a-f0-9]{64}$' | tail -1)

echo -e "Board WASM hash: ${YELLOW}$BOARD_WASM_HASH${NC}"

stellar contract invoke \
    --id $REGISTRY_ID \
    --source $DEPLOYER \
    --network $NETWORK \
    -- set_board_wasm_hash \
    --wasm_hash $BOARD_WASM_HASH \
    --caller $DEPLOYER_ADDR

echo -e "${GREEN}Board WASM hash installed${NC}"

# Initialize Permissions
echo ""
echo -e "${GREEN}=== Initializing Permissions ===${NC}"
stellar contract invoke \
    --id $PERMISSIONS_ID \
    --source $DEPLOYER \
    --network $NETWORK \
    -- init \
    --registry $REGISTRY_ID

echo -e "${GREEN}Permissions initialized${NC}"

# Initialize Content
echo ""
echo -e "${GREEN}=== Initializing Content ===${NC}"
stellar contract invoke \
    --id $CONTENT_ID \
    --source $DEPLOYER \
    --network $NETWORK \
    -- init \
    --registry $REGISTRY_ID

echo -e "${GREEN}Content initialized${NC}"

# Initialize Theme (needs permissions, content, admin, and config addresses)
echo ""
echo -e "${GREEN}=== Initializing Theme ===${NC}"
stellar contract invoke \
    --id $THEME_ID \
    --source $DEPLOYER \
    --network $NETWORK \
    -- init \
    --registry $REGISTRY_ID \
    --permissions $PERMISSIONS_ID \
    --content $CONTENT_ID \
    --admin $ADMIN_CONTRACT_ID \
    --config $CONFIG_ID

echo -e "${GREEN}Theme initialized${NC}"

# Initialize Admin (needs registry, permissions, content, theme, and config addresses)
echo ""
echo -e "${GREEN}=== Initializing Admin ===${NC}"
stellar contract invoke \
    --id $ADMIN_CONTRACT_ID \
    --source $DEPLOYER \
    --network $NETWORK \
    -- init \
    --registry $REGISTRY_ID \
    --permissions $PERMISSIONS_ID \
    --content $CONTENT_ID \
    --theme $THEME_ID \
    --config $CONFIG_ID

echo -e "${GREEN}Admin initialized${NC}"

# Initialize Community (needs registry, permissions, and theme addresses)
echo ""
echo -e "${GREEN}=== Initializing Community ===${NC}"
stellar contract invoke \
    --id $COMMUNITY_ID \
    --source $DEPLOYER \
    --network $NETWORK \
    -- init \
    --registry $REGISTRY_ID \
    --permissions $PERMISSIONS_ID \
    --theme $THEME_ID

echo -e "${GREEN}Community initialized${NC}"

# Initialize Voting (needs registry and permissions addresses)
echo ""
echo -e "${GREEN}=== Initializing Voting ===${NC}"
stellar contract invoke \
    --id $VOTING_ID \
    --source $DEPLOYER \
    --network $NETWORK \
    -- init \
    --registry $REGISTRY_ID \
    --permissions $PERMISSIONS_ID

echo -e "${GREEN}Voting initialized${NC}"

# Initialize Main (needs registry, theme, permissions, content, admin, community, and config)
echo ""
echo -e "${GREEN}=== Initializing Main ===${NC}"
stellar contract invoke \
    --id $MAIN_ID \
    --source $DEPLOYER \
    --network $NETWORK \
    -- init \
    --registry $REGISTRY_ID \
    --theme $THEME_ID \
    --permissions $PERMISSIONS_ID \
    --content $CONTENT_ID \
    --admin $ADMIN_CONTRACT_ID \
    --community $COMMUNITY_ID \
    --config $CONFIG_ID

echo -e "${GREEN}Main initialized${NC}"

# Register main contract with registry as "main" alias
echo ""
echo -e "${GREEN}=== Registering Main with Registry ===${NC}"
stellar contract invoke \
    --id $REGISTRY_ID \
    --source $DEPLOYER \
    --network $NETWORK \
    -- set_contract \
    --alias main \
    --address $MAIN_ID \
    --caller $DEPLOYER_ADDR

echo -e "${GREEN}Main registered as @main${NC}"

# Register config contract with registry as "config" alias
echo ""
echo -e "${GREEN}=== Registering Config with Registry ===${NC}"
stellar contract invoke \
    --id $REGISTRY_ID \
    --source $DEPLOYER \
    --network $NETWORK \
    -- set_contract \
    --alias config \
    --address $CONFIG_ID \
    --caller $DEPLOYER_ADDR

echo -e "${GREEN}Config registered as @config${NC}"

# Register community contract with registry using generic set_contract
echo ""
echo -e "${GREEN}=== Registering Community with Registry ===${NC}"
stellar contract invoke \
    --id $REGISTRY_ID \
    --source $DEPLOYER \
    --network $NETWORK \
    -- set_contract \
    --alias community \
    --address $COMMUNITY_ID \
    --caller $DEPLOYER_ADDR

echo -e "${GREEN}Community registered${NC}"

# Register voting contract with registry using generic set_contract
echo ""
echo -e "${GREEN}=== Registering Voting with Registry ===${NC}"
stellar contract invoke \
    --id $REGISTRY_ID \
    --source $DEPLOYER \
    --network $NETWORK \
    -- set_contract \
    --alias voting \
    --address $VOTING_ID \
    --caller $DEPLOYER_ADDR

echo -e "${GREEN}Voting registered${NC}"

# Create first board manually (deploy + register + init)
# Note: create_board via main has auth chaining issues, so we do it directly
echo ""
echo -e "${GREEN}=== Creating First Board ===${NC}"

# Deploy board contract
echo -e "${YELLOW}Deploying board contract...${NC}"
BOARD_DEPLOY_OUTPUT=$(stellar contract deploy \
    --wasm "$WASM_DIR/boards_board.wasm" \
    --source $DEPLOYER \
    --network $NETWORK 2>&1)
BOARD_CONTRACT=$(echo "$BOARD_DEPLOY_OUTPUT" | grep -E '^C[A-Z0-9]{55}$' | tail -1)

if [ -z "$BOARD_CONTRACT" ]; then
    echo -e "${RED}Error: Failed to deploy board contract${NC}"
    echo "$BOARD_DEPLOY_OUTPUT"
    exit 1
fi

echo -e "${GREEN}Board contract deployed: $BOARD_CONTRACT${NC}"

# Register board with registry
echo -e "${YELLOW}Registering board with registry...${NC}"
BOARD_NUM_OUTPUT=$(stellar contract invoke \
    --id $REGISTRY_ID \
    --source $DEPLOYER \
    --network $NETWORK \
    -- register_board_contract \
    --board_contract $BOARD_CONTRACT \
    --caller $DEPLOYER_ADDR 2>&1)
BOARD_NUM=$(echo "$BOARD_NUM_OUTPUT" | grep -E '^[0-9]+$' | tail -1)

echo -e "${GREEN}Board registered as #$BOARD_NUM${NC}"

# Initialize board contract
echo -e "${YELLOW}Initializing board...${NC}"
stellar contract invoke \
    --id $BOARD_CONTRACT \
    --source $DEPLOYER \
    --network $NETWORK \
    -- init \
    --board_id $BOARD_NUM \
    --registry $REGISTRY_ID \
    --permissions "\"$PERMISSIONS_ID\"" \
    --content "\"$CONTENT_ID\"" \
    --theme "\"$THEME_ID\"" \
    --name "General" \
    --description "General discussion board" \
    --is_private false \
    --creator "\"$DEPLOYER_ADDR\"" \
    --is_listed true

echo -e "${GREEN}Board initialized${NC}"

# Set deployer as board owner in permissions
echo -e "${YELLOW}Setting board ownership...${NC}"
stellar contract invoke \
    --id $PERMISSIONS_ID \
    --source $DEPLOYER \
    --network $NETWORK \
    -- set_role \
    --board_id $BOARD_NUM \
    --user $DEPLOYER_ADDR \
    --role 4 \
    --caller $DEPLOYER_ADDR

echo -e "${GREEN}Created board #$BOARD_NUM${NC}"
echo -e "Board contract: ${YELLOW}$BOARD_CONTRACT${NC}"

# Create a sample thread using the auto-deployed board contract
echo ""
echo -e "${GREEN}=== Creating Sample Thread ===${NC}"
THREAD_OUTPUT=$(stellar contract invoke \
    --id $BOARD_CONTRACT \
    --source $DEPLOYER \
    --network $NETWORK \
    -- create_thread \
    --title "Welcome to Soroban Boards!" \
    --creator $DEPLOYER_ADDR 2>&1)
# Extract thread ID (number)
THREAD_ID=$(echo "$THREAD_OUTPUT" | grep -E '^[0-9]+$' | tail -1)

echo -e "${GREEN}Created thread #$THREAD_ID${NC}"

# Set thread body content (hex encoded "Welcome to Soroban Boards! This is a decentralized forum running on the Stellar blockchain.")
stellar contract invoke \
    --id $CONTENT_ID \
    --source $DEPLOYER \
    --network $NETWORK \
    -- set_thread_body \
    --board_id 0 \
    --thread_id 0 \
    --content 57656c636f6d6520746f20536f726f62616e20426f617264732120546869732069732061206465636e7472616c697a656420666f72756d2072756e6e696e67206f6e20746865205374656c6c617220626c6f636b636861696e2e \
    --author $DEPLOYER_ADDR

echo -e "${GREEN}Thread body set${NC}"

echo ""
echo -e "${GREEN}=== Deployment Complete! ===${NC}"
echo ""
echo "Contract IDs:"
echo "  Main (entry): $MAIN_ID"
echo "  Registry:     $REGISTRY_ID"
echo "  Permissions:  $PERMISSIONS_ID"
echo "  Content:      $CONTENT_ID"
echo "  Theme:        $THEME_ID"
echo "  Admin:        $ADMIN_CONTRACT_ID"
echo "  Community:    $COMMUNITY_ID"
echo "  Voting:       $VOTING_ID"
echo "  Config:       $CONFIG_ID"
echo "  Board 0:      $BOARD_CONTRACT (auto-deployed)"
echo ""
echo "To interact with the contracts:"
echo "  source .deployed-contracts.env"
echo ""
echo "Example commands:"
echo ""
echo "  # Render home page"
echo "  ./render.sh /"
echo ""
echo "  # Render communities page"
echo "  ./render.sh /communities"
echo ""
echo "  # Render board"
echo "  ./render.sh /b/0"
echo ""
echo "  # Render thread"
echo "  ./render.sh /b/0/t/0"
echo ""
echo "  # List boards (raw)"
echo "  stellar contract invoke --id \$REGISTRY_ID --source $DEPLOYER --network local -- list_boards --start 0 --limit 10"
echo ""
echo "Note: Board contracts are now auto-deployed when boards are created."
echo "New boards created via the UI will automatically have their own board contract."
