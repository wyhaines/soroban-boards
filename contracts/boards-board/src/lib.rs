#![no_std]

use soroban_render_sdk::prelude::*;
use soroban_sdk::{contract, contractimpl, contracttype, Address, Bytes, BytesN, Env, IntoVal, String, Symbol, Val, Vec};

// Declare render capabilities
soroban_render!(markdown, styles);

/// Storage keys for a board contract
#[contracttype]
#[derive(Clone)]
pub enum BoardKey {
    /// Board ID (assigned by registry)
    BoardId,
    /// Registry contract address
    Registry,
    /// Permissions contract address
    Permissions,
    /// Content contract address
    Content,
    /// Theme contract address
    Theme,
    /// Thread count
    ThreadCount,
    /// Thread metadata by ID
    Thread(u64),
    /// Pinned thread IDs
    PinnedThreads,
    /// Board configuration
    Config,
    /// Edit window in seconds (stored separately for backwards compatibility)
    EditWindow,
}

/// Thread metadata
#[contracttype]
#[derive(Clone)]
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

/// Board configuration
#[contracttype]
#[derive(Clone)]
pub struct BoardConfig {
    pub name: String,
    pub description: String,
    pub is_private: bool,
    pub is_readonly: bool,
    pub max_reply_depth: u32,
    /// Number of replies to load per chunk in waterfall loading (default: 6)
    pub reply_chunk_size: u32,
}

/// Reply metadata from content contract
#[contracttype]
#[derive(Clone)]
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

/// Role levels from permissions contract
#[contracttype]
#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum Role {
    Guest = 0,
    Member = 1,
    Moderator = 2,
    Admin = 3,
    Owner = 4,
}

#[contract]
pub struct BoardsBoard;

#[contractimpl]
impl BoardsBoard {
    /// Initialize a board contract (called by registry after deployment)
    /// permissions is optional - if None, permission checks are skipped (useful for testing)
    pub fn init(
        env: Env,
        board_id: u64,
        registry: Address,
        permissions: Option<Address>,
        content: Option<Address>,
        theme: Option<Address>,
        name: String,
        description: String,
        is_private: bool,
    ) {
        if env.storage().instance().has(&BoardKey::BoardId) {
            panic!("Already initialized");
        }

        env.storage().instance().set(&BoardKey::BoardId, &board_id);
        env.storage().instance().set(&BoardKey::Registry, &registry);

        // Only set permissions if provided
        if let Some(perms) = permissions {
            env.storage().instance().set(&BoardKey::Permissions, &perms);
        }

        // Only set content if provided
        if let Some(content_addr) = content {
            env.storage().instance().set(&BoardKey::Content, &content_addr);
        }

        // Only set theme if provided
        if let Some(theme_addr) = theme {
            env.storage().instance().set(&BoardKey::Theme, &theme_addr);
        }

        env.storage().instance().set(&BoardKey::ThreadCount, &0u64);
        env.storage()
            .instance()
            .set(&BoardKey::PinnedThreads, &Vec::<u64>::new(&env));
        env.storage().instance().set(
            &BoardKey::Config,
            &BoardConfig {
                name,
                description,
                is_private,
                is_readonly: false,
                max_reply_depth: 10,
                reply_chunk_size: 6,  // Default: 6 replies per chunk
            },
        );

        // Set default edit window (24 hours)
        env.storage().instance().set(&BoardKey::EditWindow, &86400u64);
    }

    /// Set content contract address (for boards created before this was added)
    pub fn set_content(env: Env, content: Address) {
        let registry: Address = env
            .storage()
            .instance()
            .get(&BoardKey::Registry)
            .expect("Not initialized");
        registry.require_auth();
        env.storage().instance().set(&BoardKey::Content, &content);
    }

    /// Set theme contract address (for boards created before this was added)
    pub fn set_theme(env: Env, theme: Address) {
        let registry: Address = env
            .storage()
            .instance()
            .get(&BoardKey::Registry)
            .expect("Not initialized");
        registry.require_auth();
        env.storage().instance().set(&BoardKey::Theme, &theme);
    }

    /// Set permissions contract address (for boards created before this was added)
    pub fn set_permissions(env: Env, permissions: Address) {
        let registry: Address = env
            .storage()
            .instance()
            .get(&BoardKey::Registry)
            .expect("Not initialized");
        registry.require_auth();
        env.storage().instance().set(&BoardKey::Permissions, &permissions);
    }

    // Permission check helpers

    /// Check if user can create threads on this board
    fn check_can_create_thread(env: &Env, board_id: u64, user: &Address) {
        let permissions: Address = env
            .storage()
            .instance()
            .get(&BoardKey::Permissions)
            .expect("Not initialized");

        let args: Vec<Val> = Vec::from_array(
            env,
            [board_id.into_val(env), user.into_val(env)],
        );
        let fn_name = Symbol::new(env, "can_create_thread");
        let can_create: bool = env.invoke_contract(&permissions, &fn_name, args);

        if !can_create {
            panic!("Not authorized to create threads");
        }
    }

    /// Check if user has moderator permissions
    fn check_can_moderate(env: &Env, board_id: u64, user: &Address) {
        let permissions: Address = env
            .storage()
            .instance()
            .get(&BoardKey::Permissions)
            .expect("Not initialized");

        let args: Vec<Val> = Vec::from_array(
            env,
            [board_id.into_val(env), user.into_val(env)],
        );
        let fn_name = Symbol::new(env, "can_moderate");
        let can_moderate: bool = env.invoke_contract(&permissions, &fn_name, args);

        if !can_moderate {
            panic!("Not authorized to moderate");
        }
    }

