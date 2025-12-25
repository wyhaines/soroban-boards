# soroban-boards

Decentralized discussion forum on Stellar's Soroban platform. Multi-contract architecture with boards, threads, nested replies, role-based permissions, and moderation.

## ARCHITECTURE

| Contract | Responsibility |
|----------|----------------|
| **Registry** | Board factory, contract discovery, global admin, upgrades |
| **Board** | Per-board thread index, config, stats, locking |
| **Content** | Thread/reply content storage (via soroban-chonk) |
| **Permissions** | Roles (RBAC), banning, invites |
| **Theme** | UI rendering (via soroban-render-sdk) |
| **Admin** | Admin panel UI and operations |

---

## ROLE HIERARCHY

| Role | Level | Permissions |
|------|-------|-------------|
| Guest | 0 | View public boards only |
| Member | 1 | Create threads/replies, edit own content, flag |
| Moderator | 2 | Hide content, lock threads, ban users, manage invites |
| Admin | 3 | Board settings, set thresholds, invite moderators |
| Owner | 4 | Transfer ownership, invite admins, all permissions |

---

## REGISTRY CONTRACT

### Initialization

```rust
fn init(env, admin: Address, permissions: Address, content: Address, theme: Address, admin_contract: Address)
fn set_board_wasm_hash(env, wasm_hash: BytesN<32>)  // For auto-deployment
```

### Board Management

| Function | Signature | Auth |
|----------|-----------|------|
| `create_board` | `(name: String, description: String, is_private: String, is_listed: String, caller: Address) -> u64` | caller |
| `get_board` | `(board_id: u64) -> Option<BoardMeta>` | none |
| `get_board_by_name` | `(name: String) -> Option<BoardMeta>` | none |
| `list_boards` | `(start: u64, limit: u64) -> Vec<BoardMeta>` | none |
| `list_listed_boards` | `(start: u64, limit: u64) -> Vec<BoardMeta>` | none |
| `board_count` | `() -> u64` | none |
| `rename_board` | `(board_id: u64, new_name: String, caller: Address)` | Admin+ |
| `get_board_aliases` | `(board_id: u64) -> Vec<String>` | none |
| `set_listed` | `(board_id: u64, is_listed: bool, caller: Address)` | Admin+ |
| `get_board_listed` | `(board_id: u64) -> bool` | none |
| `set_private` | `(board_id: u64, is_private: bool, caller: Address)` | Admin+ |
| `get_board_private` | `(board_id: u64) -> bool` | none |
| `set_readonly` | `(board_id: u64, is_readonly: bool, caller: Address)` | Admin+ |
| `get_board_readonly` | `(board_id: u64) -> bool` | none |

### Contract Resolution

| Function | Signature |
|----------|-----------|
| `get_contracts` | `() -> ServiceContracts` |
| `get_contract_by_alias` | `(alias: Symbol) -> Option<Address>` |
| `get_board_contract` | `(board_id: u64) -> Option<Address>` |

### Upgrades

| Function | Signature | Auth |
|----------|-----------|------|
| `upgrade` | `(new_wasm_hash: BytesN<32>)` | admin |
| `upgrade_contract` | `(contract_id: Address, new_wasm_hash: BytesN<32>)` | admin |

---

## BOARD CONTRACT

Per-board contract, auto-deployed by registry.

### Initialization

```rust
fn init(env, board_id: u64, registry: Address, permissions: Address, content: Address, theme: Address, creator: Address)
```

### Thread Management

| Function | Signature | Auth |
|----------|-----------|------|
| `create_thread` | `(title: String, creator: Address) -> u64` | Member+ |
| `get_thread` | `(thread_id: u64) -> Option<ThreadMeta>` | none |
| `list_threads` | `(start: u64, limit: u64) -> Vec<ThreadMeta>` | none |
| `list_pinned_threads` | `() -> Vec<ThreadMeta>` | none |
| `thread_count` | `() -> u64` | none |
| `edit_thread_title` | `(thread_id: u64, new_title: String, caller: Address)` | creator/Mod+ |
| `delete_thread` | `(thread_id: u64, caller: Address)` | Mod+ |

### Thread Moderation

| Function | Signature | Auth |
|----------|-----------|------|
| `lock_thread` | `(thread_id: u64, caller: Address)` | Mod+ |
| `unlock_thread` | `(thread_id: u64, caller: Address)` | Mod+ |
| `is_thread_locked` | `(thread_id: u64) -> bool` | none |
| `pin_thread` | `(thread_id: u64, caller: Address)` | Mod+ |
| `unpin_thread` | `(thread_id: u64, caller: Address)` | Mod+ |
| `hide_thread` | `(thread_id: u64, caller: Address)` | Mod+ |
| `unhide_thread` | `(thread_id: u64, caller: Address)` | Mod+ |

