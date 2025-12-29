#![no_std]

use soroban_sdk::{contract, contractimpl, contracttype, Address, BytesN, Env, IntoVal, String, Symbol, Val, Vec};

/// Storage keys for the registry contract
#[contracttype]
#[derive(Clone)]
pub enum RegistryKey {
    /// Contract admin addresses (multiple admins supported)
    Admins,
    /// Total number of boards
    BoardCount,
    /// Board metadata by ID
    Board(u64),
    /// Board ID by name (Symbol)
    BoardByName(Symbol),
    /// Board contract address by ID
    BoardContract(u64),
    /// Contract addresses for shared services (legacy, for backwards compatibility)
    Contracts,
    /// Generic contract storage by alias (e.g., "perms", "content", "theme", "profile")
    Contract(Symbol),
    /// Global pause flag
    Paused,
    /// WASM hash for deploying board contracts
    BoardWasmHash,
    /// Whether a board is listed publicly (stored separately for backwards compatibility)
    BoardListed(u64),
    /// Whether a board is private (stored separately for backwards compatibility)
    BoardPrivate(u64),
    /// Whether a board is read-only (stored separately for backwards compatibility)
    BoardReadonly(u64),
    /// Maps board name (and aliases) to board ID for lookup
    BoardNameToId(String),
    /// Stores aliases for a board (previous names that still resolve)
    BoardAliases(u64),
    /// Community contract address
    CommunityContract,
    /// Board to community mapping (board_id -> community_id)
    BoardCommunity(u64),
    /// Voting contract address
    VotingContract,
}

/// Addresses of shared service contracts
#[contracttype]
#[derive(Clone)]
pub struct ContractAddresses {
    pub permissions: Address,
    pub content: Address,
    pub theme: Address,
    pub admin: Address,
}

/// Board metadata stored in registry
#[contracttype]
#[derive(Clone)]
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

/// Permission set from permissions contract
#[contracttype]
#[derive(Clone)]
pub struct PermissionSet {
    pub role: Role,
    pub can_view: bool,
    pub can_post: bool,
    pub can_moderate: bool,
    pub can_admin: bool,
    pub is_banned: bool,
}

#[contract]
pub struct BoardsRegistry;

#[contractimpl]
impl BoardsRegistry {
    /// Initialize the registry with admin and service contract addresses
    pub fn init(
        env: Env,
        admins: Vec<Address>,
        permissions: Address,
        content: Address,
        theme: Address,
        admin_contract: Address,
    ) {
        // Only allow initialization once
        if env.storage().instance().has(&RegistryKey::Admins) {
            panic!("Already initialized");
        }

        if admins.is_empty() {
            panic!("At least one admin required");
        }

        // Require auth from the first admin
        admins.get(0).unwrap().require_auth();

        env.storage().instance().set(&RegistryKey::Admins, &admins);
        env.storage().instance().set(&RegistryKey::BoardCount, &0u64);
        env.storage().instance().set(&RegistryKey::Paused, &false);

        // Store in legacy Contracts struct for backwards compatibility
        env.storage().instance().set(
            &RegistryKey::Contracts,
            &ContractAddresses {
                permissions: permissions.clone(),
                content: content.clone(),
                theme: theme.clone(),
                admin: admin_contract.clone(),
            },
        );

        // Also store in generic Contract keys for new lookup pattern
        env.storage().instance().set(
            &RegistryKey::Contract(Symbol::new(&env, "perms")),
            &permissions,
        );
        env.storage().instance().set(
            &RegistryKey::Contract(Symbol::new(&env, "content")),
            &content,
        );
        env.storage().instance().set(
            &RegistryKey::Contract(Symbol::new(&env, "theme")),
            &theme,
        );
        env.storage().instance().set(
            &RegistryKey::Contract(Symbol::new(&env, "admin")),
            &admin_contract,
        );
    }

    /// Set the WASM hash for deploying board contracts (admin only)
    pub fn set_board_wasm_hash(env: Env, wasm_hash: BytesN<32>, caller: Address) {
        Self::require_admin_auth(&env, &caller);

        env.storage()
            .instance()
            .set(&RegistryKey::BoardWasmHash, &wasm_hash);
    }

    /// Get the board WASM hash
    pub fn get_board_wasm_hash(env: Env) -> Option<BytesN<32>> {
        env.storage().instance().get(&RegistryKey::BoardWasmHash)
    }

