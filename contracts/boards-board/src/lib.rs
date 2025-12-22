#![no_std]

use soroban_sdk::{contract, contractimpl, contracttype, Address, BytesN, Env, IntoVal, String, Symbol, Val, Vec};

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
    /// Thread count
    ThreadCount,
    /// Thread metadata by ID
    Thread(u64),
    /// Pinned thread IDs
    PinnedThreads,
    /// Board configuration
    Config,
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
            },
        );
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

    /// Create a new thread (returns thread ID)
    /// Set skip_permission_check to true only for testing
    pub fn create_thread(env: Env, title: String, creator: Address) -> u64 {
        creator.require_auth();

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

        // Pass None for permissions to skip permission checks in tests
        client.init(&0, &registry, &None, &name, &desc, &false);

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

        // Pass None for permissions to skip permission checks in tests
        client.init(&0, &registry, &None, &name, &desc, &false);

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
}