    /// Get board ID
    pub fn get_board_id(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&BoardKey::BoardId)
            .expect("Not initialized")
    }

    /// Get board configuration
    pub fn get_config(env: Env) -> BoardConfig {
        env.storage()
            .instance()
            .get(&BoardKey::Config)
            .expect("Not initialized")
    }

    /// Get reply chunk size for waterfall loading
    pub fn get_chunk_size(env: Env) -> u32 {
        let config: BoardConfig = env
            .storage()
            .instance()
            .get(&BoardKey::Config)
            .expect("Not initialized");
        // Handle old configs that don't have this field (default to 6)
        if config.reply_chunk_size == 0 {
            6
        } else {
            config.reply_chunk_size
        }
    }

    /// Set reply chunk size (must be >= 1, owner/admin only)
    pub fn set_chunk_size(env: Env, size: u32, caller: Address) {
        caller.require_auth();

        if size < 1 {
            panic!("Chunk size must be at least 1");
        }

        let board_id: u64 = env
            .storage()
            .instance()
            .get(&BoardKey::BoardId)
            .expect("Not initialized");

        // Check admin permissions (only if permissions contract is set)
        if env.storage().instance().has(&BoardKey::Permissions) {
            let permissions: Address = env
                .storage()
                .instance()
                .get(&BoardKey::Permissions)
                .unwrap();
            let args: Vec<Val> = Vec::from_array(
                &env,
                [board_id.into_val(&env), caller.into_val(&env)],
            );
            let fn_name = Symbol::new(&env, "can_admin");
            let can_admin: bool = env.invoke_contract(&permissions, &fn_name, args);
            if !can_admin {
                panic!("Only owner or admin can change chunk size");
            }
        }

        let mut config: BoardConfig = env
            .storage()
            .instance()
            .get(&BoardKey::Config)
            .expect("Not initialized");
        config.reply_chunk_size = size;
        env.storage().instance().set(&BoardKey::Config, &config);
    }

    /// Get edit window in seconds (0 = no limit)
    pub fn get_edit_window(env: Env) -> u64 {
        // Use separate storage key for backwards compatibility
        // Default to 86400 (24 hours) if not set
        env.storage()
            .instance()
            .get(&BoardKey::EditWindow)
            .unwrap_or(86400u64)
    }

    /// Set edit window in seconds (0 = no limit, owner/admin only)
    pub fn set_edit_window(env: Env, seconds: u64, caller: Address) {
        caller.require_auth();

        let board_id: u64 = env
            .storage()
            .instance()
            .get(&BoardKey::BoardId)
            .expect("Not initialized");

        // Check admin permissions (only if permissions contract is set)
        if env.storage().instance().has(&BoardKey::Permissions) {
            let permissions: Address = env
                .storage()
                .instance()
                .get(&BoardKey::Permissions)
                .unwrap();
            let args: Vec<Val> = Vec::from_array(
                &env,
                [board_id.into_val(&env), caller.into_val(&env)],
            );
            let fn_name = Symbol::new(&env, "can_admin");
            let can_admin: bool = env.invoke_contract(&permissions, &fn_name, args);
            if !can_admin {
                panic!("Only owner or admin can change edit window");
            }
        }

        // Store in separate key for backwards compatibility
        env.storage().instance().set(&BoardKey::EditWindow, &seconds);
    }

    /// Check if content is within the edit window
    /// Returns true if content can be edited (within window or no limit)
    fn is_within_edit_window(env: &Env, created_at: u64) -> bool {
        // Use separate storage key, default to 86400 (24 hours)
        let edit_window_seconds: u64 = env
            .storage()
            .instance()
            .get(&BoardKey::EditWindow)
            .unwrap_or(86400u64);

        // 0 = no time limit
        if edit_window_seconds == 0 {
            return true;
        }

        let current_time = env.ledger().timestamp();
        current_time <= created_at + edit_window_seconds
    }

    /// Create a new thread (returns thread ID)
    /// Note: Auth is handled by the calling contract (theme). When called directly
    /// (e.g., via CLI), callers should ensure proper authorization.
    pub fn create_thread(env: Env, title: String, creator: Address) -> u64 {
        // Note: require_auth() removed because this is called by the theme contract,
        // which already handles authentication. Cross-contract auth doesn't propagate
        // automatically in Soroban.

        let board_id: u64 = env
            .storage()
            .instance()
            .get(&BoardKey::BoardId)
            .expect("Not initialized");

        // Check permissions (only if permissions contract is set)
        if env.storage().instance().has(&BoardKey::Permissions) {
            Self::check_can_create_thread(&env, board_id, &creator);
        }

        // Check if board is readonly
        let config: BoardConfig = env
            .storage()
            .instance()
            .get(&BoardKey::Config)
            .expect("Not initialized");
        if config.is_readonly {
            panic!("Board is read-only");
        }

        let thread_id: u64 = env
            .storage()
            .instance()
            .get(&BoardKey::ThreadCount)
            .unwrap_or(0);

        let thread = ThreadMeta {
            id: thread_id,
            board_id,
            title,
            creator,
            created_at: env.ledger().timestamp(),
            updated_at: env.ledger().timestamp(),
            reply_count: 0,
            is_locked: false,
            is_pinned: false,
            is_hidden: false,
            is_deleted: false,
        };

        env.storage()
            .persistent()
            .set(&BoardKey::Thread(thread_id), &thread);
        env.storage()
            .instance()
            .set(&BoardKey::ThreadCount, &(thread_id + 1));

        thread_id
    }

    /// Get thread metadata
    pub fn get_thread(env: Env, thread_id: u64) -> Option<ThreadMeta> {
        env.storage()
            .persistent()
            .get(&BoardKey::Thread(thread_id))
    }

    /// List threads with pagination
    pub fn list_threads(env: Env, start: u64, limit: u64) -> Vec<ThreadMeta> {
        let count: u64 = env
            .storage()
            .instance()
            .get(&BoardKey::ThreadCount)
            .unwrap_or(0);

        let mut threads = Vec::new(&env);

        // Return newest first (reverse order)
        let actual_start = if count > start { count - start - 1 } else { 0 };

        for i in 0..limit {
            if actual_start < i {
                break;
            }
            let idx = actual_start - i;
            if let Some(thread) = env.storage().persistent().get(&BoardKey::Thread(idx)) {
                threads.push_back(thread);
            }
        }

        threads
    }

    /// Get thread count
    pub fn thread_count(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&BoardKey::ThreadCount)
            .unwrap_or(0)
    }

    /// Lock a thread (no more replies)
    pub fn lock_thread(env: Env, thread_id: u64, caller: Address) {
        caller.require_auth();

        let board_id: u64 = env
            .storage()
            .instance()
            .get(&BoardKey::BoardId)
            .expect("Not initialized");

        // Check moderator permissions (only if permissions contract is set)
        if env.storage().instance().has(&BoardKey::Permissions) {
            Self::check_can_moderate(&env, board_id, &caller);
        }

        if let Some(mut thread) = env
            .storage()
            .persistent()
            .get::<_, ThreadMeta>(&BoardKey::Thread(thread_id))
        {
            thread.is_locked = true;
            env.storage()
                .persistent()
                .set(&BoardKey::Thread(thread_id), &thread);
        }
    }

    /// Unlock a thread
    pub fn unlock_thread(env: Env, thread_id: u64, caller: Address) {
        caller.require_auth();

        let board_id: u64 = env
            .storage()
            .instance()
            .get(&BoardKey::BoardId)
            .expect("Not initialized");

        // Check moderator permissions (only if permissions contract is set)
        if env.storage().instance().has(&BoardKey::Permissions) {
            Self::check_can_moderate(&env, board_id, &caller);
        }

        if let Some(mut thread) = env
            .storage()
            .persistent()
            .get::<_, ThreadMeta>(&BoardKey::Thread(thread_id))
        {
            thread.is_locked = false;
            env.storage()
                .persistent()
                .set(&BoardKey::Thread(thread_id), &thread);
        }
    }

    /// Pin a thread
    pub fn pin_thread(env: Env, thread_id: u64, caller: Address) {
        caller.require_auth();

        let board_id: u64 = env
            .storage()
            .instance()
            .get(&BoardKey::BoardId)
            .expect("Not initialized");

        // Check moderator permissions (only if permissions contract is set)
        if env.storage().instance().has(&BoardKey::Permissions) {
            Self::check_can_moderate(&env, board_id, &caller);
        }

        if let Some(mut thread) = env
            .storage()
            .persistent()
            .get::<_, ThreadMeta>(&BoardKey::Thread(thread_id))
        {
            thread.is_pinned = true;
            env.storage()
                .persistent()
                .set(&BoardKey::Thread(thread_id), &thread);

            let mut pinned: Vec<u64> = env
                .storage()
                .instance()
                .get(&BoardKey::PinnedThreads)
                .unwrap_or(Vec::new(&env));
            pinned.push_back(thread_id);
            env.storage()
                .instance()
                .set(&BoardKey::PinnedThreads, &pinned);
        }
    }

    /// Unpin a thread
    pub fn unpin_thread(env: Env, thread_id: u64, caller: Address) {
        caller.require_auth();

        let board_id: u64 = env
            .storage()
            .instance()
            .get(&BoardKey::BoardId)
            .expect("Not initialized");

        // Check moderator permissions (only if permissions contract is set)
        if env.storage().instance().has(&BoardKey::Permissions) {
            Self::check_can_moderate(&env, board_id, &caller);
        }

        if let Some(mut thread) = env
            .storage()
            .persistent()
            .get::<_, ThreadMeta>(&BoardKey::Thread(thread_id))
        {
            thread.is_pinned = false;
            env.storage()
                .persistent()
                .set(&BoardKey::Thread(thread_id), &thread);

            // Remove from pinned list
            let pinned: Vec<u64> = env
                .storage()
                .instance()
                .get(&BoardKey::PinnedThreads)
                .unwrap_or(Vec::new(&env));
            let mut new_pinned = Vec::new(&env);
            for id in pinned.iter() {
                if id != thread_id {
                    new_pinned.push_back(id);
                }
            }
            env.storage()
                .instance()
                .set(&BoardKey::PinnedThreads, &new_pinned);
        }
    }

    /// Get pinned threads
    pub fn get_pinned_threads(env: Env) -> Vec<ThreadMeta> {
        let pinned_ids: Vec<u64> = env
            .storage()
            .instance()
            .get(&BoardKey::PinnedThreads)
            .unwrap_or(Vec::new(&env));

        let mut threads = Vec::new(&env);
        for id in pinned_ids.iter() {
            if let Some(thread) = env.storage().persistent().get(&BoardKey::Thread(id)) {
                threads.push_back(thread);
            }
        }
        threads
    }

    /// Hide a thread (moderator action)
    pub fn hide_thread(env: Env, thread_id: u64, caller: Address) {
        caller.require_auth();

        let board_id: u64 = env
            .storage()
            .instance()
            .get(&BoardKey::BoardId)
            .expect("Not initialized");

        // Check moderator permissions (only if permissions contract is set)
        if env.storage().instance().has(&BoardKey::Permissions) {
            Self::check_can_moderate(&env, board_id, &caller);
        }

        if let Some(mut thread) = env
            .storage()
            .persistent()
            .get::<_, ThreadMeta>(&BoardKey::Thread(thread_id))
        {
            thread.is_hidden = true;
            env.storage()
                .persistent()
                .set(&BoardKey::Thread(thread_id), &thread);
        }
    }

    /// Unhide a thread (moderator action)
    pub fn unhide_thread(env: Env, thread_id: u64, caller: Address) {
        caller.require_auth();

        let board_id: u64 = env
            .storage()
            .instance()
            .get(&BoardKey::BoardId)
            .expect("Not initialized");

        // Check moderator permissions (only if permissions contract is set)
        if env.storage().instance().has(&BoardKey::Permissions) {
            Self::check_can_moderate(&env, board_id, &caller);
        }

        if let Some(mut thread) = env
            .storage()
            .persistent()
            .get::<_, ThreadMeta>(&BoardKey::Thread(thread_id))
        {
            thread.is_hidden = false;
            env.storage()
                .persistent()
                .set(&BoardKey::Thread(thread_id), &thread);
        }
    }

    /// Set thread hidden state (called by admin contract)
    pub fn set_thread_hidden(env: Env, thread_id: u64, hidden: bool) {
        // Note: Auth is handled by the calling admin contract
        if let Some(mut thread) = env
            .storage()
            .persistent()
            .get::<_, ThreadMeta>(&BoardKey::Thread(thread_id))
        {
            thread.is_hidden = hidden;
            thread.updated_at = env.ledger().timestamp();
            env.storage()
                .persistent()
                .set(&BoardKey::Thread(thread_id), &thread);
        }
    }

    /// Set thread locked state (called by admin contract)
    pub fn set_thread_locked(env: Env, thread_id: u64, locked: bool) {
        // Note: Auth is handled by the calling admin contract
        if let Some(mut thread) = env
            .storage()
            .persistent()
            .get::<_, ThreadMeta>(&BoardKey::Thread(thread_id))
        {
            thread.is_locked = locked;
            thread.updated_at = env.ledger().timestamp();
            env.storage()
                .persistent()
                .set(&BoardKey::Thread(thread_id), &thread);
        }
    }

    /// Set thread pinned state (called by admin contract)
    pub fn set_thread_pinned(env: Env, thread_id: u64, pinned: bool) {
        // Note: Auth is handled by the calling admin contract
        if let Some(mut thread) = env
            .storage()
            .persistent()
            .get::<_, ThreadMeta>(&BoardKey::Thread(thread_id))
        {
            thread.is_pinned = pinned;
            thread.updated_at = env.ledger().timestamp();

            // Update pinned list
            let mut pinned_list: Vec<u64> = env
                .storage()
                .instance()
                .get(&BoardKey::PinnedThreads)
                .unwrap_or(Vec::new(&env));

            if pinned {
                // Add to list if not already present
                let mut found = false;
                for i in 0..pinned_list.len() {
                    if pinned_list.get(i).unwrap() == thread_id {
                        found = true;
                        break;
                    }
                }
                if !found {
                    pinned_list.push_back(thread_id);
                }
            } else {
                // Remove from list
                let mut new_list = Vec::new(&env);
                for i in 0..pinned_list.len() {
                    let id = pinned_list.get(i).unwrap();
                    if id != thread_id {
                        new_list.push_back(id);
                    }
                }
                pinned_list = new_list;
            }

            env.storage()
                .instance()
                .set(&BoardKey::PinnedThreads, &pinned_list);
            env.storage()
                .persistent()
                .set(&BoardKey::Thread(thread_id), &thread);
        }
    }

    /// Delete a thread (soft delete - sets is_deleted flag)
    /// Only author or moderator+ can delete
    pub fn delete_thread(env: Env, thread_id: u64, caller: Address) {
        caller.require_auth();

        let board_id: u64 = env
            .storage()
            .instance()
            .get(&BoardKey::BoardId)
            .expect("Not initialized");

        if let Some(mut thread) = env
            .storage()
            .persistent()
            .get::<_, ThreadMeta>(&BoardKey::Thread(thread_id))
        {
            // Verify caller is author or moderator
            let is_author = thread.creator == caller;
            let is_moderator = if env.storage().instance().has(&BoardKey::Permissions) {
                let permissions: Address = env
                    .storage()
                    .instance()
                    .get(&BoardKey::Permissions)
                    .unwrap();
                let args: Vec<Val> = Vec::from_array(
                    &env,
                    [board_id.into_val(&env), caller.into_val(&env)],
                );
                let fn_name = Symbol::new(&env, "can_moderate");
                env.invoke_contract(&permissions, &fn_name, args)
            } else {
                false
            };

            if !is_author && !is_moderator {
                panic!("Only author or moderator can delete thread");
            }

            thread.is_deleted = true;
            thread.updated_at = env.ledger().timestamp();
            env.storage()
                .persistent()
                .set(&BoardKey::Thread(thread_id), &thread);
        }
    }

    /// Edit thread title (author or moderator)
    pub fn edit_thread_title(env: Env, thread_id: u64, new_title: String, caller: Address) {
        caller.require_auth();

        if let Some(mut thread) = env
            .storage()
            .persistent()
            .get::<_, ThreadMeta>(&BoardKey::Thread(thread_id))
        {
            // Verify caller is author or moderator
            let is_author = thread.creator == caller;
            let is_moderator = if env.storage().instance().has(&BoardKey::Permissions) {
                let board_id: u64 = env
                    .storage()
                    .instance()
                    .get(&BoardKey::BoardId)
                    .expect("Not initialized");
                let permissions: Address = env
                    .storage()
                    .instance()
                    .get(&BoardKey::Permissions)
                    .unwrap();
                let args: Vec<Val> = Vec::from_array(
                    &env,
                    [board_id.into_val(&env), caller.into_val(&env)],
                );
                let fn_name = Symbol::new(&env, "can_moderate");
                env.invoke_contract(&permissions, &fn_name, args)
            } else {
                false
            };

            if !is_author && !is_moderator {
                panic!("Only author or moderator can edit title");
            }

            thread.title = new_title;
            thread.updated_at = env.ledger().timestamp();
            env.storage()
                .persistent()
                .set(&BoardKey::Thread(thread_id), &thread);
        }
    }

    /// Increment reply count for a thread (called by content contract)
    pub fn increment_reply_count(env: Env, thread_id: u64) {
        if let Some(mut thread) = env
            .storage()
            .persistent()
            .get::<_, ThreadMeta>(&BoardKey::Thread(thread_id))
        {
            thread.reply_count += 1;
            thread.updated_at = env.ledger().timestamp();
            env.storage()
                .persistent()
                .set(&BoardKey::Thread(thread_id), &thread);
        }
    }

    // ========================================================================
    // Rendering - Board, thread, and reply views
    // ========================================================================

    /// Main render entry point for board routes
    /// Routes are relative to the board (e.g., "/" = board view, "/t/0" = thread 0)
    pub fn render(env: Env, path: Option<String>, viewer: Option<Address>) -> Bytes {
        let board_id: u64 = env
            .storage()
            .instance()
            .get(&BoardKey::BoardId)
            .expect("Not initialized");

        Router::new(&env, path.clone())
            // Board view (thread list)
            .handle(b"/", |_| Self::render_board(&env, board_id, &viewer))
            // Create thread form
            .or_handle(b"/new", |_| Self::render_create_thread(&env, board_id, &viewer))
            // Thread reply form (must be before thread view)
            .or_handle(b"/t/{tid}/reply", |req| {
                let thread_id = req.get_var_u32(b"tid").unwrap_or(0) as u64;
                Self::render_reply_form(&env, board_id, thread_id, None, &viewer)
            })
            // Load top-level replies batch (waterfall loading)
            .or_handle(b"/t/{tid}/replies/{start}", |req| {
                let thread_id = req.get_var_u32(b"tid").unwrap_or(0) as u64;
                let start = req.get_var_u32(b"start").unwrap_or(0);
                Self::render_replies_batch(&env, board_id, thread_id, start, &viewer)
            })
            // Load children of a reply batch
            .or_handle(b"/t/{tid}/r/{rid}/children/{start}", |req| {
                let thread_id = req.get_var_u32(b"tid").unwrap_or(0) as u64;
                let reply_id = req.get_var_u32(b"rid").unwrap_or(0) as u64;
                let start = req.get_var_u32(b"start").unwrap_or(0);
                Self::render_children_batch(&env, board_id, thread_id, reply_id, start, &viewer)
            })
            // Nested reply form
            .or_handle(b"/t/{tid}/r/{rid}/reply", |req| {
                let thread_id = req.get_var_u32(b"tid").unwrap_or(0) as u64;
                let reply_id = req.get_var_u32(b"rid").unwrap_or(0) as u64;
                Self::render_reply_form(&env, board_id, thread_id, Some(reply_id), &viewer)
            })
            // Edit thread form
            .or_handle(b"/t/{tid}/edit", |req| {
                let thread_id = req.get_var_u32(b"tid").unwrap_or(0) as u64;
                Self::render_edit_thread(&env, board_id, thread_id, &viewer)
            })
            // Edit reply form
            .or_handle(b"/t/{tid}/r/{rid}/edit", |req| {
                let thread_id = req.get_var_u32(b"tid").unwrap_or(0) as u64;
                let reply_id = req.get_var_u32(b"rid").unwrap_or(0) as u64;
                Self::render_edit_reply(&env, board_id, thread_id, reply_id, &viewer)
            })
            // Thread view
            .or_handle(b"/t/{tid}", |req| {
                let thread_id = req.get_var_u32(b"tid").unwrap_or(0) as u64;
                Self::render_thread(&env, board_id, thread_id, &viewer)
            })
            // Default - board view
            .or_default(|_| Self::render_board(&env, board_id, &viewer))
    }

    /// Render navigation bar
    fn render_nav(env: &Env, _board_id: u64) -> MarkdownBuilder<'_> {
        MarkdownBuilder::new(env)
            .div_start("nav-bar")
            .render_link("Soroban Boards", "/")
            .render_link("Help", "/help")
            .div_end()
    }

    /// Append footer to builder
    fn render_footer_into(md: MarkdownBuilder<'_>) -> MarkdownBuilder<'_> {
        md.div_start("footer")
            .text("Powered by ")
            .link("Soroban Render", "https://github.com/wyhaines/soroban-render")
            .text(" on ")
            .link("Stellar", "https://stellar.org")
            .div_end()
    }

    /// Render board view with thread list
    fn render_board(env: &Env, board_id: u64, viewer: &Option<Address>) -> Bytes {
        let config: BoardConfig = env
            .storage()
            .instance()
            .get(&BoardKey::Config)
            .expect("Not initialized");

        // Check permissions for private boards
        if config.is_private {
            if let Some(perms_addr) = env.storage().instance().get::<_, Address>(&BoardKey::Permissions) {
                let viewer_role = if let Some(user) = viewer {
                    let args: Vec<Val> = Vec::from_array(env, [board_id.into_val(env), user.into_val(env)]);
                    let role: Role = env.invoke_contract(
                        &perms_addr,
                        &Symbol::new(env, "get_role"),
                        args,
                    );
                    role
                } else {
                    Role::Guest
                };

                // If not a member, show access denied
                if (viewer_role as u32) < (Role::Member as u32) {
                    return Self::render_private_board_message(env, board_id, &config, viewer, &perms_addr);
                }
            }
        }

        let mut md = Self::render_nav(env, board_id)
            .render_link("< Back", "/")
            .div_start("page-header")
            .raw_str("<h1>")
            .text_string(&config.name)
            .raw_str("</h1>")
            .raw_str("<p>")
            .text_string(&config.description)
            .raw_str("</p>")
            .div_end()
            .newline();

        if config.is_private {
            md = md.raw_str("<span class=\"badge badge-private\">private</span> ");
        }

        if config.is_readonly {
            md = md.note("This board is read-only.");
        }

        // Show create thread button if logged in
        if viewer.is_some() && !config.is_readonly {
            md = md.raw_str("<a href=\"render:/b/")
                .number(board_id as u32)
                .raw_str("/new\" class=\"action-btn\">+ New Thread</a>")
                .newline();
        }

        // Show settings button for Admin+ users
        if let Some(user) = viewer {
            if let Some(perms_addr) = env.storage().instance().get::<_, Address>(&BoardKey::Permissions) {
                let role_args: Vec<Val> = Vec::from_array(env, [board_id.into_val(env), user.into_val(env)]);
                let viewer_role: Role = env.invoke_contract(
                    &perms_addr,
                    &Symbol::new(env, "get_role"),
                    role_args,
                );

                if (viewer_role as u32) >= (Role::Admin as u32) {
                    md = md.raw_str("<a href=\"render:/admin/b/")
                        .number(board_id as u32)
                        .raw_str("/settings\" class=\"action-btn action-btn-secondary\">⚙ Settings</a>")
                        .newline();
                }
            }
        }

        md = md.raw_str("<h2>Threads</h2>\n")
            .div_start("thread-list");

        // Fetch threads
        let thread_count: u64 = env
            .storage()
            .instance()
            .get(&BoardKey::ThreadCount)
            .unwrap_or(0);

        if thread_count == 0 {
            md = md.div_end()
                .paragraph("No threads yet. Be the first to post!");
        } else {
            // List threads (newest first, up to 20)
            let limit = if thread_count > 20 { 20 } else { thread_count };
            let start_idx = thread_count - 1;

            for i in 0..limit {
                let idx = start_idx - i;
                if let Some(thread) = env.storage().persistent().get::<_, ThreadMeta>(&BoardKey::Thread(idx)) {
                    md = md.raw_str("<a href=\"render:/b/")
                        .number(board_id as u32)
                        .raw_str("/t/")
                        .number(thread.id as u32)
                        .raw_str("\" class=\"thread-card\"><span class=\"thread-card-title\">")
                        .text_string(&thread.title)
                        .raw_str("</span><span class=\"thread-card-meta\">");
                    if thread.is_pinned {
                        md = md.raw_str("<span class=\"badge badge-pinned\">pinned</span> ");
                    }
                    if thread.is_locked {
                        md = md.raw_str("<span class=\"badge badge-locked\">locked</span> ");
                    }
                    md = md.number(thread.reply_count)
                        .text(" replies")
                        .raw_str("</span></a>\n");
                }
            }
            md = md.div_end();
        }

        Self::render_footer_into(md).build()
    }

    /// Render private board access denied message
    fn render_private_board_message(
        env: &Env,
        board_id: u64,
        config: &BoardConfig,
        viewer: &Option<Address>,
        perms_addr: &Address,
    ) -> Bytes {
        let mut md = Self::render_nav(env, board_id)
            .render_link("< Back", "/")
            .div_start("page-header")
            .raw_str("<h1>")
            .text_string(&config.name)
            .raw_str("</h1>")
            .raw_str("<p>")
            .text_string(&config.description)
            .raw_str("</p>")
            .div_end()
            .newline()
            .raw_str("<span class=\"badge badge-private\">private</span>")
            .newline()
            .newline();

        if viewer.is_none() {
            md = md.warning("This is a private board. Please connect your wallet to request access.");
        } else {
            // Check if user has a pending invite request
            let has_request_args: Vec<Val> = Vec::from_array(env, [
                board_id.into_val(env),
                viewer.as_ref().unwrap().into_val(env),
            ]);
            let has_request: bool = env.invoke_contract(
                perms_addr,
                &Symbol::new(env, "has_invite_request"),
                has_request_args,
            );

            if has_request {
                md = md.tip("Your request to join this board is pending approval.");
            } else {
                md = md.note("This is a private board. You can request access from the board administrators.")
                    .newline()
                    .raw_str("<input type=\"hidden\" name=\"board_id\" value=\"")
                    .number(board_id as u32)
                    .raw_str("\" />\n")
                    .raw_str("<input type=\"hidden\" name=\"caller\" value=\"")
                    .text_string(&viewer.as_ref().unwrap().to_string())
                    .raw_str("\" />\n")
                    .form_link_to("Request to Join", "perms", "request_invite");
            }
        }

        Self::render_footer_into(md).build()
    }

    /// Render create thread form
    fn render_create_thread(env: &Env, board_id: u64, viewer: &Option<Address>) -> Bytes {
        let mut md = Self::render_nav(env, board_id)
            .newline()  // Blank line after nav-bar div for markdown parsing
            .raw_str("[< Back to Board](render:/b/")
            .number(board_id as u32)
            .raw_str(")")
            .newline()
            .newline()  // Blank line before h1 for markdown parsing
            .h1("New Thread");

        if viewer.is_none() {
            md = md.warning("Please connect your wallet to create a thread.");
            return Self::render_footer_into(md).build();
        }

        md = md
            .raw_str("<input type=\"hidden\" name=\"_redirect\" value=\"/b/")
            .number(board_id as u32)
            .raw_str("\" />\n")
            .raw_str("<input type=\"hidden\" name=\"board_id\" value=\"")
            .number(board_id as u32)
            .raw_str("\" />\n")
            .input("title", "Thread title")
            .newline()
            .textarea("body", 10, "Write your post content here...")
            .newline()
            .raw_str("<input type=\"hidden\" name=\"caller\" value=\"")
            .text_string(&viewer.as_ref().unwrap().to_string())
            .raw_str("\" />\n")
            .newline()
            .form_link_to("Create Thread", "content", "create_thread")
            .newline()
            .newline()
            .raw_str("[Cancel](render:/b/")
            .number(board_id as u32)
            .raw_str(")");

        Self::render_footer_into(md).build()
    }

    /// Render thread view
    fn render_thread(env: &Env, board_id: u64, thread_id: u64, viewer: &Option<Address>) -> Bytes {
        let content: Address = env
            .storage()
            .instance()
            .get(&BoardKey::Content)
            .expect("Content contract not configured");

        // Get board config for readonly check
        let config: BoardConfig = env
            .storage()
            .instance()
            .get(&BoardKey::Config)
            .expect("Not initialized");

        // Get thread metadata
        let thread = env
            .storage()
            .persistent()
            .get::<_, ThreadMeta>(&BoardKey::Thread(thread_id));

        // Determine if posting is allowed
        let is_readonly = config.is_readonly;
        let is_locked = thread.as_ref().map(|t| t.is_locked).unwrap_or(false);
        let can_post = !is_readonly && !is_locked;

        let mut md = Self::render_nav(env, board_id)
            .newline()
            .raw_str("[< Back to Board](render:/b/")
            .number(board_id as u32)
            .raw_str(")")
            .newline();

        // Show thread title if available
        if let Some(ref t) = thread {
            md = md.raw_str("<h1>")
                .text_string(&t.title)
                .raw_str("</h1>\n");
        } else {
            md = md.raw_str("<h1>Thread</h1>\n");
        }

        // Show status badges
        if is_readonly {
            md = md.raw_str("<span class=\"badge badge-readonly\">read-only board</span> ");
        }
        if is_locked {
            md = md.raw_str("<span class=\"badge badge-locked\">locked</span> ");
        }
        if is_readonly || is_locked {
            md = md.newline();
        }

        // Thread body in a container
        md = md.div_start("thread-body");

        // Get thread body from content contract
        let args: Vec<Val> = Vec::from_array(env, [board_id.into_val(env), thread_id.into_val(env)]);
        let body: Bytes = env.invoke_contract(
            &content,
            &Symbol::new(env, "get_thread_body"),
            args.clone(),
        );

        if body.len() > 0 {
            md = md.raw(body);
        } else {
            md = md.italic("No content");
        }

        md = md.div_end()
            .newline();

        // Thread actions (only show if viewer is logged in and posting is allowed)
        if viewer.is_some() && can_post {
            md = md.div_start("thread-actions")
                .raw_str("[Reply to Thread](render:/b/")
                .number(board_id as u32)
                .raw_str("/t/")
                .number(thread_id as u32)
                .raw_str("/reply)");

            // Show edit button if user can edit
            if let Some(ref t) = thread {
                let (is_author, is_moderator) = Self::can_edit(env, board_id, &t.creator, viewer);
                let can_edit_time = is_moderator || Self::is_within_edit_window(env, t.created_at);

                if (is_author || is_moderator) && can_edit_time {
                    md = md.text(" ")
                        .raw_str("[Edit](render:/b/")
                        .number(board_id as u32)
                        .raw_str("/t/")
                        .number(thread_id as u32)
                        .raw_str("/edit)");
                }
            }

            md = md.div_end()
                .newline();
        }

        md = md.raw_str("<h2>Replies</h2>\n");

        // Fetch reply count
        let reply_count: u64 = env.invoke_contract(
            &content,
            &Symbol::new(env, "get_reply_count"),
            args,
        );

        if reply_count == 0 {
            md = md.paragraph("No replies yet. Be the first to respond!");
        } else {
            // Use waterfall loading
            md = md.raw_str("{{render path=\"/b/")
                .number(board_id as u32)
                .raw_str("/t/")
                .number(thread_id as u32)
                .raw_str("/replies/0\"}}");
        }

        Self::render_footer_into(md).build()
    }

    /// Render a batch of top-level replies
    fn render_replies_batch(env: &Env, board_id: u64, thread_id: u64, start: u32, viewer: &Option<Address>) -> Bytes {
        let content: Address = env
            .storage()
            .instance()
            .get(&BoardKey::Content)
            .expect("Content contract not configured");

        let config: BoardConfig = env
            .storage()
            .instance()
            .get(&BoardKey::Config)
            .expect("Not initialized");
        let chunk_size = if config.reply_chunk_size == 0 { 6 } else { config.reply_chunk_size };

        // Determine if posting is allowed
        let thread = env
            .storage()
            .persistent()
            .get::<_, ThreadMeta>(&BoardKey::Thread(thread_id));
        let is_locked = thread.as_ref().map(|t| t.is_locked).unwrap_or(false);
        let can_post = !config.is_readonly && !is_locked;

        // Get total reply count
        let count_args: Vec<Val> = Vec::from_array(env, [board_id.into_val(env), thread_id.into_val(env)]);
        let total_count: u64 = env.invoke_contract(
            &content,
            &Symbol::new(env, "get_reply_count"),
            count_args,
        );

        // Fetch this batch of replies
        let list_args: Vec<Val> = Vec::from_array(env, [
            board_id.into_val(env),
            thread_id.into_val(env),
            start.into_val(env),
            chunk_size.into_val(env),
        ]);
        let replies: Vec<ReplyMeta> = env.invoke_contract(
            &content,
            &Symbol::new(env, "list_top_level_replies"),
            list_args,
        );

        let mut md = MarkdownBuilder::new(env);

        for i in 0..replies.len() {
            if let Some(reply) = replies.get(i) {
                md = Self::render_reply_item_waterfall(env, md, &content, &reply, board_id, thread_id, viewer, can_post);
            }
        }

        // If more replies exist, add continuation
        let next_start = start + chunk_size;
        if (next_start as u64) < total_count {
            md = md.raw_str("{{render path=\"/b/")
                .number(board_id as u32)
                .raw_str("/t/")
                .number(thread_id as u32)
                .raw_str("/replies/")
                .number(next_start)
                .raw_str("\"}}");
        }

        md.build()
    }

    /// Render a batch of children for a reply
    fn render_children_batch(env: &Env, board_id: u64, thread_id: u64, parent_id: u64, start: u32, viewer: &Option<Address>) -> Bytes {
        let content: Address = env
            .storage()
            .instance()
            .get(&BoardKey::Content)
            .expect("Content contract not configured");

        let config: BoardConfig = env
            .storage()
            .instance()
            .get(&BoardKey::Config)
            .expect("Not initialized");
        let chunk_size = if config.reply_chunk_size == 0 { 6 } else { config.reply_chunk_size };

        // Determine if posting is allowed
        let thread = env
            .storage()
            .persistent()
            .get::<_, ThreadMeta>(&BoardKey::Thread(thread_id));
        let is_locked = thread.as_ref().map(|t| t.is_locked).unwrap_or(false);
        let can_post = !config.is_readonly && !is_locked;

        // Get total children count
        let count_args: Vec<Val> = Vec::from_array(env, [
            board_id.into_val(env),
            thread_id.into_val(env),
            parent_id.into_val(env),
        ]);
        let total_count: u32 = env.invoke_contract(
            &content,
            &Symbol::new(env, "get_children_count"),
            count_args,
        );

        // Fetch this batch of children
        let list_args: Vec<Val> = Vec::from_array(env, [
            board_id.into_val(env),
            thread_id.into_val(env),
            parent_id.into_val(env),
            start.into_val(env),
            chunk_size.into_val(env),
        ]);
        let children: Vec<ReplyMeta> = env.invoke_contract(
            &content,
            &Symbol::new(env, "list_children_replies"),
            list_args,
        );

        let mut md = MarkdownBuilder::new(env);

        for i in 0..children.len() {
            if let Some(child) = children.get(i) {
                md = Self::render_reply_item_waterfall(env, md, &content, &child, board_id, thread_id, viewer, can_post);
            }
        }

        // If more children exist, add continuation
        let next_start = start + chunk_size;
        if next_start < total_count {
            md = md.raw_str("{{render path=\"/b/")
                .number(board_id as u32)
                .raw_str("/t/")
                .number(thread_id as u32)
                .raw_str("/r/")
                .number(parent_id as u32)
                .raw_str("/children/")
                .number(next_start)
                .raw_str("\"}}");
        }

        md.build()
    }

    /// Render a single reply with waterfall loading for children
    fn render_reply_item_waterfall<'a>(
        env: &Env,
        mut md: MarkdownBuilder<'a>,
        content: &Address,
        reply: &ReplyMeta,
        board_id: u64,
        thread_id: u64,
        viewer: &Option<Address>,
        can_post: bool,
    ) -> MarkdownBuilder<'a> {
        md = md.div_start("reply");

        // Reply content
        if reply.is_hidden {
            md = md.div_start("reply-content reply-hidden")
                .text("[This reply has been hidden by a moderator]")
                .div_end();
        } else if reply.is_deleted {
            md = md.div_start("reply-content reply-deleted")
                .text("[This reply has been deleted]")
                .div_end();
        } else {
            let args: Vec<Val> = Vec::from_array(env, [
                board_id.into_val(env),
                thread_id.into_val(env),
                reply.id.into_val(env),
            ]);
            let content_bytes: Bytes = env.invoke_contract(
                content,
                &Symbol::new(env, "get_reply_content"),
                args,
            );

            md = md.div_start("reply-content")
                .raw(content_bytes)
                .div_end();
        }

        // Reply metadata and actions
        md = md.div_start("reply-meta")
            .span_start("reply-id")
            .text("Reply #")
            .number(reply.id as u32)
            .span_end();

        // Only show Reply button if posting is allowed
        if viewer.is_some() && can_post {
            md = md.text(" ")
                .raw_str("[Reply](render:/b/")
                .number(board_id as u32)
                .raw_str("/t/")
                .number(thread_id as u32)
                .raw_str("/r/")
                .number(reply.id as u32)
                .raw_str("/reply)");

            // Show edit button if user can edit (and reply is not deleted)
            if !reply.is_deleted {
                let (is_author, is_moderator) = Self::can_edit(env, board_id, &reply.creator, viewer);
                let can_edit_time = is_moderator || Self::is_within_edit_window(env, reply.created_at);

                if (is_author || is_moderator) && can_edit_time {
                    md = md.text(" ")
                        .raw_str("[Edit](render:/b/")
                        .number(board_id as u32)
                        .raw_str("/t/")
                        .number(thread_id as u32)
                        .raw_str("/r/")
                        .number(reply.id as u32)
                        .raw_str("/edit)");
                }
            }
        }

        // Flag button is always available to logged in users
        if viewer.is_some() {
            md = md.text(" ")
                .raw_str("[Flag](tx:@content:flag_reply {\"board_id\":")
                .number(board_id as u32)
                .raw_str(",\"thread_id\":")
                .number(thread_id as u32)
                .raw_str(",\"reply_id\":")
                .number(reply.id as u32)
                .raw_str(",\"reason\":\"\"})");
        }

        md = md.div_end();

        // Get children count
        let count_args: Vec<Val> = Vec::from_array(env, [
            board_id.into_val(env),
            thread_id.into_val(env),
            reply.id.into_val(env),
        ]);
        let children_count: u32 = env.invoke_contract(
            content,
            &Symbol::new(env, "get_children_count"),
            count_args,
        );

        // If has children, embed continuation for waterfall loading
        if children_count > 0 {
            md = md.raw_str("{{render path=\"/b/")
                .number(board_id as u32)
                .raw_str("/t/")
                .number(thread_id as u32)
                .raw_str("/r/")
                .number(reply.id as u32)
                .raw_str("/children/0\"}}");
        }

        md = md.div_end();
        md
    }

    /// Render reply form
    fn render_reply_form(env: &Env, board_id: u64, thread_id: u64, parent_reply_id: Option<u64>, viewer: &Option<Address>) -> Bytes {
        let mut md = Self::render_nav(env, board_id)
            .newline()
            .raw_str("[< Back to Thread](render:/b/")
            .number(board_id as u32)
            .raw_str("/t/")
            .number(thread_id as u32)
            .raw_str(")")
            .newline()
            .newline();

        if parent_reply_id.is_some() {
            md = md.raw_str("<h1>Reply to Comment</h1>\n");
        } else {
            md = md.raw_str("<h1>Reply to Thread</h1>\n");
        }

        if viewer.is_none() {
            md = md.warning("Please connect your wallet to reply.");
            return Self::render_footer_into(md).build();
        }

        // Calculate parent_id and depth
        let (parent_id, depth): (u64, u32) = if let Some(pid) = parent_reply_id {
            if let Some(content_addr) = env.storage().instance().get::<_, Address>(&BoardKey::Content) {
                let args: Vec<Val> = Vec::from_array(env, [
                    board_id.into_val(env),
                    thread_id.into_val(env),
                    pid.into_val(env),
                ]);
                let parent_reply: Option<ReplyMeta> = env.invoke_contract(
                    &content_addr,
                    &Symbol::new(env, "get_reply"),
                    args,
                );
                match parent_reply {
                    Some(reply) => (pid, reply.depth + 1),
                    None => (pid, 1),
                }
            } else {
                (pid, 1)
            }
        } else {
            (0, 0)
        };

        md = md
            .raw_str("<input type=\"hidden\" name=\"_redirect\" value=\"/b/")
            .number(board_id as u32)
            .raw_str("/t/")
            .number(thread_id as u32)
            .raw_str("\" />\n")
            .raw_str("<input type=\"hidden\" name=\"board_id\" value=\"")
            .number(board_id as u32)
            .raw_str("\" />\n")
            .raw_str("<input type=\"hidden\" name=\"thread_id\" value=\"")
            .number(thread_id as u32)
            .raw_str("\" />\n")
            .raw_str("<input type=\"hidden\" name=\"parent_id\" value=\"")
            .number(parent_id as u32)
            .raw_str("\" />\n")
            .raw_str("<input type=\"hidden\" name=\"depth\" value=\"")
            .number(depth)
            .raw_str("\" />\n")
            .textarea("content_str", 6, "Write your reply...")
            .newline()
            .raw_str("<input type=\"hidden\" name=\"caller\" value=\"")
            .text_string(&viewer.as_ref().unwrap().to_string())
            .raw_str("\" />\n")
            .newline()
            .form_link_to("Post Reply", "content", "create_reply")
            .newline()
            .newline()
            .raw_str("[Cancel](render:/b/")
            .number(board_id as u32)
            .raw_str("/t/")
            .number(thread_id as u32)
            .raw_str(")");

        Self::render_footer_into(md).build()
    }

    /// Check if user can edit content (author or moderator)
    fn can_edit(env: &Env, board_id: u64, creator: &Address, viewer: &Option<Address>) -> (bool, bool) {
        let viewer = match viewer {
            Some(v) => v,
            None => return (false, false),
        };

        let is_author = creator == viewer;

        // Check if viewer is moderator
        let is_moderator = if env.storage().instance().has(&BoardKey::Permissions) {
            let permissions: Address = env
                .storage()
                .instance()
                .get(&BoardKey::Permissions)
                .unwrap();
            let args: Vec<Val> = Vec::from_array(env, [board_id.into_val(env), viewer.into_val(env)]);
            let can_moderate: bool = env.invoke_contract(
                &permissions,
                &Symbol::new(env, "can_moderate"),
                args,
            );
            can_moderate
        } else {
            false
        };

        (is_author, is_moderator)
    }

    /// Render edit thread form
    fn render_edit_thread(env: &Env, board_id: u64, thread_id: u64, viewer: &Option<Address>) -> Bytes {
        let content: Address = env
            .storage()
            .instance()
            .get(&BoardKey::Content)
            .expect("Content contract not configured");

        let mut md = Self::render_nav(env, board_id)
            .newline()
            .raw_str("[< Back to Thread](render:/b/")
            .number(board_id as u32)
            .raw_str("/t/")
            .number(thread_id as u32)
            .raw_str(")")
            .newline()
            .newline()
            .h1("Edit Thread");

        if viewer.is_none() {
            md = md.warning("Please connect your wallet to edit.");
            return Self::render_footer_into(md).build();
        }

        // Get thread metadata
        let thread = match env.storage().persistent().get::<_, ThreadMeta>(&BoardKey::Thread(thread_id)) {
            Some(t) => t,
            None => {
                md = md.warning("Thread not found.");
                return Self::render_footer_into(md).build();
            }
        };

        // Check if locked
        if thread.is_locked {
            md = md.warning("This thread is locked and cannot be edited.");
            return Self::render_footer_into(md).build();
        }

        // Check edit permission
        let (is_author, is_moderator) = Self::can_edit(env, board_id, &thread.creator, viewer);

        if !is_author && !is_moderator {
            md = md.warning("You don't have permission to edit this thread.");
            return Self::render_footer_into(md).build();
        }

        // Check edit window (only applies to non-moderators)
        if is_author && !is_moderator && !Self::is_within_edit_window(env, thread.created_at) {
            md = md.warning("The edit window has expired for this thread.");
            return Self::render_footer_into(md).build();
        }

        // Get current thread body
        let args: Vec<Val> = Vec::from_array(env, [board_id.into_val(env), thread_id.into_val(env)]);
        let body: Bytes = env.invoke_contract(
            &content,
            &Symbol::new(env, "get_thread_body"),
            args,
        );

        // Convert Bytes to escaped string for textarea
        // Note: We need to include the body content in a way the form can use
        // For now, we'll use a hidden field with base64 or just render the textarea

        md = md
            .raw_str("<input type=\"hidden\" name=\"_redirect\" value=\"/b/")
            .number(board_id as u32)
            .raw_str("/t/")
            .number(thread_id as u32)
            .raw_str("\" />\n")
            .raw_str("<input type=\"hidden\" name=\"board_id\" value=\"")
            .number(board_id as u32)
            .raw_str("\" />\n")
            .raw_str("<input type=\"hidden\" name=\"thread_id\" value=\"")
            .number(thread_id as u32)
            .raw_str("\" />\n")
            .raw_str("<label>Title</label>\n")
            .raw_str("<input type=\"text\" name=\"new_title\" value=\"")
            .text_string(&thread.title)
            .raw_str("\" />\n")
            .newline()
            .raw_str("<label>Content</label>\n")
            .raw_str("<textarea name=\"new_body\" rows=\"10\">");

        // Include the current body content in the textarea
        if body.len() > 0 {
            md = md.raw(body);
        }

        md = md.raw_str("</textarea>\n")
            .newline()
            .raw_str("<input type=\"hidden\" name=\"caller\" value=\"")
            .text_string(&viewer.as_ref().unwrap().to_string())
            .raw_str("\" />\n")
            .newline()
            .form_link_to("Save Changes", "content", "edit_thread")
            .newline()
            .newline()
            .raw_str("[Cancel](render:/b/")
            .number(board_id as u32)
            .raw_str("/t/")
            .number(thread_id as u32)
            .raw_str(")");

        Self::render_footer_into(md).build()
    }

    /// Render edit reply form
    fn render_edit_reply(env: &Env, board_id: u64, thread_id: u64, reply_id: u64, viewer: &Option<Address>) -> Bytes {
        let content: Address = env
            .storage()
            .instance()
            .get(&BoardKey::Content)
            .expect("Content contract not configured");

        let mut md = Self::render_nav(env, board_id)
            .newline()
            .raw_str("[< Back to Thread](render:/b/")
            .number(board_id as u32)
            .raw_str("/t/")
            .number(thread_id as u32)
            .raw_str(")")
            .newline()
            .newline()
            .h1("Edit Reply");

        if viewer.is_none() {
            md = md.warning("Please connect your wallet to edit.");
            return Self::render_footer_into(md).build();
        }

        // Get reply metadata from content contract
        let args: Vec<Val> = Vec::from_array(env, [
            board_id.into_val(env),
            thread_id.into_val(env),
            reply_id.into_val(env),
        ]);
        let reply: Option<ReplyMeta> = env.invoke_contract(
            &content,
            &Symbol::new(env, "get_reply"),
            args.clone(),
        );

        let reply = match reply {
            Some(r) => r,
            None => {
                md = md.warning("Reply not found.");
                return Self::render_footer_into(md).build();
            }
        };

        // Check if deleted
        if reply.is_deleted {
            md = md.warning("This reply has been deleted and cannot be edited.");
            return Self::render_footer_into(md).build();
        }

        // Check edit permission
        let (is_author, is_moderator) = Self::can_edit(env, board_id, &reply.creator, viewer);

        if !is_author && !is_moderator {
            md = md.warning("You don't have permission to edit this reply.");
            return Self::render_footer_into(md).build();
        }

        // Check edit window (only applies to non-moderators)
        if is_author && !is_moderator && !Self::is_within_edit_window(env, reply.created_at) {
            md = md.warning("The edit window has expired for this reply.");
            return Self::render_footer_into(md).build();
        }

        // Get current reply content
        let reply_content: Bytes = env.invoke_contract(
            &content,
            &Symbol::new(env, "get_reply_content"),
            args,
        );

        md = md
            .raw_str("<input type=\"hidden\" name=\"_redirect\" value=\"/b/")
            .number(board_id as u32)
            .raw_str("/t/")
            .number(thread_id as u32)
            .raw_str("\" />\n")
            .raw_str("<input type=\"hidden\" name=\"board_id\" value=\"")
            .number(board_id as u32)
            .raw_str("\" />\n")
            .raw_str("<input type=\"hidden\" name=\"thread_id\" value=\"")
            .number(thread_id as u32)
            .raw_str("\" />\n")
            .raw_str("<input type=\"hidden\" name=\"reply_id\" value=\"")
            .number(reply_id as u32)
            .raw_str("\" />\n")
            .raw_str("<label>Content</label>\n")
            .raw_str("<textarea name=\"content\" rows=\"6\">");

        // Include the current content in the textarea
        if reply_content.len() > 0 {
            md = md.raw(reply_content);
        }

        md = md.raw_str("</textarea>\n")
            .newline()
            .raw_str("<input type=\"hidden\" name=\"caller\" value=\"")
            .text_string(&viewer.as_ref().unwrap().to_string())
            .raw_str("\" />\n")
            .newline()
            .form_link_to("Save Changes", "content", "edit_reply_content")
            .newline()
            .newline()
            .raw_str("[Cancel](render:/b/")
            .number(board_id as u32)
            .raw_str("/t/")
            .number(thread_id as u32)
            .raw_str(")");

        Self::render_footer_into(md).build()
    }

    /// Get CSS from Theme contract
    pub fn styles(env: Env) -> Bytes {
        let theme: Address = env
            .storage()
            .instance()
            .get(&BoardKey::Theme)
            .expect("Theme contract not configured");

        env.invoke_contract(&theme, &Symbol::new(&env, "styles"), Vec::new(&env))
    }

    /// Upgrade the contract WASM
    pub fn upgrade(env: Env, new_wasm_hash: BytesN<32>) {
        let registry: Address = env
            .storage()
            .instance()
            .get(&BoardKey::Registry)
            .expect("Not initialized");
        registry.require_auth();

        env.deployer().update_current_contract_wasm(new_wasm_hash);
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::Env;

    #[test]
    fn test_init_and_create_thread() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsBoard, ());
        let client = BoardsBoardClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        let name = String::from_str(&env, "General");
        let desc = String::from_str(&env, "General discussion");

        // Pass None for permissions, content, theme to skip checks in tests
        client.init(&0, &registry, &None, &None, &None, &name, &desc, &false);

        assert_eq!(client.get_board_id(), 0);
        assert_eq!(client.thread_count(), 0);

        let creator = Address::generate(&env);
        let title = String::from_str(&env, "Hello World");

        let thread_id = client.create_thread(&title, &creator);
        assert_eq!(thread_id, 0);
        assert_eq!(client.thread_count(), 1);

        let thread = client.get_thread(&thread_id).unwrap();
        assert_eq!(thread.title, title);
        assert_eq!(thread.creator, creator);
    }

    #[test]
    fn test_pin_and_lock() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsBoard, ());
        let client = BoardsBoardClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        let name = String::from_str(&env, "General");
        let desc = String::from_str(&env, "General discussion");

        // Pass None for permissions, content, theme to skip checks in tests
        client.init(&0, &registry, &None, &None, &None, &name, &desc, &false);

        let creator = Address::generate(&env);
        let moderator = Address::generate(&env);
        let title = String::from_str(&env, "Important Announcement");

        let thread_id = client.create_thread(&title, &creator);

        // Pin thread
        client.pin_thread(&thread_id, &moderator);
        let thread = client.get_thread(&thread_id).unwrap();
        assert!(thread.is_pinned);

        let pinned = client.get_pinned_threads();
        assert_eq!(pinned.len(), 1);

        // Lock thread
        client.lock_thread(&thread_id, &moderator);
        let thread = client.get_thread(&thread_id).unwrap();
        assert!(thread.is_locked);
    }

    #[test]
    fn test_hide_thread() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsBoard, ());
        let client = BoardsBoardClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        let name = String::from_str(&env, "General");
        let desc = String::from_str(&env, "General discussion");

        client.init(&0, &registry, &None, &None, &None, &name, &desc, &false);

        let creator = Address::generate(&env);
        let moderator = Address::generate(&env);
        let title = String::from_str(&env, "Test Thread");

        let thread_id = client.create_thread(&title, &creator);

        // Hide thread
        client.hide_thread(&thread_id, &moderator);
        let thread = client.get_thread(&thread_id).unwrap();
        assert!(thread.is_hidden);

        // Unhide thread
        client.unhide_thread(&thread_id, &moderator);
        let thread = client.get_thread(&thread_id).unwrap();
        assert!(!thread.is_hidden);
    }

    #[test]
    fn test_edit_thread_title() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsBoard, ());
        let client = BoardsBoardClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        let name = String::from_str(&env, "General");
        let desc = String::from_str(&env, "General discussion");

        client.init(&0, &registry, &None, &None, &None, &name, &desc, &false);

        let creator = Address::generate(&env);
        let title = String::from_str(&env, "Original Title");

        let thread_id = client.create_thread(&title, &creator);

        // Edit title as author
        let new_title = String::from_str(&env, "Updated Title");
        client.edit_thread_title(&thread_id, &new_title, &creator);

        let thread = client.get_thread(&thread_id).unwrap();
        assert_eq!(thread.title, new_title);
    }

    #[test]
    fn test_list_threads() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsBoard, ());
        let client = BoardsBoardClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        let name = String::from_str(&env, "General");
        let desc = String::from_str(&env, "General discussion");

        client.init(&0, &registry, &None, &None, &None, &name, &desc, &false);

        let creator = Address::generate(&env);

        // Create multiple threads
        let titles = ["Thread 0", "Thread 1", "Thread 2", "Thread 3", "Thread 4"];
        for title_str in titles.iter() {
            let title = String::from_str(&env, title_str);
            client.create_thread(&title, &creator);
        }

        // List threads (should return newest first)
        let threads = client.list_threads(&0, &3);
        assert_eq!(threads.len(), 3);
        // First thread should be the newest (id 4)
        assert_eq!(threads.get(0).unwrap().id, 4);

        // Get remaining threads
        let more_threads = client.list_threads(&3, &10);
        assert_eq!(more_threads.len(), 2);
    }

    #[test]
    fn test_get_config() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsBoard, ());
        let client = BoardsBoardClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        let name = String::from_str(&env, "Private Board");
        let desc = String::from_str(&env, "Secret discussions");

        client.init(&0, &registry, &None, &None, &None, &name, &desc, &true);

        let config = client.get_config();
        assert_eq!(config.name, name);
        assert_eq!(config.description, desc);
        assert!(config.is_private);
        assert!(!config.is_readonly);
        assert_eq!(config.max_reply_depth, 10);
    }

    #[test]
    fn test_get_nonexistent_thread() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsBoard, ());
        let client = BoardsBoardClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        let name = String::from_str(&env, "General");
        let desc = String::from_str(&env, "Discussion");

        client.init(&0, &registry, &None, &None, &None, &name, &desc, &false);

        // Get thread that doesn't exist
        let thread = client.get_thread(&999);
        assert!(thread.is_none());
    }

    #[test]
    fn test_increment_reply_count() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsBoard, ());
        let client = BoardsBoardClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        let name = String::from_str(&env, "General");
        let desc = String::from_str(&env, "Discussion");

        client.init(&0, &registry, &None, &None, &None, &name, &desc, &false);

        let creator = Address::generate(&env);
        let title = String::from_str(&env, "Test Thread");
        let thread_id = client.create_thread(&title, &creator);

        // Initially 0 replies
        assert_eq!(client.get_thread(&thread_id).unwrap().reply_count, 0);

        // Increment reply count
        client.increment_reply_count(&thread_id);
        assert_eq!(client.get_thread(&thread_id).unwrap().reply_count, 1);

        client.increment_reply_count(&thread_id);
        assert_eq!(client.get_thread(&thread_id).unwrap().reply_count, 2);
    }

    #[test]
    fn test_unpin_thread() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsBoard, ());
        let client = BoardsBoardClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        let name = String::from_str(&env, "General");
        let desc = String::from_str(&env, "Discussion");

        client.init(&0, &registry, &None, &None, &None, &name, &desc, &false);

        let creator = Address::generate(&env);
        let moderator = Address::generate(&env);
        let title = String::from_str(&env, "Pinned Thread");
        let thread_id = client.create_thread(&title, &creator);

        // Pin thread
        client.pin_thread(&thread_id, &moderator);
        assert!(client.get_thread(&thread_id).unwrap().is_pinned);
        assert_eq!(client.get_pinned_threads().len(), 1);

        // Unpin thread
        client.unpin_thread(&thread_id, &moderator);
        assert!(!client.get_thread(&thread_id).unwrap().is_pinned);
        assert_eq!(client.get_pinned_threads().len(), 0);
    }

    #[test]
    fn test_unlock_thread() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsBoard, ());
        let client = BoardsBoardClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        let name = String::from_str(&env, "General");
        let desc = String::from_str(&env, "Discussion");

        client.init(&0, &registry, &None, &None, &None, &name, &desc, &false);

        let creator = Address::generate(&env);
        let moderator = Address::generate(&env);
        let title = String::from_str(&env, "Thread");
        let thread_id = client.create_thread(&title, &creator);

        // Lock thread
        client.lock_thread(&thread_id, &moderator);
        assert!(client.get_thread(&thread_id).unwrap().is_locked);

        // Unlock thread
        client.unlock_thread(&thread_id, &moderator);
        assert!(!client.get_thread(&thread_id).unwrap().is_locked);
    }

    #[test]
    fn test_multiple_pinned_threads() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsBoard, ());
        let client = BoardsBoardClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        let name = String::from_str(&env, "General");
        let desc = String::from_str(&env, "Discussion");

        client.init(&0, &registry, &None, &None, &None, &name, &desc, &false);

        let creator = Address::generate(&env);
        let moderator = Address::generate(&env);

        // Create and pin multiple threads
        let titles = ["Pinned 0", "Pinned 1", "Pinned 2"];
        for title_str in titles.iter() {
            let title = String::from_str(&env, title_str);
            let thread_id = client.create_thread(&title, &creator);
            client.pin_thread(&thread_id, &moderator);
        }

        // All should be in pinned list
        let pinned = client.get_pinned_threads();
        assert_eq!(pinned.len(), 3);

        // All should be marked as pinned
        for i in 0..pinned.len() {
            assert!(pinned.get(i).unwrap().is_pinned);
        }
    }

    #[test]
    fn test_thread_timestamps() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsBoard, ());
        let client = BoardsBoardClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        let name = String::from_str(&env, "General");
        let desc = String::from_str(&env, "Discussion");

        client.init(&0, &registry, &None, &None, &None, &name, &desc, &false);

        let creator = Address::generate(&env);
        let title = String::from_str(&env, "Thread");
        let thread_id = client.create_thread(&title, &creator);

        let thread = client.get_thread(&thread_id).unwrap();

        // created_at and updated_at should be equal when first created
        assert_eq!(thread.created_at, thread.updated_at);

        // Increment reply count should update updated_at
        client.increment_reply_count(&thread_id);
        let updated_thread = client.get_thread(&thread_id).unwrap();
        assert!(updated_thread.updated_at >= thread.updated_at);
    }

    #[test]
    fn test_empty_pinned_list() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsBoard, ());
        let client = BoardsBoardClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        let name = String::from_str(&env, "General");
        let desc = String::from_str(&env, "Discussion");

        client.init(&0, &registry, &None, &None, &None, &name, &desc, &false);

        // No pinned threads initially
        let pinned = client.get_pinned_threads();
        assert_eq!(pinned.len(), 0);
    }

    #[test]
    fn test_thread_defaults() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsBoard, ());
        let client = BoardsBoardClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        let name = String::from_str(&env, "General");
        let desc = String::from_str(&env, "Discussion");

        client.init(&0, &registry, &None, &None, &None, &name, &desc, &false);

        let creator = Address::generate(&env);
        let title = String::from_str(&env, "New Thread");
        let thread_id = client.create_thread(&title, &creator);

        let thread = client.get_thread(&thread_id).unwrap();

        // Default values
        assert_eq!(thread.reply_count, 0);
        assert!(!thread.is_locked);
        assert!(!thread.is_pinned);
        assert!(!thread.is_hidden);
        assert!(!thread.is_deleted);
    }

    #[test]
    fn test_delete_thread() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsBoard, ());
        let client = BoardsBoardClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        let name = String::from_str(&env, "General");
        let desc = String::from_str(&env, "Discussion");

        client.init(&0, &registry, &None, &None, &None, &name, &desc, &false);

        let creator = Address::generate(&env);
        let title = String::from_str(&env, "Thread to Delete");
        let thread_id = client.create_thread(&title, &creator);

        // Initially not deleted
        let thread = client.get_thread(&thread_id).unwrap();
        assert!(!thread.is_deleted);

        // Delete thread (author can delete)
        client.delete_thread(&thread_id, &creator);
        let deleted_thread = client.get_thread(&thread_id).unwrap();
        assert!(deleted_thread.is_deleted);
    }

    #[test]
    fn test_set_thread_states() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsBoard, ());
        let client = BoardsBoardClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        let name = String::from_str(&env, "General");
        let desc = String::from_str(&env, "Discussion");

        client.init(&0, &registry, &None, &None, &None, &name, &desc, &false);

        let creator = Address::generate(&env);
        let title = String::from_str(&env, "Test Thread");
        let thread_id = client.create_thread(&title, &creator);

        // Test set_thread_hidden
        client.set_thread_hidden(&thread_id, &true);
        assert!(client.get_thread(&thread_id).unwrap().is_hidden);
        client.set_thread_hidden(&thread_id, &false);
        assert!(!client.get_thread(&thread_id).unwrap().is_hidden);

        // Test set_thread_locked
        client.set_thread_locked(&thread_id, &true);
        assert!(client.get_thread(&thread_id).unwrap().is_locked);
        client.set_thread_locked(&thread_id, &false);
        assert!(!client.get_thread(&thread_id).unwrap().is_locked);

        // Test set_thread_pinned
        client.set_thread_pinned(&thread_id, &true);
        assert!(client.get_thread(&thread_id).unwrap().is_pinned);
        assert_eq!(client.get_pinned_threads().len(), 1);
        client.set_thread_pinned(&thread_id, &false);
        assert!(!client.get_thread(&thread_id).unwrap().is_pinned);
        assert_eq!(client.get_pinned_threads().len(), 0);
    }
}