    /// Create a new board
    /// Note: is_private and is_listed are Strings from form input ("true"/"false")
    pub fn create_board(
        env: Env,
        name: String,
        description: String,
        is_private: String,  // String from form, parse to bool
        is_listed: String,   // String from form, parse to bool (default: true if empty)
        caller: Address,
    ) -> u64 {
        caller.require_auth();

        // Parse is_private string to bool
        // "true" = private, anything else = public
        let is_private_bool = is_private.len() == 4 && {
            let mut buf = [0u8; 4];
            is_private.copy_into_slice(&mut buf);
            &buf == b"true"
        };

        // Parse is_listed string to bool
        // Empty string or "true" = listed (default), "false" = unlisted
        let is_listed_bool = if is_listed.len() == 0 {
            true  // Default to listed
        } else if is_listed.len() == 5 {
            let mut buf = [0u8; 5];
            is_listed.copy_into_slice(&mut buf);
            &buf != b"false"  // Listed unless explicitly "false"
        } else {
            true  // Listed for any other value
        };

        // Check not paused
        let paused: bool = env
            .storage()
            .instance()
            .get(&RegistryKey::Paused)
            .unwrap_or(false);
        if paused {
            panic!("Registry is paused");
        }

        // Get next board ID
        let board_id: u64 = env
            .storage()
            .instance()
            .get(&RegistryKey::BoardCount)
            .unwrap_or(0);

        // Create board metadata
        let board = BoardMeta {
            id: board_id,
            name: name.clone(),
            description: description.clone(),
            creator: caller.clone(),
            created_at: env.ledger().timestamp(),
            thread_count: 0,
            is_readonly: false,
            is_private: is_private_bool,
        };

        // Store board
        env.storage()
            .persistent()
            .set(&RegistryKey::Board(board_id), &board);

        // Store listed status separately (for backwards compatibility)
        env.storage()
            .persistent()
            .set(&RegistryKey::BoardListed(board_id), &is_listed_bool);

        // Register board name for lookup (check for conflicts first)
        if env
            .storage()
            .persistent()
            .has(&RegistryKey::BoardNameToId(name.clone()))
        {
            panic!("Board name already exists");
        }
        env.storage()
            .persistent()
            .set(&RegistryKey::BoardNameToId(name.clone()), &board_id);

        // Initialize empty aliases list
        let aliases: Vec<String> = Vec::new(&env);
        env.storage()
            .persistent()
            .set(&RegistryKey::BoardAliases(board_id), &aliases);

        // Update count
        env.storage()
            .instance()
            .set(&RegistryKey::BoardCount, &(board_id + 1));

        // Get service contract addresses
        let contracts: ContractAddresses = env
            .storage()
            .instance()
            .get(&RegistryKey::Contracts)
            .expect("Not initialized");

        // Deploy a board contract for this board if WASM hash is set
        if let Some(wasm_hash) = env
            .storage()
            .instance()
            .get::<_, BytesN<32>>(&RegistryKey::BoardWasmHash)
        {
            // Generate a unique salt based on board_id
            let mut salt_bytes = [0u8; 32];
            let id_bytes = board_id.to_be_bytes();
            salt_bytes[24..32].copy_from_slice(&id_bytes);
            let salt = BytesN::from_array(&env, &salt_bytes);

            // Deploy the board contract
            let board_contract = env.deployer().with_current_contract(salt).deploy_v2(
                wasm_hash,
                (),  // No constructor args - we'll init separately
            );

            // Initialize the board contract
            // Pass contract addresses for rendering capabilities
            let init_args: Vec<Val> = Vec::from_array(
                &env,
                [
                    board_id.into_val(&env),
                    env.current_contract_address().into_val(&env),  // registry
                    Some(contracts.permissions.clone()).into_val(&env),  // permissions contract
                    Some(contracts.content.clone()).into_val(&env),  // content contract
                    Some(contracts.theme.clone()).into_val(&env),  // theme contract
                    name.into_val(&env),
                    description.into_val(&env),
                    is_private_bool.into_val(&env),
                ],
            );
            env.invoke_contract::<()>(
                &board_contract,
                &Symbol::new(&env, "init"),
                init_args,
            );

            // Store the board contract address
            env.storage()
                .persistent()
                .set(&RegistryKey::BoardContract(board_id), &board_contract);
        }

        // Set board owner in permissions contract
        // In tests without a real permissions contract, this will be a no-op
        let args: Vec<Val> = Vec::from_array(
            &env,
            [board_id.into_val(&env), caller.into_val(&env)],
        );
        let fn_name = Symbol::new(&env, "set_board_owner");

        // Use try_invoke_contract to handle the case where permissions isn't set up
        let _ = env.try_invoke_contract::<(), soroban_sdk::Error>(
            &contracts.permissions,
            &fn_name,
            args,
        );

        board_id
    }

    /// Get board metadata by ID
    pub fn get_board(env: Env, board_id: u64) -> Option<BoardMeta> {
        env.storage()
            .persistent()
            .get(&RegistryKey::Board(board_id))
    }

    /// Get board by name or alias
    /// Returns the board metadata if found by name or any alias
    pub fn get_board_by_name(env: Env, name: String) -> Option<BoardMeta> {
        // Look up board ID by name (includes aliases)
        if let Some(board_id) = env
            .storage()
            .persistent()
            .get::<_, u64>(&RegistryKey::BoardNameToId(name))
        {
            return env
                .storage()
                .persistent()
                .get(&RegistryKey::Board(board_id));
        }
        None
    }

    /// Get aliases for a board
    pub fn get_board_aliases(env: Env, board_id: u64) -> Vec<String> {
        env.storage()
            .persistent()
            .get(&RegistryKey::BoardAliases(board_id))
            .unwrap_or_else(|| Vec::new(&env))
    }

    /// Rename a board (Admin+ on board only)
    /// The old name becomes an alias that continues to resolve to this board
    pub fn rename_board(env: Env, board_id: u64, new_name: String, caller: Address) {
        caller.require_auth();

        // Verify board exists and get current metadata
        let mut board: BoardMeta = env
            .storage()
            .persistent()
            .get(&RegistryKey::Board(board_id))
            .expect("Board does not exist");

        // Verify caller has admin permission on this board
        let contracts: ContractAddresses = env
            .storage()
            .instance()
            .get(&RegistryKey::Contracts)
            .expect("Not initialized");

        let args: Vec<Val> = Vec::from_array(
            &env,
            [board_id.into_val(&env), caller.into_val(&env)],
        );
        let perms: PermissionSet = env.invoke_contract(
            &contracts.permissions,
            &Symbol::new(&env, "get_permissions"),
            args,
        );

        if !perms.can_admin {
            panic!("Caller must be admin or owner");
        }

        // Check new name doesn't conflict with existing name or alias
        if env
            .storage()
            .persistent()
            .has(&RegistryKey::BoardNameToId(new_name.clone()))
        {
            panic!("Name already in use");
        }

        // Validate new name (3-50 chars, alphanumeric + underscore + hyphen, starts with letter)
        Self::validate_board_name(&env, &new_name);

        // Get current aliases and add old name
        let mut aliases: Vec<String> = env
            .storage()
            .persistent()
            .get(&RegistryKey::BoardAliases(board_id))
            .unwrap_or_else(|| Vec::new(&env));

        let old_name = board.name.clone();
        aliases.push_back(old_name);

        // Register new name in lookup
        env.storage()
            .persistent()
            .set(&RegistryKey::BoardNameToId(new_name.clone()), &board_id);

        // Update board metadata with new name
        board.name = new_name;
        env.storage()
            .persistent()
            .set(&RegistryKey::Board(board_id), &board);

        // Update aliases list
        env.storage()
            .persistent()
            .set(&RegistryKey::BoardAliases(board_id), &aliases);
    }

