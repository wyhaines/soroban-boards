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

use soroban_sdk::{contract, contractimpl, contracttype, Address, Bytes, BytesN, Env, IntoVal, Symbol, Val, Vec};

// Note: Board contract mapping (BoardContract, BoardContractCount, BoardWasmHash) removed.
// Boards are now stored in a single boards-board contract, accessed via the "board" alias.

/// Storage keys for the registry contract
///
/// The registry is a pure contract discovery service. It stores:
/// - Contract addresses by alias (for form:@alias:method protocol)
/// - Admin list for registry operations
#[contracttype]
#[derive(Clone)]
pub enum RegistryKey {
    /// Contract admin addresses (multiple admins supported)
    Admins,
    /// Contract addresses for shared services (legacy, for backwards compatibility)
    Contracts,
    /// Generic contract storage by alias (e.g., "perms", "content", "theme", "profile", "board")
    Contract(Symbol),
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

    /// Render `{{aliases ...}}` tag with all registered contract aliases.
    ///
    /// This generates a tag that soroban-render viewers parse to enable
    /// friendly alias names in `{{include contract=alias ...}}` directives.
    ///
    /// Other contracts can call this cross-contract to get alias definitions:
    /// ```rust,ignore
    /// let aliases_tag: Bytes = env.invoke_contract(
    ///     &registry,
    ///     &Symbol::new(env, "render_aliases"),
    ///     Vec::new(env),
    /// );
    /// ```
    ///
    /// Returns Bytes containing `{{aliases registry=C... perms=C... theme=C... ...}}`
    pub fn render_aliases(env: Env) -> Bytes {
        let mut result = Bytes::from_slice(&env, b"{{aliases ");

        // Add registry itself first
        let self_addr = env.current_contract_address();
        result.append(&Bytes::from_slice(&env, b"registry="));
        result.append(&Self::address_to_bytes(&env, &self_addr));
        result.append(&Bytes::from_slice(&env, b" "));

        // Known aliases with their string representations
        // (alias_name_bytes, alias_symbol_str)
        const ALIASES: &[(&[u8], &str)] = &[
            (b"main", "main"),
            (b"theme", "theme"),
            (b"admin", "admin"),
            (b"perms", "perms"),
            (b"content", "content"),
            (b"community", "community"),
            (b"config", "config"),
            (b"pages", "pages"),
            (b"board", "board"),
            (b"voting", "voting"),
            (b"profile", "profile"),
        ];

        for (alias_bytes, alias_str) in ALIASES {
            let alias = Symbol::new(&env, alias_str);
            if let Some(addr) = env.storage().instance().get(&RegistryKey::Contract(alias)) {
                result.append(&Bytes::from_slice(&env, alias_bytes));
                result.append(&Bytes::from_slice(&env, b"="));
                result.append(&Self::address_to_bytes(&env, &addr));
                result.append(&Bytes::from_slice(&env, b" "));
            }
        }

        result.append(&Bytes::from_slice(&env, b"}}"));
        result
    }

