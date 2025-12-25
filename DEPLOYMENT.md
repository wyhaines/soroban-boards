# Deployment Guide

This guide covers deploying Soroban Boards to local development, testnet, and mainnet environments.

## Table of Contents

- [Prerequisites](#prerequisites)
- [Building Contracts](#building-contracts)
- [Local Development](#local-development)
- [Testnet Deployment](#testnet-deployment)
- [Contract Initialization](#contract-initialization)
- [Upgrading Contracts](#upgrading-contracts)
- [Verification](#verification)
- [Troubleshooting](#troubleshooting)

---

## Prerequisites

### Required Tools

1. **Rust** with the WASM target:
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   rustup target add wasm32-unknown-unknown
   ```

2. **Stellar CLI** (v21.0.0 or later):
   ```bash
   cargo install --locked stellar-cli
   ```

3. **Docker** (for local development):
   ```bash
   # Install Docker from https://docs.docker.com/get-docker/
   docker --version
   ```

### Identity Setup

Create a deployer identity for each network:

```bash
# Local (auto-funded via friendbot)
stellar keys generate local-deployer --network local

# Testnet (needs friendbot funding)
stellar keys generate testnet-deployer --network testnet
curl "https://friendbot.stellar.org?addr=$(stellar keys address testnet-deployer)"

# Mainnet (needs real XLM)
stellar keys generate mainnet-deployer --network mainnet
# Fund from exchange or existing wallet
```

---

## Building Contracts

Build all contracts for WASM deployment:

```bash
# Clone repository
git clone https://github.com/wyhaines/soroban-boards.git
cd soroban-boards

# Build release WASM files
cargo build --release --target wasm32-unknown-unknown

# Verify builds (should see 6 .wasm files)
ls -la target/wasm32-unknown-unknown/release/*.wasm
```

### Expected Output Files

| Contract | WASM File | Approximate Size |
|----------|-----------|------------------|
| Registry | `boards_registry.wasm` | ~40KB |
| Permissions | `boards_permissions.wasm` | ~35KB |
| Content | `boards_content.wasm` | ~45KB |
| Board | `boards_board.wasm` | ~30KB |
| Theme | `boards_theme.wasm` | ~55KB |
| Admin | `boards_admin.wasm` | ~50KB |

---

## Local Development

The fastest way to get started is with the local deployment script.

### Start Local Stellar Network

```bash
# Start local Stellar node with Docker
docker run --rm -d \
  -p 8000:8000 \
  --name stellar-local \
  stellar/quickstart:latest \
  --local \
  --enable-soroban-rpc

# Wait for RPC to be ready (~30 seconds)
until curl -s http://localhost:8000/soroban/rpc > /dev/null 2>&1; do
  echo "Waiting for Stellar RPC..."
  sleep 2
done
echo "Stellar RPC is ready!"
```

### Deploy All Contracts

```bash
# Run the deployment script
./deploy-local.sh
```

This script:
1. Builds all contracts
2. Deploys the 5 shared contracts (registry, permissions, content, theme, admin)
3. Initializes all contracts with cross-references
4. Installs the board WASM hash for auto-deployment
5. Creates a sample board and thread
6. Saves contract IDs to `.deployed-contracts.env`

### Environment File

After deployment, `.deployed-contracts.env` contains:

```bash
NETWORK=local
RPC_URL=http://localhost:8000/soroban/rpc
ADMIN=G...

REGISTRY_ID=C...
PERMISSIONS_ID=C...
CONTENT_ID=C...
THEME_ID=C...
ADMIN_CONTRACT_ID=C...
```

Load it before running commands:

```bash
source .deployed-contracts.env
```

### Testing the Deployment

```bash
# Render home page
./render.sh /

# Render a board
./render.sh /b/0

# Render a thread
./render.sh /b/0/t/0

# Raw contract interaction
stellar contract invoke \
  --id $REGISTRY_ID \
  --source local-deployer \
  --network local \
  -- list_boards --start 0 --limit 10
```

---

## Testnet Deployment

### Manual Deployment Steps

1. **Deploy shared contracts:**

```bash
NETWORK="testnet"
DEPLOYER="testnet-deployer"
WASM_DIR="target/wasm32-unknown-unknown/release"

# Deploy permissions contract
PERMISSIONS_ID=$(stellar contract deploy \
  --wasm "$WASM_DIR/boards_permissions.wasm" \
  --source $DEPLOYER \
  --network $NETWORK)
echo "Permissions: $PERMISSIONS_ID"

# Deploy content contract
CONTENT_ID=$(stellar contract deploy \
  --wasm "$WASM_DIR/boards_content.wasm" \
  --source $DEPLOYER \
  --network $NETWORK)
echo "Content: $CONTENT_ID"

# Deploy theme contract
THEME_ID=$(stellar contract deploy \
  --wasm "$WASM_DIR/boards_theme.wasm" \
  --source $DEPLOYER \
  --network $NETWORK)
echo "Theme: $THEME_ID"

# Deploy admin contract
ADMIN_CONTRACT_ID=$(stellar contract deploy \
  --wasm "$WASM_DIR/boards_admin.wasm" \
  --source $DEPLOYER \
  --network $NETWORK)
echo "Admin: $ADMIN_CONTRACT_ID"

# Deploy registry contract (last, as it references others)
REGISTRY_ID=$(stellar contract deploy \
  --wasm "$WASM_DIR/boards_registry.wasm" \
  --source $DEPLOYER \
  --network $NETWORK)
echo "Registry: $REGISTRY_ID"
```

2. **Save contract IDs** to `.deployed-contracts-testnet.env`:

```bash
cat > .deployed-contracts-testnet.env << EOF
NETWORK=testnet
ADMIN=$(stellar keys address $DEPLOYER)
REGISTRY_ID=$REGISTRY_ID
PERMISSIONS_ID=$PERMISSIONS_ID
CONTENT_ID=$CONTENT_ID
THEME_ID=$THEME_ID
ADMIN_CONTRACT_ID=$ADMIN_CONTRACT_ID
EOF
```

3. **Initialize contracts** (see [Contract Initialization](#contract-initialization))

---

## Contract Initialization

Contracts must be initialized in a specific order due to cross-references.

### Initialization Order

1. **Registry** - first, establishes admin and stores contract references
2. **Permissions** - needs registry address
3. **Content** - needs registry address
4. **Theme** - needs registry, permissions, content, admin addresses
5. **Admin** - needs registry, permissions, content, theme addresses

### Initialization Commands

```bash
# Get admin address
ADMIN=$(stellar keys address $DEPLOYER)

# 1. Initialize Registry
stellar contract invoke \
  --id $REGISTRY_ID \
  --source $DEPLOYER \
  --network $NETWORK \
  -- init \
  --admin $ADMIN \
  --permissions $PERMISSIONS_ID \
  --content $CONTENT_ID \
  --theme $THEME_ID \
  --admin_contract $ADMIN_CONTRACT_ID

# 2. Install board WASM hash (for auto-deployment)
BOARD_WASM_HASH=$(stellar contract install \
  --wasm "$WASM_DIR/boards_board.wasm" \
  --source $DEPLOYER \
  --network $NETWORK)

stellar contract invoke \
  --id $REGISTRY_ID \
  --source $DEPLOYER \
  --network $NETWORK \
  -- set_board_wasm_hash \
  --wasm_hash $BOARD_WASM_HASH

# 3. Initialize Permissions
stellar contract invoke \
  --id $PERMISSIONS_ID \
  --source $DEPLOYER \
  --network $NETWORK \
  -- init \
  --registry $REGISTRY_ID

# 4. Initialize Content
stellar contract invoke \
  --id $CONTENT_ID \
  --source $DEPLOYER \
  --network $NETWORK \
  -- init \
  --registry $REGISTRY_ID

# 5. Initialize Theme
stellar contract invoke \
  --id $THEME_ID \
  --source $DEPLOYER \
  --network $NETWORK \
  -- init \
  --registry $REGISTRY_ID \
  --permissions $PERMISSIONS_ID \
  --content $CONTENT_ID \
  --admin $ADMIN_CONTRACT_ID

# 6. Initialize Admin
stellar contract invoke \
  --id $ADMIN_CONTRACT_ID \
  --source $DEPLOYER \
  --network $NETWORK \
  -- init \
  --registry $REGISTRY_ID \
  --permissions $PERMISSIONS_ID \
  --content $CONTENT_ID \
  --theme $THEME_ID
```

### Board Auto-Deployment

When you create a board via the registry, a board contract is automatically deployed:

```bash
# Create a board (board contract auto-deployed)
stellar contract invoke \
  --id $REGISTRY_ID \
  --source $DEPLOYER \
  --network $NETWORK \
  -- create_board \
  --name "General" \
  --description "General discussion" \
  --creator $ADMIN \
  --is_private false

# Get the auto-deployed board contract address
stellar contract invoke \
  --id $REGISTRY_ID \
  --source $DEPLOYER \
  --network $NETWORK \
  -- get_board_contract \
  --board_id 0
```

---

## Upgrading Contracts

Contracts can be upgraded in place, preserving all data and addresses.

### Using the Upgrade Script

```bash
# Upgrade all contracts
./upgrade-contracts.sh

# Upgrade specific contracts
./upgrade-contracts.sh theme admin

# Upgrade only board contracts
./upgrade-contracts.sh boards
```

### Valid Contract Names

| Name | Description |
|------|-------------|
| `registry` | Central registry contract |
| `permissions` | Role and permissions contract |
| `content` | Thread/reply content storage |
| `theme` | UI rendering contract |
| `admin` | Admin UI contract |
| `boards` | All per-board contracts |
| `all` | All contracts (default) |

### Manual Upgrade Process

If you need to upgrade manually:

1. **Build new WASM:**
   ```bash
   cargo build --release --target wasm32-unknown-unknown
   ```

2. **Install new WASM and get hash:**
   ```bash
   NEW_HASH=$(stellar contract install \
     --wasm target/wasm32-unknown-unknown/release/boards_theme.wasm \
     --source $DEPLOYER \
     --network $NETWORK)
   ```

3. **Upgrade via registry proxy:**
   ```bash
   stellar contract invoke \
     --id $REGISTRY_ID \
     --source $DEPLOYER \
     --network $NETWORK \
     -- upgrade_contract \
     --contract_id $THEME_ID \
     --new_wasm_hash $NEW_HASH
   ```

The registry can upgrade any contract it knows about because it's authorized as admin during initialization.

---

## Verification

### Check Contract Metadata

```bash
# Verify render support
stellar contract info \
  --id $THEME_ID \
  --network $NETWORK \
  metadata

# Should show:
# render: v1
# render_formats: markdown
```

### Test Rendering

```bash
# Render home page
stellar contract invoke \
  --id $THEME_ID \
  --source $DEPLOYER \
  --network $NETWORK \
  -- render --path '""' | xxd -r -p

# Render with viewer context
stellar contract invoke \
  --id $THEME_ID \
  --source $DEPLOYER \
  --network $NETWORK \
  -- render \
  --path '"/b/0"' \
  --viewer $ADMIN | xxd -r -p
```

### Test with Soroban Render Viewer

1. Open https://wyhaines.github.io/soroban-render/
2. Enter the Theme contract ID
3. Configure the network (local: `http://localhost:8000/soroban/rpc`)
4. Browse the rendered UI

---

## Troubleshooting

### Common Issues

**"Contract not found" error:**
- Verify the contract ID is correct
- Check the network matches where you deployed
- For local: ensure Docker container is running

**"Authorization failed" error:**
- Verify you're using the correct deployer identity
- Check the identity has sufficient XLM for fees
- Ensure `require_auth()` calls are satisfied

**"WASM too large" error:**
- Contracts must be under 64KB
- Use `cargo build --release` for smaller builds
- Check for unnecessary dependencies in Cargo.toml

**Local network reset:**
```bash
# Stop and remove container
docker stop stellar-local
docker rm stellar-local

# Re-deploy
./deploy-local.sh
```

**RPC not responding:**
```bash
# Check container logs
docker logs stellar-local

# Restart container
docker restart stellar-local
```

### Logs and Debugging

Enable verbose output for contract invocations:

```bash
RUST_LOG=debug stellar contract invoke ...
```

Check transaction status on Stellar Expert:
- Testnet: https://testnet.stellar.expert/explorer/testnet
- Mainnet: https://stellar.expert/explorer/public

---

## Mainnet Considerations

Before deploying to mainnet:

1. **Audit contracts** - Have code reviewed for security issues
2. **Test thoroughly** - Run all scenarios on testnet first
3. **Plan upgrades** - Document your upgrade strategy
4. **Secure keys** - Use hardware wallets or HSM for admin keys
5. **Monitor costs** - Estimate storage and execution costs
6. **Backup data** - While contracts are upgradeable, plan for recovery

### Cost Estimation

Approximate costs (varies with network conditions):

| Operation | Estimated Cost |
|-----------|----------------|
| Contract deployment (each) | ~1-2 XLM |
| Contract initialization | ~0.01-0.05 XLM |
| Create board | ~0.5-1 XLM |
| Create thread | ~0.01-0.05 XLM |
| Create reply | ~0.01-0.05 XLM |

---

## Related Documentation

- [README.md](./README.md) - Project overview
- [FEATURES.md](./FEATURES.md) - Feature documentation
- [API-REFERENCE.md](./API-REFERENCE.md) - Function reference
- [ROADMAP.md](./ROADMAP.md) - Development roadmap