    /// Validate board name format
    /// - 3-50 characters
    /// - Alphanumeric + underscore + hyphen
    /// - Must start with a letter
    fn validate_board_name(env: &Env, name: &String) {
        let len = name.len() as usize;
        if len < 3 || len > 50 {
            panic!("Board name must be 3-50 characters");
        }

        // Copy name to buffer for validation
        let mut buf = [0u8; 50];
        let copy_len = core::cmp::min(len, 50);
        name.copy_into_slice(&mut buf[..copy_len]);

        // First character must be a letter
        let first = buf[0];
        if !((first >= b'a' && first <= b'z') || (first >= b'A' && first <= b'Z')) {
            panic!("Board name must start with a letter");
        }

        // Remaining characters must be alphanumeric, underscore, or hyphen
        for i in 1..copy_len {
            let c = buf[i];
            let valid = (c >= b'a' && c <= b'z')
                || (c >= b'A' && c <= b'Z')
                || (c >= b'0' && c <= b'9')
                || c == b'_'
                || c == b'-';
            if !valid {
                panic!("Board name can only contain letters, numbers, underscore, and hyphen");
            }
        }

        // Suppress unused warning
        let _ = env;
    }

    /// List boards with pagination (all boards, for admin use)
    pub fn list_boards(env: Env, start: u64, limit: u64) -> Vec<BoardMeta> {
        let count: u64 = env
            .storage()
            .instance()
            .get(&RegistryKey::BoardCount)
            .unwrap_or(0);

        let mut boards = Vec::new(&env);
        let end = core::cmp::min(start + limit, count);

        for i in start..end {
            if let Some(board) = env.storage().persistent().get(&RegistryKey::Board(i)) {
                boards.push_back(board);
            }
        }

        boards
    }

    /// List only publicly listed boards with pagination (for home page)
    pub fn list_listed_boards(env: Env, start: u64, limit: u64) -> Vec<BoardMeta> {
        let count: u64 = env
            .storage()
            .instance()
            .get(&RegistryKey::BoardCount)
            .unwrap_or(0);

        let mut boards = Vec::new(&env);
        let mut collected = 0u64;
        let mut skipped = 0u64;

        for i in 0..count {
            // Check if board is listed (default to true for backwards compatibility)
            let is_listed: bool = env
                .storage()
                .persistent()
                .get(&RegistryKey::BoardListed(i))
                .unwrap_or(true);

            if is_listed {
                if skipped < start {
                    skipped += 1;
                    continue;
                }
                if collected >= limit {
                    break;
                }
                if let Some(board) = env.storage().persistent().get(&RegistryKey::Board(i)) {
                    boards.push_back(board);
                    collected += 1;
                }
            }
        }

        boards
    }

    /// Check if a board is listed publicly
    pub fn get_board_listed(env: Env, board_id: u64) -> bool {
        // Default to true for backwards compatibility with existing boards
        env.storage()
            .persistent()
            .get(&RegistryKey::BoardListed(board_id))
            .unwrap_or(true)
    }

    /// Set whether a board is listed publicly (admin+ on board only)
    pub fn set_listed(env: Env, board_id: u64, is_listed: bool, caller: Address) {
        caller.require_auth();

        // Verify board exists
        if !env
            .storage()
            .persistent()
            .has(&RegistryKey::Board(board_id))
        {
            panic!("Board does not exist");
        }

        // Verify caller has admin permission on this board
        let contracts: ContractAddresses = env
            .storage()
            .instance()
            .get(&RegistryKey::Contracts)
            .expect("Not initialized");

        // Check permissions
        let args: Vec<Val> = Vec::from_array(
            &env,
            [board_id.into_val(&env), caller.into_val(&env)],
        );
        let perms: PermissionSet = env.invoke_contract(
            &contracts.permissions,
            &Symbol::new(&env, "get_permissions"),
            args,
        );

        if !perms.can_admin {
            panic!("Caller must be admin or owner");
        }

        // Set listed status
        env.storage()
            .persistent()
            .set(&RegistryKey::BoardListed(board_id), &is_listed);
    }

    /// Check if a board is private
    /// Falls back to BoardMeta.is_private for backwards compatibility
    pub fn get_board_private(env: Env, board_id: u64) -> bool {
        // First check the separate storage key
        if let Some(is_private) = env
            .storage()
            .persistent()
            .get::<_, bool>(&RegistryKey::BoardPrivate(board_id))
        {
            return is_private;
        }
        // Fall back to BoardMeta for backwards compatibility
        if let Some(board) = env
            .storage()
            .persistent()
            .get::<_, BoardMeta>(&RegistryKey::Board(board_id))
        {
            return board.is_private;
        }
        false
    }

    /// Set whether a board is private (admin+ on board only)
    pub fn set_private(env: Env, board_id: u64, is_private: bool, caller: Address) {
        caller.require_auth();

        // Verify board exists
        if !env
            .storage()
            .persistent()
            .has(&RegistryKey::Board(board_id))
        {
            panic!("Board does not exist");
        }

        // Verify caller has admin permission on this board
        let contracts: ContractAddresses = env
            .storage()
            .instance()
            .get(&RegistryKey::Contracts)
            .expect("Not initialized");

        let args: Vec<Val> = Vec::from_array(
            &env,
            [board_id.into_val(&env), caller.into_val(&env)],
        );
        let perms: PermissionSet = env.invoke_contract(
            &contracts.permissions,
            &Symbol::new(&env, "get_permissions"),
            args,
        );

        if !perms.can_admin {
            panic!("Caller must be admin or owner");
        }

        // Set private status
        env.storage()
            .persistent()
            .set(&RegistryKey::BoardPrivate(board_id), &is_private);
    }

    /// Check if a board is read-only
    /// Falls back to BoardMeta.is_readonly for backwards compatibility
    pub fn get_board_readonly(env: Env, board_id: u64) -> bool {
        // First check the separate storage key
        if let Some(is_readonly) = env
            .storage()
            .persistent()
            .get::<_, bool>(&RegistryKey::BoardReadonly(board_id))
        {
            return is_readonly;
        }
        // Fall back to BoardMeta for backwards compatibility
        if let Some(board) = env
            .storage()
            .persistent()
            .get::<_, BoardMeta>(&RegistryKey::Board(board_id))
        {
            return board.is_readonly;
        }
        false
    }

