# Soroban Boards API Reference

Complete reference for all public contract functions in Soroban Boards.

## Table of Contents

- [Registry Contract](#registry-contract)
- [Board Contract](#board-contract)
- [Content Contract](#content-contract)
- [Permissions Contract](#permissions-contract)
- [Admin Contract](#admin-contract)
- [Theme Contract](#theme-contract)
- [Types](#types)

---

## Registry Contract

The registry manages board discovery, creation, and service contract addresses.

### Initialization

#### `init`
Initialize the registry with admin and service contract addresses.

```rust
fn init(
    env: Env,
    admin: Address,
    permissions: Address,
    content: Address,
    theme: Address,
    admin_contract: Address,
)
```

| Parameter | Type | Description |
|-----------|------|-------------|
| `admin` | `Address` | Registry administrator |
| `permissions` | `Address` | Permissions contract address |
| `content` | `Address` | Content contract address |
| `theme` | `Address` | Theme contract address |
| `admin_contract` | `Address` | Admin UI contract address |

**Panics:** If already initialized.

---

### Board Creation & Retrieval

#### `create_board`
Create a new board.

```rust
fn create_board(
    env: Env,
    name: String,
    description: String,
    is_private: String,
    is_listed: String,
    caller: Address,
) -> u64
```

| Parameter | Type | Description |
|-----------|------|-------------|
| `name` | `String` | Board name (3-50 chars, alphanumeric + `_-`) |
| `description` | `String` | Board description |
| `is_private` | `String` | `"true"` or `"false"` |
| `is_listed` | `String` | `"true"`, `"false"`, or empty (defaults to `true`) |
| `caller` | `Address` | Creator address (becomes Owner) |

**Returns:** New board ID.

**Panics:** If registry is paused, or if name already exists.

---

#### `get_board`
Get board metadata by ID.

```rust
fn get_board(env: Env, board_id: u64) -> Option<BoardMeta>
```

**Returns:** `Some(BoardMeta)` or `None` if not found.

---

#### `get_board_by_name`
Look up a board by name or any of its aliases.

```rust
fn get_board_by_name(env: Env, name: String) -> Option<BoardMeta>
```

**Returns:** `Some(BoardMeta)` or `None` if not found.

---

#### `list_boards`
List all boards with pagination (for admin use).

```rust
fn list_boards(env: Env, start: u64, limit: u64) -> Vec<BoardMeta>
```

| Parameter | Type | Description |
|-----------|------|-------------|
| `start` | `u64` | Starting index |
| `limit` | `u64` | Maximum boards to return |

**Returns:** Vector of board metadata.

---

#### `list_listed_boards`
List only publicly listed boards (for home page).

```rust
fn list_listed_boards(env: Env, start: u64, limit: u64) -> Vec<BoardMeta>
```

**Returns:** Vector of listed board metadata.

---

#### `board_count`
Get total number of boards.

```rust
fn board_count(env: Env) -> u64
```

---

### Board Settings

#### `rename_board`
Rename a board. Old name becomes an alias.

```rust
fn rename_board(env: Env, board_id: u64, new_name: String, caller: Address)
```

**Authorization:** Admin+ on the board.

**Panics:** If board doesn't exist, caller not authorized, or name already in use.

---

#### `get_board_aliases`
Get list of aliases (previous names) for a board.

```rust
fn get_board_aliases(env: Env, board_id: u64) -> Vec<String>
```

---

#### `set_listed` / `get_board_listed`
Toggle whether board appears on home page.

```rust
fn set_listed(env: Env, board_id: u64, is_listed: bool, caller: Address)
fn get_board_listed(env: Env, board_id: u64) -> bool
```

**Authorization:** Admin+ on the board.

---

#### `set_private` / `get_board_private`
Toggle whether board requires membership to view.

```rust
fn set_private(env: Env, board_id: u64, is_private: bool, caller: Address)
fn get_board_private(env: Env, board_id: u64) -> bool
```

**Authorization:** Admin+ on the board.

---

#### `set_readonly` / `get_board_readonly`
Toggle whether board allows new content.

```rust
fn set_readonly(env: Env, board_id: u64, is_readonly: bool, caller: Address)
fn get_board_readonly(env: Env, board_id: u64) -> bool
```

**Authorization:** Admin+ on the board.

---

### Contract Management

#### `get_contracts`
Get addresses of all service contracts.

```rust
fn get_contracts(env: Env) -> ContractAddresses
```

---

#### `get_contract_by_alias`
Look up a contract address by alias. Used by the viewer for `form:@alias:method` protocol.

```rust
fn get_contract_by_alias(env: Env, alias: Symbol) -> Option<Address>
```

| Alias | Contract |
|-------|----------|
| `"registry"` | This registry contract |
| `"perms"` | Permissions contract |
| `"content"` | Content contract |
| `"theme"` | Theme contract |
| `"admin"` | Admin contract |

---

#### `get_board_contract` / `set_board_contract`
Get or set the per-board contract address.

```rust
fn get_board_contract(env: Env, board_id: u64) -> Option<Address>
fn set_board_contract(env: Env, board_id: u64, board_contract: Address)  // Admin only
```

---

#### `set_board_wasm_hash` / `get_board_wasm_hash`
Set WASM hash for auto-deploying board contracts.

```rust
fn set_board_wasm_hash(env: Env, wasm_hash: BytesN<32>)  // Admin only
fn get_board_wasm_hash(env: Env) -> Option<BytesN<32>>
```

---

#### `upgrade`
Upgrade the registry contract WASM.

```rust
fn upgrade(env: Env, new_wasm_hash: BytesN<32>)
```

**Authorization:** Registry admin.

---

#### `upgrade_contract`
Upgrade another contract via proxy.

```rust
fn upgrade_contract(env: Env, contract_id: Address, new_wasm_hash: BytesN<32>)
```

**Authorization:** Registry admin.

---

#### `configure_board`
Configure a board contract with service addresses.

```rust
fn configure_board(env: Env, board_id: u64)
```

**Authorization:** Registry admin.

---

### Admin Management

#### `get_admin`
Get registry admin address.

```rust
fn get_admin(env: Env) -> Address
```

---

#### `transfer_admin`
Start two-step admin transfer.

```rust
fn transfer_admin(env: Env, new_admin: Address)
```

**Authorization:** Current admin.

---

#### `accept_admin`
Accept pending admin transfer.

```rust
fn accept_admin(env: Env)
```

**Authorization:** New admin address.

**Panics:** If no pending transfer or transfer expired.

---

#### `set_paused` / `is_paused`
Pause or unpause board creation.

```rust
fn set_paused(env: Env, paused: bool)  // Admin only
fn is_paused(env: Env) -> bool
```

---

### Rendering

#### `render`
Main render entry point for home page.

```rust
fn render(env: Env, path: Option<String>, viewer: Option<Address>) -> Bytes
```

| Route | Description |
|-------|-------------|
| `/` | Home page with board list |
| `/create` | Create board form |
| `/help` | Help page |

---

#### `styles`
Get CSS from theme contract.

```rust
fn styles(env: Env) -> Bytes
```

---

## Board Contract

Each board has its own contract for thread management.

### Initialization

#### `init`
Initialize a board contract.

```rust
fn init(
    env: Env,
    board_id: u64,
    registry: Address,
    permissions: Option<Address>,
    content: Option<Address>,
    theme: Option<Address>,
    name: String,
    description: String,
    is_private: bool,
)
```

---

### Thread Operations

#### `create_thread`
Create a new thread (metadata only).

```rust
fn create_thread(env: Env, title: String, caller: Address) -> u64
```

**Authorization:** Member+ on board.

**Returns:** New thread ID.

---

#### `get_thread`
Get thread metadata.

```rust
fn get_thread(env: Env, thread_id: u64) -> Option<ThreadMeta>
```

---

#### `list_threads`
List threads with pagination.

```rust
fn list_threads(env: Env, start: u64, limit: u64) -> Vec<ThreadMeta>
```

---

#### `get_thread_count`
Get total thread count.

```rust
fn get_thread_count(env: Env) -> u64
```

---

### Thread Metadata Operations

#### `edit_thread_title`
Edit a thread's title.

```rust
fn edit_thread_title(env: Env, thread_id: u64, new_title: String, caller: Address)
```

**Authorization:** Thread creator (within edit window) or Moderator+.

---

#### `delete_thread`
Soft-delete a thread.

```rust
fn delete_thread(env: Env, thread_id: u64, caller: Address)
```

**Authorization:** Thread creator or Moderator+.

---

#### `lock_thread` / `unlock_thread`
Lock or unlock a thread.

```rust
fn lock_thread(env: Env, thread_id: u64, caller: Address)
fn unlock_thread(env: Env, thread_id: u64, caller: Address)
```

**Authorization:** Moderator+.

---

#### `is_thread_locked`
Check if thread is locked.

```rust
fn is_thread_locked(env: Env, thread_id: u64) -> bool
```

---

#### `pin_thread` / `unpin_thread`
Pin or unpin a thread.

```rust
fn pin_thread(env: Env, thread_id: u64, caller: Address)
fn unpin_thread(env: Env, thread_id: u64, caller: Address)
```

**Authorization:** Moderator+.

---

#### `hide_thread` / `unhide_thread`
Hide or show a thread.

```rust
fn hide_thread(env: Env, thread_id: u64, caller: Address)
fn unhide_thread(env: Env, thread_id: u64, caller: Address)
```

**Authorization:** Moderator+.

---

#### `increment_reply_count`
Increment reply count (called by content contract).

```rust
fn increment_reply_count(env: Env, thread_id: u64)
```

---

### Configuration

#### `get_config`
Get board configuration.

```rust
fn get_config(env: Env) -> BoardConfig
```

---

#### `get_chunk_size` / `set_chunk_size`
Get or set waterfall loading chunk size.

```rust
fn get_chunk_size(env: Env) -> u32
fn set_chunk_size(env: Env, size: u32, caller: Address)  // Admin+ only
```

---

#### `get_max_reply_depth` / `set_max_reply_depth`
Get or set maximum nesting depth for replies.

```rust
fn get_max_reply_depth(env: Env) -> u32
fn set_max_reply_depth(env: Env, depth: u32, caller: Address)  // Admin+ only
```

- Default: 10
- Range: 1-20
- Replies at max depth cannot have children

---

#### `get_edit_window` / `set_edit_window`
Get or set edit time window.

```rust
fn get_edit_window(env: Env) -> u64  // Seconds, 0 = no limit
fn set_edit_window(env: Env, seconds: u64, caller: Address)  // Admin+ only
```

---

### Rendering

#### `render`
Main render entry point for board pages.

```rust
fn render(env: Env, path: Option<String>, viewer: Option<Address>) -> Bytes
```

| Route | Description |
|-------|-------------|
| `/` | Board page with thread list |
| `/new` | Create thread form |
| `/t/{id}` | Thread view with replies |
| `/t/{id}/edit` | Edit thread form |
| `/t/{id}/replies/{batch}` | Reply batch (waterfall) |
| `/t/{id}/r/{rid}/children/{batch}` | Child replies batch |
| `/t/{id}/r/{rid}/reply` | Reply form |
| `/t/{id}/r/{rid}/edit` | Edit reply form |

---

## Content Contract

Stores thread bodies and replies using chunked storage.

### Initialization

#### `init`
Initialize content contract.

```rust
fn init(env: Env, registry: Address, permissions: Option<Address>)
```

---

### Thread Content

#### `create_thread`
Create a thread with body content.

```rust
fn create_thread(
    env: Env,
    board_id: u64,
    title: String,
    body: String,
    caller: Address,
) -> u64
```

**Authorization:** Member+ on board.

**Panics:** If board is read-only.

---

#### `get_thread_body`
Get thread body content.

```rust
fn get_thread_body(env: Env, board_id: u64, thread_id: u64) -> Bytes
```

---

#### `get_thread_body_chunk`
Get a specific chunk of thread body.

```rust
fn get_thread_body_chunk(env: Env, board_id: u64, thread_id: u64, chunk_index: u32) -> Option<Bytes>
```

---

#### `edit_thread_body`
Edit thread body content.

```rust
fn edit_thread_body(env: Env, board_id: u64, thread_id: u64, content: Bytes, caller: Address)
```

**Authorization:** Thread creator or Moderator+.

**Panics:** If board is read-only (unless moderator).

---

### Reply Operations

#### `create_reply`
Create a reply.

```rust
fn create_reply(
    env: Env,
    board_id: u64,
    thread_id: u64,
    parent_id: u64,
    content: String,
    caller: Address,
) -> u64
```

| Parameter | Type | Description |
|-----------|------|-------------|
| `parent_id` | `u64` | `0` for top-level reply, otherwise parent reply ID |

**Authorization:** Member+ on board.

**Panics:** If board is read-only or thread is locked.

---

#### `get_reply`
Get reply metadata.

```rust
fn get_reply(env: Env, board_id: u64, thread_id: u64, reply_id: u64) -> Option<ReplyMeta>
```

---

#### `get_replies`
Get replies for a thread (top-level).

```rust
fn get_replies(env: Env, board_id: u64, thread_id: u64, start: u64, limit: u64) -> Vec<ReplyMeta>
```

---

#### `get_reply_content`
Get reply content.

```rust
fn get_reply_content(env: Env, board_id: u64, thread_id: u64, reply_id: u64) -> Bytes
```

---

#### `get_child_replies`
Get child replies for a parent.

```rust
fn get_child_replies(env: Env, board_id: u64, thread_id: u64, parent_id: u64, start: u64, limit: u64) -> Vec<ReplyMeta>
```

---

#### `get_children_count`
Get count of child replies.

```rust
fn get_children_count(env: Env, board_id: u64, thread_id: u64, parent_id: u64) -> u32
```

---

#### `edit_reply` / `edit_reply_content`
Edit reply content.

```rust
fn edit_reply(env: Env, board_id: u64, thread_id: u64, reply_id: u64, content: String, caller: Address)
fn edit_reply_content(env: Env, board_id: u64, thread_id: u64, reply_id: u64, content: Bytes, caller: Address)
```

**Authorization:** Reply creator or Moderator+.

**Panics:** If board is read-only or thread is locked.

---

#### `hide_reply` / `unhide_reply`
Hide or show a reply.

```rust
fn hide_reply(env: Env, board_id: u64, thread_id: u64, reply_id: u64, caller: Address)
fn unhide_reply(env: Env, board_id: u64, thread_id: u64, reply_id: u64, caller: Address)
```

**Authorization:** Moderator+.

---

### Flagging

#### `flag_content`
Flag a thread or reply.

```rust
fn flag_content(
    env: Env,
    board_id: u64,
    thread_id: u64,
    reply_id: u64,
    reason: String,
    caller: Address,
)
```

| Parameter | Type | Description |
|-----------|------|-------------|
| `reply_id` | `u64` | `0` to flag thread, otherwise reply ID |

---

#### `get_flag_count`
Get flag count for content.

```rust
fn get_flag_count(env: Env, board_id: u64, thread_id: u64, reply_id: u64) -> u32
```

---

#### `get_flagged_content`
List all flagged content for a board.

```rust
fn get_flagged_content(env: Env, board_id: u64) -> Vec<FlaggedItem>
```

---

## Permissions Contract

Manages roles, bans, and invites.

### Initialization

#### `init`
Initialize permissions contract.

```rust
fn init(env: Env, registry: Address)
```

---

### Role Management

#### `set_board_owner`
Set board owner (called by registry).

```rust
fn set_board_owner(env: Env, board_id: u64, owner: Address)
```

---

#### `get_board_owner`
Get board owner.

```rust
fn get_board_owner(env: Env, board_id: u64) -> Option<Address>
```

---

#### `set_role`
Set a user's role on a board.

```rust
fn set_role(env: Env, board_id: u64, user: Address, role: Role, caller: Address)
```

**Authorization:** See [FEATURES.md](./FEATURES.md#setting-roles).

---

#### `get_role`
Get a user's role on a board.

```rust
fn get_role(env: Env, board_id: u64, user: Address) -> Role
```

**Returns:** Role (defaults to `Guest`).

---

#### `has_role`
Check if user has at least specified role.

```rust
fn has_role(env: Env, board_id: u64, user: Address, min_role: Role) -> bool
```

---

#### `get_permissions`
Get full permission set for a user.

```rust
fn get_permissions(env: Env, board_id: u64, user: Address) -> PermissionSet
```

---

### Membership Lists

#### `list_admins` / `list_moderators` / `list_members`
List users with specific roles.

```rust
fn list_admins(env: Env, board_id: u64) -> Vec<Address>
fn list_moderators(env: Env, board_id: u64) -> Vec<Address>
fn list_members(env: Env, board_id: u64) -> Vec<Address>
```

---

#### `role_count`
Get count of users with a role.

```rust
fn role_count(env: Env, board_id: u64, role: Role) -> u32
```

---

### Banning

#### `ban_user`
Ban a user from a board.

```rust
fn ban_user(
    env: Env,
    board_id: u64,
    user: Address,
    reason: String,
    duration_hours: Option<u64>,
    caller: Address,
)
```

| Parameter | Type | Description |
|-----------|------|-------------|
| `duration_hours` | `Option<u64>` | `None` = permanent, `Some(24)` = 24 hours |

**Authorization:** Moderator+ (cannot ban higher roles).

---

#### `unban_user`
Remove a ban.

```rust
fn unban_user(env: Env, board_id: u64, user: Address, caller: Address)
```

**Authorization:** Moderator+.

---

#### `is_banned`
Check if user is banned.

```rust
fn is_banned(env: Env, board_id: u64, user: Address) -> bool
```

---

#### `get_ban`
Get ban details.

```rust
fn get_ban(env: Env, board_id: u64, user: Address) -> Option<Ban>
```

---

#### `list_banned` / `list_bans`
List banned users.

```rust
fn list_banned(env: Env, board_id: u64) -> Vec<Address>  // Active bans only
fn list_bans(env: Env, board_id: u64) -> Vec<Ban>        // With details
```

---

### Invite System

#### `request_invite`
Request to join a board (user-initiated).

```rust
fn request_invite(env: Env, board_id: u64, caller: Address)
```

**Panics:** If already member, banned, or request pending.

---

#### `accept_invite`
Accept a pending invite request.

```rust
fn accept_invite(env: Env, board_id: u64, user: Address, caller: Address)
```

**Authorization:** Moderator+.

---

#### `revoke_invite`
Reject/revoke a pending invite request.

```rust
fn revoke_invite(env: Env, board_id: u64, user: Address, caller: Address)
```

**Authorization:** Moderator+.

---

#### `invite_member`
Directly invite a user with a role.

```rust
fn invite_member(env: Env, board_id: u64, user: Address, role: Role, caller: Address)
```

**Authorization:** See [FEATURES.md](./FEATURES.md#direct-invite-flow-admin-initiated).

---

#### `list_invite_requests`
List pending invite requests.

```rust
fn list_invite_requests(env: Env, board_id: u64) -> Vec<InviteRequest>
```

---

#### `has_invite_request`
Check if user has pending request.

```rust
fn has_invite_request(env: Env, board_id: u64, user: Address) -> bool
```

---

#### `get_invite_request`
Get specific invite request.

```rust
fn get_invite_request(env: Env, board_id: u64, user: Address) -> Option<InviteRequest>
```

---

### Permission Checks

#### `can_create_thread` / `can_reply`
Check posting permissions.

```rust
fn can_create_thread(env: Env, board_id: u64, user: Address) -> bool
fn can_reply(env: Env, board_id: u64, user: Address) -> bool
```

---

#### `can_moderate` / `can_admin`
Check moderation/admin permissions.

```rust
fn can_moderate(env: Env, board_id: u64, user: Address) -> bool
fn can_admin(env: Env, board_id: u64, user: Address) -> bool
```

---

### Flag Threshold

#### `set_flag_threshold` / `get_flag_threshold`
Set or get auto-hide threshold.

```rust
fn set_flag_threshold(env: Env, board_id: u64, threshold: u32, caller: Address)  // Admin+
fn get_flag_threshold(env: Env, board_id: u64) -> u32  // Default: 3
```

---

## Admin Contract

Provides admin UI for board management.

### Rendering

#### `render`
Main render entry point for admin pages.

```rust
fn render(env: Env, path: Option<String>, viewer: Option<Address>) -> Bytes
```

| Route | Description |
|-------|-------------|
| `/b/{id}/` | Admin dashboard |
| `/b/{id}/settings` | Board settings |
| `/b/{id}/members` | Member management |
| `/b/{id}/bans` | Ban management |
| `/b/{id}/flags` | Flagged content queue |
| `/b/{id}/invites` | Invite requests |
| `/b/{id}/appearance` | Appearance settings |

---

### Admin Operations

#### `rename_board`
Rename a board (proxy to registry).

```rust
fn rename_board(env: Env, board_id: u64, new_name: String, caller: Address)
```

---

#### `set_role`
Set user role (proxy to permissions).

```rust
fn set_role(env: Env, board_id: u64, user: Address, role_value: u32, caller: Address)
```

---

#### `ban_user`
Ban a user (proxy to permissions).

```rust
fn ban_user(env: Env, board_id: u64, user: Address, reason: String, duration_hours: u64, caller: Address)
```

---

#### `unban_user`
Unban a user (proxy to permissions).

```rust
fn unban_user(env: Env, board_id: u64, user: Address, caller: Address)
```

---

#### `accept_invite`
Accept invite request (proxy to permissions).

```rust
fn accept_invite(env: Env, board_id: u64, user: Address, caller: Address)
```

---

#### `revoke_invite`
Revoke invite request (proxy to permissions).

```rust
fn revoke_invite(env: Env, board_id: u64, user: Address, caller: Address)
```

---

#### `hide_thread` / `unhide_thread`
Hide or show thread (proxy to board).

```rust
fn hide_thread(env: Env, board_id: u64, thread_id: u64, caller: Address)
fn unhide_thread(env: Env, board_id: u64, thread_id: u64, caller: Address)
```

---

#### `hide_reply` / `unhide_reply`
Hide or show reply (proxy to content).

```rust
fn hide_reply(env: Env, board_id: u64, thread_id: u64, reply_id: u64, caller: Address)
fn unhide_reply(env: Env, board_id: u64, thread_id: u64, reply_id: u64, caller: Address)
```

---

### Settings Operations

#### `set_flag_threshold`
Set auto-hide flag threshold (proxy to permissions).

```rust
fn set_flag_threshold(env: Env, board_id: u64, threshold: String, caller: Address)
```

---

#### `set_chunk_size`
Set waterfall loading chunk size (proxy to board).

```rust
fn set_chunk_size(env: Env, board_id: u64, chunk_size: String, caller: Address)
```

---

#### `set_max_reply_depth`
Set maximum reply nesting depth (proxy to board).

```rust
fn set_max_reply_depth(env: Env, board_id: u64, max_depth: String, caller: Address)
```

---

#### `set_edit_window`
Set edit time window in hours (proxy to board).

```rust
fn set_edit_window(env: Env, board_id: u64, edit_hours: String, caller: Address)
```

---

#### `list_board` / `unlist_board`
Show or hide board from home page (proxy to registry).

```rust
fn list_board(env: Env, board_id: u64, caller: Address)
fn unlist_board(env: Env, board_id: u64, caller: Address)
```

---

#### `make_public` / `make_private`
Set board access control (proxy to registry).

```rust
fn make_public(env: Env, board_id: u64, caller: Address)
fn make_private(env: Env, board_id: u64, caller: Address)
```

---

#### `enable_posting` / `make_readonly`
Set board posting status (proxy to registry).

```rust
fn enable_posting(env: Env, board_id: u64, caller: Address)
fn make_readonly(env: Env, board_id: u64, caller: Address)
```

---

## Theme Contract

Generates CSS for the UI.

### Rendering

#### `render`
Returns CSS stylesheet.

```rust
fn render(env: Env, path: Option<String>, viewer: Option<Address>) -> Bytes
```

---

#### `styles`
Returns CSS stylesheet.

```rust
fn styles(env: Env) -> Bytes
```

---

## Types

### Role

```rust
#[repr(u32)]
pub enum Role {
    Guest = 0,
    Member = 1,
    Moderator = 2,
    Admin = 3,
    Owner = 4,
}
```

### BoardMeta

```rust
pub struct BoardMeta {
    pub id: u64,
    pub name: String,
    pub description: String,
    pub creator: Address,
    pub created_at: u64,
    pub thread_count: u64,
    pub is_readonly: bool,
    pub is_private: bool,
}
```

### BoardConfig

```rust
pub struct BoardConfig {
    pub name: String,
    pub description: String,
    pub is_private: bool,
    pub is_readonly: bool,
    pub max_reply_depth: u32,
    pub reply_chunk_size: u32,
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
    pub parent_id: u64,
    pub depth: u32,
    pub creator: Address,
    pub created_at: u64,
    pub updated_at: u64,
    pub is_hidden: bool,
    pub is_deleted: bool,
    pub flag_count: u32,
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
    pub expires_at: Option<u64>,
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

### PermissionSet

```rust
pub struct PermissionSet {
    pub role: Role,
    pub can_view: bool,
    pub can_post: bool,
    pub can_moderate: bool,
    pub can_admin: bool,
    pub is_banned: bool,
}
```

### Flag

```rust
pub struct Flag {
    pub flagger: Address,
    pub reason: String,
    pub created_at: u64,
    pub resolved: bool,
}
```

### FlaggedItem

```rust
pub struct FlaggedItem {
    pub board_id: u64,
    pub thread_id: u64,
    pub reply_id: u64,
    pub item_type: FlaggedType,
    pub flag_count: u32,
    pub first_flagged_at: u64,
}
```

### ContractAddresses

```rust
pub struct ContractAddresses {
    pub permissions: Address,
    pub content: Address,
    pub theme: Address,
    pub admin: Address,
}
```

---

## Related Documentation

- [README.md](./README.md) - Project overview
- [FEATURES.md](./FEATURES.md) - Feature descriptions
- [DEPLOYMENT.md](./DEPLOYMENT.md) - Deployment guide
