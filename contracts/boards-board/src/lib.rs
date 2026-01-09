#![no_std]

use soroban_render_sdk::prelude::*;
use soroban_sdk::{contract, contractimpl, contracttype, Address, Bytes, BytesN, Env, IntoVal, String, Symbol, Val, Vec};

// Declare render capabilities
soroban_render!(markdown, styles);

/// Storage keys for boards contract (supports multiple boards)
#[contracttype]
#[derive(Clone)]
pub enum BoardKey {
    // === Instance storage (global) ===
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
    /// Community contract address (for board-community lookups)
    Community,
    /// Temporary: community slug for current render (avoids re-entrant calls)
    RenderCommunitySlug,
    /// Total number of boards (auto-increment counter)
    BoardCount,

    // === Persistent storage (per-board, namespaced by board_id) ===
    /// Board metadata by ID
    Board(u64),
    /// Board configuration by ID
    BoardConfig(u64),
    /// Thread count per board
    BoardThreadCount(u64),
    /// Thread metadata: (board_id, thread_id) -> ThreadMeta
    BoardThread(u64, u64),
    /// Pinned thread IDs per board
    BoardPinnedThreads(u64),
    /// Edit window in seconds per board
    BoardEditWindow(u64),
    /// Flair definitions per board
    BoardFlairDefs(u64),
    /// Next flair ID counter per board
    BoardNextFlairId(u64),
    /// Board rules (markdown text) per board
    BoardRules(u64),
    /// Whether board is listed publicly (for home page)
    BoardListed(u64),
    /// Board creator address
    BoardCreator(u64),
    /// Board creation timestamp
    BoardCreatedAt(u64),
    /// Count of boards created by a user (for threshold limits)
    UserBoardCount(Address),
    /// Slug-to-board-ID index (for standalone boards only - community boards use CommunityBoardBySlug)
    BoardBySlug(String),
}

/// Board metadata (stored per-board)
#[contracttype]
#[derive(Clone)]
pub struct BoardMeta {
    pub id: u64,
    pub slug: String,
    pub name: String,
    pub description: String,
    pub creator: Address,
    pub created_at: u64,
    pub thread_count: u64,
    pub is_readonly: bool,
    pub is_private: bool,
    pub is_listed: bool,
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
    /// Initialize the boards contract globally (called once during deployment).
    /// This sets up the contract to store multiple boards.
    pub fn init(
        env: Env,
        registry: Address,
        permissions: Option<Address>,
        content: Option<Address>,
        theme: Option<Address>,
    ) {
        if env.storage().instance().has(&BoardKey::Registry) {
            panic!("Already initialized");
        }

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

        // Initialize board count to 0
        env.storage().instance().set(&BoardKey::BoardCount, &0u64);
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

    /// Get the community contract address
    pub fn get_community(env: Env) -> Option<Address> {
        env.storage().instance().get(&BoardKey::Community)
    }

    /// Debug: test cross-contract call to community (get community ID)
    pub fn test_community_call(env: Env, board_id: u64) -> Option<u64> {
        let community_contract: Address = env.storage().instance().get(&BoardKey::Community)?;
        let args: Vec<Val> = Vec::from_array(&env, [board_id.into_val(&env)]);
        env.invoke_contract(
            &community_contract,
            &Symbol::new(&env, "get_board_community"),
            args,
        )
    }

    /// Debug: test full get_board_community
    pub fn test_get_board_community(env: Env, board_id: u64) -> Option<CommunityInfo> {
        Self::get_board_community(&env, board_id)
    }

    /// Set the community contract address (for board-community lookups)
    /// Only callable by registry admins
    pub fn set_community(env: Env, community: Address, caller: Address) {
        caller.require_auth();

        let registry: Address = env
            .storage()
            .instance()
            .get(&BoardKey::Registry)
            .expect("Not initialized");

        // Verify caller is a registry admin
        let admin_args: Vec<Val> = Vec::from_array(&env, [caller.clone().into_val(&env)]);
        let is_admin: bool = env.invoke_contract(&registry, &Symbol::new(&env, "is_admin"), admin_args);

        if !is_admin {
            panic!("Only registry admin can set community");
        }

        env.storage().instance().set(&BoardKey::Community, &community);
    }

    /// Create a new board in this contract.
    ///
    /// This creates a new board entry with an auto-incrementing ID.
    /// Handles site admin bypass, creation threshold checks, and sets board owner.
    ///
    /// Parameters:
    /// - name: Board name
    /// - description: Board description
    /// - is_private: String "true" or "false" from form input
    /// - is_listed: String "true" or "false" from form input
    /// - caller: The address creating the board (must authorize)
    ///
    /// Returns the assigned board ID.
    pub fn create_board(
        env: Env,
        name: String,
        description: String,
        is_private: String,
        is_listed: String,
        caller: Address,
    ) -> u64 {
        // Call create_board_with_slug with no explicit slug
        Self::create_board_with_slug(env, name, description, is_private, is_listed, None, caller)
    }

    /// Create a new board with an optional explicit slug.
    /// If slug is None, one will be generated from the board name.
    /// Returns the assigned board ID.
    pub fn create_board_with_slug(
        env: Env,
        name: String,
        description: String,
        is_private: String,
        is_listed: String,
        slug: Option<String>,
        caller: Address,
    ) -> u64 {
        caller.require_auth();

        // Get required contract addresses
        let registry: Address = env
            .storage()
            .instance()
            .get(&BoardKey::Registry)
            .expect("Contract not initialized");

        let permissions: Option<Address> = env.storage().instance().get(&BoardKey::Permissions);

        // Parse is_private string to bool
        let is_private_bool = is_private == String::from_str(&env, "true");

        // Parse is_listed string to bool (default to listed)
        let is_listed_bool = is_listed.len() == 0 || is_listed != String::from_str(&env, "false");

        // Check if caller is site admin (bypass all threshold checks)
        let is_site_admin = if let Some(ref perms) = permissions {
            let args: Vec<Val> = Vec::from_array(&env, [caller.clone().into_val(&env)]);
            env.try_invoke_contract::<bool, soroban_sdk::Error>(
                perms,
                &Symbol::new(&env, "is_site_admin"),
                args,
            )
            .ok()
            .and_then(|r| r.ok())
            .unwrap_or(false)
        } else {
            false
        };

        // If not site admin, check creation thresholds
        if !is_site_admin {
            // Get config contract from registry
            let config_alias = Symbol::new(&env, "config");
            let config_args: Vec<Val> = Vec::from_array(&env, [config_alias.into_val(&env)]);
            let config: Option<Address> = env
                .try_invoke_contract::<Option<Address>, soroban_sdk::Error>(
                    &registry,
                    &Symbol::new(&env, "get_contract_by_alias"),
                    config_args,
                )
                .ok()
                .and_then(|r| r.ok())
                .flatten();

            if let Some(config_addr) = config {
                // Get user's board count (local tracking)
                let user_board_count: u32 = env
                    .storage()
                    .persistent()
                    .get(&BoardKey::UserBoardCount(caller.clone()))
                    .unwrap_or(0);

                // Get user's account age from permissions
                let user_account_age = if let Some(ref perms) = permissions {
                    let age_args: Vec<Val> =
                        Vec::from_array(&env, [caller.clone().into_val(&env)]);
                    env.try_invoke_contract::<u64, soroban_sdk::Error>(
                        perms,
                        &Symbol::new(&env, "get_account_age"),
                        age_args,
                    )
                    .ok()
                    .and_then(|r| r.ok())
                    .unwrap_or(0)
                } else {
                    0
                };

                // Check thresholds via config contract
                let check_args: Vec<Val> = Vec::from_array(
                    &env,
                    [
                        0u32.into_val(&env),           // CreationType::Board = 0
                        caller.clone().into_val(&env), // user
                        user_board_count.into_val(&env),
                        0i64.into_val(&env),  // user_karma (TODO)
                        user_account_age.into_val(&env),
                        0u32.into_val(&env),  // user_post_count (TODO)
                        false.into_val(&env), // has_profile (TODO)
                    ],
                );

                let result: (bool, String) = env.invoke_contract(
                    &config_addr,
                    &Symbol::new(&env, "check_thresholds"),
                    check_args,
                );

                if !result.0 {
                    let mut buf = [0u8; 64];
                    let len = core::cmp::min(result.1.len() as usize, 64);
                    result.1.copy_into_slice(&mut buf[..len]);
                    let reason =
                        core::str::from_utf8(&buf[..len]).unwrap_or("Threshold check failed");
                    panic!("{}", reason);
                }
            }
        }

        // Get next board ID from counter
        let board_id: u64 = env
            .storage()
            .instance()
            .get(&BoardKey::BoardCount)
            .unwrap_or(0);

        // Generate or validate slug
        let final_slug = if let Some(explicit_slug) = slug {
            // Validate explicit slug
            Self::validate_slug(&env, &explicit_slug);
            // Check uniqueness and modify if needed
            Self::generate_unique_slug(&env, &explicit_slug)
        } else {
            // Generate slug from board name
            let base_slug = Self::generate_slug_from_name(&env, &name);
            Self::generate_unique_slug(&env, &base_slug)
        };

        // Create and store BoardMeta
        let board_meta = BoardMeta {
            id: board_id,
            slug: final_slug.clone(),
            name: name.clone(),
            description: description.clone(),
            creator: caller.clone(),
            created_at: env.ledger().timestamp(),
            thread_count: 0,
            is_readonly: false,
            is_private: is_private_bool,
            is_listed: is_listed_bool,
        };
        env.storage()
            .persistent()
            .set(&BoardKey::Board(board_id), &board_meta);

        // Store slug -> board_id index (for standalone boards)
        env.storage()
            .persistent()
            .set(&BoardKey::BoardBySlug(final_slug), &board_id);

        // Create and store BoardConfig
        let config = BoardConfig {
            name,
            description,
            is_private: is_private_bool,
            is_readonly: false,
            max_reply_depth: 10,
            reply_chunk_size: 6,
        };
        env.storage()
            .persistent()
            .set(&BoardKey::BoardConfig(board_id), &config);

        // Initialize per-board storage with namespaced keys
        env.storage()
            .persistent()
            .set(&BoardKey::BoardThreadCount(board_id), &0u64);
        env.storage()
            .persistent()
            .set(&BoardKey::BoardPinnedThreads(board_id), &Vec::<u64>::new(&env));
        env.storage()
            .persistent()
            .set(&BoardKey::BoardEditWindow(board_id), &86400u64);
        env.storage()
            .persistent()
            .set(&BoardKey::BoardListed(board_id), &is_listed_bool);
        env.storage()
            .persistent()
            .set(&BoardKey::BoardCreator(board_id), &caller.clone());
        env.storage()
            .persistent()
            .set(&BoardKey::BoardCreatedAt(board_id), &env.ledger().timestamp());

        // Increment board count
        env.storage()
            .instance()
            .set(&BoardKey::BoardCount, &(board_id + 1));

        // Increment user's board count (local tracking)
        let user_count: u32 = env
            .storage()
            .persistent()
            .get(&BoardKey::UserBoardCount(caller.clone()))
            .unwrap_or(0);
        env.storage()
            .persistent()
            .set(&BoardKey::UserBoardCount(caller.clone()), &(user_count + 1));

        // Set caller as board owner in permissions
        if let Some(ref perms) = permissions {
            let owner_args: Vec<Val> = Vec::from_array(
                &env,
                [board_id.into_val(&env), caller.into_val(&env)],
            );
            let _ = env.try_invoke_contract::<(), soroban_sdk::Error>(
                perms,
                &Symbol::new(&env, "set_board_owner"),
                owner_args,
            );
        }

        board_id
    }

    // =========================================================================
    // Board Query Functions
    // =========================================================================

    /// Get board metadata by ID
    pub fn get_board(env: Env, board_id: u64) -> Option<BoardMeta> {
        env.storage()
            .persistent()
            .get(&BoardKey::Board(board_id))
    }

    /// Get board ID by slug (for standalone boards).
    /// Returns None if slug not found.
    pub fn get_board_id_by_slug(env: Env, slug: String) -> Option<u64> {
        env.storage()
            .persistent()
            .get(&BoardKey::BoardBySlug(slug))
    }

    /// Get board metadata by slug (for standalone boards).
    /// Returns None if slug not found.
    pub fn get_board_by_slug(env: Env, slug: String) -> Option<BoardMeta> {
        if let Some(board_id) = Self::get_board_id_by_slug(env.clone(), slug) {
            Self::get_board(env, board_id)
        } else {
            None
        }
    }

    /// Get board slug by ID.
    /// Returns None if board not found.
    pub fn get_board_slug(env: Env, board_id: u64) -> Option<String> {
        Self::get_board(env, board_id).map(|meta| meta.slug)
    }

    /// Update board slug (for use by community contract during add/remove).
    /// Only callable by registry or community contracts.
    pub fn update_board_slug(env: Env, board_id: u64, new_slug: String, caller: Address) {
        caller.require_auth();

        // Verify caller is registry or community contract
        let registry: Option<Address> = env.storage().instance().get(&BoardKey::Registry);
        let is_registry = registry.as_ref().map(|r| r == &caller).unwrap_or(false);

        // Also allow community contract (get from registry)
        let is_community = if let Some(ref reg) = registry {
            let community_alias = Symbol::new(&env, "community");
            let args: Vec<Val> = Vec::from_array(&env, [community_alias.into_val(&env)]);
            let community: Option<Address> = env
                .try_invoke_contract::<Option<Address>, soroban_sdk::Error>(
                    reg,
                    &Symbol::new(&env, "get_contract_by_alias"),
                    args,
                )
                .ok()
                .and_then(|r| r.ok())
                .flatten();
            community.map(|c| c == caller).unwrap_or(false)
        } else {
            false
        };

        if !is_registry && !is_community {
            panic!("Only registry or community contract can update board slug");
        }

        // Validate new slug
        Self::validate_slug(&env, &new_slug);

        // Get current board and its old slug
        let mut board: BoardMeta = env
            .storage()
            .persistent()
            .get(&BoardKey::Board(board_id))
            .expect("Board not found");

        let old_slug = board.slug.clone();

        // Remove old slug index
        env.storage()
            .persistent()
            .remove(&BoardKey::BoardBySlug(old_slug));

        // Update board with new slug
        board.slug = new_slug.clone();
        env.storage()
            .persistent()
            .set(&BoardKey::Board(board_id), &board);

        // Add new slug index
        env.storage()
            .persistent()
            .set(&BoardKey::BoardBySlug(new_slug), &board_id);
    }

    /// Remove board from standalone slug index (called when board joins a community).
    pub fn remove_standalone_slug_index(env: Env, board_id: u64, caller: Address) {
        caller.require_auth();

        // Verify caller is community contract (via registry lookup)
        let registry: Address = env
            .storage()
            .instance()
            .get(&BoardKey::Registry)
            .expect("Contract not initialized");

        let community_alias = Symbol::new(&env, "community");
        let args: Vec<Val> = Vec::from_array(&env, [community_alias.into_val(&env)]);
        let community: Option<Address> = env
            .try_invoke_contract::<Option<Address>, soroban_sdk::Error>(
                &registry,
                &Symbol::new(&env, "get_contract_by_alias"),
                args,
            )
            .ok()
            .and_then(|r| r.ok())
            .flatten();

        if community.map(|c| c != caller).unwrap_or(true) {
            panic!("Only community contract can remove standalone slug index");
        }

        // Get board slug and remove from index
        if let Some(board) = Self::get_board(env.clone(), board_id) {
            env.storage()
                .persistent()
                .remove(&BoardKey::BoardBySlug(board.slug));
        }
    }

    /// Add board to standalone slug index (called when board leaves a community).
    /// Checks for conflicts and updates slug if needed.
    pub fn add_standalone_slug_index(env: Env, board_id: u64, caller: Address) {
        caller.require_auth();

        // Verify caller is community contract (via registry lookup)
        let registry: Address = env
            .storage()
            .instance()
            .get(&BoardKey::Registry)
            .expect("Contract not initialized");

        let community_alias = Symbol::new(&env, "community");
        let args: Vec<Val> = Vec::from_array(&env, [community_alias.into_val(&env)]);
        let community: Option<Address> = env
            .try_invoke_contract::<Option<Address>, soroban_sdk::Error>(
                &registry,
                &Symbol::new(&env, "get_contract_by_alias"),
                args,
            )
            .ok()
            .and_then(|r| r.ok())
            .flatten();

        if community.map(|c| c != caller).unwrap_or(true) {
            panic!("Only community contract can add standalone slug index");
        }

        // Get board and check for slug conflicts
        let mut board: BoardMeta = env
            .storage()
            .persistent()
            .get(&BoardKey::Board(board_id))
            .expect("Board not found");

        // Generate unique slug (handles conflicts)
        let unique_slug = Self::generate_unique_slug(&env, &board.slug);

        // Update board if slug changed
        if unique_slug != board.slug {
            board.slug = unique_slug.clone();
            env.storage()
                .persistent()
                .set(&BoardKey::Board(board_id), &board);
        }

        // Add to standalone index
        env.storage()
            .persistent()
            .set(&BoardKey::BoardBySlug(unique_slug), &board_id);
    }

    /// List boards with pagination
    pub fn list_boards(env: Env, start: u64, limit: u64) -> Vec<BoardMeta> {
        let count: u64 = env
            .storage()
            .instance()
            .get(&BoardKey::BoardCount)
            .unwrap_or(0);

        let mut boards = Vec::new(&env);
        let end = core::cmp::min(start + limit, count);

        for i in start..end {
            if let Some(board) = env.storage().persistent().get(&BoardKey::Board(i)) {
                boards.push_back(board);
            }
        }

        boards
    }

    /// List only publicly listed boards with pagination
    pub fn list_listed_boards(env: Env, start: u64, limit: u64) -> Vec<BoardMeta> {
        let count: u64 = env
            .storage()
            .instance()
            .get(&BoardKey::BoardCount)
            .unwrap_or(0);

        let mut boards = Vec::new(&env);
        let mut found = 0u64;
        let mut skipped = 0u64;

        for i in 0..count {
            if let Some(board) = env.storage().persistent().get::<_, BoardMeta>(&BoardKey::Board(i)) {
                if board.is_listed {
                    if skipped < start {
                        skipped += 1;
                    } else if found < limit {
                        boards.push_back(board);
                        found += 1;
                    } else {
                        break;
                    }
                }
            }
        }

        boards
    }

    /// Get total board count
    pub fn board_count(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&BoardKey::BoardCount)
            .unwrap_or(0)
    }