    /// Set whether a board is read-only (admin+ on board only)
    pub fn set_readonly(env: Env, board_id: u64, is_readonly: bool, caller: Address) {
        caller.require_auth();

        // Verify board exists
        if !env
            .storage()
            .persistent()
            .has(&RegistryKey::Board(board_id))
        {
            panic!("Board does not exist");
        }

        // Verify caller has admin permission on this board
        let contracts: ContractAddresses = env
            .storage()
            .instance()
            .get(&RegistryKey::Contracts)
            .expect("Not initialized");

        let args: Vec<Val> = Vec::from_array(
            &env,
            [board_id.into_val(&env), caller.into_val(&env)],
        );
        let perms: PermissionSet = env.invoke_contract(
            &contracts.permissions,
            &Symbol::new(&env, "get_permissions"),
            args,
        );

        if !perms.can_admin {
            panic!("Caller must be admin or owner");
        }

        // Set readonly status
        env.storage()
            .persistent()
            .set(&RegistryKey::BoardReadonly(board_id), &is_readonly);
    }

    /// Get the total number of boards
    pub fn board_count(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&RegistryKey::BoardCount)
            .unwrap_or(0)
    }

    /// Get service contract addresses
    pub fn get_contracts(env: Env) -> ContractAddresses {
        env.storage()
            .instance()
            .get(&RegistryKey::Contracts)
            .expect("Not initialized")
    }

    /// Get a contract address by its alias name.
    ///
    /// This enables the `form:@alias:method` protocol in soroban-render.
    /// Built-in aliases:
    /// - "registry" → This registry contract
    ///
    /// Registered aliases (set via init or set_contract):
    /// - "perms" → Permissions contract
    /// - "content" → Content contract
    /// - "theme" → Theme contract
    /// - "admin" → Admin contract
    /// - "profile" → User profile contract (if registered)
    /// - Any other alias registered via set_contract
    pub fn get_contract_by_alias(env: Env, alias: Symbol) -> Option<Address> {
        // Handle "registry" specially - return self
        if alias == Symbol::new(&env, "registry") {
            return Some(env.current_contract_address());
        }

        // Look up in generic contract storage
        env.storage()
            .instance()
            .get(&RegistryKey::Contract(alias))
    }

    /// Get a contract address by alias (convenience wrapper).
    pub fn get_contract(env: Env, alias: Symbol) -> Option<Address> {
        Self::get_contract_by_alias(env, alias)
    }

    /// Register or update a contract address by alias (admin only).
    ///
    /// This allows adding new service contracts (e.g., "profile") without
    /// code changes. The alias can then be used with `form:@alias:method`.
    pub fn set_contract(env: Env, alias: Symbol, address: Address, caller: Address) {
        Self::require_admin_auth(&env, &caller);

        env.storage()
            .instance()
            .set(&RegistryKey::Contract(alias), &address);
    }

    /// Get all admin addresses
    pub fn get_admins(env: Env) -> Vec<Address> {
        env.storage()
            .instance()
            .get(&RegistryKey::Admins)
            .expect("Not initialized")
    }

    /// Check if an address is an admin
    pub fn is_admin(env: Env, address: Address) -> bool {
        let admins: Vec<Address> = env
            .storage()
            .instance()
            .get(&RegistryKey::Admins)
            .unwrap_or(Vec::new(&env));

        for i in 0..admins.len() {
            if admins.get(i).unwrap() == address {
                return true;
            }
        }
        false
    }

    /// Add a new admin (requires existing admin auth)
    pub fn add_admin(env: Env, new_admin: Address, caller: Address) {
        Self::require_admin_auth(&env, &caller);

        let mut admins: Vec<Address> = env
            .storage()
            .instance()
            .get(&RegistryKey::Admins)
            .expect("Not initialized");

        // Check if already an admin
        for i in 0..admins.len() {
            if admins.get(i).unwrap() == new_admin {
                panic!("Already an admin");
            }
        }

        admins.push_back(new_admin);
        env.storage().instance().set(&RegistryKey::Admins, &admins);
    }

    /// Remove an admin (requires existing admin auth)
    /// Cannot remove the last admin
    pub fn remove_admin(env: Env, admin_to_remove: Address, caller: Address) {
        Self::require_admin_auth(&env, &caller);

        let admins: Vec<Address> = env
            .storage()
            .instance()
            .get(&RegistryKey::Admins)
            .expect("Not initialized");

        if admins.len() <= 1 {
            panic!("Cannot remove the last admin");
        }

        let mut new_admins = Vec::new(&env);
        let mut found = false;
        for i in 0..admins.len() {
            let admin = admins.get(i).unwrap();
            if admin == admin_to_remove {
                found = true;
            } else {
                new_admins.push_back(admin);
            }
        }

        if !found {
            panic!("Address is not an admin");
        }

        env.storage().instance().set(&RegistryKey::Admins, &new_admins);
    }

    /// Helper: require caller is an admin and has authorized
    fn require_admin_auth(env: &Env, caller: &Address) {
        let admins: Vec<Address> = env
            .storage()
            .instance()
            .get(&RegistryKey::Admins)
            .expect("Not initialized");

        let mut is_admin = false;
        for i in 0..admins.len() {
            if &admins.get(i).unwrap() == caller {
                is_admin = true;
                break;
            }
        }

        if !is_admin {
            panic!("Not an admin");
        }

        caller.require_auth();
    }

    /// Pause/unpause the registry
    pub fn set_paused(env: Env, paused: bool, caller: Address) {
        Self::require_admin_auth(&env, &caller);
        env.storage().instance().set(&RegistryKey::Paused, &paused);
    }

    /// Check if registry is paused
    pub fn is_paused(env: Env) -> bool {
        env.storage()
            .instance()
            .get(&RegistryKey::Paused)
            .unwrap_or(false)
    }

    /// Upgrade the contract WASM
    pub fn upgrade(env: Env, new_wasm_hash: BytesN<32>, caller: Address) {
        Self::require_admin_auth(&env, &caller);
        env.deployer().update_current_contract_wasm(new_wasm_hash);
    }

    /// Migrate from legacy Contracts struct to generic Contract(Symbol) storage.
    ///
    /// Call this after upgrading an existing registry to populate the new
    /// generic contract storage from the legacy ContractAddresses struct.
    /// Safe to call multiple times - will overwrite existing values.
    pub fn migrate_contracts(env: Env, caller: Address) {
        Self::require_admin_auth(&env, &caller);

        // Read from legacy Contracts struct
        if let Some(contracts) = env
            .storage()
            .instance()
            .get::<_, ContractAddresses>(&RegistryKey::Contracts)
        {
            // Populate generic Contract keys
            env.storage().instance().set(
                &RegistryKey::Contract(Symbol::new(&env, "perms")),
                &contracts.permissions,
            );
            env.storage().instance().set(
                &RegistryKey::Contract(Symbol::new(&env, "content")),
                &contracts.content,
            );
            env.storage().instance().set(
                &RegistryKey::Contract(Symbol::new(&env, "theme")),
                &contracts.theme,
            );
            env.storage().instance().set(
                &RegistryKey::Contract(Symbol::new(&env, "admin")),
                &contracts.admin,
            );
        }
    }

    /// Set board contract address for a board ID (admin only)
    pub fn set_board_contract(env: Env, board_id: u64, board_contract: Address, caller: Address) {
        Self::require_admin_auth(&env, &caller);

        // Verify board exists
        if !env
            .storage()
            .persistent()
            .has(&RegistryKey::Board(board_id))
        {
            panic!("Board does not exist");
        }

        env.storage()
            .persistent()
            .set(&RegistryKey::BoardContract(board_id), &board_contract);
    }

    /// Get board contract address for a board ID
    pub fn get_board_contract(env: Env, board_id: u64) -> Option<Address> {
        env.storage()
            .persistent()
            .get(&RegistryKey::BoardContract(board_id))
    }

    /// Increment thread count for a board (called when a thread is created)
    pub fn increment_thread_count(env: Env, board_id: u64) {
        if let Some(mut board) = env
            .storage()
            .persistent()
            .get::<_, BoardMeta>(&RegistryKey::Board(board_id))
        {
            board.thread_count += 1;
            env.storage()
                .persistent()
                .set(&RegistryKey::Board(board_id), &board);
        }
    }

    /// Upgrade another contract (admin only, proxies to contract's upgrade function)
    /// This allows the registry admin to upgrade any contract that trusts the registry.
    pub fn upgrade_contract(env: Env, contract_id: Address, new_wasm_hash: BytesN<32>, caller: Address) {
        Self::require_admin_auth(&env, &caller);

        // Call the target contract's upgrade function
        // The target contract will verify that we (the registry) are calling it
        let args: Vec<Val> = Vec::from_array(&env, [new_wasm_hash.into_val(&env)]);
        env.invoke_contract::<()>(
            &contract_id,
            &Symbol::new(&env, "upgrade"),
            args,
        );
    }

    /// Configure a board contract with permissions, content, and theme addresses (admin only)
    /// Used to update existing board contracts after adding these fields
    pub fn configure_board(env: Env, board_id: u64, caller: Address) {
        Self::require_admin_auth(&env, &caller);

        let contracts: ContractAddresses = env
            .storage()
            .instance()
            .get(&RegistryKey::Contracts)
            .expect("Not initialized");

        let board_contract: Address = env
            .storage()
            .persistent()
            .get(&RegistryKey::BoardContract(board_id))
            .expect("Board contract not found");

        // Set permissions address
        let perms_args: Vec<Val> = Vec::from_array(&env, [contracts.permissions.into_val(&env)]);
        env.invoke_contract::<()>(
            &board_contract,
            &Symbol::new(&env, "set_permissions"),
            perms_args,
        );

        // Set content address
        let content_args: Vec<Val> = Vec::from_array(&env, [contracts.content.into_val(&env)]);
        env.invoke_contract::<()>(
            &board_contract,
            &Symbol::new(&env, "set_content"),
            content_args,
        );

        // Set theme address
        let theme_args: Vec<Val> = Vec::from_array(&env, [contracts.theme.into_val(&env)]);
        env.invoke_contract::<()>(
            &board_contract,
            &Symbol::new(&env, "set_theme"),
            theme_args,
        );
    }

    // === Community Functions ===

    /// Set the community contract address (admin only)
    pub fn set_community_contract(env: Env, community: Address, caller: Address) {
        Self::require_admin_auth(&env, &caller);

        env.storage()
            .instance()
            .set(&RegistryKey::CommunityContract, &community);

        // Also register with alias "community" for form:@community:method
        env.storage().instance().set(
            &RegistryKey::Contract(Symbol::new(&env, "community")),
            &community,
        );
    }

    /// Get the community contract address
    pub fn get_community_contract(env: Env) -> Option<Address> {
        env.storage()
            .instance()
            .get(&RegistryKey::CommunityContract)
    }

    /// Set the voting contract address (admin only)
    pub fn set_voting_contract(env: Env, voting: Address, caller: Address) {
        Self::require_admin_auth(&env, &caller);

        env.storage()
            .instance()
            .set(&RegistryKey::VotingContract, &voting);

        // Also register with alias "voting" for form:@voting:method
        env.storage().instance().set(
            &RegistryKey::Contract(Symbol::new(&env, "voting")),
            &voting,
        );
    }

    /// Get the voting contract address
    pub fn get_voting_contract(env: Env) -> Option<Address> {
        env.storage()
            .instance()
            .get(&RegistryKey::VotingContract)
    }

    /// Create a board within a community
    /// This creates the board and associates it with the community
    pub fn create_board_in_community(
        env: Env,
        community_id: u64,
        name: String,
        description: String,
        is_private: String,
        is_listed: String,
        caller: Address,
    ) -> u64 {
        caller.require_auth();

        // Get community contract
        let community_contract: Address = env
            .storage()
            .instance()
            .get(&RegistryKey::CommunityContract)
            .expect("Community contract not set");

        // Verify the community exists by invoking the community contract
        // The community contract will panic if the community doesn't exist when we add the board

        // Create the board using existing create_board
        let board_id = Self::create_board(
            env.clone(),
            name,
            description,
            is_private,
            is_listed,
            caller,
        );

        // Store the board-community association
        env.storage()
            .persistent()
            .set(&RegistryKey::BoardCommunity(board_id), &community_id);

        // Notify the community contract about the new board
        let add_args: Vec<Val> = Vec::from_array(
            &env,
            [community_id.into_val(&env), board_id.into_val(&env)],
        );
        env.invoke_contract::<()>(
            &community_contract,
            &Symbol::new(&env, "add_board"),
            add_args,
        );

        board_id
    }

    /// Get the community ID for a board (if any)
    pub fn get_board_community(env: Env, board_id: u64) -> Option<u64> {
        env.storage()
            .persistent()
            .get(&RegistryKey::BoardCommunity(board_id))
    }

    /// Move a board to a community (board owner only)
    pub fn move_board_to_community(
        env: Env,
        board_id: u64,
        community_id: u64,
        caller: Address,
    ) {
        caller.require_auth();

        // Verify board exists
        let board: BoardMeta = env
            .storage()
            .persistent()
            .get(&RegistryKey::Board(board_id))
            .expect("Board does not exist");

        // Verify caller is board creator (owner)
        if caller != board.creator {
            panic!("Only board owner can move board to community");
        }

        // Get community contract
        let community_contract: Address = env
            .storage()
            .instance()
            .get(&RegistryKey::CommunityContract)
            .expect("Community contract not set");

        // If board was already in a community, remove it
        if let Some(old_community_id) = env
            .storage()
            .persistent()
            .get::<_, u64>(&RegistryKey::BoardCommunity(board_id))
        {
            let remove_args: Vec<Val> = Vec::from_array(
                &env,
                [
                    old_community_id.into_val(&env),
                    board_id.into_val(&env),
                    caller.clone().into_val(&env),
                ],
            );
            let _ = env.try_invoke_contract::<(), soroban_sdk::Error>(
                &community_contract,
                &Symbol::new(&env, "remove_board"),
                remove_args,
            );
        }

        // Store the new board-community association
        env.storage()
            .persistent()
            .set(&RegistryKey::BoardCommunity(board_id), &community_id);

        // Notify the community contract about the new board
        let add_args: Vec<Val> = Vec::from_array(
            &env,
            [community_id.into_val(&env), board_id.into_val(&env)],
        );
        env.invoke_contract::<()>(
            &community_contract,
            &Symbol::new(&env, "add_board"),
            add_args,
        );
    }

    /// Remove a board from its community (returns it to standalone)
    pub fn remove_board_from_community(env: Env, board_id: u64, caller: Address) {
        caller.require_auth();

        // Verify board exists
        let board: BoardMeta = env
            .storage()
            .persistent()
            .get(&RegistryKey::Board(board_id))
            .expect("Board does not exist");

        // Verify caller is board creator (owner)
        if caller != board.creator {
            panic!("Only board owner can remove board from community");
        }

        // Get current community
        let community_id: u64 = env
            .storage()
            .persistent()
            .get(&RegistryKey::BoardCommunity(board_id))
            .expect("Board is not in a community");

        // Get community contract
        let community_contract: Address = env
            .storage()
            .instance()
            .get(&RegistryKey::CommunityContract)
            .expect("Community contract not set");

        // Notify community contract to remove the board
        let remove_args: Vec<Val> = Vec::from_array(
            &env,
            [
                community_id.into_val(&env),
                board_id.into_val(&env),
                caller.into_val(&env),
            ],
        );
        let _ = env.try_invoke_contract::<(), soroban_sdk::Error>(
            &community_contract,
            &Symbol::new(&env, "remove_board"),
            remove_args,
        );

        // Remove the association
        env.storage()
            .persistent()
            .remove(&RegistryKey::BoardCommunity(board_id));
    }

}

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::Env;

    #[test]
    fn test_init_and_create_board() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsRegistry, ());
        let client = BoardsRegistryClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let admins = Vec::from_array(&env, [admin.clone()]);
        let permissions = Address::generate(&env);
        let content = Address::generate(&env);
        let theme = Address::generate(&env);
        let admin_contract = Address::generate(&env);

        // Initialize
        client.init(&admins, &permissions, &content, &theme, &admin_contract);

        // Verify admin
        assert!(client.is_admin(&admin));

        // Create a board
        let creator = Address::generate(&env);
        let name = String::from_str(&env, "General");
        let desc = String::from_str(&env, "General discussion");
        let is_private = String::from_str(&env, "false");
        let is_listed = String::from_str(&env, "true");

        let board_id = client.create_board(&name, &desc, &is_private, &is_listed, &creator);
        assert_eq!(board_id, 0);

        // Verify board
        let board = client.get_board(&board_id).unwrap();
        assert_eq!(board.name, name);
        assert_eq!(board.creator, creator);
        assert_eq!(board.thread_count, 0);

        // Verify count
        assert_eq!(client.board_count(), 1);
    }

    #[test]
    fn test_list_boards() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsRegistry, ());
        let client = BoardsRegistryClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let permissions = Address::generate(&env);
        let content = Address::generate(&env);
        let theme = Address::generate(&env);
        let admin_contract = Address::generate(&env);

        let admins = Vec::from_array(&env, [admin.clone()]);
        client.init(&admins, &permissions, &content, &theme, &admin_contract);

        let creator = Address::generate(&env);
        let is_private = String::from_str(&env, "false");
        let is_listed = String::from_str(&env, "true");

        // Create multiple boards with different names
        let names = ["Board1", "Board2", "Board3", "Board4", "Board5"];
        for name in names.iter() {
            let name = String::from_str(&env, name);
            let desc = String::from_str(&env, "Description");
            client.create_board(&name, &desc, &is_private, &is_listed, &creator);
        }

        // List with pagination
        let boards = client.list_boards(&0, &3);
        assert_eq!(boards.len(), 3);

        let boards = client.list_boards(&3, &10);
        assert_eq!(boards.len(), 2);
    }

    #[test]
    fn test_pause_and_unpause() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsRegistry, ());
        let client = BoardsRegistryClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let permissions = Address::generate(&env);
        let content = Address::generate(&env);
        let theme = Address::generate(&env);
        let admin_contract = Address::generate(&env);

        let admins = Vec::from_array(&env, [admin.clone()]);
        client.init(&admins, &permissions, &content, &theme, &admin_contract);

        // Verify initially not paused
        assert!(!client.is_paused());

        // Pause
        client.set_paused(&true);
        assert!(client.is_paused());

        // Unpause
        client.set_paused(&false);
        assert!(!client.is_paused());
    }

    #[test]
    #[should_panic(expected = "Registry is paused")]
    fn test_create_board_when_paused() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsRegistry, ());
        let client = BoardsRegistryClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let permissions = Address::generate(&env);
        let content = Address::generate(&env);
        let theme = Address::generate(&env);
        let admin_contract = Address::generate(&env);

        let admins = Vec::from_array(&env, [admin.clone()]);
        client.init(&admins, &permissions, &content, &theme, &admin_contract);

        // Pause the registry
        client.set_paused(&true);

        // Try to create a board - should panic
        let creator = Address::generate(&env);
        let name = String::from_str(&env, "Test");
        let desc = String::from_str(&env, "Test desc");
        let is_private = String::from_str(&env, "false");
        let is_listed = String::from_str(&env, "true");
        client.create_board(&name, &desc, &is_private, &is_listed, &creator);
    }

    #[test]
    fn test_get_nonexistent_board() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsRegistry, ());
        let client = BoardsRegistryClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let permissions = Address::generate(&env);
        let content = Address::generate(&env);
        let theme = Address::generate(&env);
        let admin_contract = Address::generate(&env);

        let admins = Vec::from_array(&env, [admin.clone()]);
        client.init(&admins, &permissions, &content, &theme, &admin_contract);

        // Try to get a board that doesn't exist
        let board = client.get_board(&999);
        assert!(board.is_none());
    }

    #[test]
    fn test_get_contracts() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsRegistry, ());
        let client = BoardsRegistryClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let permissions = Address::generate(&env);
        let content = Address::generate(&env);
        let theme = Address::generate(&env);
        let admin_contract = Address::generate(&env);

        let admins = Vec::from_array(&env, [admin.clone()]);
        client.init(&admins, &permissions, &content, &theme, &admin_contract);

        let contracts = client.get_contracts();
        assert_eq!(contracts.permissions, permissions);
        assert_eq!(contracts.content, content);
        assert_eq!(contracts.theme, theme);
        assert_eq!(contracts.admin, admin_contract);
    }

    #[test]
    fn test_private_board() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsRegistry, ());
        let client = BoardsRegistryClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let permissions = Address::generate(&env);
        let content = Address::generate(&env);
        let theme = Address::generate(&env);
        let admin_contract = Address::generate(&env);

        let admins = Vec::from_array(&env, [admin.clone()]);
        client.init(&admins, &permissions, &content, &theme, &admin_contract);

        let creator = Address::generate(&env);
        let name = String::from_str(&env, "Private Board");
        let desc = String::from_str(&env, "Secret discussions");
        let is_private = String::from_str(&env, "true");
        let is_listed = String::from_str(&env, "true");

        let board_id = client.create_board(&name, &desc, &is_private, &is_listed, &creator);
        let board = client.get_board(&board_id).unwrap();

        assert!(board.is_private);
        assert!(!board.is_readonly);
    }

    #[test]
    fn test_board_count_increment() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsRegistry, ());
        let client = BoardsRegistryClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let permissions = Address::generate(&env);
        let content = Address::generate(&env);
        let theme = Address::generate(&env);
        let admin_contract = Address::generate(&env);

        let admins = Vec::from_array(&env, [admin.clone()]);
        client.init(&admins, &permissions, &content, &theme, &admin_contract);

        assert_eq!(client.board_count(), 0);

        let creator = Address::generate(&env);
        let desc = String::from_str(&env, "Desc");
        let is_private = String::from_str(&env, "false");
        let is_listed = String::from_str(&env, "true");

        // Create boards with unique names and verify count increments
        let names = ["Board0", "Board1", "Board2", "Board3", "Board4"];
        for (i, name_str) in names.iter().enumerate() {
            let name = String::from_str(&env, name_str);
            let id = client.create_board(&name, &desc, &is_private, &is_listed, &creator);
            assert_eq!(id, i as u64);
            assert_eq!(client.board_count(), (i + 1) as u64);
        }
    }

    #[test]
    fn test_empty_list_boards() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsRegistry, ());
        let client = BoardsRegistryClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let permissions = Address::generate(&env);
        let content = Address::generate(&env);
        let theme = Address::generate(&env);
        let admin_contract = Address::generate(&env);

        let admins = Vec::from_array(&env, [admin.clone()]);
        client.init(&admins, &permissions, &content, &theme, &admin_contract);

        // List boards when none exist
        let boards = client.list_boards(&0, &10);
        assert_eq!(boards.len(), 0);
    }

    #[test]
    fn test_get_contract_by_alias() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsRegistry, ());
        let client = BoardsRegistryClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let permissions = Address::generate(&env);
        let content = Address::generate(&env);
        let theme = Address::generate(&env);
        let admin_contract = Address::generate(&env);

        let admins = Vec::from_array(&env, [admin.clone()]);
        client.init(&admins, &permissions, &content, &theme, &admin_contract);

        // Test each alias
        assert_eq!(
            client.get_contract_by_alias(&Symbol::new(&env, "registry")),
            Some(contract_id)
        );
        assert_eq!(
            client.get_contract_by_alias(&Symbol::new(&env, "perms")),
            Some(permissions)
        );
        assert_eq!(
            client.get_contract_by_alias(&Symbol::new(&env, "content")),
            Some(content)
        );
        assert_eq!(
            client.get_contract_by_alias(&Symbol::new(&env, "theme")),
            Some(theme)
        );
        assert_eq!(
            client.get_contract_by_alias(&Symbol::new(&env, "admin")),
            Some(admin_contract)
        );

        // Test unknown alias
        assert_eq!(
            client.get_contract_by_alias(&Symbol::new(&env, "unknown")),
            None
        );
    }

    #[test]
    fn test_set_contract() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsRegistry, ());
        let client = BoardsRegistryClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let permissions = Address::generate(&env);
        let content = Address::generate(&env);
        let theme = Address::generate(&env);
        let admin_contract = Address::generate(&env);

        let admins = Vec::from_array(&env, [admin.clone()]);
        client.init(&admins, &permissions, &content, &theme, &admin_contract);

        // Register a new contract (e.g., profile service)
        let profile_contract = Address::generate(&env);
        client.set_contract(&Symbol::new(&env, "profile"), &profile_contract);

        // Should be retrievable via get_contract_by_alias
        assert_eq!(
            client.get_contract_by_alias(&Symbol::new(&env, "profile")),
            Some(profile_contract.clone())
        );

        // Should also work via get_contract convenience method
        assert_eq!(
            client.get_contract(&Symbol::new(&env, "profile")),
            Some(profile_contract)
        );

        // Can update existing contract
        let new_permissions = Address::generate(&env);
        client.set_contract(&Symbol::new(&env, "perms"), &new_permissions);
        assert_eq!(
            client.get_contract(&Symbol::new(&env, "perms")),
            Some(new_permissions)
        );
    }

    #[test]
    fn test_listed_unlisted_boards() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsRegistry, ());
        let client = BoardsRegistryClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let permissions = Address::generate(&env);
        let content = Address::generate(&env);
        let theme = Address::generate(&env);
        let admin_contract = Address::generate(&env);

        let admins = Vec::from_array(&env, [admin.clone()]);
        client.init(&admins, &permissions, &content, &theme, &admin_contract);

        let creator = Address::generate(&env);
        let desc = String::from_str(&env, "Description");
        let is_private = String::from_str(&env, "false");
        let is_listed = String::from_str(&env, "true");
        let is_unlisted = String::from_str(&env, "false");

        // Create a listed board
        let name1 = String::from_str(&env, "Listed Board");
        let board1_id = client.create_board(&name1, &desc, &is_private, &is_listed, &creator);

        // Create an unlisted board
        let name2 = String::from_str(&env, "Unlisted Board");
        let board2_id = client.create_board(&name2, &desc, &is_private, &is_unlisted, &creator);

        // Verify listed status
        assert!(client.get_board_listed(&board1_id));
        assert!(!client.get_board_listed(&board2_id));

        // list_boards should return both
        let all_boards = client.list_boards(&0, &10);
        assert_eq!(all_boards.len(), 2);

        // list_listed_boards should return only listed board
        let listed_boards = client.list_listed_boards(&0, &10);
        assert_eq!(listed_boards.len(), 1);
        assert_eq!(listed_boards.get(0).unwrap().id, board1_id);

        // Both boards should still be accessible directly
        assert!(client.get_board(&board1_id).is_some());
        assert!(client.get_board(&board2_id).is_some());
    }

    #[test]
    fn test_list_listed_boards_pagination() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsRegistry, ());
        let client = BoardsRegistryClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let permissions = Address::generate(&env);
        let content = Address::generate(&env);
        let theme = Address::generate(&env);
        let admin_contract = Address::generate(&env);

        let admins = Vec::from_array(&env, [admin.clone()]);
        client.init(&admins, &permissions, &content, &theme, &admin_contract);

        let creator = Address::generate(&env);
        let desc = String::from_str(&env, "Description");
        let is_private = String::from_str(&env, "false");
        let is_listed = String::from_str(&env, "true");
        let is_unlisted = String::from_str(&env, "false");

        // Create 5 boards: listed, unlisted, listed, unlisted, listed
        let names = ["Board0", "Board1", "Board2", "Board3", "Board4"];
        for (i, name) in names.iter().enumerate() {
            let name = String::from_str(&env, name);
            let listed = if i % 2 == 0 { &is_listed } else { &is_unlisted };
            client.create_board(&name, &desc, &is_private, listed, &creator);
        }

        // Total boards = 5, listed boards = 3 (indices 0, 2, 4)
        let all_boards = client.list_boards(&0, &10);
        assert_eq!(all_boards.len(), 5);

        let listed_boards = client.list_listed_boards(&0, &10);
        assert_eq!(listed_boards.len(), 3);

        // Test pagination - skip 1, get 2
        let listed_boards_page = client.list_listed_boards(&1, &2);
        assert_eq!(listed_boards_page.len(), 2);
    }

    #[test]
    fn test_get_board_by_name() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsRegistry, ());
        let client = BoardsRegistryClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let permissions = Address::generate(&env);
        let content = Address::generate(&env);
        let theme = Address::generate(&env);
        let admin_contract = Address::generate(&env);

        let admins = Vec::from_array(&env, [admin.clone()]);
        client.init(&admins, &permissions, &content, &theme, &admin_contract);

        let creator = Address::generate(&env);
        let name = String::from_str(&env, "TestBoard");
        let desc = String::from_str(&env, "Test Description");
        let is_private = String::from_str(&env, "false");
        let is_listed = String::from_str(&env, "true");

        let board_id = client.create_board(&name, &desc, &is_private, &is_listed, &creator);

        // Lookup by name should work
        let board = client.get_board_by_name(&name).unwrap();
        assert_eq!(board.id, board_id);
        assert_eq!(board.name, name);

        // Lookup nonexistent name should return None
        let nonexistent = String::from_str(&env, "NonExistent");
        assert!(client.get_board_by_name(&nonexistent).is_none());
    }

    #[test]
    fn test_board_aliases() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsRegistry, ());
        let client = BoardsRegistryClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let permissions = Address::generate(&env);
        let content = Address::generate(&env);
        let theme = Address::generate(&env);
        let admin_contract = Address::generate(&env);

        let admins = Vec::from_array(&env, [admin.clone()]);
        client.init(&admins, &permissions, &content, &theme, &admin_contract);

        let creator = Address::generate(&env);
        let name = String::from_str(&env, "OriginalName");
        let desc = String::from_str(&env, "Description");
        let is_private = String::from_str(&env, "false");
        let is_listed = String::from_str(&env, "true");

        let board_id = client.create_board(&name, &desc, &is_private, &is_listed, &creator);

        // Initially no aliases
        let aliases = client.get_board_aliases(&board_id);
        assert_eq!(aliases.len(), 0);
    }

    #[test]
    #[should_panic(expected = "Board name already exists")]
    fn test_duplicate_board_name() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsRegistry, ());
        let client = BoardsRegistryClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let permissions = Address::generate(&env);
        let content = Address::generate(&env);
        let theme = Address::generate(&env);
        let admin_contract = Address::generate(&env);

        let admins = Vec::from_array(&env, [admin.clone()]);
        client.init(&admins, &permissions, &content, &theme, &admin_contract);

        let creator = Address::generate(&env);
        let name = String::from_str(&env, "SameName");
        let desc = String::from_str(&env, "Description");
        let is_private = String::from_str(&env, "false");
        let is_listed = String::from_str(&env, "true");

        // First create should succeed
        client.create_board(&name, &desc, &is_private, &is_listed, &creator);

        // Second create with same name should fail
        client.create_board(&name, &desc, &is_private, &is_listed, &creator);
    }
}
