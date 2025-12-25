# Soroban Boards Features

This document describes the major features of Soroban Boards, a decentralized discussion forum running on Stellar's Soroban smart contract platform.

## Table of Contents

- [Permissions & Roles](#permissions--roles)
- [Invite System](#invite-system)
- [Board Management](#board-management)
- [Content Editing](#content-editing)
- [Content Moderation](#content-moderation)
- [Thread Management](#thread-management)

---

## Permissions & Roles

Soroban Boards uses a hierarchical 5-tier role system for access control. Higher roles inherit all permissions of lower roles.

### Role Hierarchy

| Role | Level | Description |
|------|-------|-------------|
| **Owner** | 4 | Full control over the board, can transfer ownership |
| **Admin** | 3 | Manage settings, roles up to Moderator |
| **Moderator** | 2 | Moderate content, manage members |
| **Member** | 1 | Create threads, post replies |
| **Guest** | 0 | View-only (default for non-members) |

### Permission Matrix

| Action | Guest | Member | Moderator | Admin | Owner |
|--------|-------|--------|-----------|-------|-------|
| View public board | Yes | Yes | Yes | Yes | Yes |
| View private board | No | Yes | Yes | Yes | Yes |
| Create threads | No | Yes | Yes | Yes | Yes |
| Post replies | No | Yes | Yes | Yes | Yes |
| Edit own content | No | Yes | Yes | Yes | Yes |
| Flag content | No | Yes | Yes | Yes | Yes |
| Hide/unhide content | No | No | Yes | Yes | Yes |
| Lock/unlock threads | No | No | Yes | Yes | Yes |
| Ban users | No | No | Yes | Yes | Yes |
| Accept invite requests | No | No | Yes | Yes | Yes |
| Invite members | No | No | Yes | Yes | Yes |
| Invite moderators | No | No | No | Yes | Yes |
| Invite admins | No | No | No | No | Yes |
| Change board settings | No | No | No | Yes | Yes |
| Set flag threshold | No | No | No | Yes | Yes |
| Transfer ownership | No | No | No | No | Yes |

### Setting Roles

Roles can be set by users with sufficient authority:

- **Owner** can assign any role, including transferring ownership
- **Admin** can assign Moderator, Member, or Guest roles
- **Moderator** can assign Member or Guest roles

```
set_role(board_id, user, role, caller)
```

**Error conditions:**
- Panics if caller doesn't have sufficient authority for the role being assigned
- Can demote users with equal or lower role

---

## Invite System

The invite system allows users to join private boards. There are two workflows:

### Request-Based Flow (User-Initiated)

1. User discovers a private board they want to join
2. User calls `request_invite(board_id)`
3. Request is stored with timestamp
4. Moderator, Admin, or Owner sees request in admin panel
5. They either accept or revoke the request

**Functions:**

```rust
// User requests to join a board
request_invite(board_id, caller)

// Moderator+ accepts a pending request (user becomes Member)
accept_invite(board_id, user, caller)

// Moderator+ rejects/revokes a pending request
revoke_invite(board_id, user, caller)
```

**Constraints:**
- User must not already be a member
- User must not be banned
- Cannot submit duplicate requests

### Direct Invite Flow (Admin-Initiated)

Administrators can directly invite users without waiting for a request:

```rust
// Directly invite a user with a specific role
invite_member(board_id, user, role, caller)
```

**Authorization rules:**
- **Moderator+** can invite as Member or Guest
- **Admin+** can invite as Moderator
- **Owner** can invite as Admin or Owner

**Behavior:**
- User is immediately assigned the specified role
- Any pending request from that user is automatically cleared
- Cannot invite banned users
- Cannot assign a role higher than the user already has

### Managing Invites

```rust
// List all pending invite requests for a board
list_invite_requests(board_id) -> Vec<InviteRequest>

// Check if a user has a pending request
has_invite_request(board_id, user) -> bool

// Get details of a specific request
get_invite_request(board_id, user) -> Option<InviteRequest>
```

### InviteRequest Structure

```rust
pub struct InviteRequest {
    pub user: Address,      // Requesting user
    pub board_id: u64,      // Board being requested
    pub created_at: u64,    // Timestamp of request
}
```

---

## Board Management

### Creating Boards

Create a new board with the registry contract:

```rust
create_board(name, description, is_private, is_listed, caller) -> u64
```

**Parameters:**
- `name`: Board name (3-50 chars, alphanumeric + underscore/hyphen, starts with letter)
- `description`: Board description
- `is_private`: "true" or "false" - controls member-only access
- `is_listed`: "true" or "false" - controls home page visibility
- `caller`: Creator address (becomes Owner)

**Returns:** The new board's ID

### Private vs. Public Boards

**Public boards** (`is_private = false`):
- Anyone can view content
- Only members can post (if not read-only)
- No invite required

**Private boards** (`is_private = true`):
- Only members can view content
- Members must be invited or have their request accepted
- Non-members see an invite request button

Toggle privacy status:
```rust
set_private(board_id, is_private, caller)  // Admin+ only
get_board_private(board_id) -> bool
```

### Listed vs. Unlisted Boards

**Listed boards** (`is_listed = true`):
- Appear on the home page board list
- Discoverable via browsing

**Unlisted boards** (`is_listed = false`):
- Hidden from the home page
- Still accessible via direct link (`/b/{id}`)
- Useful for private communities or testing

Toggle listing status:
```rust
set_listed(board_id, is_listed, caller)  // Admin+ only
get_board_listed(board_id) -> bool
```

You can combine these settings:
- **Public + Listed**: Normal public board
- **Public + Unlisted**: Public but hidden from browse (direct link required)
- **Private + Listed**: Visible on home page but requires membership
- **Private + Unlisted**: Completely hidden, invite-only

### Board Name Aliases

When you rename a board, the old name becomes an alias that continues to work:

```rust
rename_board(board_id, new_name, caller)  // Admin+ only
get_board_aliases(board_id) -> Vec<String>
get_board_by_name(name) -> Option<BoardMeta>  // Works with aliases too
```

**Behavior:**
- Old name is preserved as an alias
- Links using old name continue to work
- Name lookup checks both current name and aliases
- New name must be unique (not used by any board or alias)

**Name validation:**
- 3-50 characters
- Alphanumeric, underscore, hyphen only
- Must start with a letter

### Read-Only Mode

Boards can be set to read-only to prevent new content:

```rust
set_readonly(board_id, is_readonly, caller)  // Admin+ only
get_board_readonly(board_id) -> bool
```

**When read-only:**
- No new threads can be created
- No new replies can be posted
- Existing content remains accessible
- Moderators can still perform moderation actions
- UI hides posting buttons

---

## Content Editing

Users can edit their own content within a configurable time window.

### Edit Window

Each board has a configurable edit window (default: 24 hours):

```rust
set_edit_window(seconds, caller)  // Admin+ only
get_edit_window() -> u64
```

**Special values:**
- `0` = No time limit (editing always allowed)
- `86400` = 24 hours (default)

### Editing Threads

```rust
// Edit thread title
edit_thread_title(thread_id, new_title, caller)

// Edit thread body
edit_thread_body(board_id, thread_id, content, caller)
```

**Who can edit:**
- **Content creator** within the edit window
- **Moderator+** at any time (for moderation purposes)

### Editing Replies

```rust
edit_reply(board_id, thread_id, reply_id, content, caller)
edit_reply_content(board_id, thread_id, reply_id, content, caller)
```

**Constraints:**
- Locked threads cannot be edited (except by moderators)
- Read-only boards cannot have content edited (except by moderators)
- Updates the `updated_at` timestamp

### Soft Deletion

Content can be soft-deleted, which marks it as deleted but preserves the data:

```rust
delete_thread(thread_id, caller)
```

**Behavior:**
- Sets `is_deleted = true` on the thread
- Content remains in storage
- Useful for moderation review
- Only moderator+ can delete others' content

---

## Content Moderation

### Flagging Content

Any authenticated user can flag inappropriate content:

```rust
flag_content(board_id, thread_id, reply_id, reason, caller)
unflag_content(board_id, thread_id, reply_id, caller)  // Remove your own flag
```

**Parameters:**
- `reply_id = 0` to flag a thread
- `reply_id > 0` to flag a specific reply
- `reason`: Text describing why content is problematic

**Auto-Hide:**
When a piece of content reaches the flag threshold, it's automatically hidden. The threshold is configurable per board:

```rust
set_flag_threshold(board_id, threshold, caller)  // Admin+ only
get_flag_threshold(board_id) -> u32  // Default: 3
```

### Hiding Content

Moderators can manually hide or show content:

```rust
hide_thread(board_id, thread_id, caller)
unhide_thread(board_id, thread_id, caller)
hide_reply(board_id, thread_id, reply_id, caller)
unhide_reply(board_id, thread_id, reply_id, caller)
```

**Behavior:**
- Hidden content is not displayed to regular users
- Moderators can still see hidden content (marked as hidden)
- Flag count is preserved

### Banning Users

Moderators can ban users from boards:

```rust
ban_user(board_id, user, reason, duration_hours, caller)
unban_user(board_id, user, caller)
is_banned(board_id, user) -> bool
get_ban(board_id, user) -> Option<Ban>
```

**Ban types:**
- **Temporary**: Specify `duration_hours` (e.g., 24 for 24-hour ban)
- **Permanent**: Pass `None` for `duration_hours`

**Effects of ban:**
- Cannot view content (on private boards)
- Cannot create threads or replies
- Cannot request invites
- Cannot flag content

**Constraints:**
- Cannot ban users with equal or higher role
- Moderators can ban Members and Guests
- Admins can ban Moderators and below
- Owner can ban anyone except themselves

### Ban Structure

```rust
pub struct Ban {
    pub user: Address,
    pub board_id: u64,
    pub issuer: Address,      // Who issued the ban
    pub reason: String,
    pub created_at: u64,
    pub expires_at: Option<u64>,  // None = permanent
}
```

### Listing Banned Users

```rust
list_banned(board_id) -> Vec<Address>  // Active bans only
list_bans(board_id) -> Vec<Ban>        // With full details
```

---

## Thread Management

### Locking Threads

Locked threads prevent new replies:

```rust
lock_thread(thread_id, caller)    // Moderator+ only
unlock_thread(thread_id, caller)  // Moderator+ only
is_thread_locked(thread_id) -> bool
```

**When locked:**
- No new replies can be posted
- Existing content remains visible
- Moderators can still moderate content
- UI shows a "locked" badge

### Pinning Threads

Important threads can be pinned to the top:

```rust
pin_thread(thread_id, caller)     // Moderator+ only
unpin_thread(thread_id, caller)   // Moderator+ only
```

**Behavior:**
- Pinned threads appear at the top of thread lists
- Multiple threads can be pinned
- Order is by pin time (most recent first)

### Thread Metadata

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

---

## Reply Threading

Soroban Boards supports nested replies with configurable depth:

### Reply Structure

```rust
pub struct ReplyMeta {
    pub id: u64,
    pub board_id: u64,
    pub thread_id: u64,
    pub parent_id: u64,    // 0 = top-level reply
    pub depth: u32,        // Nesting level (0-based)
    pub creator: Address,
    pub created_at: u64,
    pub updated_at: u64,
    pub is_hidden: bool,
    pub is_deleted: bool,
    pub flag_count: u32,
}
```

### Creating Replies

```rust
create_reply(board_id, thread_id, parent_id, content, caller) -> u64
```

**Parameters:**
- `parent_id = 0`: Reply to the thread itself (top-level)
- `parent_id > 0`: Reply to a specific reply (nested)

### Reply Depth Limits

Each board has a configurable maximum reply depth:

```rust
get_max_reply_depth() -> u32      // Default: 10
set_max_reply_depth(depth, caller)  // Admin+ only, range: 1-20
```

Replies at maximum depth cannot have children. Admins can adjust this in the board settings page to control discussion depth.

### Waterfall Loading

Large threads use progressive loading to stay within contract execution limits:

```rust
get_chunk_size() -> u32      // Replies per chunk (default: 6)
set_chunk_size(size, caller)  // Admin+ only
```

The UI loads replies in chunks, with a "Load more" button to fetch additional batches.

---

## Storage Keys Reference

### Permissions Contract

| Key | Type | Description |
|-----|------|-------------|
| `Registry` | `Address` | Registry contract |
| `BoardOwner(board_id)` | `Address` | Board owner |
| `BoardRole(board_id, user)` | `Role` | User's role on board |
| `BoardBan(board_id, user)` | `Ban` | Ban record |
| `FlagThreshold(board_id)` | `u32` | Auto-hide threshold |
| `BoardAdmins(board_id)` | `Vec<Address>` | Admin list |
| `BoardModerators(board_id)` | `Vec<Address>` | Moderator list |
| `BoardMembers(board_id)` | `Vec<Address>` | Member list |
| `BannedUsers(board_id)` | `Vec<Address>` | Banned user list |
| `InviteRequest(board_id, user)` | `InviteRequest` | Pending request |
| `InviteRequests(board_id)` | `Vec<Address>` | Request index |

### Registry Contract

| Key | Type | Description |
|-----|------|-------------|
| `Admin` | `Address` | Registry admin |
| `BoardCount` | `u64` | Total boards |
| `Board(id)` | `BoardMeta` | Board metadata |
| `BoardContract(id)` | `Address` | Per-board contract |
| `BoardListed(id)` | `bool` | Listed on home page |
| `BoardPrivate(id)` | `bool` | Requires membership |
| `BoardReadonly(id)` | `bool` | No new content |
| `BoardNameToId(name)` | `u64` | Name/alias lookup |
| `BoardAliases(id)` | `Vec<String>` | Previous names |

### Board Contract

| Key | Type | Description |
|-----|------|-------------|
| `BoardId` | `u64` | Board ID |
| `Registry` | `Address` | Registry contract |
| `Permissions` | `Address` | Permissions contract |
| `Content` | `Address` | Content contract |
| `Theme` | `Address` | Theme contract |
| `ThreadCount` | `u64` | Thread count |
| `Thread(id)` | `ThreadMeta` | Thread metadata |
| `PinnedThreads` | `Vec<u64>` | Pinned thread IDs |
| `Config` | `BoardConfig` | Board settings |
| `EditWindow` | `u64` | Edit window in seconds |

---

## Related Documentation

- [README.md](./README.md) - Project overview and quick start
- [ROADMAP.md](./ROADMAP.md) - Development roadmap
- [API-REFERENCE.md](./API-REFERENCE.md) - Complete function reference
- [DEPLOYMENT.md](./DEPLOYMENT.md) - Deployment guide
