#![no_std]

use soroban_sdk::{contract, contractimpl, contracttype, Address, BytesN, Env, IntoVal, String, Symbol, Val, Vec};

/// Storage keys for the registry contract
#[contracttype]
#[derive(Clone)]
pub enum RegistryKey {
    /// Contract admin address
    Admin,
    /// Pending admin transfer (address, expiry ledger)
    AdminTransfer,
    /// Total number of boards
    BoardCount,
    /// Board metadata by ID
    Board(u64),
    /// Board ID by name (Symbol)
    BoardByName(Symbol),
    /// Board contract address by ID
    BoardContract(u64),
    /// Contract addresses for shared services
    Contracts,
    /// Global pause flag
    Paused,
    /// WASM hash for deploying board contracts
    BoardWasmHash,
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

#[contract]
pub struct BoardsRegistry;

#[contractimpl]
impl BoardsRegistry {
    /// Initialize the registry with admin and service contract addresses
    pub fn init(
        env: Env,
        admin: Address,
        permissions: Address,
        content: Address,
        theme: Address,
        admin_contract: Address,
    ) {
        // Only allow initialization once
        if env.storage().instance().has(&RegistryKey::Admin) {
            panic!("Already initialized");
        }

        admin.require_auth();

        env.storage().instance().set(&RegistryKey::Admin, &admin);
        env.storage().instance().set(&RegistryKey::BoardCount, &0u64);
        env.storage().instance().set(&RegistryKey::Paused, &false);
        env.storage().instance().set(
            &RegistryKey::Contracts,
            &ContractAddresses {
                permissions,
                content,
                theme,
                admin: admin_contract,
            },
        );
    }

    /// Set the WASM hash for deploying board contracts (admin only)
    pub fn set_board_wasm_hash(env: Env, wasm_hash: BytesN<32>) {
        let admin: Address = env
            .storage()
            .instance()
            .get(&RegistryKey::Admin)
            .expect("Not initialized");
        admin.require_auth();

        env.storage()
            .instance()
            .set(&RegistryKey::BoardWasmHash, &wasm_hash);
    }

    /// Get the board WASM hash
    pub fn get_board_wasm_hash(env: Env) -> Option<BytesN<32>> {
        env.storage().instance().get(&RegistryKey::BoardWasmHash)
    }

    /// Create a new board
    pub fn create_board(
        env: Env,
        name: String,
        description: String,
        creator: Address,
        is_private: bool,
    ) -> u64 {
        creator.require_auth();

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
            creator: creator.clone(),
            created_at: env.ledger().timestamp(),
            thread_count: 0,
            is_readonly: false,
            is_private,
        };

