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
    /// Voting contract address
    Voting,
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
    /// Flair definitions
    FlairDefs,
    /// Next flair ID counter
    NextFlairId,
    /// Board rules (markdown text)
    Rules,
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
    pub flair_id: Option<u32>,
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

/// Vote direction from voting contract
#[contracttype]
#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum VoteDirection {
    None = 0,
    Up = 1,
    Down = 2,
}

/// Vote tally from voting contract
#[contracttype]
#[derive(Clone)]
pub struct VoteTally {
    pub upvotes: u32,
    pub downvotes: u32,
    pub score: i32,
    pub first_vote_at: u64,
}

/// Flair definition for categorizing threads
#[contracttype]
#[derive(Clone)]
pub struct FlairDef {
    pub id: u32,
    pub name: String,       // max 32 chars
    pub color: String,      // CSS text color (e.g., "#ffffff")
    pub bg_color: String,   // CSS background color (e.g., "#ff4500")
    pub required: bool,     // Must select flair when posting
    pub mod_only: bool,     // Only moderators can assign
    pub enabled: bool,      // Whether flair is active
}

/// Crosspost reference from content contract
#[contracttype]
#[derive(Clone)]
pub struct CrosspostRef {
    pub original_board_id: u64,
    pub original_thread_id: u64,
    pub original_title: String,
    pub original_author: Address,
    pub crossposted_by: Address,
    pub crossposted_at: u64,
}

/// Community info for navigation (minimal struct for cross-contract calls)
#[contracttype]
#[derive(Clone)]
pub struct CommunityInfo {
    pub id: u64,
    pub name: String,
    pub display_name: String,
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

    /// Set voting contract address (for boards created before this was added)
    pub fn set_voting(env: Env, voting: Address) {
        let registry: Address = env
            .storage()
            .instance()
            .get(&BoardKey::Registry)
            .expect("Not initialized");
        registry.require_auth();
        env.storage().instance().set(&BoardKey::Voting, &voting);
    }

    // Flair management functions