    /// Count boards created by a user
    pub fn count_user_boards(env: Env, user: Address) -> u32 {
        env.storage()
            .persistent()
            .get(&BoardKey::UserBoardCount(user))
            .unwrap_or(0)
    }

    // =========================================================================
    // Flair Management Functions
    // =========================================================================

    /// Create a new flair (Admin+ only)
    pub fn create_flair(
        env: Env,
        board_id: u64,
        name: String,
        color: String,
        bg_color: String,
        required: bool,
        mod_only: bool,
        caller: Address,
    ) -> u32 {
        caller.require_auth();

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
            .persistent()
            .get(&BoardKey::BoardNextFlairId(board_id))
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
            .get(&BoardKey::BoardFlairDefs(board_id))
            .unwrap_or(Vec::new(&env));

        flairs.push_back(flair);
        env.storage().persistent().set(&BoardKey::BoardFlairDefs(board_id), &flairs);
        env.storage().persistent().set(&BoardKey::BoardNextFlairId(board_id), &(flair_id + 1));

        flair_id
    }

    /// Update an existing flair (Admin+ only)
    pub fn update_flair(env: Env, board_id: u64, flair_id: u32, flair: FlairDef, caller: Address) {
        caller.require_auth();

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
            .get(&BoardKey::BoardFlairDefs(board_id))
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

        env.storage().persistent().set(&BoardKey::BoardFlairDefs(board_id), &new_flairs);
    }

    /// Disable a flair (Admin+ only)
    pub fn disable_flair(env: Env, board_id: u64, flair_id: u32, caller: Address) {
        caller.require_auth();

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
            .get(&BoardKey::BoardFlairDefs(board_id))
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

        env.storage().persistent().set(&BoardKey::BoardFlairDefs(board_id), &new_flairs);
    }

    /// List all flairs for a board
    pub fn list_flairs(env: Env, board_id: u64) -> Vec<FlairDef> {
        env.storage()
            .persistent()
            .get(&BoardKey::BoardFlairDefs(board_id))
            .unwrap_or(Vec::new(&env))
    }

    /// Get a specific flair by ID
    pub fn get_flair(env: Env, board_id: u64, flair_id: u32) -> Option<FlairDef> {
        let flairs: Vec<FlairDef> = env
            .storage()
            .persistent()
            .get(&BoardKey::BoardFlairDefs(board_id))
            .unwrap_or(Vec::new(&env));

        for i in 0..flairs.len() {
            let flair = flairs.get(i).unwrap();
            if flair.id == flair_id {
                return Some(flair);
            }
        }
        None
    }

    /// Check if any flair is required for new posts on this board
    /// Returns true if at least one enabled, non-mod-only flair is marked as required
    pub fn is_flair_required(env: Env, board_id: u64) -> bool {
        let flairs: Vec<FlairDef> = env
            .storage()
            .persistent()
            .get(&BoardKey::BoardFlairDefs(board_id))
            .unwrap_or(Vec::new(&env));

        for i in 0..flairs.len() {
            let flair = flairs.get(i).unwrap();
            // Consider flair required if it's enabled, required, and not mod-only
            // (mod-only flairs can't be required for regular users)
            if flair.required && flair.enabled && !flair.mod_only {
                return true;
            }
        }
        false
    }

    /// Set thread flair (Moderator+ for mod_only flairs, thread creator for others)
    pub fn set_thread_flair(env: Env, board_id: u64, thread_id: u64, flair_id: Option<u32>, caller: Address) {
        caller.require_auth();

        // Get thread
        let mut thread: ThreadMeta = env
            .storage()
            .persistent()
            .get(&BoardKey::BoardThread(board_id, thread_id))
            .expect("Thread not found");

        // Check if flair exists and get its properties
        if let Some(fid) = flair_id {
            let flairs: Vec<FlairDef> = env
                .storage()
                .persistent()
                .get(&BoardKey::BoardFlairDefs(board_id))
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
            .set(&BoardKey::BoardThread(board_id, thread_id), &thread);
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

    /// Get board configuration
    pub fn get_config(env: Env, board_id: u64) -> BoardConfig {
        env.storage()
            .persistent()
            .get(&BoardKey::BoardConfig(board_id))
            .expect("Board not found")
    }

    /// Get whether board is listed publicly (for home page)
    pub fn get_board_listed(env: Env, board_id: u64) -> bool {
        env.storage()
            .persistent()
            .get(&BoardKey::BoardListed(board_id))
            .unwrap_or(true) // Default to listed for backward compatibility
    }

    /// Set whether board is listed publicly (owner/admin only)
    pub fn set_board_listed(env: Env, board_id: u64, is_listed: bool, caller: Address) {
        caller.require_auth();

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
                panic!("Only owner or admin can change listed status");
            }
        }

        env.storage().persistent().set(&BoardKey::BoardListed(board_id), &is_listed);

        // Update BoardMeta too
        if let Some(mut meta) = env.storage().persistent().get::<_, BoardMeta>(&BoardKey::Board(board_id)) {
            meta.is_listed = is_listed;
            env.storage().persistent().set(&BoardKey::Board(board_id), &meta);
        }
    }

    /// Get board creator address
    pub fn get_creator(env: Env, board_id: u64) -> Option<Address> {
        env.storage().persistent().get(&BoardKey::BoardCreator(board_id))
    }

    /// Get board creation timestamp
    pub fn get_created_at(env: Env, board_id: u64) -> u64 {
        env.storage()
            .persistent()
            .get(&BoardKey::BoardCreatedAt(board_id))
            .unwrap_or(0) // Default to 0 for legacy boards
    }

    /// Increment thread count (called by content contract when creating threads)
    /// This is separate from create_thread to allow external contracts to track thread counts
    pub fn increment_thread_count(env: Env, board_id: u64) -> u64 {
        let count: u64 = env
            .storage()
            .persistent()
            .get(&BoardKey::BoardThreadCount(board_id))
            .unwrap_or(0);
        let new_count = count + 1;
        env.storage().persistent().set(&BoardKey::BoardThreadCount(board_id), &new_count);

        // Update BoardMeta too
        if let Some(mut meta) = env.storage().persistent().get::<_, BoardMeta>(&BoardKey::Board(board_id)) {
            meta.thread_count = new_count;
            env.storage().persistent().set(&BoardKey::Board(board_id), &meta);
        }

        new_count
    }

    /// Set thread count (admin function for correcting counts)
    pub fn set_thread_count(env: Env, board_id: u64, count: u64, caller: Address) {
        caller.require_auth();

        let meta: BoardMeta = env
            .storage()
            .persistent()
            .get(&BoardKey::Board(board_id))
            .expect("Board not found");

        // Allow board creator or registry admin to set thread count
        let is_registry_admin = if let Some(registry) = env.storage().instance().get::<_, Address>(&BoardKey::Registry) {
            let args: Vec<Val> = Vec::from_array(&env, [caller.clone().into_val(&env)]);
            env.invoke_contract::<bool>(&registry, &Symbol::new(&env, "is_admin"), args)
        } else {
            false
        };

        if meta.creator != caller && !is_registry_admin {
            panic!("Only board creator or registry admin can set thread count");
        }

        env.storage().persistent().set(&BoardKey::BoardThreadCount(board_id), &count);

        let mut updated_meta = meta;
        updated_meta.thread_count = count;
        env.storage().persistent().set(&BoardKey::Board(board_id), &updated_meta);
    }

    /// Get reply chunk size for waterfall loading
    pub fn get_chunk_size(env: Env, board_id: u64) -> u32 {
        let config: BoardConfig = env
            .storage()
            .persistent()
            .get(&BoardKey::BoardConfig(board_id))
            .expect("Board not found");
        // Handle old configs that don't have this field (default to 6)
        if config.reply_chunk_size == 0 {
            6
        } else {
            config.reply_chunk_size
        }
    }