### Reply Counts

| Function | Signature |
|----------|-----------|
| `increment_reply_count` | `(thread_id: u64)` |
| `get_reply_count` | `(thread_id: u64) -> u32` |

### Configuration

| Function | Signature | Auth |
|----------|-----------|------|
| `set_edit_window` | `(seconds: u64, caller: Address)` | Admin+ |
| `get_edit_window` | `() -> u64` | none |
| `set_chunk_size` | `(size: u32, caller: Address)` | Admin+ |
| `get_chunk_size` | `() -> u32` | none |
| `set_max_reply_depth` | `(depth: u32, caller: Address)` | Admin+ |
| `get_max_reply_depth` | `() -> u32` | none |

---

## CONTENT CONTRACT

Thread body and reply content storage using soroban-chonk.

### Thread Body

| Function | Signature | Auth |
|----------|-----------|------|
| `set_thread_body` | `(board_id: u64, thread_id: u64, content: Bytes, author: Address)` | Member+ |
| `get_thread_body` | `(board_id: u64, thread_id: u64) -> Bytes` | none |
| `edit_thread_body` | `(board_id: u64, thread_id: u64, content: Bytes, caller: Address)` | creator/Mod+ |

### Replies

| Function | Signature | Auth |
|----------|-----------|------|
| `create_reply` | `(board_id: u64, thread_id: u64, parent_id: u64, content: Bytes, depth: u32, caller: Address) -> u64` | Member+ |
| `get_reply` | `(board_id: u64, thread_id: u64, reply_id: u64) -> Option<ReplyMeta>` | none |
| `get_reply_content` | `(board_id: u64, thread_id: u64, reply_id: u64) -> Bytes` | none |
| `get_replies` | `(board_id: u64, thread_id: u64, parent_id: u64, start: u64, limit: u64) -> Vec<ReplyMeta>` | none |
| `get_children_count` | `(board_id: u64, thread_id: u64, parent_id: u64) -> u32` | none |
| `edit_reply` | `(board_id: u64, thread_id: u64, reply_id: u64, content: Bytes, caller: Address)` | creator/Mod+ |
| `edit_reply_content` | `(board_id: u64, thread_id: u64, reply_id: u64, content: Bytes, caller: Address)` | creator/Mod+ |

### Content Moderation

| Function | Signature | Auth |
|----------|-----------|------|
| `flag_content` | `(board_id: u64, thread_id: u64, reply_id: u64, reason: String, caller: Address)` | Member+ |
| `unflag_content` | `(board_id: u64, thread_id: u64, reply_id: u64, caller: Address)` | flagger |
| `hide_reply` | `(board_id: u64, thread_id: u64, reply_id: u64, caller: Address)` | Mod+ |
| `unhide_reply` | `(board_id: u64, thread_id: u64, reply_id: u64, caller: Address)` | Mod+ |

---

## PERMISSIONS CONTRACT

### Initialization

```rust
fn init(env, registry: Address)
```

### Role Management

| Function | Signature | Auth |
|----------|-----------|------|
| `set_role` | `(board_id: u64, user: Address, role: Role, caller: Address)` | higher role |
| `get_role` | `(board_id: u64, user: Address) -> Role` | none |
| `check_permission` | `(board_id: u64, user: Address, required: Role) -> bool` | none |
| `list_admins` | `(board_id: u64) -> Vec<Address>` | none |
| `list_moderators` | `(board_id: u64) -> Vec<Address>` | none |
| `list_members` | `(board_id: u64) -> Vec<Address>` | none |

### Banning

| Function | Signature | Auth |
|----------|-----------|------|
| `ban_user` | `(board_id: u64, user: Address, reason: String, duration_hours: Option<u32>, caller: Address)` | Mod+ |
| `unban_user` | `(board_id: u64, user: Address, caller: Address)` | Mod+ |
| `is_banned` | `(board_id: u64, user: Address) -> bool` | none |
| `get_ban` | `(board_id: u64, user: Address) -> Option<Ban>` | none |
| `list_banned` | `(board_id: u64) -> Vec<Address>` | none |
| `list_bans` | `(board_id: u64) -> Vec<Ban>` | none |

### Invite System