    /// Create a new flair (Admin+ only)
    pub fn create_flair(
        env: Env,
        name: String,
        color: String,
        bg_color: String,
        required: bool,
        mod_only: bool,
        caller: Address,
    ) -> u32 {
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
                panic!("Only owner or admin can create flairs");
            }
        }

        // Get next flair ID
        let flair_id: u32 = env
            .storage()
            .instance()
            .get(&BoardKey::NextFlairId)
            .unwrap_or(0);

        let flair = FlairDef {
            id: flair_id,
            name,
            color,
            bg_color,
            required,
            mod_only,
            enabled: true,
        };

        // Get existing flairs or create empty vec
        let mut flairs: Vec<FlairDef> = env
            .storage()
            .persistent()
            .get(&BoardKey::FlairDefs)
            .unwrap_or(Vec::new(&env));

        flairs.push_back(flair);
        env.storage().persistent().set(&BoardKey::FlairDefs, &flairs);
        env.storage().instance().set(&BoardKey::NextFlairId, &(flair_id + 1));

        flair_id
    }

    /// Update an existing flair (Admin+ only)
    pub fn update_flair(env: Env, flair_id: u32, flair: FlairDef, caller: Address) {
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
                panic!("Only owner or admin can update flairs");
            }
        }

        let flairs: Vec<FlairDef> = env
            .storage()
            .persistent()
            .get(&BoardKey::FlairDefs)
            .expect("No flairs defined");

        // Find and update the flair
        let mut updated = false;
        let mut new_flairs = Vec::new(&env);
        for i in 0..flairs.len() {
            let existing = flairs.get(i).unwrap();
            if existing.id == flair_id {
                new_flairs.push_back(flair.clone());
                updated = true;
            } else {
                new_flairs.push_back(existing);
            }
        }

        if !updated {
            panic!("Flair not found");
        }

        env.storage().persistent().set(&BoardKey::FlairDefs, &new_flairs);
    }

    /// Disable a flair (Admin+ only)
    pub fn disable_flair(env: Env, flair_id: u32, caller: Address) {
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
                panic!("Only owner or admin can disable flairs");
            }
        }

        let flairs: Vec<FlairDef> = env
            .storage()
            .persistent()
            .get(&BoardKey::FlairDefs)
            .expect("No flairs defined");

        // Find and disable the flair
        let mut found = false;
        let mut new_flairs = Vec::new(&env);
        for i in 0..flairs.len() {
            let mut existing = flairs.get(i).unwrap();
            if existing.id == flair_id {
                existing.enabled = false;
                found = true;
            }
            new_flairs.push_back(existing);
        }

        if !found {
            panic!("Flair not found");
        }

        env.storage().persistent().set(&BoardKey::FlairDefs, &new_flairs);
    }

    /// List all flairs
    pub fn list_flairs(env: Env) -> Vec<FlairDef> {
        env.storage()
            .persistent()
            .get(&BoardKey::FlairDefs)
            .unwrap_or(Vec::new(&env))
    }

    /// Get a specific flair by ID
    pub fn get_flair(env: Env, flair_id: u32) -> Option<FlairDef> {
        let flairs: Vec<FlairDef> = env
            .storage()
            .persistent()
            .get(&BoardKey::FlairDefs)
            .unwrap_or(Vec::new(&env));

        for i in 0..flairs.len() {
            let flair = flairs.get(i).unwrap();
            if flair.id == flair_id {
                return Some(flair);
            }
        }
        None
    }

    /// Set thread flair (Moderator+ for mod_only flairs, thread creator for others)
    pub fn set_thread_flair(env: Env, thread_id: u64, flair_id: Option<u32>, caller: Address) {
        caller.require_auth();

        let board_id: u64 = env
            .storage()
            .instance()
            .get(&BoardKey::BoardId)
            .expect("Not initialized");

        // Get thread
        let mut thread: ThreadMeta = env
            .storage()
            .persistent()
            .get(&BoardKey::Thread(thread_id))
            .expect("Thread not found");

        // Check if flair exists and get its properties
        if let Some(fid) = flair_id {
            let flairs: Vec<FlairDef> = env
                .storage()
                .persistent()
                .get(&BoardKey::FlairDefs)
                .expect("No flairs defined");

            let mut flair_found = false;
            let mut is_mod_only = false;
            for i in 0..flairs.len() {
                let flair = flairs.get(i).unwrap();
                if flair.id == fid {
                    if !flair.enabled {
                        panic!("Flair is disabled");
                    }
                    flair_found = true;
                    is_mod_only = flair.mod_only;
                    break;
                }
            }
            if !flair_found {
                panic!("Flair not found");
            }

            // For mod_only flairs, require moderator permissions
            if is_mod_only {
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
                    let fn_name = Symbol::new(&env, "can_moderate");
                    let can_moderate: bool = env.invoke_contract(&permissions, &fn_name, args);
                    if !can_moderate {
                        panic!("Only moderators can assign this flair");
                    }
                }
            } else {
                // For regular flairs, allow thread creator or moderators
                if thread.creator != caller {
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
                        let fn_name = Symbol::new(&env, "can_moderate");
                        let can_moderate: bool = env.invoke_contract(&permissions, &fn_name, args);
                        if !can_moderate {
                            panic!("Only thread creator or moderators can set flair");
                        }
                    }
                }
            }
        }

        thread.flair_id = flair_id;
        thread.updated_at = env.ledger().timestamp();
        env.storage()
            .persistent()
            .set(&BoardKey::Thread(thread_id), &thread);
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

    /// Get maximum reply depth for nested replies
    pub fn get_max_reply_depth(env: Env) -> u32 {
        let config: BoardConfig = env
            .storage()
            .instance()
            .get(&BoardKey::Config)
            .expect("Not initialized");
        config.max_reply_depth
    }

    /// Set maximum reply depth (must be >= 1, owner/admin only)
    pub fn set_max_reply_depth(env: Env, depth: u32, caller: Address) {
        caller.require_auth();

        if depth < 1 {
            panic!("Max reply depth must be at least 1");
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
                panic!("Only owner or admin can change max reply depth");
            }
        }

        let mut config: BoardConfig = env
            .storage()
            .instance()
            .get(&BoardKey::Config)
            .expect("Not initialized");
        config.max_reply_depth = depth;
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

    /// Check if board is read-only
    pub fn is_readonly(env: Env) -> bool {
        env.storage()
            .instance()
            .get::<_, BoardConfig>(&BoardKey::Config)
            .map(|c| c.is_readonly)
            .unwrap_or(false)
    }

    /// Set board read-only status (owner/admin only)
    pub fn set_readonly(env: Env, is_readonly: bool, caller: Address) {
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
                panic!("Only owner or admin can change read-only status");
            }
        }

        let mut config: BoardConfig = env
            .storage()
            .instance()
            .get(&BoardKey::Config)
            .expect("Not initialized");
        config.is_readonly = is_readonly;
        env.storage().instance().set(&BoardKey::Config, &config);
    }

    /// Set board rules (markdown text, Admin+ only)
    /// Rules are displayed on the board page and when creating new posts
    pub fn set_rules(env: Env, rules: String, caller: Address) {
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
                panic!("Only owner or admin can set board rules");
            }
        }

        env.storage().persistent().set(&BoardKey::Rules, &rules);
    }

    /// Get board rules (returns None if no rules are set)
    pub fn get_rules(env: Env) -> Option<String> {
        env.storage().persistent().get(&BoardKey::Rules)
    }

    /// Clear board rules (Admin+ only)
    pub fn clear_rules(env: Env, caller: Address) {
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
                panic!("Only owner or admin can clear board rules");
            }
        }

        env.storage().persistent().remove(&BoardKey::Rules);
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
            flair_id: None,
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

    /// Get thread title and author (for crossposting)
    pub fn get_thread_title_and_author(env: Env, thread_id: u64) -> Option<(String, Address)> {
        let thread: Option<ThreadMeta> = env.storage().persistent().get(&BoardKey::Thread(thread_id));
        thread.map(|t| (t.title, t.creator))
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
    fn render_nav<'a>(env: &'a Env, board_id: u64, viewer: &Option<Address>) -> MarkdownBuilder<'a> {
        let mut md = MarkdownBuilder::new(env)
            .div_start("nav-bar")
            .render_link("Soroban Boards", "/")
            .render_link("Help", "/help");

        // Add profile link if profile contract is available
        if let Some(profile_addr) = Self::get_profile_contract(env) {
            // Build return path to current board: @main:/b/{board_id}
            let mut return_path = Bytes::from_slice(env, b"@main:/b/");
            return_path.append(&Self::u64_to_bytes(env, board_id));

            let args: Vec<Val> = Vec::from_array(env, [
                viewer.into_val(env),
                return_path.into_val(env),
            ]);
            let profile_link: Bytes = env.invoke_contract(
                &profile_addr,
                &Symbol::new(env, "render_nav_link_return"),
                args,
            );
            md = md.raw(profile_link);
        } else if viewer.is_some() {
            // No profile contract registered - show a placeholder link
            let mut return_path = Bytes::from_slice(env, b"@main:/b/");
            return_path.append(&Self::u64_to_bytes(env, board_id));

            md = md
                .raw_str("<a href=\"render:@profile:/register/from/")
                .raw(return_path)
                .raw_str("\">Create Profile</a>");
        }

        md.div_end()
    }

    /// Render back navigation with optional community link
    /// Shows "← Community Name" if board is in a community, plus "← Home"
    fn render_back_nav<'a>(env: &'a Env, mut md: MarkdownBuilder<'a>, board_id: u64) -> MarkdownBuilder<'a> {
        md = md.div_start("back-nav");

        // Check if board is in a community
        if let Some(community) = Self::get_board_community(env, board_id) {
            // Build community display name
            let mut display_buf = [0u8; 64];
            let display_len = community.display_name.len() as usize;
            let display_copy_len = core::cmp::min(display_len, 60);
            community.display_name.copy_into_slice(&mut display_buf[0..display_copy_len]);
            let display = core::str::from_utf8(&display_buf[0..display_copy_len]).unwrap_or("Community");

            // Build community name for URL
            let mut name_buf = [0u8; 32];
            let name_len = community.name.len() as usize;
            let name_copy_len = core::cmp::min(name_len, 32);
            community.name.copy_into_slice(&mut name_buf[0..name_copy_len]);
            let name = core::str::from_utf8(&name_buf[0..name_copy_len]).unwrap_or("");

            // Community link
            md = md.raw_str("<a href=\"render:/c/")
                .raw_str(name)
                .raw_str("\" class=\"back-link back-community\">← ")
                .raw_str(display)
                .raw_str("</a>");
        }

        // Home link (always shown)
        md = md.raw_str("<a href=\"render:/\" class=\"back-link back-home\">← Home</a>");

        md.div_end()
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

    /// Render a thread card for the board list
    fn render_thread_card<'a>(
        env: &'a Env,
        md: MarkdownBuilder<'a>,
        board_id: u64,
        thread: &ThreadMeta,
        voting_contract: &Option<Address>,
        flairs: &Vec<FlairDef>,
    ) -> MarkdownBuilder<'a> {
        // Get vote tally if voting contract is available
        let score = if let Some(voting) = voting_contract {
            let args: Vec<Val> = Vec::from_array(env, [
                board_id.into_val(env),
                thread.id.into_val(env),
            ]);
            let tally: VoteTally = env.invoke_contract(
                voting,
                &Symbol::new(env, "get_thread_tally"),
                args,
            );
            Some(tally.score)
        } else {
            None
        };

        // Thread card with optional vote score
        let mut md = md.raw_str("<div class=\"thread-card-wrapper\">");

        // Vote score display (if voting enabled)
        if let Some(s) = score {
            md = md.raw_str("<span class=\"vote-score-compact\">")
                .number(s as u32)  // Display score (handle negatives in CSS)
                .raw_str("</span>");
        }

        md = md.raw_str("<a href=\"render:/b/")
            .number(board_id as u32)
            .raw_str("/t/")
            .number(thread.id as u32)
            .raw_str("\" class=\"thread-card\">");

        // Display flair if thread has one
        if let Some(flair_id) = thread.flair_id {
            for i in 0..flairs.len() {
                let flair = flairs.get(i).unwrap();
                if flair.id == flair_id && flair.enabled {
                    md = md.raw_str("<span class=\"flair\" style=\"color:")
                        .text_string(&flair.color)
                        .raw_str(";background:")
                        .text_string(&flair.bg_color)
                        .raw_str("\">")
                        .text_string(&flair.name)
                        .raw_str("</span> ");
                    break;
                }
            }
        }

        md = md.raw_str("<span class=\"thread-card-title\">")
            .text_string(&thread.title)
            .raw_str("</span><span class=\"thread-card-meta\">");
        if thread.is_hidden {
            md = md.raw_str("<span class=\"badge badge-hidden\">hidden</span> ");
        }
        if thread.is_pinned {
            md = md.raw_str("<span class=\"badge badge-pinned\">pinned</span> ");
        }
        if thread.is_locked {
            md = md.raw_str("<span class=\"badge badge-locked\">locked</span> ");
        }
        md.number(thread.reply_count)
            .text(" replies · ")
            .raw(Self::format_timestamp(env, thread.created_at))
            .raw_str("</span></a></div>\n")
    }

    /// Render board view with thread list
    fn render_board(env: &Env, board_id: u64, viewer: &Option<Address>) -> Bytes {
        let config: BoardConfig = env
            .storage()
            .instance()
            .get(&BoardKey::Config)
            .expect("Not initialized");

        // Get viewer role (needed for private board check, admin button, and hidden thread filtering)
        let perms_addr_opt = env.storage().instance().get::<_, Address>(&BoardKey::Permissions);
        let viewer_role = if let Some(ref perms_addr) = perms_addr_opt {
            if let Some(user) = viewer {
                let args: Vec<Val> = Vec::from_array(env, [board_id.into_val(env), user.into_val(env)]);
                env.invoke_contract(perms_addr, &Symbol::new(env, "get_role"), args)
            } else {
                Role::Guest
            }
        } else {
            Role::Guest
        };
        let viewer_can_moderate = (viewer_role as u32) >= (Role::Moderator as u32);

        // Get voting contract for displaying vote scores
        let voting_contract: Option<Address> = env.storage().instance().get(&BoardKey::Voting);

        // Get flairs for displaying on thread cards
        let flairs: Vec<FlairDef> = env
            .storage()
            .persistent()
            .get(&BoardKey::FlairDefs)
            .unwrap_or(Vec::new(env));

        // Check permissions for private boards
        if config.is_private {
            if let Some(ref perms_addr) = perms_addr_opt {
                // If not a member, show access denied
                if (viewer_role as u32) < (Role::Member as u32) {
                    return Self::render_private_board_message(env, board_id, &config, viewer, perms_addr);
                }
            }
        }

        let mut md = Self::render_nav(env, board_id, viewer);
        md = Self::render_back_nav(env, md, board_id);
        md = md.div_start("page-header")
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

        // Show board rules if set
        if let Some(rules) = env.storage().persistent().get::<_, String>(&BoardKey::Rules) {
            if rules.len() > 0 {
                md = md.div_start("board-rules")
                    .raw_str("<details><summary><strong>Board Rules</strong></summary>")
                    .div_start("rules-content")
                    .text_string(&rules)
                    .div_end()
                    .raw_str("</details>")
                    .div_end();
            }
        }

        // Show create thread button if logged in
        if viewer.is_some() && !config.is_readonly {
            md = md.raw_str("<a href=\"render:/b/")
                .number(board_id as u32)
                .raw_str("/new\" class=\"action-btn\">+ New Thread</a>")
                .newline();
        }

        // Show settings button for Admin+ users
        if (viewer_role as u32) >= (Role::Admin as u32) {
            md = md.raw_str("<a href=\"render:/admin/b/")
                .number(board_id as u32)
                .raw_str("/settings\" class=\"action-btn action-btn-secondary\">⚙ Settings</a>")
                .newline();
        }

        md = md.raw_str("<h2>Threads</h2>\n");

        // Sort order selector (if voting contract is configured)
        if voting_contract.is_some() {
            md = md.div_start("sort-selector")
                .raw_str("<span class=\"sort-label\">Sort:</span>")
                // Hot is the default when voting is available
                .raw_str("<a href=\"render:/b/")
                .number(board_id as u32)
                .raw_str("\" class=\"sort-option sort-active\">Hot</a>")
                .raw_str("<a href=\"render:/b/")
                .number(board_id as u32)
                .raw_str("?sort=new\" class=\"sort-option\">New</a>")
                .raw_str("<a href=\"render:/b/")
                .number(board_id as u32)
                .raw_str("?sort=top\" class=\"sort-option\">Top</a>")
                .raw_str("<a href=\"render:/b/")
                .number(board_id as u32)
                .raw_str("?sort=controversial\" class=\"sort-option\">Controversial</a>")
                .div_end();
        }

        md = md.div_start("thread-list");

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
            // Get pinned threads list
            let pinned_threads: Vec<u64> = env
                .storage()
                .instance()
                .get(&BoardKey::PinnedThreads)
                .unwrap_or(Vec::new(env));

            let limit = 20u64;
            let mut shown = 0u64;

            // First, render pinned threads
            for i in 0..pinned_threads.len() {
                if shown >= limit {
                    break;
                }
                let thread_id = pinned_threads.get(i).unwrap();
                if let Some(thread) = env.storage().persistent().get::<_, ThreadMeta>(&BoardKey::Thread(thread_id)) {
                    // Skip hidden threads for non-moderators
                    if thread.is_hidden && !viewer_can_moderate {
                        continue;
                    }
                    md = Self::render_thread_card(env, md, board_id, &thread, &voting_contract, &flairs);
                    shown += 1;
                }
            }

            // Then render remaining threads (newest first), skipping pinned ones
            let start_idx = thread_count - 1;
            let mut idx = start_idx;
            while shown < limit && idx < thread_count {
                if let Some(thread) = env.storage().persistent().get::<_, ThreadMeta>(&BoardKey::Thread(idx)) {
                    // Skip pinned threads (already shown above)
                    if thread.is_pinned {
                        if idx > 0 {
                            idx -= 1;
                        } else {
                            break;
                        }
                        continue;
                    }
                    // Skip hidden threads for non-moderators
                    if thread.is_hidden && !viewer_can_moderate {
                        if idx > 0 {
                            idx -= 1;
                        } else {
                            break;
                        }
                        continue;
                    }
                    md = Self::render_thread_card(env, md, board_id, &thread, &voting_contract, &flairs);
                    shown += 1;
                }
                if idx > 0 {
                    idx -= 1;
                } else {
                    break;
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
        let mut md = Self::render_nav(env, board_id, viewer);
        md = Self::render_back_nav(env, md, board_id);
        md = md.div_start("page-header")
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

    /// Render hidden thread access denied message
    fn render_hidden_thread_message(env: &Env, board_id: u64, viewer: &Option<Address>) -> Bytes {
        let md = Self::render_nav(env, board_id, viewer)
            .newline()
            .raw_str("[< Back to Board](render:/b/")
            .number(board_id as u32)
            .raw_str(")")
            .newline()
            .newline()
            .warning("This thread has been hidden by a moderator.");
        Self::render_footer_into(md).build()
    }

    /// Render create thread form
    fn render_create_thread(env: &Env, board_id: u64, viewer: &Option<Address>) -> Bytes {
        let mut md = Self::render_nav(env, board_id, viewer)
            .newline()  // Blank line after nav-bar div for markdown parsing
            .raw_str("[< Back to Board](render:/b/")
            .number(board_id as u32)
            .raw_str(")")
            .newline()
            .newline()  // Blank line before h1 for markdown parsing
            .h1("New Thread");

        // Show rules reminder if rules are set
        if let Some(rules) = env.storage().persistent().get::<_, String>(&BoardKey::Rules) {
            if rules.len() > 0 {
                md = md.div_start("rules-reminder")
                    .raw_str("<details open><summary><strong>Before posting, please read the board rules:</strong></summary>")
                    .div_start("rules-content")
                    .text_string(&rules)
                    .div_end()
                    .raw_str("</details>")
                    .div_end();
            }
        }

        // Check if board is read-only
        if let Some(config) = env.storage().instance().get::<_, BoardConfig>(&BoardKey::Config) {
            if config.is_readonly {
                md = md.warning("This board is read-only. New threads cannot be created.");
                return Self::render_footer_into(md).build();
            }
        }

        if viewer.is_none() {
            md = md.warning("Please connect your wallet to create a thread.");
            return Self::render_footer_into(md).build();
        }

        // Get flairs for the selector
        let flairs: Vec<FlairDef> = env
            .storage()
            .persistent()
            .get(&BoardKey::FlairDefs)
            .unwrap_or(Vec::new(env));

        // Check if viewer is a moderator (for mod_only flairs)
        let is_moderator = if env.storage().instance().has(&BoardKey::Permissions) {
            let permissions: Address = env
                .storage()
                .instance()
                .get(&BoardKey::Permissions)
                .unwrap();
            let args: Vec<Val> = Vec::from_array(
                env,
                [board_id.into_val(env), viewer.as_ref().unwrap().into_val(env)],
            );
            let fn_name = Symbol::new(env, "can_moderate");
            env.invoke_contract(&permissions, &fn_name, args)
        } else {
            false
        };

        // Check if any flair is required
        let mut flair_required = false;
        for i in 0..flairs.len() {
            let flair = flairs.get(i).unwrap();
            if flair.required && flair.enabled && (!flair.mod_only || is_moderator) {
                flair_required = true;
                break;
            }
        }

        md = md
            .raw_str("<input type=\"hidden\" name=\"_redirect\" value=\"/b/")
            .number(board_id as u32)
            .raw_str("\" />\n")
            .raw_str("<input type=\"hidden\" name=\"board_id\" value=\"")
            .number(board_id as u32)
            .raw_str("\" />\n")
            .input("title", "Thread title")
            .newline();

        // Add flair selector if flairs exist
        let mut has_visible_flairs = false;
        for i in 0..flairs.len() {
            let flair = flairs.get(i).unwrap();
            if flair.enabled && (!flair.mod_only || is_moderator) {
                has_visible_flairs = true;
                break;
            }
        }

        if has_visible_flairs {
            md = md.raw_str("<div class=\"flair-selector\">\n")
                .raw_str("<label>Flair");
            if flair_required {
                md = md.raw_str(" <span class=\"required\">*</span>");
            }
            md = md.raw_str(":</label>\n")
                .raw_str("<select name=\"flair_id\">\n")
                .raw_str("<option value=\"\">-- Select flair --</option>\n");

            for i in 0..flairs.len() {
                let flair = flairs.get(i).unwrap();
                if flair.enabled && (!flair.mod_only || is_moderator) {
                    md = md.raw_str("<option value=\"")
                        .number(flair.id)
                        .raw_str("\" style=\"color:")
                        .text_string(&flair.color)
                        .raw_str(";background:")
                        .text_string(&flair.bg_color)
                        .raw_str("\">")
                        .text_string(&flair.name)
                        .raw_str("</option>\n");
                }
            }

            md = md.raw_str("</select>\n")
                .raw_str("</div>\n");
        }

        md = md.textarea("body", 10, "Write your post content here...")
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

        // Get profile contract for author display
        let profile_contract = Self::get_profile_contract(env);

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

        // Get viewer role for hidden thread check and moderator controls
        let perms_addr_opt = env.storage().instance().get::<_, Address>(&BoardKey::Permissions);
        let viewer_role = if let Some(ref perms_addr) = perms_addr_opt {
            if let Some(user) = viewer {
                let args: Vec<Val> = Vec::from_array(env, [board_id.into_val(env), user.into_val(env)]);
                env.invoke_contract(perms_addr, &Symbol::new(env, "get_role"), args)
            } else {
                Role::Guest
            }
        } else {
            Role::Guest
        };
        let viewer_can_moderate = (viewer_role as u32) >= (Role::Moderator as u32);

        // Check if thread is hidden - only moderators can view hidden threads
        let is_hidden = thread.as_ref().map(|t| t.is_hidden).unwrap_or(false);
        if is_hidden && !viewer_can_moderate {
            return Self::render_hidden_thread_message(env, board_id, viewer);
        }

        // Determine if posting is allowed
        let is_readonly = config.is_readonly;
        let is_locked = thread.as_ref().map(|t| t.is_locked).unwrap_or(false);
        let can_post = !is_readonly && !is_locked;

        let mut md = Self::render_nav(env, board_id, viewer)
            .newline()
            .raw_str("[< Back to Board](render:/b/")
            .number(board_id as u32)
            .raw_str(")")
            .newline();

        // Get flairs for display
        let flairs: Vec<FlairDef> = env
            .storage()
            .persistent()
            .get(&BoardKey::FlairDefs)
            .unwrap_or(Vec::new(env));

        // Show thread title if available
        if let Some(ref t) = thread {
            md = md.raw_str("<h1>");

            // Display flair before title if thread has one
            if let Some(flair_id) = t.flair_id {
                for i in 0..flairs.len() {
                    let flair = flairs.get(i).unwrap();
                    if flair.id == flair_id && flair.enabled {
                        md = md.raw_str("<span class=\"flair\" style=\"color:")
                            .text_string(&flair.color)
                            .raw_str(";background:")
                            .text_string(&flair.bg_color)
                            .raw_str("\">")
                            .text_string(&flair.name)
                            .raw_str("</span> ");
                        break;
                    }
                }
            }

            md = md.text_string(&t.title)
                .raw_str("</h1>\n");

            // Show author (with return path so "Go Back" returns here)
            let return_path = Self::build_thread_return_path(env, board_id, thread_id);
            md = md.raw_str("<div class=\"thread-meta\">by ");
            md = Self::render_author(env, md, &t.creator, &profile_contract, Some(return_path));
            md = md.raw_str(" · ")
                .raw(Self::format_timestamp(env, t.created_at))
                .raw_str("</div>\n");
        } else {
            md = md.raw_str("<h1>Thread</h1>\n");
        }

        // Show status badges
        let is_pinned = thread.as_ref().map(|t| t.is_pinned).unwrap_or(false);
        if is_hidden && viewer_can_moderate {
            md = md.raw_str("<span class=\"badge badge-hidden\">hidden</span> ");
        }
        if is_pinned {
            md = md.raw_str("<span class=\"badge badge-pinned\">pinned</span> ");
        }
        if is_locked {
            md = md.raw_str("<span class=\"badge badge-locked\">locked</span> ");
        }
        if is_readonly {
            md = md.raw_str("<span class=\"badge badge-readonly\">read-only board</span> ");
        }
        if is_hidden || is_pinned || is_locked || is_readonly {
            md = md.newline();
        }

        // Check if this is a crosspost and show header
        let crosspost_args: Vec<Val> = Vec::from_array(env, [board_id.into_val(env), thread_id.into_val(env)]);
        let crosspost_ref: Option<CrosspostRef> = env
            .try_invoke_contract::<Option<CrosspostRef>, soroban_sdk::Error>(
                &content,
                &Symbol::new(env, "get_crosspost_ref"),
                crosspost_args,
            )
            .ok()
            .and_then(|r| r.ok())
            .flatten();

        if let Some(ref xpost) = crosspost_ref {
            md = md.div_start("crosspost-header")
                .raw_str("<span class=\"crosspost-badge\">⤴ Crosspost</span> ")
                .raw_str("Originally posted in ")
                .raw_str("[Board #")
                .number(xpost.original_board_id as u32)
                .raw_str("](render:/b/")
                .number(xpost.original_board_id as u32)
                .raw_str("/t/")
                .number(xpost.original_thread_id as u32)
                .raw_str(") by ");
            // Show original author
            let return_path = Self::build_thread_return_path(env, board_id, thread_id);
            md = Self::render_author(env, md, &xpost.original_author, &profile_contract, Some(return_path.clone()));
            md = md.div_end()
                .newline();
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

        // Vote buttons (if voting contract is configured and user is logged in)
        let voting_contract: Option<Address> = env.storage().instance().get(&BoardKey::Voting);
        if let Some(ref voting) = voting_contract {
            // Get thread vote tally
            let tally_args: Vec<Val> = Vec::from_array(env, [
                board_id.into_val(env),
                thread_id.into_val(env),
            ]);
            let tally: VoteTally = env.invoke_contract(
                voting,
                &Symbol::new(env, "get_thread_tally"),
                tally_args,
            );

            // Get viewer's current vote (if logged in)
            let viewer_vote = if let Some(ref user) = viewer {
                let vote_args: Vec<Val> = Vec::from_array(env, [
                    board_id.into_val(env),
                    thread_id.into_val(env),
                    user.into_val(env),
                ]);
                env.invoke_contract::<VoteDirection>(
                    voting,
                    &Symbol::new(env, "get_user_thread_vote"),
                    vote_args,
                )
            } else {
                VoteDirection::None
            };

            md = md.div_start("vote-buttons");

            // Upvote button
            if viewer.is_some() {
                let up_class = if viewer_vote == VoteDirection::Up { "vote-up vote-active" } else { "vote-up" };
                md = md.raw_str("<a href=\"tx:@voting:vote_thread {&quot;board_id&quot;:")
                    .number(board_id as u32)
                    .raw_str(",&quot;thread_id&quot;:")
                    .number(thread_id as u32)
                    .raw_str(",&quot;direction&quot;:");
                // Toggle: if already up, set to none; otherwise set to up
                if viewer_vote == VoteDirection::Up {
                    md = md.raw_str("0");  // None
                } else {
                    md = md.raw_str("1");  // Up
                }
                md = md.raw_str("}\" class=\"")
                    .raw_str(up_class)
                    .raw_str("\">▲</a>");
            } else {
                md = md.raw_str("<span class=\"vote-up vote-disabled\">▲</span>");
            }

            // Score display
            md = md.raw_str("<span class=\"vote-score\">")
                .number(tally.score as u32)
                .raw_str("</span>");

            // Downvote button
            if viewer.is_some() {
                let down_class = if viewer_vote == VoteDirection::Down { "vote-down vote-active" } else { "vote-down" };
                md = md.raw_str("<a href=\"tx:@voting:vote_thread {&quot;board_id&quot;:")
                    .number(board_id as u32)
                    .raw_str(",&quot;thread_id&quot;:")
                    .number(thread_id as u32)
                    .raw_str(",&quot;direction&quot;:");
                // Toggle: if already down, set to none; otherwise set to down
                if viewer_vote == VoteDirection::Down {
                    md = md.raw_str("0");  // None
                } else {
                    md = md.raw_str("2");  // Down
                }
                md = md.raw_str("}\" class=\"")
                    .raw_str(down_class)
                    .raw_str("\">▼</a>");
            } else {
                md = md.raw_str("<span class=\"vote-down vote-disabled\">▼</span>");
            }

            md = md.div_end()
                .newline();
        }

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

            // Crosspost button (only if not already a crosspost)
            if crosspost_ref.is_none() {
                md = md.text(" ")
                    .raw_str("[Crosspost](render:/crosspost?from_board=")
                    .number(board_id as u32)
                    .raw_str("&from_thread=")
                    .number(thread_id as u32)
                    .raw_str(")");

                // Show crosspost count if any
                let xpost_count_args: Vec<Val> = Vec::from_array(env, [board_id.into_val(env), thread_id.into_val(env)]);
                let xpost_count: u32 = env
                    .try_invoke_contract::<u32, soroban_sdk::Error>(
                        &content,
                        &Symbol::new(env, "get_crosspost_count"),
                        xpost_count_args,
                    )
                    .ok()
                    .and_then(|r| r.ok())
                    .unwrap_or(0);

                if xpost_count > 0 {
                    md = md.raw_str(" <span class=\"crosspost-count\">")
                        .number(xpost_count)
                        .raw_str(" crosspost");
                    if xpost_count > 1 {
                        md = md.raw_str("s");
                    }
                    md = md.raw_str("</span>");
                }
            }

            md = md.div_end()
                .newline();
        }

        // Moderator controls (only show for moderator+ users)
        // Uses raw HTML links since we're inside a div (markdown not processed in HTML blocks)
        if viewer_can_moderate {
            if let Some(ref user) = viewer {
                md = md.div_start("mod-actions")
                    .raw_str("<strong>Mod Actions:</strong> ")
                    // Hidden fields for all actions
                    .raw_str("<input type=\"hidden\" name=\"board_id\" value=\"")
                    .number(board_id as u32)
                    .raw_str("\" />")
                    .raw_str("<input type=\"hidden\" name=\"thread_id\" value=\"")
                    .number(thread_id as u32)
                    .raw_str("\" />")
                    .raw_str("<input type=\"hidden\" name=\"caller\" value=\"")
                    .text_string(&user.to_string())
                    .raw_str("\" />");

                // Lock/Unlock
                if is_locked {
                    md = md.raw_str("<a href=\"form:@admin:unlock_thread\">[Unlock]</a>");
                } else {
                    md = md.raw_str("<a href=\"form:@admin:lock_thread\">[Lock]</a>");
                }

                // Hide/Unhide
                if is_hidden {
                    md = md.raw_str(" <a href=\"form:@admin:unhide_thread\">[Unhide]</a>");
                } else {
                    md = md.raw_str(" <a href=\"form:@admin:hide_thread\">[Hide]</a>");
                }

                // Pin/Unpin
                if is_pinned {
                    md = md.raw_str(" <a href=\"form:@admin:unpin_thread\">[Unpin]</a>");
                } else {
                    md = md.raw_str(" <a href=\"form:@admin:pin_thread\">[Pin]</a>");
                }

                md = md.div_end().newline();
            }
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

        // Get profile contract for author display
        let profile_contract = Self::get_profile_contract(env);

        // Get voting contract for vote buttons
        let voting_contract: Option<Address> = env.storage().instance().get(&BoardKey::Voting);

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
                md = Self::render_reply_item_waterfall(env, md, &content, &reply, board_id, thread_id, viewer, can_post, &profile_contract, &voting_contract);
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

        // Get profile contract for author display
        let profile_contract = Self::get_profile_contract(env);

        // Get voting contract for vote buttons
        let voting_contract: Option<Address> = env.storage().instance().get(&BoardKey::Voting);

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
                md = Self::render_reply_item_waterfall(env, md, &content, &child, board_id, thread_id, viewer, can_post, &profile_contract, &voting_contract);
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
        profile_contract: &Option<Address>,
        voting_contract: &Option<Address>,
    ) -> MarkdownBuilder<'a> {
        md = md.div_start("reply");

        // Reply header with author (with return path so "Go Back" returns to thread)
        let return_path = Self::build_thread_return_path(env, board_id, thread_id);
        md = md.div_start("reply-header");
        md = Self::render_author(env, md, &reply.creator, profile_contract, Some(return_path));
        md = md.raw_str(" · Reply #")
            .number(reply.id as u32)
            .raw_str(" · ")
            .raw(Self::format_timestamp(env, reply.created_at))
            .div_end();

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

        // Vote buttons for reply (if voting contract is configured)
        if let Some(ref voting) = voting_contract {
            // Get reply vote tally
            let tally_args: Vec<Val> = Vec::from_array(env, [
                board_id.into_val(env),
                thread_id.into_val(env),
                reply.id.into_val(env),
            ]);
            let tally: VoteTally = env.invoke_contract(
                voting,
                &Symbol::new(env, "get_reply_tally"),
                tally_args,
            );

            // Get viewer's current vote (if logged in)
            let viewer_vote = if let Some(ref user) = viewer {
                let vote_args: Vec<Val> = Vec::from_array(env, [
                    board_id.into_val(env),
                    thread_id.into_val(env),
                    reply.id.into_val(env),
                    user.into_val(env),
                ]);
                env.invoke_contract::<VoteDirection>(
                    voting,
                    &Symbol::new(env, "get_user_reply_vote"),
                    vote_args,
                )
            } else {
                VoteDirection::None
            };

            md = md.div_start("reply-votes");

            // Upvote button
            if viewer.is_some() {
                let up_class = if viewer_vote == VoteDirection::Up { "vote-up vote-active" } else { "vote-up" };
                md = md.raw_str("<a href=\"tx:@voting:vote_reply {&quot;board_id&quot;:")
                    .number(board_id as u32)
                    .raw_str(",&quot;thread_id&quot;:")
                    .number(thread_id as u32)
                    .raw_str(",&quot;reply_id&quot;:")
                    .number(reply.id as u32)
                    .raw_str(",&quot;direction&quot;:");
                if viewer_vote == VoteDirection::Up {
                    md = md.raw_str("0");
                } else {
                    md = md.raw_str("1");
                }
                md = md.raw_str("}\" class=\"")
                    .raw_str(up_class)
                    .raw_str("\">▲</a>");
            } else {
                md = md.raw_str("<span class=\"vote-up vote-disabled\">▲</span>");
            }

            // Score
            md = md.raw_str("<span class=\"vote-score-inline\">")
                .number(tally.score as u32)
                .raw_str("</span>");

            // Downvote button
            if viewer.is_some() {
                let down_class = if viewer_vote == VoteDirection::Down { "vote-down vote-active" } else { "vote-down" };
                md = md.raw_str("<a href=\"tx:@voting:vote_reply {&quot;board_id&quot;:")
                    .number(board_id as u32)
                    .raw_str(",&quot;thread_id&quot;:")
                    .number(thread_id as u32)
                    .raw_str(",&quot;reply_id&quot;:")
                    .number(reply.id as u32)
                    .raw_str(",&quot;direction&quot;:");
                if viewer_vote == VoteDirection::Down {
                    md = md.raw_str("0");
                } else {
                    md = md.raw_str("2");
                }
                md = md.raw_str("}\" class=\"")
                    .raw_str(down_class)
                    .raw_str("\">▼</a>");
            } else {
                md = md.raw_str("<span class=\"vote-down vote-disabled\">▼</span>");
            }

            md = md.div_end();
        }

        // Reply actions
        md = md.div_start("reply-meta");

        // Only show Reply button if posting is allowed
        if viewer.is_some() && can_post {
            md = md.raw_str("[Reply](render:/b/")
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
        let mut md = Self::render_nav(env, board_id, viewer)
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

        // Check if board is read-only
        if let Some(config) = env.storage().instance().get::<_, BoardConfig>(&BoardKey::Config) {
            if config.is_readonly {
                md = md.warning("This board is read-only. Replies cannot be posted.");
                return Self::render_footer_into(md).build();
            }
        }

        // Check if thread is locked
        if let Some(thread) = env.storage().persistent().get::<_, ThreadMeta>(&BoardKey::Thread(thread_id)) {
            if thread.is_locked {
                md = md.warning("This thread is locked. Replies cannot be posted.");
                return Self::render_footer_into(md).build();
            }
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

        let mut md = Self::render_nav(env, board_id, viewer)
            .newline()
            .raw_str("[< Back to Thread](render:/b/")
            .number(board_id as u32)
            .raw_str("/t/")
            .number(thread_id as u32)
            .raw_str(")")
            .newline()
            .newline()
            .h1("Edit Thread");

        // Check if board is read-only
        if let Some(config) = env.storage().instance().get::<_, BoardConfig>(&BoardKey::Config) {
            if config.is_readonly {
                md = md.warning("This board is read-only. Threads cannot be edited.");
                return Self::render_footer_into(md).build();
            }
        }

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

        let mut md = Self::render_nav(env, board_id, viewer)
            .newline()
            .raw_str("[< Back to Thread](render:/b/")
            .number(board_id as u32)
            .raw_str("/t/")
            .number(thread_id as u32)
            .raw_str(")")
            .newline()
            .newline()
            .h1("Edit Reply");

        // Check if board is read-only
        if let Some(config) = env.storage().instance().get::<_, BoardConfig>(&BoardKey::Config) {
            if config.is_readonly {
                md = md.warning("This board is read-only. Replies cannot be edited.");
                return Self::render_footer_into(md).build();
            }
        }

        // Check if thread is locked
        if let Some(thread) = env.storage().persistent().get::<_, ThreadMeta>(&BoardKey::Thread(thread_id)) {
            if thread.is_locked {
                md = md.warning("This thread is locked. Replies cannot be edited.");
                return Self::render_footer_into(md).build();
            }
        }

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

    /// Get profile contract from registry (if available)
    fn get_profile_contract(env: &Env) -> Option<Address> {
        let registry: Address = env
            .storage()
            .instance()
            .get(&BoardKey::Registry)?;

        // Look up profile contract from registry using get_contract
        let args: Vec<Val> = Vec::from_array(env, [Symbol::new(env, "profile").into_val(env)]);
        env.invoke_contract(&registry, &Symbol::new(env, "get_contract"), args)
    }

    /// Get the community this board belongs to (if any)
    fn get_board_community(env: &Env, board_id: u64) -> Option<CommunityInfo> {
        let registry: Address = env.storage().instance().get(&BoardKey::Registry)?;

        // Query registry for board's community ID
        let args: Vec<Val> = Vec::from_array(env, [board_id.into_val(env)]);
        let community_id_opt: Option<u64> = env.invoke_contract(
            &registry,
            &Symbol::new(env, "get_board_community"),
            args,
        );

        let community_id = community_id_opt?;

        // Get community contract address from registry
        let comm_args: Vec<Val> = Vec::from_array(env, [Symbol::new(env, "community").into_val(env)]);
        let community_contract: Option<Address> = env.invoke_contract(
            &registry,
            &Symbol::new(env, "get_contract"),
            comm_args,
        );

        let community_addr = community_contract?;

        // Fetch community metadata
        let meta_args: Vec<Val> = Vec::from_array(env, [community_id.into_val(env)]);
        let community_meta: Option<CommunityInfo> = env.invoke_contract(
            &community_addr,
            &Symbol::new(env, "get_community_info"),
            meta_args,
        );

        community_meta
    }

    /// Render author info (username link or truncated address)
    ///
    /// When return_path is provided, clicking the author link and then "Go Back"
    /// on the profile page will return to that path.
    fn render_author<'a>(
        env: &Env,
        md: MarkdownBuilder<'a>,
        creator: &Address,
        profile_contract: &Option<Address>,
        return_path: Option<Bytes>,
    ) -> MarkdownBuilder<'a> {
        match profile_contract {
            Some(profile_addr) => {
                // Use the return path variant if we have a return path
                let rendered: Bytes = match return_path {
                    Some(path) => {
                        let args: Vec<Val> = Vec::from_array(
                            env,
                            [creator.into_val(env), path.into_val(env)],
                        );
                        env.invoke_contract(
                            profile_addr,
                            &Symbol::new(env, "render_profile_compact_return"),
                            args,
                        )
                    }
                    None => {
                        let args: Vec<Val> = Vec::from_array(env, [creator.into_val(env)]);
                        env.invoke_contract(
                            profile_addr,
                            &Symbol::new(env, "render_profile_card_compact"),
                            args,
                        )
                    }
                };
                md.raw(rendered)
            }
            None => {
                // Fallback: show truncated address
                md.raw_str("<span class=\"author\">")
                    .raw(Self::truncate_address(env, creator))
                    .raw_str("</span>")
            }
        }
    }

    /// Build a return path for the current thread view
    fn build_thread_return_path(env: &Env, board_id: u64, thread_id: u64) -> Bytes {
        // Build cross-contract return path: @main:/b/{board_id}/t/{thread_id}
        let mut path = Bytes::from_slice(env, b"@main:/b/");
        path.append(&soroban_render_sdk::bytes::u32_to_bytes(env, board_id as u32));
        path.append(&Bytes::from_slice(env, b"/t/"));
        path.append(&soroban_render_sdk::bytes::u32_to_bytes(env, thread_id as u32));
        path
    }

    /// Truncate an address for display
    fn truncate_address(env: &Env, address: &Address) -> Bytes {
        let addr_string = address.to_string();
        let full_bytes = soroban_render_sdk::bytes::string_to_bytes(env, &addr_string);
        let len = full_bytes.len();

        if len <= 12 {
            return full_bytes;
        }

        let mut result = Bytes::new(env);

        // First 4 characters
        for i in 0..4 {
            if let Some(c) = full_bytes.get(i) {
                result.push_back(c);
            }
        }

        // Ellipsis
        result.push_back(b'.');
        result.push_back(b'.');
        result.push_back(b'.');

        // Last 4 characters
        for i in (len - 4)..len {
            if let Some(c) = full_bytes.get(i) {
                result.push_back(c);
            }
        }

        result
    }

    /// Format a Unix timestamp as a human-readable date string.
    /// Returns "YYYY-MM-DD HH:MM UTC" format.
    fn format_timestamp(env: &Env, timestamp: u64) -> Bytes {
        // Handle legacy ledger sequence numbers (small values)
        // Unix timestamps for 2024+ are ~1700000000+
        if timestamp < 1_000_000_000 {
            // This is likely a ledger sequence, not a timestamp
            let mut result = Bytes::from_slice(env, b"Ledger ");
            result.append(&Self::u64_to_bytes(env, timestamp));
            return result;
        }

        // Convert Unix timestamp to date components
        let total_seconds = timestamp;
        let total_minutes = total_seconds / 60;
        let total_hours = total_minutes / 60;
        let total_days = total_hours / 24;

        let minutes = (total_minutes % 60) as u8;
        let hours = (total_hours % 24) as u8;

        // Calculate year, month, day from days since epoch (Jan 1, 1970)
        let (year, month, day) = Self::days_to_date(total_days as i64);

        // Format: "YYYY-MM-DD HH:MM UTC"
        let mut buffer = [0u8; 20];

        // Year (4 digits)
        buffer[0] = b'0' + ((year / 1000) % 10) as u8;
        buffer[1] = b'0' + ((year / 100) % 10) as u8;
        buffer[2] = b'0' + ((year / 10) % 10) as u8;
        buffer[3] = b'0' + (year % 10) as u8;
        buffer[4] = b'-';

        // Month (2 digits)
        buffer[5] = b'0' + ((month / 10) % 10) as u8;
        buffer[6] = b'0' + (month % 10) as u8;
        buffer[7] = b'-';

        // Day (2 digits)
        buffer[8] = b'0' + ((day / 10) % 10) as u8;
        buffer[9] = b'0' + (day % 10) as u8;
        buffer[10] = b' ';

        // Hour (2 digits)
        buffer[11] = b'0' + ((hours / 10) % 10) as u8;
        buffer[12] = b'0' + (hours % 10) as u8;
        buffer[13] = b':';

        // Minute (2 digits)
        buffer[14] = b'0' + ((minutes / 10) % 10) as u8;
        buffer[15] = b'0' + (minutes % 10) as u8;
        buffer[16] = b' ';

        // UTC
        buffer[17] = b'U';
        buffer[18] = b'T';
        buffer[19] = b'C';

        Bytes::from_slice(env, &buffer)
    }

    /// Convert days since Unix epoch to (year, month, day).
    /// Algorithm based on Howard Hinnant's date algorithms.
    fn days_to_date(days: i64) -> (i32, u8, u8) {
        let z = days + 719468;
        let era = if z >= 0 { z } else { z - 146096 } / 146097;
        let doe = (z - era * 146097) as u32; // day of era
        let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365; // year of era
        let y = yoe as i64 + era * 400;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // day of year
        let mp = (5 * doy + 2) / 153; // month index
        let d = doy - (153 * mp + 2) / 5 + 1;
        let m = if mp < 10 { mp + 3 } else { mp - 9 };
        let year = if m <= 2 { y + 1 } else { y };

        (year as i32, m as u8, d as u8)
    }

    /// Convert u64 to Bytes for display.
    fn u64_to_bytes(env: &Env, n: u64) -> Bytes {
        if n == 0 {
            return Bytes::from_slice(env, b"0");
        }

        let mut buffer = [0u8; 20];
        let mut idx = 20;
        let mut num = n;

        while num > 0 {
            idx -= 1;
            buffer[idx] = b'0' + (num % 10) as u8;
            num /= 10;
        }

        Bytes::from_slice(env, &buffer[idx..])
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