    /// Set reply chunk size (must be >= 1, owner/admin only)
    pub fn set_chunk_size(env: Env, board_id: u64, size: u32, caller: Address) {
        caller.require_auth();

        if size < 1 {
            panic!("Chunk size must be at least 1");
        }

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
            .persistent()
            .get(&BoardKey::BoardConfig(board_id))
            .expect("Board not found");
        config.reply_chunk_size = size;
        env.storage().persistent().set(&BoardKey::BoardConfig(board_id), &config);
    }

    /// Get maximum reply depth for nested replies
    pub fn get_max_reply_depth(env: Env, board_id: u64) -> u32 {
        let config: BoardConfig = env
            .storage()
            .persistent()
            .get(&BoardKey::BoardConfig(board_id))
            .expect("Board not found");
        config.max_reply_depth
    }

    /// Set maximum reply depth (must be >= 1, owner/admin only)
    pub fn set_max_reply_depth(env: Env, board_id: u64, depth: u32, caller: Address) {
        caller.require_auth();

        if depth < 1 {
            panic!("Max reply depth must be at least 1");
        }

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
            .persistent()
            .get(&BoardKey::BoardConfig(board_id))
            .expect("Board not found");
        config.max_reply_depth = depth;
        env.storage().persistent().set(&BoardKey::BoardConfig(board_id), &config);
    }

    /// Get edit window in seconds (0 = no limit)
    pub fn get_edit_window(env: Env, board_id: u64) -> u64 {
        // Default to 86400 (24 hours) if not set
        env.storage()
            .persistent()
            .get(&BoardKey::BoardEditWindow(board_id))
            .unwrap_or(86400u64)
    }

    /// Set edit window in seconds (0 = no limit, owner/admin only)
    pub fn set_edit_window(env: Env, board_id: u64, seconds: u64, caller: Address) {
        caller.require_auth();

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

        env.storage().persistent().set(&BoardKey::BoardEditWindow(board_id), &seconds);
    }

    /// Check if board is read-only
    pub fn is_readonly(env: Env, board_id: u64) -> bool {
        env.storage()
            .persistent()
            .get::<_, BoardConfig>(&BoardKey::BoardConfig(board_id))
            .map(|c| c.is_readonly)
            .unwrap_or(false)
    }

    /// Set board read-only status (owner/admin only)
    pub fn set_readonly(env: Env, board_id: u64, is_readonly: bool, caller: Address) {
        caller.require_auth();

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
            .persistent()
            .get(&BoardKey::BoardConfig(board_id))
            .expect("Board not found");
        config.is_readonly = is_readonly;
        env.storage().persistent().set(&BoardKey::BoardConfig(board_id), &config);

        // Update BoardMeta too
        if let Some(mut meta) = env.storage().persistent().get::<_, BoardMeta>(&BoardKey::Board(board_id)) {
            meta.is_readonly = is_readonly;
            env.storage().persistent().set(&BoardKey::Board(board_id), &meta);
        }
    }

    /// Check if board is private (members only)
    pub fn is_private(env: Env, board_id: u64) -> bool {
        env.storage()
            .persistent()
            .get::<_, BoardConfig>(&BoardKey::BoardConfig(board_id))
            .map(|c| c.is_private)
            .unwrap_or(false)
    }

