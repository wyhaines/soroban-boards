# Soroban Boards

Decentralized discussion forums on Stellar's Soroban smart contract platform.

Inspired by [gno.land's Boards2](https://github.com/gnolang/gno/tree/master/examples/gno.land/r/gnoland/boards2), Soroban Boards provides a fully on-chain forum system where content is stored and rendered directly from smart contracts using [Soroban Render](https://github.com/wyhaines/soroban-render).

## Features

- **Boards**: Create topic-focused discussion boards
- **Threads**: Post threads with titles and content
- **Nested Replies**: Support for threaded discussions with configurable depth
- **Role-Based Permissions**: Owner, Admin, Moderator, Member, Guest roles
- **Content Moderation**: Flagging system with configurable thresholds
- **Banning**: Time-based and permanent bans
- **Progressive Loading**: Large threads load progressively using [soroban-chonk](https://github.com/wyhaines/soroban-chonk)
- **Upgradeable**: All contracts support WASM upgrades without data loss

## Architecture

The system uses a hybrid multi-contract architecture to stay within Soroban's 64KB WASM limit:

| Contract | Responsibility |
|----------|----------------|
| **boards-registry** | Central discovery, board factory, global admin |
| **boards-board** | Per-board thread index, config, stats |
| **boards-content** | Thread/reply content storage via soroban-chonk |
| **boards-permissions** | Role management (RBAC), banning |
| **boards-theme** | UI rendering via soroban-render-sdk |

## Prerequisites

- [Rust](https://www.rust-lang.org/tools/install)
- [Stellar CLI](https://developers.stellar.org/docs/tools/developer-tools#cli)

```bash
# Add WASM target
rustup target add wasm32-unknown-unknown
```

## Building

```bash
# Clone the repository
git clone https://github.com/wyhaines/soroban-boards.git
cd soroban-boards

# Build all contracts
cargo build --release --target wasm32-unknown-unknown

# Run tests
cargo test
```

## Deployment

```bash
# Deploy shared contracts
stellar contract deploy \
  --wasm target/wasm32-unknown-unknown/release/boards_permissions.wasm \
  --source admin \
  --network testnet

stellar contract deploy \
  --wasm target/wasm32-unknown-unknown/release/boards_content.wasm \
  --source admin \
  --network testnet

stellar contract deploy \
  --wasm target/wasm32-unknown-unknown/release/boards_theme.wasm \
  --source admin \
  --network testnet

stellar contract deploy \
  --wasm target/wasm32-unknown-unknown/release/boards_registry.wasm \
  --source admin \
  --network testnet

# Initialize registry with contract addresses
stellar contract invoke \
  --id $REGISTRY_ID \
  --source admin \
  --network testnet \
  -- init \
  --admin $ADMIN_ADDRESS \
  --permissions $PERMISSIONS_ID \
  --content $CONTENT_ID \
  --theme $THEME_ID
```

## Usage

Once deployed, you can:

1. **Create a board**:
```bash
stellar contract invoke \
  --id $REGISTRY_ID \
  --source user \
  --network testnet \
  -- create_board \
  --name "General" \
  --description "General discussion" \
  --creator $USER_ADDRESS \
  --is_private false
```

2. **View via Soroban Render viewer**:
Visit https://wyhaines.github.io/soroban-render/ and enter the theme contract ID.

## Dependencies

- [soroban-sdk](https://github.com/stellar/rs-soroban-sdk) - Stellar smart contract SDK
- [soroban-render-sdk](https://github.com/wyhaines/soroban-render-sdk) - UI rendering builders
- [soroban-chonk](https://github.com/wyhaines/soroban-chonk) - Chunked content storage

## Project Status

This project is in active development. Current phase: Phase 1 (Foundation).

See the [implementation plan](/home/wyhaines/.claude/plans/jolly-leaping-lark.md) for full roadmap.

## License

Apache-2.0