    /// Convert an Address to its contract ID string as Bytes
    fn address_to_bytes(env: &Env, addr: &Address) -> Bytes {
        let addr_str = addr.to_string();
        let len = addr_str.len() as usize;
        let mut buf = [0u8; 56]; // Contract IDs are 56 chars
        let copy_len = core::cmp::min(len, 56);
        addr_str.copy_into_slice(&mut buf[..copy_len]);
        Bytes::from_slice(env, &buf[..copy_len])
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
    // Admin Management
    // =========================================================================

    /// Get all admin addresses.
    ///
    /// This returns the registry's local Admins list which is THE single
    /// source of truth for admin status. Does NOT call other contracts.
    pub fn get_admins(env: Env) -> Vec<Address> {
        env.storage()
            .instance()
            .get(&RegistryKey::Admins)
            .unwrap_or(Vec::new(&env))
    }

    /// Check if an address is an admin.
    ///
    /// This is THE source of truth for admin status. The registry's Admins list
    /// is the single admin list for the entire system. All other contracts
    /// should call this function to check admin status.
    ///
    /// IMPORTANT: This function ONLY checks local storage. It does NOT call
    /// any other contracts to avoid re-entry issues. Other contracts (like
    /// permissions.is_site_admin) should call this function.
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

    /// Add a new admin to the registry's admin list.
    ///
    /// The registry's Admins list is THE single source of truth for admin status.
    /// This function directly modifies the local list (no delegation to avoid loops).
    pub fn add_admin(env: Env, new_admin: Address, caller: Address) {
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

    /// Remove an admin from the registry's admin list.
    ///
    /// The registry's Admins list is THE single source of truth for admin status.
    /// This function directly modifies the local list (no delegation to avoid loops).
    /// Cannot remove the last admin.
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

    /// Add an admin directly to local storage (bypasses permissions contract).
    /// Use this to add contract addresses that need admin rights but can't sign.
    /// Requires existing admin auth.
    pub fn add_admin_direct(env: Env, new_admin: Address, caller: Address) {
        Self::require_admin_auth(&env, &caller);

        let mut admins: Vec<Address> = env
            .storage()
            .instance()
            .get(&RegistryKey::Admins)
            .expect("Not initialized");

        // Check not already an admin
        for i in 0..admins.len() {
            if admins.get(i).unwrap() == new_admin {
                panic!("Already an admin");
            }
        }

        admins.push_back(new_admin);
        env.storage().instance().set(&RegistryKey::Admins, &admins);
    }

    /// Helper: require caller is an admin and has authorized
    fn require_admin_auth(env: &Env, caller: &Address) {
        Self::require_admin(env, caller);
        caller.require_auth();
    }

    /// Helper: require caller is an admin (no auth check - for internal/contract calls)
    fn require_admin(env: &Env, caller: &Address) {
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

    /// Helper to setup a boards-registry contract with all dependencies
    fn setup_registry(env: &Env) -> (BoardsRegistryClient, Address, Address, Address, Address, Address, Address) {
        env.mock_all_auths();

        let contract_id = env.register(BoardsRegistry, ());
        let client = BoardsRegistryClient::new(env, &contract_id);

        let admin = Address::generate(env);
        let permissions = Address::generate(env);
        let content = Address::generate(env);
        let theme = Address::generate(env);
        let admin_contract = Address::generate(env);

        let admins = Vec::from_array(env, [admin.clone()]);
        client.init(&admins, &permissions, &content, &theme, &admin_contract);

        (client, contract_id, admin, permissions, content, theme, admin_contract)
    }

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
    }

    #[test]
    #[should_panic(expected = "Already initialized")]
    fn test_double_init() {
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

        client.init(&admins, &permissions, &content, &theme, &admin_contract);
        // Second init should panic
        client.init(&admins, &permissions, &content, &theme, &admin_contract);
    }

    #[test]
    fn test_pause_and_unpause() {
        let env = Env::default();
        let (client, _, admin, _, _, _, _) = setup_registry(&env);

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
        let (client, _, _, permissions, content, theme, admin_contract) = setup_registry(&env);

        let contracts = client.get_contracts();
        assert_eq!(contracts.permissions, permissions);
        assert_eq!(contracts.content, content);
        assert_eq!(contracts.theme, theme);
        assert_eq!(contracts.admin, admin_contract);
    }

    #[test]
    fn test_get_contract_by_alias() {
        let env = Env::default();
        let (client, contract_id, _, permissions, content, theme, admin_contract) = setup_registry(&env);

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
        let (client, _, admin, _, _, _, _) = setup_registry(&env);

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
    fn test_add_admin() {
        let env = Env::default();
        let (client, _, admin, _, _, _, _) = setup_registry(&env);

        // Add a second admin
        let new_admin = Address::generate(&env);
        client.add_admin(&new_admin, &admin);

        // Verify both are admins
        assert!(client.is_admin(&admin));
        assert!(client.is_admin(&new_admin));

        // Verify admins list
        let admins = client.get_admins();
        assert_eq!(admins.len(), 2);
    }

    #[test]
    fn test_remove_admin() {
        let env = Env::default();
        let (client, _, admin, _, _, _, _) = setup_registry(&env);

        // Add a second admin first
        let new_admin = Address::generate(&env);
        client.add_admin(&new_admin, &admin);
        assert!(client.is_admin(&new_admin));

        // Remove the second admin
        client.remove_admin(&new_admin, &admin);
        assert!(!client.is_admin(&new_admin));

        // Original admin should still be admin
        assert!(client.is_admin(&admin));
    }

    #[test]
    #[should_panic(expected = "Cannot remove the last admin")]
    fn test_cannot_remove_last_admin() {
        let env = Env::default();
        let (client, _, admin, _, _, _, _) = setup_registry(&env);

        // Try to remove the only admin - should panic
        client.remove_admin(&admin, &admin);
    }

    #[test]
    fn test_get_admins() {
        let env = Env::default();
        let (client, _, admin, _, _, _, _) = setup_registry(&env);

        // Initially should have one admin
        let admins = client.get_admins();
        assert_eq!(admins.len(), 1);
        assert_eq!(admins.get(0).unwrap(), admin);
    }

    #[test]
    fn test_is_admin_false_for_non_admin() {
        let env = Env::default();
        let (client, _, _, _, _, _, _) = setup_registry(&env);

        let random_user = Address::generate(&env);
        assert!(!client.is_admin(&random_user));
    }

    #[test]
    fn test_get_unknown_alias_returns_none() {
        let env = Env::default();
        let (client, _, _, _, _, _, _) = setup_registry(&env);

        assert_eq!(
            client.get_contract_by_alias(&Symbol::new(&env, "nonexistent")),
            None
        );
        assert_eq!(
            client.get_contract(&Symbol::new(&env, "nonexistent")),
            None
        );
    }

    #[test]
    fn test_render_aliases() {
        let env = Env::default();
        let (client, _, _, _, _, _, _) = setup_registry(&env);

        // render_aliases should return non-empty bytes
        let aliases = client.render_aliases();
        assert!(aliases.len() > 0);
    }

    #[test]
    fn test_multiple_admins() {
        let env = Env::default();
        let (client, _, admin, _, _, _, _) = setup_registry(&env);

        // Add multiple admins
        let admin2 = Address::generate(&env);
        let admin3 = Address::generate(&env);
        client.add_admin(&admin2, &admin);
        client.add_admin(&admin3, &admin);

        // Verify all are admins
        assert!(client.is_admin(&admin));
        assert!(client.is_admin(&admin2));
        assert!(client.is_admin(&admin3));

        let admins = client.get_admins();
        assert_eq!(admins.len(), 3);
    }
}