    /// Set board private status (owner/admin only)
    pub fn set_private(env: Env, board_id: u64, is_private: bool, caller: Address) {
        caller.require_auth();

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
                panic!("Only owner or admin can change private status");
            }
        }

        let mut config: BoardConfig = env
            .storage()
            .persistent()
            .get(&BoardKey::BoardConfig(board_id))
            .expect("Board not found");
        config.is_private = is_private;
        env.storage().persistent().set(&BoardKey::BoardConfig(board_id), &config);

        // Update BoardMeta too
        if let Some(mut meta) = env.storage().persistent().get::<_, BoardMeta>(&BoardKey::Board(board_id)) {
            meta.is_private = is_private;
            env.storage().persistent().set(&BoardKey::Board(board_id), &meta);
        }
    }

    /// Set board rules (markdown text, Admin+ only)
    /// Rules are displayed on the board page and when creating new posts
    pub fn set_rules(env: Env, board_id: u64, rules: String, caller: Address) {
        caller.require_auth();

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

        env.storage().persistent().set(&BoardKey::BoardRules(board_id), &rules);
    }

    /// Get board rules (returns None if no rules are set)
    pub fn get_rules(env: Env, board_id: u64) -> Option<String> {
        env.storage().persistent().get(&BoardKey::BoardRules(board_id))
    }

    /// Clear board rules (Admin+ only)
    pub fn clear_rules(env: Env, board_id: u64, caller: Address) {
        caller.require_auth();

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

        env.storage().persistent().remove(&BoardKey::BoardRules(board_id));
    }

    /// Check if content is within the edit window
    /// Returns true if content can be edited (within window or no limit)
    fn is_within_edit_window(env: &Env, board_id: u64, created_at: u64) -> bool {
        let edit_window_seconds: u64 = env
            .storage()
            .persistent()
            .get(&BoardKey::BoardEditWindow(board_id))
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
    pub fn create_thread(env: Env, board_id: u64, title: String, flair_id: Option<String>, creator: Address) -> u64 {
        // Note: require_auth() removed because this is called by the theme contract,
        // which already handles authentication. Cross-contract auth doesn't propagate
        // automatically in Soroban.

        // Check permissions (only if permissions contract is set)
        if env.storage().instance().has(&BoardKey::Permissions) {
            Self::check_can_create_thread(&env, board_id, &creator);
        }

        // Check if board is readonly
        let config: BoardConfig = env
            .storage()
            .persistent()
            .get(&BoardKey::BoardConfig(board_id))
            .expect("Board not found");
        if config.is_readonly {
            panic!("Board is read-only");
        }

        // Parse flair_id from string: "none" or empty → None, "flair_N" → Some(N)
        let parsed_flair_id: Option<u32> = if let Some(ref flair_str) = flair_id {
            let none_str = String::from_str(&env, "none");
            if flair_str.len() == 0 || flair_str == &none_str {
                None
            } else {
                // Expect format "flair_N" - parse N as u32
                let mut buf = [0u8; 32];
                let len = core::cmp::min(flair_str.len() as usize, 32);
                flair_str.copy_into_slice(&mut buf[..len]);

                // Check for "flair_" prefix (6 chars)
                if len > 6 && buf[0] == b'f' && buf[1] == b'l' && buf[2] == b'a'
                    && buf[3] == b'i' && buf[4] == b'r' && buf[5] == b'_' {
                    // Parse the number after "flair_"
                    let mut result: u32 = 0;
                    for i in 6..len {
                        let b = buf[i];
                        if b >= b'0' && b <= b'9' {
                            result = result * 10 + (b - b'0') as u32;
                        } else {
                            panic!("Invalid flair_id format");
                        }
                    }
                    Some(result)
                } else {
                    panic!("Invalid flair_id: expected 'none' or 'flair_N' format");
                }
            }
        } else {
            None
        };

        // Get flairs for this board
        let flairs: Vec<FlairDef> = env
            .storage()
            .persistent()
            .get(&BoardKey::BoardFlairDefs(board_id))
            .unwrap_or(Vec::new(&env));

        // Check if user is moderator (needed for mod_only flair access)
        let is_moderator = if env.storage().instance().has(&BoardKey::Permissions) {
            let permissions: Address = env
                .storage()
                .instance()
                .get(&BoardKey::Permissions)
                .expect("Not initialized");
            let args: Vec<Val> = Vec::from_array(
                &env,
                [board_id.into_val(&env), creator.clone().into_val(&env)],
            );
            env.try_invoke_contract::<bool, soroban_sdk::Error>(
                &permissions,
                &Symbol::new(&env, "can_moderate"),
                args,
            )
            .unwrap_or(Ok(false))
            .unwrap_or(false)
        } else {
            false
        };

        // Check if any flair is required (only consider flairs the user can actually select)
        let mut has_required_flair = false;
        for i in 0..flairs.len() {
            let flair = flairs.get(i).unwrap();
            if flair.required && flair.enabled && (!flair.mod_only || is_moderator) {
                has_required_flair = true;
                break;
            }
        }

        // Validate flair selection
        let validated_flair_id = if let Some(fid) = parsed_flair_id {
            // User selected a flair - validate it exists and is usable
            let mut flair_valid = false;
            let mut flair_mod_only = false;
            for i in 0..flairs.len() {
                let flair = flairs.get(i).unwrap();
                if flair.id == fid {
                    if !flair.enabled {
                        panic!("Selected flair is disabled");
                    }
                    flair_valid = true;
                    flair_mod_only = flair.mod_only;
                    break;
                }
            }
            if !flair_valid {
                panic!("Selected flair does not exist");
            }
            if flair_mod_only && !is_moderator {
                panic!("Selected flair is moderator-only");
            }
            Some(fid)
        } else if has_required_flair {
            // No flair selected but one is required
            panic!("A flair is required for new posts on this board");
        } else {
            None
        };

        let thread_id: u64 = env
            .storage()
            .persistent()
            .get(&BoardKey::BoardThreadCount(board_id))
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
            flair_id: validated_flair_id,
        };

        env.storage()
            .persistent()
            .set(&BoardKey::BoardThread(board_id, thread_id), &thread);
        env.storage()
            .persistent()
            .set(&BoardKey::BoardThreadCount(board_id), &(thread_id + 1));

        // Update BoardMeta thread count
        if let Some(mut meta) = env.storage().persistent().get::<_, BoardMeta>(&BoardKey::Board(board_id)) {
            meta.thread_count = thread_id + 1;
            env.storage().persistent().set(&BoardKey::Board(board_id), &meta);
        }

        thread_id
    }

    /// Get thread metadata
    pub fn get_thread(env: Env, board_id: u64, thread_id: u64) -> Option<ThreadMeta> {
        env.storage()
            .persistent()
            .get(&BoardKey::BoardThread(board_id, thread_id))
    }

    /// Get thread title and author (for crossposting)
    pub fn get_thread_title_and_author(env: Env, board_id: u64, thread_id: u64) -> Option<(String, Address)> {
        let thread: Option<ThreadMeta> = env.storage().persistent().get(&BoardKey::BoardThread(board_id, thread_id));
        thread.map(|t| (t.title, t.creator))
    }

    /// List threads with pagination
    pub fn list_threads(env: Env, board_id: u64, start: u64, limit: u64) -> Vec<ThreadMeta> {
        let count: u64 = env
            .storage()
            .persistent()
            .get(&BoardKey::BoardThreadCount(board_id))
            .unwrap_or(0);

        let mut threads = Vec::new(&env);

        // Return newest first (reverse order)
        let actual_start = if count > start { count - start - 1 } else { 0 };

        for i in 0..limit {
            if actual_start < i {
                break;
            }
            let idx = actual_start - i;
            if let Some(thread) = env.storage().persistent().get(&BoardKey::BoardThread(board_id, idx)) {
                threads.push_back(thread);
            }
        }

        threads
    }

    /// Get thread count for a board
    pub fn thread_count(env: Env, board_id: u64) -> u64 {
        env.storage()
            .persistent()
            .get(&BoardKey::BoardThreadCount(board_id))
            .unwrap_or(0)
    }

    /// Lock a thread (no more replies)
    pub fn lock_thread(env: Env, board_id: u64, thread_id: u64, caller: Address) {
        caller.require_auth();

        // Check moderator permissions (only if permissions contract is set)
        if env.storage().instance().has(&BoardKey::Permissions) {
            Self::check_can_moderate(&env, board_id, &caller);
        }

        if let Some(mut thread) = env
            .storage()
            .persistent()
            .get::<_, ThreadMeta>(&BoardKey::BoardThread(board_id, thread_id))
        {
            thread.is_locked = true;
            env.storage()
                .persistent()
                .set(&BoardKey::BoardThread(board_id, thread_id), &thread);
        }
    }

    /// Unlock a thread
    pub fn unlock_thread(env: Env, board_id: u64, thread_id: u64, caller: Address) {
        caller.require_auth();

        // Check moderator permissions (only if permissions contract is set)
        if env.storage().instance().has(&BoardKey::Permissions) {
            Self::check_can_moderate(&env, board_id, &caller);
        }

        if let Some(mut thread) = env
            .storage()
            .persistent()
            .get::<_, ThreadMeta>(&BoardKey::BoardThread(board_id, thread_id))
        {
            thread.is_locked = false;
            env.storage()
                .persistent()
                .set(&BoardKey::BoardThread(board_id, thread_id), &thread);
        }
    }

    /// Pin a thread
    pub fn pin_thread(env: Env, board_id: u64, thread_id: u64, caller: Address) {
        caller.require_auth();

        // Check moderator permissions (only if permissions contract is set)
        if env.storage().instance().has(&BoardKey::Permissions) {
            Self::check_can_moderate(&env, board_id, &caller);
        }

        if let Some(mut thread) = env
            .storage()
            .persistent()
            .get::<_, ThreadMeta>(&BoardKey::BoardThread(board_id, thread_id))
        {
            thread.is_pinned = true;
            env.storage()
                .persistent()
                .set(&BoardKey::BoardThread(board_id, thread_id), &thread);

            let mut pinned: Vec<u64> = env
                .storage()
                .persistent()
                .get(&BoardKey::BoardPinnedThreads(board_id))
                .unwrap_or(Vec::new(&env));
            pinned.push_back(thread_id);
            env.storage()
                .persistent()
                .set(&BoardKey::BoardPinnedThreads(board_id), &pinned);
        }
    }

    /// Unpin a thread
    pub fn unpin_thread(env: Env, board_id: u64, thread_id: u64, caller: Address) {
        caller.require_auth();

        // Check moderator permissions (only if permissions contract is set)
        if env.storage().instance().has(&BoardKey::Permissions) {
            Self::check_can_moderate(&env, board_id, &caller);
        }

        if let Some(mut thread) = env
            .storage()
            .persistent()
            .get::<_, ThreadMeta>(&BoardKey::BoardThread(board_id, thread_id))
        {
            thread.is_pinned = false;
            env.storage()
                .persistent()
                .set(&BoardKey::BoardThread(board_id, thread_id), &thread);

            // Remove from pinned list
            let pinned: Vec<u64> = env
                .storage()
                .persistent()
                .get(&BoardKey::BoardPinnedThreads(board_id))
                .unwrap_or(Vec::new(&env));
            let mut new_pinned = Vec::new(&env);
            for id in pinned.iter() {
                if id != thread_id {
                    new_pinned.push_back(id);
                }
            }
            env.storage()
                .persistent()
                .set(&BoardKey::BoardPinnedThreads(board_id), &new_pinned);
        }
    }

    /// Get pinned threads for a board
    pub fn get_pinned_threads(env: Env, board_id: u64) -> Vec<ThreadMeta> {
        let pinned_ids: Vec<u64> = env
            .storage()
            .persistent()
            .get(&BoardKey::BoardPinnedThreads(board_id))
            .unwrap_or(Vec::new(&env));

        let mut threads = Vec::new(&env);
        for id in pinned_ids.iter() {
            if let Some(thread) = env.storage().persistent().get(&BoardKey::BoardThread(board_id, id)) {
                threads.push_back(thread);
            }
        }
        threads
    }

    /// Hide a thread (moderator action)
    pub fn hide_thread(env: Env, board_id: u64, thread_id: u64, caller: Address) {
        caller.require_auth();

        // Check moderator permissions (only if permissions contract is set)
        if env.storage().instance().has(&BoardKey::Permissions) {
            Self::check_can_moderate(&env, board_id, &caller);
        }

        if let Some(mut thread) = env
            .storage()
            .persistent()
            .get::<_, ThreadMeta>(&BoardKey::BoardThread(board_id, thread_id))
        {
            thread.is_hidden = true;
            env.storage()
                .persistent()
                .set(&BoardKey::BoardThread(board_id, thread_id), &thread);
        }
    }

    /// Unhide a thread (moderator action)
    pub fn unhide_thread(env: Env, board_id: u64, thread_id: u64, caller: Address) {
        caller.require_auth();

        // Check moderator permissions (only if permissions contract is set)
        if env.storage().instance().has(&BoardKey::Permissions) {
            Self::check_can_moderate(&env, board_id, &caller);
        }

        if let Some(mut thread) = env
            .storage()
            .persistent()
            .get::<_, ThreadMeta>(&BoardKey::BoardThread(board_id, thread_id))
        {
            thread.is_hidden = false;
            env.storage()
                .persistent()
                .set(&BoardKey::BoardThread(board_id, thread_id), &thread);
        }
    }

    /// Set thread hidden state (called by admin contract)
    pub fn set_thread_hidden(env: Env, board_id: u64, thread_id: u64, hidden: bool) {
        // Note: Auth is handled by the calling admin contract
        if let Some(mut thread) = env
            .storage()
            .persistent()
            .get::<_, ThreadMeta>(&BoardKey::BoardThread(board_id, thread_id))
        {
            thread.is_hidden = hidden;
            thread.updated_at = env.ledger().timestamp();
            env.storage()
                .persistent()
                .set(&BoardKey::BoardThread(board_id, thread_id), &thread);
        }
    }

    /// Set thread locked state (called by admin contract)
    pub fn set_thread_locked(env: Env, board_id: u64, thread_id: u64, locked: bool) {
        // Note: Auth is handled by the calling admin contract
        if let Some(mut thread) = env
            .storage()
            .persistent()
            .get::<_, ThreadMeta>(&BoardKey::BoardThread(board_id, thread_id))
        {
            thread.is_locked = locked;
            thread.updated_at = env.ledger().timestamp();
            env.storage()
                .persistent()
                .set(&BoardKey::BoardThread(board_id, thread_id), &thread);
        }
    }

    /// Set thread pinned state (called by admin contract)
    pub fn set_thread_pinned(env: Env, board_id: u64, thread_id: u64, pinned: bool) {
        // Note: Auth is handled by the calling admin contract
        if let Some(mut thread) = env
            .storage()
            .persistent()
            .get::<_, ThreadMeta>(&BoardKey::BoardThread(board_id, thread_id))
        {
            thread.is_pinned = pinned;
            thread.updated_at = env.ledger().timestamp();

            // Update pinned list
            let mut pinned_list: Vec<u64> = env
                .storage()
                .persistent()
                .get(&BoardKey::BoardPinnedThreads(board_id))
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
                .persistent()
                .set(&BoardKey::BoardPinnedThreads(board_id), &pinned_list);
            env.storage()
                .persistent()
                .set(&BoardKey::BoardThread(board_id, thread_id), &thread);
        }
    }

    /// Delete a thread (soft delete - sets is_deleted flag)
    /// Only author or moderator+ can delete
    pub fn delete_thread(env: Env, board_id: u64, thread_id: u64, caller: Address) {
        caller.require_auth();

        if let Some(mut thread) = env
            .storage()
            .persistent()
            .get::<_, ThreadMeta>(&BoardKey::BoardThread(board_id, thread_id))
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
                .set(&BoardKey::BoardThread(board_id, thread_id), &thread);
        }
    }

    /// Edit thread title (author or moderator)
    pub fn edit_thread_title(env: Env, board_id: u64, thread_id: u64, new_title: String, caller: Address) {
        caller.require_auth();

        if let Some(mut thread) = env
            .storage()
            .persistent()
            .get::<_, ThreadMeta>(&BoardKey::BoardThread(board_id, thread_id))
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
                panic!("Only author or moderator can edit title");
            }

            thread.title = new_title;
            thread.updated_at = env.ledger().timestamp();
            env.storage()
                .persistent()
                .set(&BoardKey::BoardThread(board_id, thread_id), &thread);
        }
    }

    /// Increment reply count for a thread (called by content contract)
    pub fn increment_reply_count(env: Env, board_id: u64, thread_id: u64) {
        if let Some(mut thread) = env
            .storage()
            .persistent()
            .get::<_, ThreadMeta>(&BoardKey::BoardThread(board_id, thread_id))
        {
            thread.reply_count += 1;
            thread.updated_at = env.ledger().timestamp();
            env.storage()
                .persistent()
                .set(&BoardKey::BoardThread(board_id, thread_id), &thread);
        }
    }

    // ========================================================================
    // Rendering - Board, thread, and reply views
    // ========================================================================

    /// Main render entry point for board routes
    /// Routes are relative to the board (e.g., "/" = board view, "/t/0" = thread 0)
    /// board_id is now passed as a parameter
    /// community_slug: If board is in a community, pass the community's URL slug here
    ///                 to enable proper path building without re-entrant calls
    pub fn render(env: Env, board_id: u64, path: Option<String>, viewer: Option<Address>, community_slug: Option<String>) -> Bytes {
        // Store community_slug in temp storage for use during this render
        // This avoids re-entrant calls back to community contract
        if let Some(ref slug) = community_slug {
            env.storage().temporary().set(&BoardKey::RenderCommunitySlug, slug);
        } else {
            // Clear any stale value
            env.storage().temporary().remove(&BoardKey::RenderCommunitySlug);
        }

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

    /// Render navigation bar via include from main contract.
    /// Uses {{include}} tag for deferred loading - no cross-contract call overhead.
    fn render_nav<'a>(env: &'a Env, board_id: u64, _viewer: &Option<Address>) -> MarkdownBuilder<'a> {
        let aliases = Self::fetch_aliases(env);

        // Get board metadata for slug-based return path
        let board_meta: Option<BoardMeta> = env
            .storage()
            .persistent()
            .get(&BoardKey::Board(board_id));

        // Build include tag with return path for this board
        // Format: {{include contract=@main func="render_nav_include" viewer return_path="@main:{base_path}"}}
        let mut include_tag = Bytes::from_slice(env, b"{{include contract=@main func=\"render_nav_include\" viewer return_path=\"@main:");

        if let Some(meta) = board_meta {
            include_tag.append(&Self::build_board_base_path(env, board_id, &meta.slug));
        } else {
            // Fallback to numeric ID if board not found
            include_tag.append(&Bytes::from_slice(env, b"/b/"));
            include_tag.append(&u64_to_bytes(env, board_id));
        }
        include_tag.append(&Bytes::from_slice(env, b"\"}}"));

        MarkdownBuilder::new(env)
            .raw(aliases)  // Emit aliases for include resolution
            .raw(include_tag)
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

    /// Append footer to builder via include from main contract.
    /// Uses {{include}} tag for deferred loading - no cross-contract call overhead.
    fn render_footer_into<'a>(_env: &'a Env, md: MarkdownBuilder<'a>) -> MarkdownBuilder<'a> {
        md.raw_str("{{include contract=@main func=\"render_footer_include\"}}")
    }

    /// Render a thread card for the board list
    fn render_thread_card<'a>(
        env: &'a Env,
        md: MarkdownBuilder<'a>,
        board_id: u64,
        base_path: &Bytes,
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

        md = md.raw_str("<a href=\"render:")
            .raw(base_path.clone())
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
        // Get board metadata for slug-based URLs
        let board_meta: BoardMeta = env
            .storage()
            .persistent()
            .get(&BoardKey::Board(board_id))
            .expect("Board not found");

        // Build base path for all links
        let base_path = Self::build_board_base_path(env, board_id, &board_meta.slug);

        let config: BoardConfig = env
            .storage()
            .persistent()
            .get(&BoardKey::BoardConfig(board_id))
            .expect("Board not found");

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
            .get(&BoardKey::BoardFlairDefs(board_id))
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
        if let Some(rules) = env.storage().persistent().get::<_, String>(&BoardKey::BoardRules(board_id)) {
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

        // Show create thread button if user has Member+ role and board is not read-only
        let can_create = viewer.is_some()
            && !config.is_readonly
            && (viewer_role as u32) >= (Role::Member as u32);
        if can_create {
            md = md.raw_str("<a href=\"render:")
                .raw(base_path.clone())
                .raw_str("/new\" class=\"action-btn\">+ New Thread</a>")
                .newline();
        }

        // Show settings button for Admin+ users (uses numeric ID for admin routes)
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
                .raw_str("<a href=\"render:")
                .raw(base_path.clone())
                .raw_str("\" class=\"sort-option sort-active\">Hot</a>")
                .raw_str("<a href=\"render:")
                .raw(base_path.clone())
                .raw_str("?sort=new\" class=\"sort-option\">New</a>")
                .raw_str("<a href=\"render:")
                .raw(base_path.clone())
                .raw_str("?sort=top\" class=\"sort-option\">Top</a>")
                .raw_str("<a href=\"render:")
                .raw(base_path.clone())
                .raw_str("?sort=controversial\" class=\"sort-option\">Controversial</a>")
                .div_end();
        }

        md = md.div_start("thread-list");

        // Fetch threads
        let thread_count: u64 = env
            .storage()
            .persistent()
            .get(&BoardKey::BoardThreadCount(board_id))
            .unwrap_or(0);

        if thread_count == 0 {
            md = md.div_end()
                .paragraph("No threads yet. Be the first to post!");
        } else {
            // Get pinned threads list
            let pinned_threads: Vec<u64> = env
                .storage()
                .persistent()
                .get(&BoardKey::BoardPinnedThreads(board_id))
                .unwrap_or(Vec::new(env));

            let limit = 20u64;
            let mut shown = 0u64;

            // First, render pinned threads
            for i in 0..pinned_threads.len() {
                if shown >= limit {
                    break;
                }
                let thread_id = pinned_threads.get(i).unwrap();
                if let Some(thread) = env.storage().persistent().get::<_, ThreadMeta>(&BoardKey::BoardThread(board_id, thread_id)) {
                    // Skip hidden threads for non-moderators
                    if thread.is_hidden && !viewer_can_moderate {
                        continue;
                    }
                    md = Self::render_thread_card(env, md, board_id, &base_path, &thread, &voting_contract, &flairs);
                    shown += 1;
                }
            }

            // Then render remaining threads (newest first), skipping pinned ones
            let start_idx = thread_count - 1;
            let mut idx = start_idx;
            while shown < limit && idx < thread_count {
                if let Some(thread) = env.storage().persistent().get::<_, ThreadMeta>(&BoardKey::BoardThread(board_id, idx)) {
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
                    md = Self::render_thread_card(env, md, board_id, &base_path, &thread, &voting_contract, &flairs);
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

        Self::render_footer_into(env, md).build()
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

        Self::render_footer_into(env, md).build()
    }

    /// Render hidden thread access denied message
    fn render_hidden_thread_message(env: &Env, board_id: u64, viewer: &Option<Address>) -> Bytes {
        // Get board metadata for slug-based URLs
        let board_meta: BoardMeta = env
            .storage()
            .persistent()
            .get(&BoardKey::Board(board_id))
            .expect("Board not found");

        // Build base path for links
        let base_path = Self::build_board_base_path(env, board_id, &board_meta.slug);

        let md = Self::render_nav(env, board_id, viewer)
            .newline()
            .raw_str("<div class=\"back-nav\"><a href=\"render:")
            .raw(base_path)
            .raw_str("\" class=\"back-link\">← Back to Board</a></div>\n")
            .newline()
            .warning("This thread has been hidden by a moderator.");
        Self::render_footer_into(env, md).build()
    }

    /// Render create thread form
    fn render_create_thread(env: &Env, board_id: u64, viewer: &Option<Address>) -> Bytes {
        // Get board metadata for slug-based URLs
        let board_meta: BoardMeta = env
            .storage()
            .persistent()
            .get(&BoardKey::Board(board_id))
            .expect("Board not found");

        // Build base path for all links
        let base_path = Self::build_board_base_path(env, board_id, &board_meta.slug);

        // Get board config for private board check
        let config: BoardConfig = env
            .storage()
            .persistent()
            .get(&BoardKey::BoardConfig(board_id))
            .expect("Board not found");

        // Get permissions contract and viewer role early for private board check
        let perms_addr_opt = env.storage().instance().get::<_, Address>(&BoardKey::Permissions);
        let (viewer_role, is_moderator) = if let Some(ref perms_addr) = perms_addr_opt {
            if let Some(user) = viewer {
                let role_args: Vec<Val> = Vec::from_array(env, [board_id.into_val(env), user.into_val(env)]);
                let role: Role = env.invoke_contract(perms_addr, &Symbol::new(env, "get_role"), role_args);
                let can_mod = (role as u32) >= (Role::Moderator as u32);
                (role, can_mod)
            } else {
                (Role::Guest, false)
            }
        } else {
            (Role::Guest, false)
        };

        // Check permissions for private boards - must be Member+ to access
        if config.is_private {
            if let Some(ref perms_addr) = perms_addr_opt {
                if (viewer_role as u32) < (Role::Member as u32) {
                    return Self::render_private_board_message(env, board_id, &config, viewer, perms_addr);
                }
            }
        }

        let mut md = Self::render_nav(env, board_id, viewer)
            .newline()  // Blank line after nav-bar div for markdown parsing
            // Error mappings for user-friendly error messages
            .raw_str("{{errors {\"1\": \"This board is read-only and does not accept new content.\", \"7\": \"Please select a flair for your post.\"}}}\n")
            .raw_str("<div class=\"back-nav\"><a href=\"render:")
            .raw(base_path.clone())
            .raw_str("\" class=\"back-link\">← Back to Board</a></div>\n")
            .newline()  // Blank line before h1 for markdown parsing
            .h1("New Thread");

        // Show rules reminder if rules are set
        if let Some(rules) = env.storage().persistent().get::<_, String>(&BoardKey::BoardRules(board_id)) {
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
        if config.is_readonly {
            md = md.warning("This board is read-only. New threads cannot be created.");
            return Self::render_footer_into(env, md).build();
        }

        if viewer.is_none() {
            md = md.warning("Please connect your wallet to create a thread.");
            return Self::render_footer_into(env, md).build();
        }

        // Check if user has permission to create threads (requires Member+ role)
        if (viewer_role as u32) < (Role::Member as u32) {
            md = md.warning("You don't have permission to create threads on this board.");
            return Self::render_footer_into(env, md).build();
        }

        // Get flairs for the selector
        let flairs: Vec<FlairDef> = env
            .storage()
            .persistent()
            .get(&BoardKey::BoardFlairDefs(board_id))
            .unwrap_or(Vec::new(env));

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
            .raw_str("<input type=\"hidden\" name=\"_redirect\" value=\"")
            .raw(base_path.clone())
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
                .raw_str("<option value=\"none\">-- Select flair --</option>\n");

            for i in 0..flairs.len() {
                let flair = flairs.get(i).unwrap();
                if flair.enabled && (!flair.mod_only || is_moderator) {
                    // Use "flair_N" format to prevent viewer from converting to integer
                    md = md.raw_str("<option value=\"flair_")
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

            md = md.raw_str("</select>\n");
            if flair_required {
                md = md.raw_str("<small class=\"flair-required-notice\" style=\"color:#c00;display:block;margin-top:4px;\">* A flair is required for new posts on this board</small>\n");
            }
            md = md.raw_str("</div>\n");
        } else {
            // Always include flair_id field - the content contract expects it
            md = md.raw_str("<input type=\"hidden\" name=\"flair_id\" value=\"none\" />\n");
        }

        md = md.textarea_markdown("body", 10, "Write your post content here...")
            .newline()
            .raw_str("<input type=\"hidden\" name=\"caller\" value=\"")
            .text_string(&viewer.as_ref().unwrap().to_string())
            .raw_str("\" />\n")
            .newline()
            .form_link_to("Create Thread", "content", "create_thread")
            .newline()
            .newline()
            .raw_str("[Cancel](render:")
            .raw(base_path)
            .raw_str(")");

        Self::render_footer_into(env, md).build()
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

        // Get board metadata for slug-based URLs
        let board_meta: BoardMeta = env
            .storage()
            .persistent()
            .get(&BoardKey::Board(board_id))
            .expect("Board not found");

        // Get board config for readonly check
        let config: BoardConfig = env
            .storage()
            .persistent()
            .get(&BoardKey::BoardConfig(board_id))
            .expect("Board not found");

        // Get thread metadata
        let thread = env
            .storage()
            .persistent()
            .get::<_, ThreadMeta>(&BoardKey::BoardThread(board_id, thread_id));

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

        // Check permissions for private boards - must be Member+ to view threads
        if config.is_private {
            if let Some(ref perms_addr) = perms_addr_opt {
                if (viewer_role as u32) < (Role::Member as u32) {
                    return Self::render_private_board_message(env, board_id, &config, viewer, perms_addr);
                }
            }
        }

        // Check if thread is hidden - only moderators can view hidden threads
        let is_hidden = thread.as_ref().map(|t| t.is_hidden).unwrap_or(false);
        if is_hidden && !viewer_can_moderate {
            return Self::render_hidden_thread_message(env, board_id, viewer);
        }

        // Determine if posting is allowed (requires Member+ role, not readonly, not locked)
        let is_readonly = config.is_readonly;
        let is_locked = thread.as_ref().map(|t| t.is_locked).unwrap_or(false);
        let viewer_can_post = (viewer_role as u32) >= (Role::Member as u32);
        let can_post = !is_readonly && !is_locked && viewer_can_post;

        // Build base path for all links in this thread
        let base_path = Self::build_board_base_path(env, board_id, &board_meta.slug);

        let mut md = Self::render_nav(env, board_id, viewer)
            .newline()
            .raw_str("<div class=\"back-nav\"><a href=\"render:")
            .raw(base_path.clone())
            .raw_str("\" class=\"back-link\">← Back to Board</a></div>\n");

        // Get flairs for display
        let flairs: Vec<FlairDef> = env
            .storage()
            .persistent()
            .get(&BoardKey::BoardFlairDefs(board_id))
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
            let return_path = Self::build_thread_return_path(env, board_id, &board_meta.slug, thread_id);
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
            // Look up original board's slug for the crosspost link
            let original_board_meta: Option<BoardMeta> = env
                .storage()
                .persistent()
                .get(&BoardKey::Board(xpost.original_board_id));
            let original_base_path = if let Some(ref orig_meta) = original_board_meta {
                Self::build_board_base_path(env, xpost.original_board_id, &orig_meta.slug)
            } else {
                // Fallback to numeric ID if board not found (shouldn't happen)
                let mut path = Bytes::from_slice(env, b"/b/");
                path.append(&soroban_render_sdk::bytes::u32_to_bytes(env, xpost.original_board_id as u32));
                path
            };

            md = md.div_start("crosspost-header")
                .raw_str("<span class=\"crosspost-badge\">⤴ Crosspost</span> ")
                .raw_str("Originally posted in [")
                .raw(original_base_path.clone())
                .raw_str("](render:")
                .raw(original_base_path)
                .raw_str("/t/")
                .number(xpost.original_thread_id as u32)
                .raw_str(") by ");
            // Show original author (return path goes back to current thread)
            let return_path = Self::build_thread_return_path(env, board_id, &board_meta.slug, thread_id);
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
                .raw_str("[Reply to Thread](render:")
                .raw(base_path.clone())
                .raw_str("/t/")
                .number(thread_id as u32)
                .raw_str("/reply)");

            // Show edit button if user can edit
            if let Some(ref t) = thread {
                let (is_author, is_moderator) = Self::can_edit(env, board_id, &t.creator, viewer);
                let can_edit_time = is_moderator || Self::is_within_edit_window(env, board_id, t.created_at);

                if (is_author || is_moderator) && can_edit_time {
                    md = md.text(" ")
                        .raw_str("[Edit](render:")
                        .raw(base_path.clone())
                        .raw_str("/t/")
                        .number(thread_id as u32)
                        .raw_str("/edit)");
                }
            }

            // Crosspost button (only if not already a crosspost)
            if crosspost_ref.is_none() {
                md = md.text(" ")
                    .raw_str("[Crosspost](render:/crosspost?from_board=")
                    .text_string(&board_meta.slug)
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
            // Use waterfall loading with slug-based path
            md = md.raw_str("{{render path=\"")
                .raw(base_path.clone())
                .raw_str("/t/")
                .number(thread_id as u32)
                .raw_str("/replies/0\"}}");
        }

        Self::render_footer_into(env, md).build()
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

        // Get board metadata for slug-based URLs
        let board_meta: BoardMeta = env
            .storage()
            .persistent()
            .get(&BoardKey::Board(board_id))
            .expect("Board not found");

        // Build base path for all links
        let base_path = Self::build_board_base_path(env, board_id, &board_meta.slug);

        let config: BoardConfig = env
            .storage()
            .persistent()
            .get(&BoardKey::BoardConfig(board_id))
            .expect("Board not found");
        let chunk_size = if config.reply_chunk_size == 0 { 6 } else { config.reply_chunk_size };

        // Get viewer role for permission check
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

        // Check permissions for private boards - must be Member+ to access
        if config.is_private {
            if let Some(ref perms_addr) = perms_addr_opt {
                if (viewer_role as u32) < (Role::Member as u32) {
                    return Self::render_private_board_message(env, board_id, &config, viewer, perms_addr);
                }
            }
        }

        // Determine if posting is allowed (requires Member+ role, not readonly, not locked)
        let thread = env
            .storage()
            .persistent()
            .get::<_, ThreadMeta>(&BoardKey::BoardThread(board_id, thread_id));
        let is_locked = thread.as_ref().map(|t| t.is_locked).unwrap_or(false);
        let viewer_can_post = (viewer_role as u32) >= (Role::Member as u32);
        let can_post = !config.is_readonly && !is_locked && viewer_can_post;

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
                md = Self::render_reply_item_waterfall(env, md, &content, &reply, board_id, thread_id, &base_path, &board_meta.slug, viewer, can_post, &profile_contract, &voting_contract);
            }
        }

        // If more replies exist, add continuation with slug-based path
        let next_start = start + chunk_size;
        if (next_start as u64) < total_count {
            md = md.raw_str("{{render path=\"")
                .raw(base_path)
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

        // Get board metadata for slug-based URLs
        let board_meta: BoardMeta = env
            .storage()
            .persistent()
            .get(&BoardKey::Board(board_id))
            .expect("Board not found");

        // Build base path for all links
        let base_path = Self::build_board_base_path(env, board_id, &board_meta.slug);

        let config: BoardConfig = env
            .storage()
            .persistent()
            .get(&BoardKey::BoardConfig(board_id))
            .expect("Board not found");
        let chunk_size = if config.reply_chunk_size == 0 { 6 } else { config.reply_chunk_size };

        // Get viewer role for permission check
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

        // Check permissions for private boards - must be Member+ to access
        if config.is_private {
            if let Some(ref perms_addr) = perms_addr_opt {
                if (viewer_role as u32) < (Role::Member as u32) {
                    return Self::render_private_board_message(env, board_id, &config, viewer, perms_addr);
                }
            }
        }

        // Determine if posting is allowed (requires Member+ role, not readonly, not locked)
        let thread = env
            .storage()
            .persistent()
            .get::<_, ThreadMeta>(&BoardKey::BoardThread(board_id, thread_id));
        let is_locked = thread.as_ref().map(|t| t.is_locked).unwrap_or(false);
        let viewer_can_post = (viewer_role as u32) >= (Role::Member as u32);
        let can_post = !config.is_readonly && !is_locked && viewer_can_post;

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
                md = Self::render_reply_item_waterfall(env, md, &content, &child, board_id, thread_id, &base_path, &board_meta.slug, viewer, can_post, &profile_contract, &voting_contract);
            }
        }

        // If more children exist, add continuation with slug-based path
        let next_start = start + chunk_size;
        if next_start < total_count {
            md = md.raw_str("{{render path=\"")
                .raw(base_path)
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
        base_path: &Bytes,
        board_slug: &String,
        viewer: &Option<Address>,
        can_post: bool,
        profile_contract: &Option<Address>,
        voting_contract: &Option<Address>,
    ) -> MarkdownBuilder<'a> {
        md = md.div_start("reply");

        // Reply header with author (with return path so "Go Back" returns to thread)
        let return_path = Self::build_thread_return_path(env, board_id, board_slug, thread_id);
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
            md = md.raw_str("[Reply](render:")
                .raw(base_path.clone())
                .raw_str("/t/")
                .number(thread_id as u32)
                .raw_str("/r/")
                .number(reply.id as u32)
                .raw_str("/reply)");

            // Show edit button if user can edit (and reply is not deleted)
            if !reply.is_deleted {
                let (is_author, is_moderator) = Self::can_edit(env, board_id, &reply.creator, viewer);
                let can_edit_time = is_moderator || Self::is_within_edit_window(env, board_id, reply.created_at);

                if (is_author || is_moderator) && can_edit_time {
                    md = md.text(" ")
                        .raw_str("[Edit](render:")
                        .raw(base_path.clone())
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

        // If has children, embed continuation for waterfall loading with slug-based path
        if children_count > 0 {
            md = md.raw_str("{{render path=\"")
                .raw(base_path.clone())
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
        // Get board metadata for slug-based URLs
        let board_meta: BoardMeta = env
            .storage()
            .persistent()
            .get(&BoardKey::Board(board_id))
            .expect("Board not found");

        // Build base path for all links
        let base_path = Self::build_board_base_path(env, board_id, &board_meta.slug);

        // Get board config for private board check
        let config: BoardConfig = env
            .storage()
            .persistent()
            .get(&BoardKey::BoardConfig(board_id))
            .expect("Board not found");

        // Get permissions contract and viewer role early for private board check
        let perms_addr_opt = env.storage().instance().get::<_, Address>(&BoardKey::Permissions);
        let viewer_role = if let Some(ref perms_addr) = perms_addr_opt {
            if let Some(user) = viewer {
                let role_args: Vec<Val> = Vec::from_array(env, [board_id.into_val(env), user.into_val(env)]);
                env.invoke_contract(perms_addr, &Symbol::new(env, "get_role"), role_args)
            } else {
                Role::Guest
            }
        } else {
            Role::Guest
        };

        // Check permissions for private boards - must be Member+ to access
        if config.is_private {
            if let Some(ref perms_addr) = perms_addr_opt {
                if (viewer_role as u32) < (Role::Member as u32) {
                    return Self::render_private_board_message(env, board_id, &config, viewer, perms_addr);
                }
            }
        }

        let mut md = Self::render_nav(env, board_id, viewer)
            .newline()
            // Error mappings for user-friendly error messages
            .raw_str("{{errors {\"1\": \"This board is read-only.\", \"2\": \"This thread is locked and does not accept new replies.\", \"3\": \"You don't have permission to perform this action.\"}}}\n")
            .raw_str("[< Back to Thread](render:")
            .raw(base_path.clone())
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
        if config.is_readonly {
            md = md.warning("This board is read-only. Replies cannot be posted.");
            return Self::render_footer_into(env, md).build();
        }

        // Check if thread is locked
        if let Some(thread) = env.storage().persistent().get::<_, ThreadMeta>(&BoardKey::BoardThread(board_id, thread_id)) {
            if thread.is_locked {
                md = md.warning("This thread is locked. Replies cannot be posted.");
                return Self::render_footer_into(env, md).build();
            }
        }

        if viewer.is_none() {
            md = md.warning("Please connect your wallet to reply.");
            return Self::render_footer_into(env, md).build();
        }

        // Check if user has permission to reply (requires Member+ role)
        if (viewer_role as u32) < (Role::Member as u32) {
            md = md.warning("You don't have permission to reply on this board.");
            return Self::render_footer_into(env, md).build();
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
            .raw_str("<input type=\"hidden\" name=\"_redirect\" value=\"")
            .raw(base_path.clone())
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
            .textarea_markdown("content_str", 6, "Write your reply...")
            .newline()
            .raw_str("<input type=\"hidden\" name=\"caller\" value=\"")
            .text_string(&viewer.as_ref().unwrap().to_string())
            .raw_str("\" />\n")
            .newline()
            .form_link_to("Post Reply", "content", "create_reply")
            .newline()
            .newline()
            .raw_str("[Cancel](render:")
            .raw(base_path)
            .raw_str("/t/")
            .number(thread_id as u32)
            .raw_str(")");

        Self::render_footer_into(env, md).build()
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
        // Get board metadata for slug-based URLs
        let board_meta: BoardMeta = env
            .storage()
            .persistent()
            .get(&BoardKey::Board(board_id))
            .expect("Board not found");

        // Build base path for all links
        let base_path = Self::build_board_base_path(env, board_id, &board_meta.slug);

        // Get board config for private board check
        let config: BoardConfig = env
            .storage()
            .persistent()
            .get(&BoardKey::BoardConfig(board_id))
            .expect("Board not found");

        // Get permissions contract and viewer role early for private board check
        let perms_addr_opt = env.storage().instance().get::<_, Address>(&BoardKey::Permissions);
        let viewer_role = if let Some(ref perms_addr) = perms_addr_opt {
            if let Some(user) = viewer {
                let role_args: Vec<Val> = Vec::from_array(env, [board_id.into_val(env), user.into_val(env)]);
                env.invoke_contract(perms_addr, &Symbol::new(env, "get_role"), role_args)
            } else {
                Role::Guest
            }
        } else {
            Role::Guest
        };

        // Check permissions for private boards - must be Member+ to access
        if config.is_private {
            if let Some(ref perms_addr) = perms_addr_opt {
                if (viewer_role as u32) < (Role::Member as u32) {
                    return Self::render_private_board_message(env, board_id, &config, viewer, perms_addr);
                }
            }
        }

        let content: Address = env
            .storage()
            .instance()
            .get(&BoardKey::Content)
            .expect("Content contract not configured");

        let mut md = Self::render_nav(env, board_id, viewer)
            .newline()
            .raw_str("[< Back to Thread](render:")
            .raw(base_path.clone())
            .raw_str("/t/")
            .number(thread_id as u32)
            .raw_str(")")
            .newline()
            .newline()
            .h1("Edit Thread");

        // Check if board is read-only
        if config.is_readonly {
            md = md.warning("This board is read-only. Threads cannot be edited.");
            return Self::render_footer_into(env, md).build();
        }

        if viewer.is_none() {
            md = md.warning("Please connect your wallet to edit.");
            return Self::render_footer_into(env, md).build();
        }

        // Get thread metadata
        let thread = match env.storage().persistent().get::<_, ThreadMeta>(&BoardKey::BoardThread(board_id, thread_id)) {
            Some(t) => t,
            None => {
                md = md.warning("Thread not found.");
                return Self::render_footer_into(env, md).build();
            }
        };

        // Check if locked
        if thread.is_locked {
            md = md.warning("This thread is locked and cannot be edited.");
            return Self::render_footer_into(env, md).build();
        }

        // Check edit permission
        let (is_author, is_moderator) = Self::can_edit(env, board_id, &thread.creator, viewer);

        if !is_author && !is_moderator {
            md = md.warning("You don't have permission to edit this thread.");
            return Self::render_footer_into(env, md).build();
        }

        // Check edit window (only applies to non-moderators)
        if is_author && !is_moderator && !Self::is_within_edit_window(env, board_id, thread.created_at) {
            md = md.warning("The edit window has expired for this thread.");
            return Self::render_footer_into(env, md).build();
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
            .raw_str("<input type=\"hidden\" name=\"_redirect\" value=\"")
            .raw(base_path.clone())
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
            .raw_str("<textarea name=\"new_body\" data-editor=\"markdown\" rows=\"10\">");

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
            .raw_str("[Cancel](render:")
            .raw(base_path)
            .raw_str("/t/")
            .number(thread_id as u32)
            .raw_str(")");

        Self::render_footer_into(env, md).build()
    }

    /// Render edit reply form
    fn render_edit_reply(env: &Env, board_id: u64, thread_id: u64, reply_id: u64, viewer: &Option<Address>) -> Bytes {
        // Get board metadata for slug-based URLs
        let board_meta: BoardMeta = env
            .storage()
            .persistent()
            .get(&BoardKey::Board(board_id))
            .expect("Board not found");

        // Build base path for all links
        let base_path = Self::build_board_base_path(env, board_id, &board_meta.slug);

        // Get board config for private board check
        let config: BoardConfig = env
            .storage()
            .persistent()
            .get(&BoardKey::BoardConfig(board_id))
            .expect("Board not found");

        // Get permissions contract and viewer role early for private board check
        let perms_addr_opt = env.storage().instance().get::<_, Address>(&BoardKey::Permissions);
        let viewer_role = if let Some(ref perms_addr) = perms_addr_opt {
            if let Some(user) = viewer {
                let role_args: Vec<Val> = Vec::from_array(env, [board_id.into_val(env), user.into_val(env)]);
                env.invoke_contract(perms_addr, &Symbol::new(env, "get_role"), role_args)
            } else {
                Role::Guest
            }
        } else {
            Role::Guest
        };

        // Check permissions for private boards - must be Member+ to access
        if config.is_private {
            if let Some(ref perms_addr) = perms_addr_opt {
                if (viewer_role as u32) < (Role::Member as u32) {
                    return Self::render_private_board_message(env, board_id, &config, viewer, perms_addr);
                }
            }
        }

        let content: Address = env
            .storage()
            .instance()
            .get(&BoardKey::Content)
            .expect("Content contract not configured");

        let mut md = Self::render_nav(env, board_id, viewer)
            .newline()
            .raw_str("[< Back to Thread](render:")
            .raw(base_path.clone())
            .raw_str("/t/")
            .number(thread_id as u32)
            .raw_str(")")
            .newline()
            .newline()
            .h1("Edit Reply");

        // Check if board is read-only
        if config.is_readonly {
            md = md.warning("This board is read-only. Replies cannot be edited.");
            return Self::render_footer_into(env, md).build();
        }

        // Check if thread is locked
        if let Some(thread) = env.storage().persistent().get::<_, ThreadMeta>(&BoardKey::BoardThread(board_id, thread_id)) {
            if thread.is_locked {
                md = md.warning("This thread is locked. Replies cannot be edited.");
                return Self::render_footer_into(env, md).build();
            }
        }

        if viewer.is_none() {
            md = md.warning("Please connect your wallet to edit.");
            return Self::render_footer_into(env, md).build();
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
                return Self::render_footer_into(env, md).build();
            }
        };

        // Check if deleted
        if reply.is_deleted {
            md = md.warning("This reply has been deleted and cannot be edited.");
            return Self::render_footer_into(env, md).build();
        }

        // Check edit permission
        let (is_author, is_moderator) = Self::can_edit(env, board_id, &reply.creator, viewer);

        if !is_author && !is_moderator {
            md = md.warning("You don't have permission to edit this reply.");
            return Self::render_footer_into(env, md).build();
        }

        // Check edit window (only applies to non-moderators)
        if is_author && !is_moderator && !Self::is_within_edit_window(env, board_id, reply.created_at) {
            md = md.warning("The edit window has expired for this reply.");
            return Self::render_footer_into(env, md).build();
        }

        // Get current reply content
        let reply_content: Bytes = env.invoke_contract(
            &content,
            &Symbol::new(env, "get_reply_content"),
            args,
        );

        md = md
            .raw_str("<input type=\"hidden\" name=\"_redirect\" value=\"")
            .raw(base_path.clone())
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
            .raw_str("<textarea name=\"content\" data-editor=\"markdown\" rows=\"6\">");

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
            .raw_str("[Cancel](render:")
            .raw(base_path)
            .raw_str("/t/")
            .number(thread_id as u32)
            .raw_str(")");

        Self::render_footer_into(env, md).build()
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

    /// Fetch `{{aliases ...}}` tag from registry via cross-contract call.
    ///
    /// This enables includes using aliases like `{{include contract=config func="..."}}`
    /// in rendered content. Call this early in render functions and prepend to output.
    fn fetch_aliases(env: &Env) -> Bytes {
        let registry_opt: Option<Address> = env.storage().instance().get(&BoardKey::Registry);

        let Some(registry) = registry_opt else {
            return Bytes::new(env);
        };

        // Call registry's render_aliases function
        let args: Vec<Val> = Vec::new(env);
        env.try_invoke_contract::<Bytes, soroban_sdk::Error>(
            &registry,
            &Symbol::new(env, "render_aliases"),
            args,
        )
        .ok()
        .and_then(|r| r.ok())
        .unwrap_or(Bytes::new(env))
    }

    /// Get the community this board belongs to (if any)
    /// Uses try_invoke to handle potential re-entrant call issues gracefully
    fn get_board_community(env: &Env, board_id: u64) -> Option<CommunityInfo> {
        // Get community contract address directly from storage
        let community_contract: Address = env.storage().instance().get(&BoardKey::Community)?;

        // Query community contract for board's community ID
        // Use try_invoke to gracefully handle any issues
        let args: Vec<Val> = Vec::from_array(env, [board_id.into_val(env)]);
        let community_id_result = env.try_invoke_contract::<Option<u64>, soroban_sdk::Error>(
            &community_contract,
            &Symbol::new(env, "get_board_community"),
            args,
        );

        // Unwrap the nested results/options
        let community_id = match community_id_result {
            Ok(Ok(Some(id))) => id,
            _ => return None,
        };

        // Fetch community metadata
        let meta_args: Vec<Val> = Vec::from_array(env, [community_id.into_val(env)]);
        let community_meta_result = env.try_invoke_contract::<Option<CommunityInfo>, soroban_sdk::Error>(
            &community_contract,
            &Symbol::new(env, "get_community_info"),
            meta_args,
        );

        match community_meta_result {
            Ok(Ok(Some(meta))) => Some(meta),
            _ => None,
        }
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

    /// Build the base path for a board (e.g., "/b/{slug}" or "/c/{community}/b/{slug}")
    ///
    /// For community boards: /c/{community_slug}/b/{board_slug}
    /// For standalone boards: /b/{board_slug}
    ///
    /// First checks temp storage for community slug (passed during render to avoid re-entrant calls).
    /// Falls back to querying community contract if not in temp storage.
    fn build_board_base_path(env: &Env, _board_id: u64, board_slug: &String) -> Bytes {
        // First check temp storage for community slug (set during render)
        let community_slug_opt: Option<String> = env.storage().temporary().get(&BoardKey::RenderCommunitySlug);

        if let Some(community_slug) = community_slug_opt {
            // Community board: /c/{community_slug}/b/{board_slug}
            let mut path = Bytes::from_slice(env, b"/c/");
            path.append(&soroban_render_sdk::bytes::string_to_bytes(env, &community_slug));
            path.append(&Bytes::from_slice(env, b"/b/"));
            path.append(&soroban_render_sdk::bytes::string_to_bytes(env, board_slug));
            path
        } else {
            // Standalone board: /b/{board_slug}
            let mut path = Bytes::from_slice(env, b"/b/");
            path.append(&soroban_render_sdk::bytes::string_to_bytes(env, board_slug));
            path
        }
    }

    /// Build a return path for the current thread view (using slugs)
    fn build_thread_return_path(env: &Env, board_id: u64, board_slug: &String, thread_id: u64) -> Bytes {
        // Build cross-contract return path: @main:{base_path}/t/{thread_id}
        let mut path = Bytes::from_slice(env, b"@main:");
        path.append(&Self::build_board_base_path(env, board_id, board_slug));
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
            result.append(&u64_to_bytes(env, timestamp));
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

    // =========================================================================
    // Slug Helper Functions
    // =========================================================================

    /// Validate slug format: 3-30 chars, starts with lowercase letter,
    /// contains only lowercase letters, numbers, and hyphens, cannot end with hyphen.
    fn validate_slug(env: &Env, slug: &String) {
        let len = slug.len() as usize;
        if len < 3 || len > 30 {
            panic!("Board slug must be 3-30 characters");
        }

        let mut buf = [0u8; 30];
        let copy_len = core::cmp::min(len, 30);
        slug.copy_into_slice(&mut buf[..copy_len]);

        // First character must be lowercase letter
        let first = buf[0];
        if !(first >= b'a' && first <= b'z') {
            panic!("Board slug must start with lowercase letter");
        }

        // All characters must be lowercase alphanumeric or hyphen
        for i in 0..copy_len {
            let c = buf[i];
            let valid = (c >= b'a' && c <= b'z')
                || (c >= b'0' && c <= b'9')
                || c == b'-';
            if !valid {
                panic!("Board slug can only contain lowercase letters, numbers, and hyphens");
            }
        }

        // Cannot end with hyphen
        if buf[copy_len - 1] == b'-' {
            panic!("Board slug cannot end with hyphen");
        }

        let _ = env;
    }

    /// Generate a slug from a board name.
    /// Converts to lowercase, replaces spaces and special chars with hyphens,
    /// collapses multiple hyphens, trims leading/trailing hyphens.
    fn generate_slug_from_name(env: &Env, name: &String) -> String {
        let len = name.len() as usize;
        if len == 0 {
            return String::from_str(env, "board");
        }

        let mut name_buf = [0u8; 64];
        let copy_len = core::cmp::min(len, 64);
        name.copy_into_slice(&mut name_buf[..copy_len]);

        let mut slug_buf = [0u8; 30];
        let mut slug_len = 0usize;
        let mut last_was_hyphen = true; // Start true to skip leading hyphens

        for i in 0..copy_len {
            if slug_len >= 30 {
                break;
            }

            let c = name_buf[i];

            // Convert to lowercase letter
            if c >= b'A' && c <= b'Z' {
                slug_buf[slug_len] = c + 32; // to lowercase
                slug_len += 1;
                last_was_hyphen = false;
            } else if c >= b'a' && c <= b'z' {
                slug_buf[slug_len] = c;
                slug_len += 1;
                last_was_hyphen = false;
            } else if c >= b'0' && c <= b'9' {
                // Numbers are OK but not as first char
                if slug_len > 0 {
                    slug_buf[slug_len] = c;
                    slug_len += 1;
                    last_was_hyphen = false;
                }
            } else if !last_was_hyphen && slug_len > 0 {
                // Replace spaces, underscores, etc with hyphen (collapse multiple)
                slug_buf[slug_len] = b'-';
                slug_len += 1;
                last_was_hyphen = true;
            }
        }

        // Trim trailing hyphen
        while slug_len > 0 && slug_buf[slug_len - 1] == b'-' {
            slug_len -= 1;
        }

        // Ensure minimum length
        if slug_len < 3 {
            return String::from_str(env, "board");
        }

        // Build String from slice
        String::from_str(
            env,
            core::str::from_utf8(&slug_buf[..slug_len]).unwrap_or("board"),
        )
    }

    /// Generate a random 4-character suffix for slug conflict resolution.
    fn generate_random_suffix(env: &Env) -> String {
        // Use ledger timestamp and sequence for pseudo-randomness
        let timestamp = env.ledger().timestamp();
        let sequence = env.ledger().sequence();
        let combined = timestamp.wrapping_mul(31).wrapping_add(sequence as u64);

        let chars = b"abcdefghijklmnopqrstuvwxyz0123456789";
        let mut suffix = [0u8; 4];
        let mut val = combined;
        for i in 0..4 {
            suffix[i] = chars[(val % 36) as usize];
            val /= 36;
        }

        String::from_str(
            env,
            core::str::from_utf8(&suffix).unwrap_or("0000"),
        )
    }

    /// Check if a slug is available for standalone boards.
    fn is_slug_available(env: &Env, slug: &String) -> bool {
        !env.storage()
            .persistent()
            .has(&BoardKey::BoardBySlug(slug.clone()))
    }

    /// Generate a unique slug, appending random suffix if needed.
    /// For standalone boards only - community boards are handled by boards-community.
    fn generate_unique_slug(env: &Env, base_slug: &String) -> String {
        // Check if base slug is available
        if Self::is_slug_available(env, base_slug) {
            return base_slug.clone();
        }

        // Try with random suffixes (up to 10 attempts)
        for _ in 0..10 {
            let suffix = Self::generate_random_suffix(env);

            // Build slug with suffix: base-suffix
            let base_len = base_slug.len() as usize;
            let mut buf = [0u8; 30];
            let copy_len = core::cmp::min(base_len, 25); // Leave room for -xxxx
            base_slug.copy_into_slice(&mut buf[..copy_len]);

            buf[copy_len] = b'-';
            let mut suffix_buf = [0u8; 4];
            suffix.copy_into_slice(&mut suffix_buf);
            for (i, &c) in suffix_buf.iter().enumerate() {
                buf[copy_len + 1 + i] = c;
            }

            let new_slug = String::from_str(
                env,
                core::str::from_utf8(&buf[..copy_len + 5]).unwrap_or("board"),
            );

            if Self::is_slug_available(env, &new_slug) {
                return new_slug;
            }
        }

        // Fallback: use timestamp as suffix
        let ts = env.ledger().timestamp();
        let ts_str = Self::u64_to_string(env, ts);

        let base_len = base_slug.len() as usize;
        let mut buf = [0u8; 30];
        let copy_len = core::cmp::min(base_len, 15);
        base_slug.copy_into_slice(&mut buf[..copy_len]);
        buf[copy_len] = b'-';

        let ts_len = ts_str.len() as usize;
        let ts_copy = core::cmp::min(ts_len, 14);
        ts_str.copy_into_slice(&mut buf[copy_len + 1..copy_len + 1 + ts_copy]);

        String::from_str(
            env,
            core::str::from_utf8(&buf[..copy_len + 1 + ts_copy]).unwrap_or("board"),
        )
    }

    /// Convert u64 to String (for timestamp suffix).
    fn u64_to_string(env: &Env, mut n: u64) -> String {
        if n == 0 {
            return String::from_str(env, "0");
        }

        let mut buf = [0u8; 20];
        let mut pos = 20;
        while n > 0 && pos > 0 {
            pos -= 1;
            buf[pos] = b'0' + (n % 10) as u8;
            n /= 10;
        }

        String::from_str(
            env,
            core::str::from_utf8(&buf[pos..]).unwrap_or("0"),
        )
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::Env;

    /// Helper to set up a contract with a board for testing
    fn setup_with_board(env: &Env) -> (BoardsBoardClient, u64, Address) {
        env.mock_all_auths();

        let contract_id = env.register(BoardsBoard, ());
        let client = BoardsBoardClient::new(env, &contract_id);

        let registry = Address::generate(env);

        // Initialize contract (no permissions for simpler testing)
        client.init(&registry, &None, &None, &None);

        // Create a board
        let name = String::from_str(env, "General");
        let desc = String::from_str(env, "General discussion");
        let is_private = String::from_str(env, "false");
        let is_listed = String::from_str(env, "true");
        let caller = Address::generate(env);

        let board_id = client.create_board(&name, &desc, &is_private, &is_listed, &caller);

        (client, board_id, caller)
    }

    #[test]
    fn test_init_and_create_board() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsBoard, ());
        let client = BoardsBoardClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        client.init(&registry, &None, &None, &None);

        assert_eq!(client.board_count(), 0);

        let name = String::from_str(&env, "General");
        let desc = String::from_str(&env, "General discussion");
        let is_private = String::from_str(&env, "false");
        let is_listed = String::from_str(&env, "true");
        let caller = Address::generate(&env);

        let board_id = client.create_board(&name, &desc, &is_private, &is_listed, &caller);
        assert_eq!(board_id, 0);
        assert_eq!(client.board_count(), 1);

        let board = client.get_board(&board_id).unwrap();
        assert_eq!(board.name, name);
    }

    #[test]
    fn test_create_thread_without_flair() {
        let env = Env::default();
        let (client, board_id, _) = setup_with_board(&env);

        let creator = Address::generate(&env);
        let title = String::from_str(&env, "Hello World");

        // Create thread without flair (None)
        let thread_id = client.create_thread(&board_id, &title, &None, &creator);
        assert_eq!(thread_id, 0);
        assert_eq!(client.thread_count(&board_id), 1);

        let thread = client.get_thread(&board_id, &thread_id).unwrap();
        assert_eq!(thread.title, title);
        assert_eq!(thread.creator, creator);
        assert!(thread.flair_id.is_none());
    }

    #[test]
    fn test_create_thread_with_optional_flair() {
        let env = Env::default();
        let (client, board_id, owner) = setup_with_board(&env);

        // Create a flair first
        let flair_name = String::from_str(&env, "Discussion");
        let flair_color = String::from_str(&env, "#ffffff");
        let flair_bg = String::from_str(&env, "#0000ff");
        let flair_id = client.create_flair(
            &board_id,
            &flair_name,
            &flair_color,
            &flair_bg,
            &false, // not required
            &false, // not mod_only
            &owner,
        );

        let creator = Address::generate(&env);
        let title = String::from_str(&env, "Thread with flair");

        // Create thread with flair (use flair_N format for form input)
        let flair_id_str = String::from_str(&env, "flair_0"); // First flair ID is 0
        let thread_id = client.create_thread(&board_id, &title, &Some(flair_id_str), &creator);

        let thread = client.get_thread(&board_id, &thread_id).unwrap();
        assert_eq!(thread.flair_id, Some(flair_id));
    }

    #[test]
    #[should_panic(expected = "A flair is required for new posts on this board")]
    fn test_required_flair_enforcement() {
        let env = Env::default();
        let (client, board_id, owner) = setup_with_board(&env);

        // Create a required flair
        let flair_name = String::from_str(&env, "Required Flair");
        let flair_color = String::from_str(&env, "#ffffff");
        let flair_bg = String::from_str(&env, "#ff0000");
        client.create_flair(
            &board_id,
            &flair_name,
            &flair_color,
            &flair_bg,
            &true,  // required!
            &false, // not mod_only
            &owner,
        );

        let creator = Address::generate(&env);
        let title = String::from_str(&env, "Thread without flair");

        // This should panic because flair is required but not provided
        client.create_thread(&board_id, &title, &None, &creator);
    }

    #[test]
    fn test_required_flair_with_selection() {
        let env = Env::default();
        let (client, board_id, owner) = setup_with_board(&env);

        // Create a required flair
        let flair_name = String::from_str(&env, "Required Flair");
        let flair_color = String::from_str(&env, "#ffffff");
        let flair_bg = String::from_str(&env, "#ff0000");
        let flair_id = client.create_flair(
            &board_id,
            &flair_name,
            &flair_color,
            &flair_bg,
            &true,  // required
            &false, // not mod_only
            &owner,
        );

        let creator = Address::generate(&env);
        let title = String::from_str(&env, "Thread with required flair");

        // This should succeed because we provide the required flair (use flair_N format)
        let flair_id_str = String::from_str(&env, "flair_0"); // First flair ID is 0
        let thread_id = client.create_thread(&board_id, &title, &Some(flair_id_str), &creator);

        let thread = client.get_thread(&board_id, &thread_id).unwrap();
        assert_eq!(thread.flair_id, Some(flair_id));
    }

    #[test]
    #[should_panic(expected = "Selected flair does not exist")]
    fn test_invalid_flair_id() {
        let env = Env::default();
        let (client, board_id, _) = setup_with_board(&env);

        let creator = Address::generate(&env);
        let title = String::from_str(&env, "Thread with invalid flair");

        // Try to use a flair ID that doesn't exist (use flair_N format)
        let invalid_flair = String::from_str(&env, "flair_999");
        client.create_thread(&board_id, &title, &Some(invalid_flair), &creator);
    }

    #[test]
    fn test_pin_and_lock() {
        let env = Env::default();
        let (client, board_id, _) = setup_with_board(&env);

        let creator = Address::generate(&env);
        let moderator = Address::generate(&env);
        let title = String::from_str(&env, "Important Announcement");

        let thread_id = client.create_thread(&board_id, &title, &None, &creator);

        // Pin thread
        client.pin_thread(&board_id, &thread_id, &moderator);
        let thread = client.get_thread(&board_id, &thread_id).unwrap();
        assert!(thread.is_pinned);

        let pinned = client.get_pinned_threads(&board_id);
        assert_eq!(pinned.len(), 1);

        // Lock thread
        client.lock_thread(&board_id, &thread_id, &moderator);
        let thread = client.get_thread(&board_id, &thread_id).unwrap();
        assert!(thread.is_locked);
    }

    #[test]
    fn test_hide_thread() {
        let env = Env::default();
        let (client, board_id, _) = setup_with_board(&env);

        let creator = Address::generate(&env);
        let moderator = Address::generate(&env);
        let title = String::from_str(&env, "Test Thread");

        let thread_id = client.create_thread(&board_id, &title, &None, &creator);

        // Hide thread
        client.hide_thread(&board_id, &thread_id, &moderator);
        let thread = client.get_thread(&board_id, &thread_id).unwrap();
        assert!(thread.is_hidden);

        // Unhide thread
        client.unhide_thread(&board_id, &thread_id, &moderator);
        let thread = client.get_thread(&board_id, &thread_id).unwrap();
        assert!(!thread.is_hidden);
    }

    #[test]
    fn test_list_threads() {
        let env = Env::default();
        let (client, board_id, _) = setup_with_board(&env);

        let creator = Address::generate(&env);

        // Create multiple threads
        let titles = ["Thread 0", "Thread 1", "Thread 2", "Thread 3", "Thread 4"];
        for title_str in titles.iter() {
            let title = String::from_str(&env, title_str);
            client.create_thread(&board_id, &title, &None, &creator);
        }

        // List threads (should return newest first)
        let threads = client.list_threads(&board_id, &0, &3);
        assert_eq!(threads.len(), 3);
        // First thread should be the newest (id 4)
        assert_eq!(threads.get(0).unwrap().id, 4);

        // Get remaining threads
        let more_threads = client.list_threads(&board_id, &3, &10);
        assert_eq!(more_threads.len(), 2);
    }

    #[test]
    fn test_get_config() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsBoard, ());
        let client = BoardsBoardClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        client.init(&registry, &None, &None, &None);

        let name = String::from_str(&env, "Private Board");
        let desc = String::from_str(&env, "Secret discussions");
        let is_private = String::from_str(&env, "true");
        let is_listed = String::from_str(&env, "false");
        let caller = Address::generate(&env);

        let board_id = client.create_board(&name, &desc, &is_private, &is_listed, &caller);

        let config = client.get_config(&board_id);
        assert_eq!(config.name, name);
        assert_eq!(config.description, desc);
        assert!(config.is_private);
        assert!(!config.is_readonly);
    }

    #[test]
    fn test_get_nonexistent_thread() {
        let env = Env::default();
        let (client, board_id, _) = setup_with_board(&env);

        // Get thread that doesn't exist
        let thread = client.get_thread(&board_id, &999);
        assert!(thread.is_none());
    }

    #[test]
    fn test_increment_reply_count() {
        let env = Env::default();
        let (client, board_id, _) = setup_with_board(&env);

        let creator = Address::generate(&env);
        let title = String::from_str(&env, "Test Thread");
        let thread_id = client.create_thread(&board_id, &title, &None, &creator);

        // Initially 0 replies
        assert_eq!(client.get_thread(&board_id, &thread_id).unwrap().reply_count, 0);

        // Increment reply count
        client.increment_reply_count(&board_id, &thread_id);
        assert_eq!(client.get_thread(&board_id, &thread_id).unwrap().reply_count, 1);

        client.increment_reply_count(&board_id, &thread_id);
        assert_eq!(client.get_thread(&board_id, &thread_id).unwrap().reply_count, 2);
    }

    #[test]
    fn test_thread_defaults() {
        let env = Env::default();
        let (client, board_id, _) = setup_with_board(&env);

        let creator = Address::generate(&env);
        let title = String::from_str(&env, "New Thread");
        let thread_id = client.create_thread(&board_id, &title, &None, &creator);

        let thread = client.get_thread(&board_id, &thread_id).unwrap();

        // Default values
        assert_eq!(thread.reply_count, 0);
        assert!(!thread.is_locked);
        assert!(!thread.is_pinned);
        assert!(!thread.is_hidden);
        assert!(!thread.is_deleted);
        assert!(thread.flair_id.is_none());
    }

    #[test]
    fn test_delete_thread() {
        let env = Env::default();
        let (client, board_id, _) = setup_with_board(&env);

        let creator = Address::generate(&env);
        let title = String::from_str(&env, "Thread to Delete");
        let thread_id = client.create_thread(&board_id, &title, &None, &creator);

        // Initially not deleted
        let thread = client.get_thread(&board_id, &thread_id).unwrap();
        assert!(!thread.is_deleted);

        // Delete thread (author can delete)
        client.delete_thread(&board_id, &thread_id, &creator);
        let deleted_thread = client.get_thread(&board_id, &thread_id).unwrap();
        assert!(deleted_thread.is_deleted);
    }

    // ========================================================================
    // Slug Tests
    // ========================================================================

    #[test]
    fn test_create_board_with_explicit_slug() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsBoard, ());
        let client = BoardsBoardClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        client.init(&registry, &None, &None, &None);

        let name = String::from_str(&env, "Test Board");
        let desc = String::from_str(&env, "A test board");
        let is_private = String::from_str(&env, "false");
        let is_listed = String::from_str(&env, "true");
        let slug = String::from_str(&env, "test-board");
        let caller = Address::generate(&env);

        let board_id = client.create_board_with_slug(&name, &desc, &is_private, &is_listed, &Some(slug.clone()), &caller);
        assert_eq!(board_id, 0);

        let board = client.get_board(&board_id).unwrap();
        assert_eq!(board.name, name);
        assert_eq!(board.slug, slug);
    }

    #[test]
    fn test_create_board_auto_generates_slug() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsBoard, ());
        let client = BoardsBoardClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        client.init(&registry, &None, &None, &None);

        let name = String::from_str(&env, "My Awesome Board");
        let desc = String::from_str(&env, "Auto-generated slug test");
        let is_private = String::from_str(&env, "false");
        let is_listed = String::from_str(&env, "true");
        let caller = Address::generate(&env);

        // Pass None for slug to auto-generate
        let board_id = client.create_board_with_slug(&name, &desc, &is_private, &is_listed, &None, &caller);

        let board = client.get_board(&board_id).unwrap();
        // Slug should be derived from name: "my-awesome-board"
        let expected_slug = String::from_str(&env, "my-awesome-board");
        assert_eq!(board.slug, expected_slug);
    }

    #[test]
    fn test_get_board_by_slug() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsBoard, ());
        let client = BoardsBoardClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        client.init(&registry, &None, &None, &None);

        let name = String::from_str(&env, "Lookup Test");
        let desc = String::from_str(&env, "Test slug lookup");
        let is_private = String::from_str(&env, "false");
        let is_listed = String::from_str(&env, "true");
        let slug = String::from_str(&env, "lookup-test");
        let caller = Address::generate(&env);

        let board_id = client.create_board_with_slug(&name, &desc, &is_private, &is_listed, &Some(slug.clone()), &caller);

        // Look up by slug
        let found_board = client.get_board_by_slug(&slug);
        assert!(found_board.is_some());
        assert_eq!(found_board.unwrap().id, board_id);

        // Look up by ID should also work
        let found_id = client.get_board_id_by_slug(&slug);
        assert_eq!(found_id, Some(board_id));
    }

    #[test]
    fn test_slug_uniqueness_adds_suffix() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsBoard, ());
        let client = BoardsBoardClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        client.init(&registry, &None, &None, &None);

        let desc = String::from_str(&env, "Description");
        let is_private = String::from_str(&env, "false");
        let is_listed = String::from_str(&env, "true");
        let caller = Address::generate(&env);

        // Create first board with slug "general"
        let name1 = String::from_str(&env, "General");
        let slug1 = String::from_str(&env, "general");
        let board1_id = client.create_board_with_slug(&name1, &desc, &is_private, &is_listed, &Some(slug1.clone()), &caller);
        let board1 = client.get_board(&board1_id).unwrap();
        assert_eq!(board1.slug, slug1);

        // Create second board with same slug - should get suffix
        let name2 = String::from_str(&env, "General 2");
        let board2_id = client.create_board_with_slug(&name2, &desc, &is_private, &is_listed, &Some(slug1.clone()), &caller);
        let board2 = client.get_board(&board2_id).unwrap();
        // Slug should NOT equal "general" - it should have a suffix
        assert!(board2.slug != slug1);
        // But should start with "general-"
        // We can't easily check prefix in Soroban, so just verify they're different boards
        assert_ne!(board1_id, board2_id);
    }

    #[test]
    #[should_panic(expected = "Board slug must be 3-30 characters")]
    fn test_slug_validation_too_short() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsBoard, ());
        let client = BoardsBoardClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        client.init(&registry, &None, &None, &None);

        let name = String::from_str(&env, "Short");
        let desc = String::from_str(&env, "Description");
        let is_private = String::from_str(&env, "false");
        let is_listed = String::from_str(&env, "true");
        let slug = String::from_str(&env, "ab"); // Too short - 2 chars
        let caller = Address::generate(&env);

        // This should panic
        client.create_board_with_slug(&name, &desc, &is_private, &is_listed, &Some(slug), &caller);
    }

    #[test]
    #[should_panic(expected = "Board slug must start with lowercase letter")]
    fn test_slug_validation_must_start_with_letter() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsBoard, ());
        let client = BoardsBoardClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        client.init(&registry, &None, &None, &None);

        let name = String::from_str(&env, "Numbers");
        let desc = String::from_str(&env, "Description");
        let is_private = String::from_str(&env, "false");
        let is_listed = String::from_str(&env, "true");
        let slug = String::from_str(&env, "123-board"); // Starts with number
        let caller = Address::generate(&env);

        // This should panic
        client.create_board_with_slug(&name, &desc, &is_private, &is_listed, &Some(slug), &caller);
    }

    #[test]
    fn test_board_metadata_includes_slug() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsBoard, ());
        let client = BoardsBoardClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        client.init(&registry, &None, &None, &None);

        let name = String::from_str(&env, "Meta Test");
        let desc = String::from_str(&env, "Testing metadata");
        let is_private = String::from_str(&env, "false");
        let is_listed = String::from_str(&env, "true");
        let slug = String::from_str(&env, "meta-test");
        let caller = Address::generate(&env);

        let board_id = client.create_board_with_slug(&name, &desc, &is_private, &is_listed, &Some(slug.clone()), &caller);

        // Get board and verify slug is in metadata
        let board = client.get_board(&board_id).unwrap();
        assert_eq!(board.id, board_id);
        assert_eq!(board.slug, slug);
        assert_eq!(board.name, name);
        assert_eq!(board.creator, caller);
    }
}
