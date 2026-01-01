#![no_std]

//! # Boards Registry Contract
//!
//! A pure contract address registry for service discovery.
//!
//! ## Purpose
//!
//! The registry enables contracts to find each other by alias:
//! - Contract address registration by alias (e.g., "perms" -> Address)
//! - Alias lookups via `get_contract_by_alias` / `get_contract`
//! - Board contract address discovery by ID
//! - WASM hash storage for deploying new board contracts
//!
//! ## NOT Handled Here
//!
//! All domain logic lives in dedicated contracts:
//! - Board creation/metadata → boards-board
//! - Community management → boards-community
//! - Content storage → boards-content
//! - Permissions/roles → boards-permissions

use soroban_sdk::{contract, contractimpl, contracttype, Address, BytesN, Env, IntoVal, Symbol, Val, Vec};

/// Storage keys for the registry contract
///
/// The registry is a pure contract discovery service. It stores:
/// - Contract addresses by alias (for form:@alias:method protocol)
/// - Board contract addresses for discovery (board_id -> Address)
/// - Admin list for registry operations
/// - WASM hash for deploying new board contracts
#[contracttype]
#[derive(Clone)]
pub enum RegistryKey {
    /// Contract admin addresses (multiple admins supported)
    Admins,
    /// Contract addresses for shared services (legacy, for backwards compatibility)
    Contracts,
    /// Generic contract storage by alias (e.g., "perms", "content", "theme", "profile")
    Contract(Symbol),
    /// Board contract address by ID (for discovery only)
    BoardContract(u64),
    /// Number of registered board contracts (next available ID)
    BoardContractCount,
    /// WASM hash for deploying board contracts
    BoardWasmHash,
    /// Global pause flag
    Paused,
}

/// Addresses of shared service contracts (legacy, for backwards compatibility)
#[contracttype]
#[derive(Clone)]
pub struct ContractAddresses {
    pub permissions: Address,
    pub content: Address,
    pub theme: Address,
    pub admin: Address,
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
        env.storage().instance().set(&RegistryKey::BoardContractCount, &0u64);
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

    // =========================================================================
    // WASM Hash Management
    // =========================================================================

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

    // =========================================================================
    // Contract Discovery (Alias Lookups)
    // =========================================================================

    /// Get service contract addresses (legacy)
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
    /// - "community" → Community contract
    /// - "voting" → Voting contract
    /// - "config" → Config contract
    /// - "profile" → User profile contract
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

    // =========================================================================
    // Board Contract Discovery
    // =========================================================================

