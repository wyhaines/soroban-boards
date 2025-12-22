#![no_std]

use soroban_chonk::prelude::*;
use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, Address, Bytes, BytesN, Env, String, Symbol, Vec,
};

/// Storage keys for the content contract
#[contracttype]
#[derive(Clone)]
pub enum ContentKey {
    /// Registry contract address
    Registry,
    /// Reply count for a thread
    ReplyCount(u64, u64),
    /// Reply metadata by ID (board_id, thread_id, reply_id)
    Reply(u64, u64, u64),
    /// Next reply ID for a thread
    NextReplyId(u64, u64),
    /// Flags on a post (board_id, thread_id, reply_id) -> Vec<Flag>
    Flags(u64, u64, u64),
}

/// Reply metadata
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

/// Flag on a post
#[contracttype]
#[derive(Clone)]
pub struct Flag {
    pub flagger: Address,
    pub reason: String,
    pub created_at: u64,
}

#[contract]
pub struct BoardsContent;

#[contractimpl]
impl BoardsContent {
    /// Initialize the content contract
    pub fn init(env: Env, registry: Address) {
        if env.storage().instance().has(&ContentKey::Registry) {
            panic!("Already initialized");
        }
        env.storage().instance().set(&ContentKey::Registry, &registry);
    }

    /// Get registry address
    pub fn get_registry(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&ContentKey::Registry)
            .expect("Not initialized")
    }

    /// Store thread body content
    pub fn set_thread_body(env: Env, board_id: u64, thread_id: u64, content: Bytes, author: Address) {
        author.require_auth();

        let key = Self::thread_body_key(board_id, thread_id);
        let chonk = Chonk::open(&env, key);
        chonk.clear();
        chonk.write_chunked(content, 4096);
    }

    /// Get thread body content
    pub fn get_thread_body(env: Env, board_id: u64, thread_id: u64) -> Bytes {
        let key = Self::thread_body_key(board_id, thread_id);
        let chonk = Chonk::open(&env, key);
        chonk.assemble()
    }

    /// Get a chunk of thread body (for progressive loading)
    pub fn get_thread_body_chunk(env: Env, board_id: u64, thread_id: u64, index: u32) -> Option<Bytes> {
        let key = Self::thread_body_key(board_id, thread_id);
        let chonk = Chonk::open(&env, key);
        chonk.get(index)
    }

    /// Create a reply
    pub fn create_reply(
        env: Env,
        board_id: u64,
        thread_id: u64,
        parent_id: u64,
        depth: u32,
        content: Bytes,
        creator: Address,
    ) -> u64 {
        creator.require_auth();

        let reply_id = Self::next_reply_id(&env, board_id, thread_id);

        // Store reply metadata
        let reply = ReplyMeta {
            id: reply_id,
            board_id,
            thread_id,
            parent_id,
            depth,
            creator: creator.clone(),
            created_at: env.ledger().timestamp(),
            updated_at: env.ledger().timestamp(),
            is_hidden: false,
            is_deleted: false,
            flag_count: 0,
        };

        env.storage()
            .persistent()
            .set(&ContentKey::Reply(board_id, thread_id, reply_id), &reply);

        // Store reply content in chonk
        let key = Self::reply_content_key(board_id, thread_id, reply_id);
        let chonk = Chonk::open(&env, key);
        chonk.write_chunked(content, 4096);

        // Increment reply count
        let count: u64 = env
            .storage()
            .persistent()
            .get(&ContentKey::ReplyCount(board_id, thread_id))
            .unwrap_or(0);
        env.storage()
            .persistent()
            .set(&ContentKey::ReplyCount(board_id, thread_id), &(count + 1));

        // Update next ID
        env.storage()
            .persistent()
            .set(&ContentKey::NextReplyId(board_id, thread_id), &(reply_id + 1));

        reply_id
    }

    /// Get reply metadata
    pub fn get_reply(env: Env, board_id: u64, thread_id: u64, reply_id: u64) -> Option<ReplyMeta> {
        env.storage()
            .persistent()
            .get(&ContentKey::Reply(board_id, thread_id, reply_id))
    }

    /// Get reply content
    pub fn get_reply_content(env: Env, board_id: u64, thread_id: u64, reply_id: u64) -> Bytes {
        let key = Self::reply_content_key(board_id, thread_id, reply_id);
        let chonk = Chonk::open(&env, key);
        chonk.assemble()
    }

    /// Get reply count for a thread
    pub fn get_reply_count(env: Env, board_id: u64, thread_id: u64) -> u64 {
        env.storage()
            .persistent()
            .get(&ContentKey::ReplyCount(board_id, thread_id))
            .unwrap_or(0)
    }

    /// Edit reply content
    pub fn edit_reply(env: Env, board_id: u64, thread_id: u64, reply_id: u64, content: Bytes, caller: Address) {
        caller.require_auth();

        if let Some(mut reply) = env
            .storage()
            .persistent()
            .get::<_, ReplyMeta>(&ContentKey::Reply(board_id, thread_id, reply_id))
        {
            // Verify caller is author or moderator
            if reply.creator != caller {
                // TODO: Check if caller is moderator via permissions contract
                panic!("Only author can edit reply");
            }

            reply.updated_at = env.ledger().timestamp();
            env.storage()
                .persistent()
                .set(&ContentKey::Reply(board_id, thread_id, reply_id), &reply);

            // Update content
            let key = Self::reply_content_key(board_id, thread_id, reply_id);
            let chonk = Chonk::open(&env, key);
            chonk.clear();
            chonk.write_chunked(content, 4096);
        }
    }

    /// Delete reply (soft delete - keeps metadata, clears content)
    pub fn delete_reply(env: Env, board_id: u64, thread_id: u64, reply_id: u64, caller: Address) {
        caller.require_auth();

        if let Some(mut reply) = env
            .storage()
            .persistent()
            .get::<_, ReplyMeta>(&ContentKey::Reply(board_id, thread_id, reply_id))
        {
            // Verify caller is author or moderator
            if reply.creator != caller {
                // TODO: Check if caller is moderator via permissions contract
                panic!("Only author or moderator can delete reply");
            }

            reply.is_deleted = true;
            reply.updated_at = env.ledger().timestamp();
            env.storage()
                .persistent()
                .set(&ContentKey::Reply(board_id, thread_id, reply_id), &reply);

            // Clear content
            let key = Self::reply_content_key(board_id, thread_id, reply_id);
            let chonk = Chonk::open(&env, key);
            chonk.clear();
            // Store deletion notice
            chonk.push(Bytes::from_slice(&env, b"[This reply has been deleted]"));
        }
    }

    /// Flag a reply
    pub fn flag_reply(env: Env, board_id: u64, thread_id: u64, reply_id: u64, reason: String, flagger: Address) {
        flagger.require_auth();

        let flag = Flag {
            flagger,
            reason,
            created_at: env.ledger().timestamp(),
        };

        // Get existing flags
        let mut flags: Vec<Flag> = env
            .storage()
            .persistent()
            .get(&ContentKey::Flags(board_id, thread_id, reply_id))
            .unwrap_or(Vec::new(&env));

        flags.push_back(flag);
        env.storage()
            .persistent()
            .set(&ContentKey::Flags(board_id, thread_id, reply_id), &flags);

        // Update flag count on reply
        if let Some(mut reply) = env
            .storage()
            .persistent()
            .get::<_, ReplyMeta>(&ContentKey::Reply(board_id, thread_id, reply_id))
        {
            reply.flag_count = flags.len() as u32;
            env.storage()
                .persistent()
                .set(&ContentKey::Reply(board_id, thread_id, reply_id), &reply);
        }
    }

    /// Hide a reply (moderator action)
    pub fn hide_reply(env: Env, board_id: u64, thread_id: u64, reply_id: u64, caller: Address) {
        caller.require_auth();
        // TODO: Verify caller is moderator

        if let Some(mut reply) = env
            .storage()
            .persistent()
            .get::<_, ReplyMeta>(&ContentKey::Reply(board_id, thread_id, reply_id))
        {
            reply.is_hidden = true;
            env.storage()
                .persistent()
                .set(&ContentKey::Reply(board_id, thread_id, reply_id), &reply);
        }
    }

    /// Unhide a reply (moderator action)
    pub fn unhide_reply(env: Env, board_id: u64, thread_id: u64, reply_id: u64, caller: Address) {
        caller.require_auth();
        // TODO: Verify caller is moderator

        if let Some(mut reply) = env
            .storage()
            .persistent()
            .get::<_, ReplyMeta>(&ContentKey::Reply(board_id, thread_id, reply_id))
        {
            reply.is_hidden = false;
            env.storage()
                .persistent()
                .set(&ContentKey::Reply(board_id, thread_id, reply_id), &reply);
        }
    }

    /// Get chunk for progressive loading (called by viewer)
    pub fn get_chunk(env: Env, collection: Symbol, index: u32) -> Option<Bytes> {
        let chonk = Chonk::open(&env, collection);
        chonk.get(index)
    }

    /// Get chunk metadata for progressive loading
    pub fn get_chunk_count(env: Env, collection: Symbol) -> u32 {
        let chonk = Chonk::open(&env, collection);
        chonk.count()
    }

    /// Upgrade the contract WASM
    pub fn upgrade(env: Env, new_wasm_hash: BytesN<32>) {
        let registry: Address = env
            .storage()
            .instance()
            .get(&ContentKey::Registry)
            .expect("Not initialized");
        registry.require_auth();

        env.deployer().update_current_contract_wasm(new_wasm_hash);
    }

    // Helper functions

    fn thread_body_key(board_id: u64, thread_id: u64) -> Symbol {
        // Create a unique symbol for thread body storage
        // Note: Symbol has length limits, so we use a simple scheme
        symbol_short!("tb")
    }

    fn reply_content_key(board_id: u64, thread_id: u64, reply_id: u64) -> Symbol {
        // Create a unique symbol for reply content storage
        symbol_short!("rc")
    }

    fn next_reply_id(env: &Env, board_id: u64, thread_id: u64) -> u64 {
        env.storage()
            .persistent()
            .get(&ContentKey::NextReplyId(board_id, thread_id))
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::Env;

    #[test]
    fn test_init_and_set_thread_body() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsContent, ());
        let client = BoardsContentClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        client.init(&registry);

        let author = Address::generate(&env);
        let content = Bytes::from_slice(&env, b"Hello, this is my first post!");

        client.set_thread_body(&0, &0, &content, &author);

        let retrieved = client.get_thread_body(&0, &0);
        assert_eq!(retrieved, content);
    }

    #[test]
    fn test_create_and_get_reply() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsContent, ());
        let client = BoardsContentClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        client.init(&registry);

        let author = Address::generate(&env);
        let content = Bytes::from_slice(&env, b"This is a reply!");

        let reply_id = client.create_reply(&0, &0, &0, &1, &content, &author);
        assert_eq!(reply_id, 0);

        let reply = client.get_reply(&0, &0, &reply_id).unwrap();
        assert_eq!(reply.creator, author);
        assert_eq!(reply.depth, 1);

        let reply_content = client.get_reply_content(&0, &0, &reply_id);
        assert_eq!(reply_content, content);

        assert_eq!(client.get_reply_count(&0, &0), 1);
    }

    #[test]
    fn test_delete_reply() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsContent, ());
        let client = BoardsContentClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        client.init(&registry);

        let author = Address::generate(&env);
        let content = Bytes::from_slice(&env, b"This reply will be deleted");

        let reply_id = client.create_reply(&0, &0, &0, &1, &content, &author);

        // Delete
        client.delete_reply(&0, &0, &reply_id, &author);

        let reply = client.get_reply(&0, &0, &reply_id).unwrap();
        assert!(reply.is_deleted);

        let deleted_content = client.get_reply_content(&0, &0, &reply_id);
        assert_eq!(deleted_content, Bytes::from_slice(&env, b"[This reply has been deleted]"));
    }

    #[test]
    fn test_flag_reply() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsContent, ());
        let client = BoardsContentClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        client.init(&registry);

        let author = Address::generate(&env);
        let flagger = Address::generate(&env);
        let content = Bytes::from_slice(&env, b"Controversial content");

        let reply_id = client.create_reply(&0, &0, &0, &1, &content, &author);

        // Flag
        let reason = String::from_str(&env, "Spam");
        client.flag_reply(&0, &0, &reply_id, &reason, &flagger);

        let reply = client.get_reply(&0, &0, &reply_id).unwrap();
        assert_eq!(reply.flag_count, 1);
    }
}