| Function | Signature | Auth |
|----------|-----------|------|
| `request_invite` | `(board_id: u64, caller: Address)` | non-member |
| `accept_invite` | `(board_id: u64, user: Address, caller: Address)` | Mod+ |
| `revoke_invite` | `(board_id: u64, user: Address, caller: Address)` | Mod+ |
| `invite_member` | `(board_id: u64, user: Address, role: Role, caller: Address)` | varies by role |
| `list_invite_requests` | `(board_id: u64) -> Vec<InviteRequest>` | none |
| `has_invite_request` | `(board_id: u64, user: Address) -> bool` | none |
| `get_invite_request` | `(board_id: u64, user: Address) -> Option<InviteRequest>` | none |

### Flag Threshold

| Function | Signature | Auth |
|----------|-----------|------|
| `set_flag_threshold` | `(board_id: u64, threshold: u32, caller: Address)` | Admin+ |
| `get_flag_threshold` | `(board_id: u64) -> u32` | none |

---

## THEME CONTRACT

UI rendering via soroban-render-sdk.

### Rendering

| Function | Signature |
|----------|-----------|
| `render` | `(path: Option<String>, viewer: Option<Address>) -> Bytes` |
| `get_css` | `() -> Bytes` |

### Routes

| Path | Description |
|------|-------------|
| `/` | Home page (board list) |
| `/b/{id}` | Board view (thread list) |
| `/b/{id}/t/{tid}` | Thread view (with replies) |
| `/b/{id}/t/{tid}/replies/{offset}` | Reply continuation (waterfall) |
| `/b/{id}/t/{tid}/r/{rid}/children/{offset}` | Child replies continuation |
| `/b/{id}/new` | New thread form |
| `/b/{id}/t/{tid}/reply` | Reply form |
| `/b/{id}/t/{tid}/r/{rid}/reply` | Nested reply form |
| `/b/{id}/t/{tid}/edit` | Edit thread form |
| `/b/{id}/t/{tid}/r/{rid}/edit` | Edit reply form |

### Write Operations (proxied)

| Function | Parameters |
|----------|------------|
| `create_board` | `name, description, is_private, is_listed, caller` |
| `create_thread` | `board_id, title, content, caller` |
| `create_reply` | `board_id, thread_id, parent_id, depth, content, caller` |
| `edit_thread` | `board_id, thread_id, new_title, new_body, caller` |
| `edit_reply` | `board_id, thread_id, reply_id, content, caller` |

---

## ADMIN CONTRACT

Admin panel UI.

### Routes

| Path | Description |
|------|-------------|
| `/b/{id}/settings` | Board settings |
| `/b/{id}/members` | Member management |
| `/b/{id}/invites` | Invite requests |
| `/b/{id}/flags` | Flag queue |
| `/b/{id}/bans` | Ban management |

---

## TYPES

### BoardMeta

```rust
pub struct BoardMeta {
    pub id: u64,
    pub name: String,
    pub description: String,
    pub creator: Address,
    pub created_at: u64,
    pub thread_count: u64,
    pub is_private: bool,
    pub is_listed: bool,
    pub is_readonly: bool,
}
```

### ThreadMeta

```rust
pub struct ThreadMeta {
    pub id: u64,
    pub board_id: u64,
    pub title: String,
    pub creator: Address,
    pub created_at: u64,
    pub updated_at: u64,
    pub reply_count: u32,
    pub is_locked: bool,
    pub is_pinned: bool,
    pub is_hidden: bool,
    pub is_deleted: bool,
}
```

### ReplyMeta

```rust
pub struct ReplyMeta {
    pub id: u64,
    pub board_id: u64,
    pub thread_id: u64,
    pub parent_id: u64,     // 0 = top-level
    pub depth: u32,         // Nesting level
    pub creator: Address,
    pub created_at: u64,
    pub updated_at: u64,
    pub is_hidden: bool,
    pub is_deleted: bool,
    pub flag_count: u32,
}
```

### Role

```rust
pub enum Role {
    Guest = 0,
    Member = 1,
    Moderator = 2,
    Admin = 3,
    Owner = 4,
}
```

### Ban

```rust
pub struct Ban {
    pub user: Address,
    pub board_id: u64,
    pub issuer: Address,
    pub reason: String,
    pub created_at: u64,
    pub expires_at: Option<u64>,  // None = permanent
}
```

### InviteRequest

```rust
pub struct InviteRequest {
    pub user: Address,
    pub board_id: u64,
    pub created_at: u64,
}
```

### ServiceContracts

```rust
pub struct ServiceContracts {
    pub registry: Address,
    pub permissions: Address,
    pub content: Address,
    pub theme: Address,
    pub admin: Address,
}
```

---

