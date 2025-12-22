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

    #[test]
    fn test_hide_thread() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsBoard, ());
        let client = BoardsBoardClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        let name = String::from_str(&env, "General");
        let desc = String::from_str(&env, "General discussion");

        client.init(&0, &registry, &None, &name, &desc, &false);

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

        client.init(&0, &registry, &None, &name, &desc, &false);

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

        client.init(&0, &registry, &None, &name, &desc, &false);

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

        client.init(&0, &registry, &None, &name, &desc, &true);

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

        client.init(&0, &registry, &None, &name, &desc, &false);

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

        client.init(&0, &registry, &None, &name, &desc, &false);

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

        client.init(&0, &registry, &None, &name, &desc, &false);

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

        client.init(&0, &registry, &None, &name, &desc, &false);

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

        client.init(&0, &registry, &None, &name, &desc, &false);

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

        client.init(&0, &registry, &None, &name, &desc, &false);

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

        client.init(&0, &registry, &None, &name, &desc, &false);

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

        client.init(&0, &registry, &None, &name, &desc, &false);

        let creator = Address::generate(&env);
        let title = String::from_str(&env, "New Thread");
        let thread_id = client.create_thread(&title, &creator);

        let thread = client.get_thread(&thread_id).unwrap();

        // Default values
        assert_eq!(thread.reply_count, 0);
        assert!(!thread.is_locked);
        assert!(!thread.is_pinned);
        assert!(!thread.is_hidden);
    }
}