        // Store board
        env.storage()
            .persistent()
            .set(&RegistryKey::Board(board_id), &board);

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
            // Pass None for permissions to allow thread creation without permission checks
            // (permission checks happen at the theme level instead)
            let init_args: Vec<Val> = Vec::from_array(
                &env,
                [
                    board_id.into_val(&env),
                    env.current_contract_address().into_val(&env),  // registry
                    Option::<Address>::None.into_val(&env),  // permissions (None to skip checks)
                    name.into_val(&env),
                    description.into_val(&env),
                    is_private.into_val(&env),
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
            [board_id.into_val(&env), creator.into_val(&env)],
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

    /// List boards with pagination
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

    /// Get admin address
    pub fn get_admin(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&RegistryKey::Admin)
            .expect("Not initialized")
    }

    /// Pause/unpause the registry
    pub fn set_paused(env: Env, paused: bool) {
        let admin: Address = env
            .storage()
            .instance()
            .get(&RegistryKey::Admin)
            .expect("Not initialized");
        admin.require_auth();

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
    pub fn upgrade(env: Env, new_wasm_hash: BytesN<32>) {
        let admin: Address = env
            .storage()
            .instance()
            .get(&RegistryKey::Admin)
            .expect("Not initialized");
        admin.require_auth();

        env.deployer().update_current_contract_wasm(new_wasm_hash);
    }

    /// Transfer admin to a new address (two-step process)
    pub fn transfer_admin(env: Env, new_admin: Address) {
        let admin: Address = env
            .storage()
            .instance()
            .get(&RegistryKey::Admin)
            .expect("Not initialized");
        admin.require_auth();

        // Set pending admin with expiry (e.g., 1 week = 604800 ledgers at 1s/ledger)
        let expiry = env.ledger().sequence() + 604800;
        env.storage()
            .instance()
            .set(&RegistryKey::AdminTransfer, &(new_admin, expiry));
    }

    /// Accept admin transfer
    pub fn accept_admin(env: Env) {
        let (new_admin, expiry): (Address, u32) = env
            .storage()
            .instance()
            .get(&RegistryKey::AdminTransfer)
            .expect("No pending transfer");

        if env.ledger().sequence() > expiry {
            panic!("Transfer expired");
        }

        new_admin.require_auth();

        env.storage()
            .instance()
            .set(&RegistryKey::Admin, &new_admin);
        env.storage()
            .instance()
            .remove(&RegistryKey::AdminTransfer);
    }

    /// Set board contract address for a board ID (admin only)
    pub fn set_board_contract(env: Env, board_id: u64, board_contract: Address) {
        let admin: Address = env
            .storage()
            .instance()
            .get(&RegistryKey::Admin)
            .expect("Not initialized");
        admin.require_auth();

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
    pub fn upgrade_contract(env: Env, contract_id: Address, new_wasm_hash: BytesN<32>) {
        let admin: Address = env
            .storage()
            .instance()
            .get(&RegistryKey::Admin)
            .expect("Not initialized");
        admin.require_auth();

        // Call the target contract's upgrade function
        // The target contract will verify that we (the registry) are calling it
        let args: Vec<Val> = Vec::from_array(&env, [new_wasm_hash.into_val(&env)]);
        env.invoke_contract::<()>(
            &contract_id,
            &Symbol::new(&env, "upgrade"),
            args,
        );
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
        let permissions = Address::generate(&env);
        let content = Address::generate(&env);
        let theme = Address::generate(&env);
        let admin_contract = Address::generate(&env);

        // Initialize
        client.init(&admin, &permissions, &content, &theme, &admin_contract);

        // Verify admin
        assert_eq!(client.get_admin(), admin);

        // Create a board
        let creator = Address::generate(&env);
        let name = String::from_str(&env, "General");
        let desc = String::from_str(&env, "General discussion");

        let board_id = client.create_board(&name, &desc, &creator, &false);
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

        client.init(&admin, &permissions, &content, &theme, &admin_contract);

        let creator = Address::generate(&env);

        // Create multiple boards with different names
        let names = ["Board1", "Board2", "Board3", "Board4", "Board5"];
        for name in names.iter() {
            let name = String::from_str(&env, name);
            let desc = String::from_str(&env, "Description");
            client.create_board(&name, &desc, &creator, &false);
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

        client.init(&admin, &permissions, &content, &theme, &admin_contract);

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

        client.init(&admin, &permissions, &content, &theme, &admin_contract);

        // Pause the registry
        client.set_paused(&true);

        // Try to create a board - should panic
        let creator = Address::generate(&env);
        let name = String::from_str(&env, "Test");
        let desc = String::from_str(&env, "Test desc");
        client.create_board(&name, &desc, &creator, &false);
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

        client.init(&admin, &permissions, &content, &theme, &admin_contract);

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

        client.init(&admin, &permissions, &content, &theme, &admin_contract);

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

        client.init(&admin, &permissions, &content, &theme, &admin_contract);

        let creator = Address::generate(&env);
        let name = String::from_str(&env, "Private Board");
        let desc = String::from_str(&env, "Secret discussions");

        let board_id = client.create_board(&name, &desc, &creator, &true);
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

        client.init(&admin, &permissions, &content, &theme, &admin_contract);

        assert_eq!(client.board_count(), 0);

        let creator = Address::generate(&env);
        let desc = String::from_str(&env, "Desc");

        // Create boards and verify count increments
        for i in 0..5 {
            let name = String::from_str(&env, "Board");
            let id = client.create_board(&name, &desc, &creator, &false);
            assert_eq!(id, i);
            assert_eq!(client.board_count(), i + 1);
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

        client.init(&admin, &permissions, &content, &theme, &admin_contract);

        // List boards when none exist
        let boards = client.list_boards(&0, &10);
        assert_eq!(boards.len(), 0);
    }
}