    /// Get the number of registered board contracts (next available ID)
    pub fn board_contract_count(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&RegistryKey::BoardContractCount)
            .unwrap_or(0)
    }

    /// Register a board contract address and return its ID
    ///
    /// Called by boards-main when deploying a new board contract.
    /// Returns the board ID assigned to this contract.
    pub fn register_board_contract(env: Env, board_contract: Address, caller: Address) -> u64 {
        Self::require_admin_auth(&env, &caller);

        // Get next board ID
        let board_id: u64 = env
            .storage()
            .instance()
            .get(&RegistryKey::BoardContractCount)
            .unwrap_or(0);

        // Store the board contract address
        env.storage()
            .persistent()
            .set(&RegistryKey::BoardContract(board_id), &board_contract);

        // Increment count
        env.storage()
            .instance()
            .set(&RegistryKey::BoardContractCount, &(board_id + 1));

        board_id
    }

    /// Get board contract address for a board ID
    pub fn get_board_contract(env: Env, board_id: u64) -> Option<Address> {
        env.storage()
            .persistent()
            .get(&RegistryKey::BoardContract(board_id))
    }

    // =========================================================================
    // Admin Management
    // =========================================================================

    /// Get all admin addresses (delegates to permissions contract)
    pub fn get_admins(env: Env) -> Vec<Address> {
        // Try permissions contract first
        if let Some(contracts) = env
            .storage()
            .instance()
            .get::<_, ContractAddresses>(&RegistryKey::Contracts)
        {
            let args: Vec<Val> = Vec::new(&env);
            if let Ok(Ok(admins)) = env.try_invoke_contract::<Vec<Address>, soroban_sdk::Error>(
                &contracts.permissions,
                &Symbol::new(&env, "get_site_admins"),
                args,
            ) {
                if admins.len() > 0 {
                    return admins;
                }
            }
        }
        // Fallback to local storage for migration compatibility
        env.storage()
            .instance()
            .get(&RegistryKey::Admins)
            .unwrap_or(Vec::new(&env))
    }

    /// Check if an address is an admin (checks both permissions contract and local storage)
    pub fn is_admin(env: Env, address: Address) -> bool {
        // Check permissions contract first
        if let Some(contracts) = env
            .storage()
            .instance()
            .get::<_, ContractAddresses>(&RegistryKey::Contracts)
        {
            let args: Vec<Val> = Vec::from_array(&env, [address.clone().into_val(&env)]);
            if let Ok(Ok(true)) = env.try_invoke_contract::<bool, soroban_sdk::Error>(
                &contracts.permissions,
                &Symbol::new(&env, "is_site_admin"),
                args,
            ) {
                return true;
            }
        }
        // Also check local storage (for backwards compatibility with pre-migration data)
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

    /// Add a new admin (delegates to permissions contract)
    pub fn add_admin(env: Env, new_admin: Address, caller: Address) {
        // Delegate to permissions contract
        if let Some(contracts) = env
            .storage()
            .instance()
            .get::<_, ContractAddresses>(&RegistryKey::Contracts)
        {
            let args: Vec<Val> = Vec::from_array(
                &env,
                [new_admin.clone().into_val(&env), caller.clone().into_val(&env)],
            );
            // This will require_auth on the caller within permissions contract
            env.invoke_contract::<()>(
                &contracts.permissions,
                &Symbol::new(&env, "add_site_admin"),
                args,
            );
            return;
        }
        // Fallback to local storage for migration compatibility
        Self::require_admin_auth(&env, &caller);
        let mut admins: Vec<Address> = env
            .storage()
            .instance()
            .get(&RegistryKey::Admins)
            .expect("Not initialized");

        for i in 0..admins.len() {
            if admins.get(i).unwrap() == new_admin {
                panic!("Already an admin");
            }
        }

        admins.push_back(new_admin);
        env.storage().instance().set(&RegistryKey::Admins, &admins);
    }

    /// Remove an admin (delegates to permissions contract)
    /// Cannot remove the last admin
    pub fn remove_admin(env: Env, admin_to_remove: Address, caller: Address) {
        // Delegate to permissions contract
        if let Some(contracts) = env
            .storage()
            .instance()
            .get::<_, ContractAddresses>(&RegistryKey::Contracts)
        {
            let args: Vec<Val> = Vec::from_array(
                &env,
                [admin_to_remove.clone().into_val(&env), caller.clone().into_val(&env)],
            );
            // This will require_auth on the caller within permissions contract
            env.invoke_contract::<()>(
                &contracts.permissions,
                &Symbol::new(&env, "remove_site_admin"),
                args,
            );
            return;
        }
        // Fallback to local storage for migration compatibility
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

    // =========================================================================
    // Pause / Upgrade
    // =========================================================================

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

    // =========================================================================
    // Migration
    // =========================================================================

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
}

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::Env;

    #[test]
    fn test_init() {
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

        // Verify board contract count starts at 0
        assert_eq!(client.board_contract_count(), 0);
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
        client.set_paused(&true, &admin);
        assert!(client.is_paused());

        // Unpause
        client.set_paused(&false, &admin);
        assert!(!client.is_paused());
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
        client.set_contract(&Symbol::new(&env, "profile"), &profile_contract, &admin);

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
        client.set_contract(&Symbol::new(&env, "perms"), &new_permissions, &admin);
        assert_eq!(
            client.get_contract(&Symbol::new(&env, "perms")),
            Some(new_permissions)
        );
    }

    #[test]
    fn test_register_board_contract() {
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

        // Initially no board contracts
        assert_eq!(client.board_contract_count(), 0);

        // Register first board contract
        let board1 = Address::generate(&env);
        let id1 = client.register_board_contract(&board1, &admin);
        assert_eq!(id1, 0);
        assert_eq!(client.board_contract_count(), 1);
        assert_eq!(client.get_board_contract(&0), Some(board1.clone()));

        // Register second board contract
        let board2 = Address::generate(&env);
        let id2 = client.register_board_contract(&board2, &admin);
        assert_eq!(id2, 1);
        assert_eq!(client.board_contract_count(), 2);
        assert_eq!(client.get_board_contract(&1), Some(board2.clone()));

        // First board still accessible
        assert_eq!(client.get_board_contract(&0), Some(board1));

        // Non-existent board returns None
        assert_eq!(client.get_board_contract(&999), None);
    }
}