## STORAGE KEYS

### Registry

| Key | Type |
|-----|------|
| `Admin` | `Address` |
| `BoardCount` | `u64` |
| `Board(id)` | `BoardMeta` |
| `BoardContract(id)` | `Address` |
| `BoardListed(id)` | `bool` |
| `BoardPrivate(id)` | `bool` |
| `BoardReadonly(id)` | `bool` |
| `BoardNameToId(name)` | `u64` |
| `BoardAliases(id)` | `Vec<String>` |
| `Contracts` | `ServiceContracts` |
| `BoardWasmHash` | `BytesN<32>` |

### Board

| Key | Type |
|-----|------|
| `BoardId` | `u64` |
| `Registry` | `Address` |
| `ThreadCount` | `u64` |
| `Thread(id)` | `ThreadMeta` |
| `PinnedThreads` | `Vec<u64>` |
| `EditWindow` | `u64` |
| `ChunkSize` | `u32` |

### Permissions

| Key | Type |
|-----|------|
| `Registry` | `Address` |
| `BoardOwner(board_id)` | `Address` |
| `BoardRole(board_id, user)` | `Role` |
| `BoardBan(board_id, user)` | `Ban` |
| `FlagThreshold(board_id)` | `u32` |
| `BoardAdmins(board_id)` | `Vec<Address>` |
| `BoardModerators(board_id)` | `Vec<Address>` |
| `BoardMembers(board_id)` | `Vec<Address>` |
| `BannedUsers(board_id)` | `Vec<Address>` |
| `InviteRequest(board_id, user)` | `InviteRequest` |
| `InviteRequests(board_id)` | `Vec<Address>` |

### Content

| Key | Type |
|-----|------|
| `Registry` | `Address` |
| `ThreadBody(board_id, thread_id)` | `Bytes` (chunked) |
| `Reply(board_id, thread_id, reply_id)` | `ReplyMeta` |
| `ReplyContent(board_id, thread_id, reply_id)` | `Bytes` (chunked) |
| `ReplyCount(board_id, thread_id)` | `u64` |
| `ChildrenCount(board_id, thread_id, parent_id)` | `u32` |
| `ChildrenIndex(board_id, thread_id, parent_id)` | `Vec<u64>` |

---

## FORM PARAMETERS

### create_board

| Field | Type | Required |
|-------|------|----------|
| `name` | String | Yes |
| `description` | String | Yes |
| `is_private` | `"true"/"false"` | Yes |
| `is_listed` | `"true"/"false"` | No (default: true) |
| `caller` | Address | Yes (from viewer) |

### create_thread

| Field | Type | Required |
|-------|------|----------|
| `board_id` | u64 | Yes (hidden) |
| `title` | String | Yes |
| `content` | Bytes (hex) | Yes |
| `caller` | Address | Yes (from viewer) |
| `_redirect` | String | No (post-submit navigation) |

### create_reply

| Field | Type | Required |
|-------|------|----------|
| `board_id` | u64 | Yes (hidden) |
| `thread_id` | u64 | Yes (hidden) |
| `parent_id` | u64 | Yes (hidden, 0 for top-level) |
| `depth` | u32 | Yes (hidden) |
| `content` | Bytes (hex) | Yes |
| `caller` | Address | Yes (from viewer) |
| `_redirect` | String | No |

---

## PROGRESSIVE LOADING

Thread views use waterfall loading for replies:

1. Initial render shows first N replies (chunk_size, default 6)
2. `{{render path="/b/{id}/t/{tid}/replies/{offset}"}}` loads more
3. Nested children use `/b/{id}/t/{tid}/r/{rid}/children/{offset}`

---

## INTEGRATION EXAMPLE

```rust
// Create a board
stellar contract invoke --id $REGISTRY_ID -- create_board \
  --name "General" \
  --description "General discussion" \
  --is_private "false" \
  --is_listed "true" \
  --caller $USER

// Create a thread (hex-encoded content)
stellar contract invoke --id $THEME_ID -- create_thread \
  --board_id 0 \
  --title "Hello World" \
  --content 48656c6c6f \
  --caller $USER

// Render the board
stellar contract invoke --id $THEME_ID -- render \
  --path '"/b/0"' \
  --viewer $USER
```

---

## DEPENDENCIES

- [soroban-sdk](https://github.com/stellar/rs-soroban-sdk) - Stellar smart contract SDK
- [soroban-render-sdk](https://github.com/wyhaines/soroban-render-sdk) - UI rendering
- [soroban-chonk](https://github.com/wyhaines/soroban-chonk) - Chunked content storage
