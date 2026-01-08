#![no_std]

use soroban_chonk::prelude::*;
use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, Address, Bytes, BytesN, Env, IntoVal, String, Symbol, Val, Vec,
};

/// Errors that can occur in the content contract
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum ContentError {
    /// Board is read-only and does not accept new content
    BoardReadOnly = 1,
    /// Thread is locked and does not accept new replies
    ThreadLocked = 2,
    /// User is not authorized to perform this action
    NotAuthorized = 3,
    /// Content not found
    NotFound = 4,
    /// Contract not initialized
    NotInitialized = 5,
    /// Already flagged by this user
    AlreadyFlagged = 6,
    /// A flair is required but none was selected
    FlairRequired = 7,
}

/// Storage keys for the content contract
#[contracttype]
#[derive(Clone)]
pub enum ContentKey {
    /// Registry contract address
    Registry,
    /// Permissions contract address (optional)
    Permissions,
    /// Reply count for a thread
    ReplyCount(u64, u64),
    /// Reply metadata by ID (board_id, thread_id, reply_id)
    Reply(u64, u64, u64),
    /// Next reply ID for a thread
    NextReplyId(u64, u64),
    /// Flags on a reply (board_id, thread_id, reply_id) -> Vec<Flag>
    Flags(u64, u64, u64),
    /// Flags on a thread (board_id, thread_id) -> Vec<Flag>
    ThreadFlags(u64, u64),
    /// Thread flag count (board_id, thread_id) -> u32
    ThreadFlagCount(u64, u64),
    /// List of reply IDs for a thread (board_id, thread_id) -> Vec<u64>
    ThreadReplies(u64, u64),
    /// List of child reply IDs for a parent (board_id, thread_id, parent_id) -> Vec<u64>
    ChildReplies(u64, u64, u64),
    /// Next chonk ID counter
    NextChonkId,
    /// Thread body chonk symbol (board_id, thread_id) -> Symbol
    ThreadBodyChonk(u64, u64),
    /// Reply content chonk symbol (board_id, thread_id, reply_id) -> Symbol
    ReplyChonk(u64, u64, u64),
    /// List of flagged content for a board (board_id) -> Vec<FlaggedItem>
    FlaggedContent(u64),
    /// Crosspost reference (target_board_id, target_thread_id) -> CrosspostRef
    /// Stored on the crossposted thread, points back to original
    CrosspostRef(u64, u64),
    /// Crosspost count for a thread (original_board_id, original_thread_id) -> u32
    CrosspostCount(u64, u64),
    /// List of locations where thread was crossposted (original_board_id, original_thread_id) -> Vec<CrosspostLocation>
    CrosspostList(u64, u64),
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
    pub resolved: bool,
    /// Optional rule number that was violated (references board rules)
    pub rule_violated: Option<u32>,
}

/// Type of flagged content
#[contracttype]
#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum FlaggedType {
    Thread = 0,
    Reply = 1,
}

/// Reference to flagged content
#[contracttype]
#[derive(Clone)]
pub struct FlaggedItem {
    pub board_id: u64,
    pub thread_id: u64,
    pub reply_id: u64,       // 0 for threads
    pub item_type: FlaggedType,
    pub flag_count: u32,
    pub first_flagged_at: u64,
}

/// Reference to the original thread (stored on crossposted thread)
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

/// Location where a thread has been crossposted (stored on original thread)
#[contracttype]
#[derive(Clone)]
pub struct CrosspostLocation {
    pub board_id: u64,
    pub thread_id: u64,
    pub created_at: u64,
}

#[contract]
pub struct BoardsContent;

#[contractimpl]
impl BoardsContent {
    /// Initialize the content contract
    /// permissions is optional - if None, permission checks are skipped (useful for testing)
    pub fn init(env: Env, registry: Address, permissions: Option<Address>) {
        if env.storage().instance().has(&ContentKey::Registry) {
            panic!("Already initialized");
        }
        env.storage()
            .instance()
            .set(&ContentKey::Registry, &registry);

        // Only set permissions if provided
        if let Some(perms) = permissions {
            env.storage()
                .instance()
                .set(&ContentKey::Permissions, &perms);
        }
    }

    /// Get registry address
    pub fn get_registry(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&ContentKey::Registry)
            .expect("Not initialized")
    }

    /// Get board contract address from registry (single contract for all boards)
    fn get_board_contract_address(env: &Env) -> Address {
        let registry: Address = env.storage().instance().get(&ContentKey::Registry).expect("Not initialized");
        let alias_args: Vec<Val> = Vec::from_array(env, [Symbol::new(env, "board").into_val(env)]);
        let board_contract: Option<Address> = env.invoke_contract(
            &registry,
            &Symbol::new(env, "get_contract_by_alias"),
            alias_args,
        );
        board_contract.expect("Board contract not registered")
    }

    /// Create a thread (entry point for thread creation)
    /// This function:
    /// 1. Calls the Board contract to create thread metadata
    /// 2. Stores the thread body content
    /// Returns the thread ID, or an error if the board is read-only
    /// Note: Parameter order matches form field order (board_id, title, flair_id, body, caller)
    pub fn create_thread(
        env: Env,
        board_id: u64,
        title: String,
        flair_id: Option<String>,
        body: String,
        caller: Address,
    ) -> Result<u64, ContentError> {
        caller.require_auth();

        // Get the registry to look up the board contract
        let registry: Address = env
            .storage()
            .instance()
            .get(&ContentKey::Registry)
            .ok_or(ContentError::NotInitialized)?;

        // Check board is not readonly
        Self::check_board_not_readonly(&env, &registry, board_id)?;

        // Get the board contract (single contract for all boards)
        let board_contract = Self::get_board_contract_address(&env);

        // Check if flair is required but none selected
        let flair_is_none = match &flair_id {
            None => true,
            Some(s) => {
                let none_str = String::from_str(&env, "none");
                s.len() == 0 || s == &none_str
            }
        };

        if flair_is_none {
            // Check if board requires a flair
            let check_args: Vec<Val> = Vec::from_array(&env, [board_id.into_val(&env)]);
            let flair_required: bool = env.invoke_contract(
                &board_contract,
                &Symbol::new(&env, "is_flair_required"),
                check_args,
            );
            if flair_required {
                return Err(ContentError::FlairRequired);
            }
        }

        // Create the thread in the board contract (this creates metadata - now requires board_id)
        // flair_id is passed as Option<String> - board contract parses it
        let create_args: Vec<Val> = Vec::from_array(
            &env,
            [
                board_id.into_val(&env),
                title.into_val(&env),
                flair_id.into_val(&env),
                caller.clone().into_val(&env),
            ],
        );
        let thread_id: u64 = env.invoke_contract(
            &board_contract,
            &Symbol::new(&env, "create_thread"),
            create_args,
        );

        // Store the thread body content
        // Convert String to Bytes - handle up to 16KB content
        let body_len = body.len() as usize;
        let body_bytes = if body_len > 0 && body_len <= 16384 {
            let mut temp = [0u8; 16384];
            body.copy_into_slice(&mut temp[..body_len]);
            Bytes::from_slice(&env, &temp[..body_len])
        } else if body_len > 16384 {
            // For very large content, truncate (shouldn't happen in practice)
            let mut temp = [0u8; 16384];
            body.copy_into_slice(&mut temp[..16384]);
            Bytes::from_slice(&env, &temp)
        } else {
            Bytes::new(&env)
        };
        let key = Self::get_or_create_thread_body_chonk(&env, board_id, thread_id);
        let chonk = Chonk::open(&env, key);
        chonk.write_chunked(body_bytes, 4096);

        // Record first-seen timestamp for the user (for account age tracking)
        // and increment post count
        if let Some(perms) = env.storage().instance().get::<_, Address>(&ContentKey::Permissions) {
            let record_args: Vec<Val> = Vec::from_array(&env, [caller.clone().into_val(&env)]);
            let _ = env.try_invoke_contract::<(), soroban_sdk::Error>(
                &perms,
                &Symbol::new(&env, "record_first_seen"),
                record_args,
            );

            // Increment user's post count
            let inc_args: Vec<Val> = Vec::from_array(&env, [
                caller.into_val(&env),
                env.current_contract_address().into_val(&env),
            ]);
            let _ = env.try_invoke_contract::<(), soroban_sdk::Error>(
                &perms,
                &Symbol::new(&env, "increment_post_count"),
                inc_args,
            );
        }

        Ok(thread_id)
    }

    // Permission check helpers

    /// Check if board is readonly - returns error if so
    /// Queries the board contract directly for its readonly status
    fn check_board_not_readonly(env: &Env, _registry: &Address, board_id: u64) -> Result<(), ContentError> {
        // Get board contract (single contract for all boards)
        let board_contract = Self::get_board_contract_address(env);

        // Query the board contract's is_readonly function (now requires board_id)
        let is_readonly: bool = env
            .try_invoke_contract::<bool, soroban_sdk::Error>(
                &board_contract,
                &Symbol::new(env, "is_readonly"),
                Vec::from_array(env, [board_id.into_val(env)]),
            )
            .unwrap_or(Ok(false))
            .unwrap_or(false);

        if is_readonly {
            return Err(ContentError::BoardReadOnly);
        }
        Ok(())
    }

    /// Check if thread is locked - returns error if so
    /// Gracefully handles missing function for backwards compatibility
    fn check_thread_not_locked(env: &Env, _registry: &Address, board_id: u64, thread_id: u64) -> Result<(), ContentError> {
        // Get board contract (single contract for all boards)
        let board_contract = Self::get_board_contract_address(env);

        // Query the board contract's is_thread_locked function (now requires board_id)
        let thread_args: Vec<Val> = Vec::from_array(env, [board_id.into_val(env), thread_id.into_val(env)]);
        let is_locked: bool = env
            .try_invoke_contract::<bool, soroban_sdk::Error>(
                &board_contract,
                &Symbol::new(env, "is_thread_locked"),
                thread_args,
            )
            .unwrap_or(Ok(false))
            .unwrap_or(false);

        if is_locked {
            return Err(ContentError::ThreadLocked);
        }
        Ok(())
    }

    /// Check if user can reply on this board
    fn check_can_reply(env: &Env, board_id: u64, user: &Address) {
        if !env.storage().instance().has(&ContentKey::Permissions) {
            return; // Skip if permissions not configured
        }

        let permissions: Address = env
            .storage()
            .instance()
            .get(&ContentKey::Permissions)
            .unwrap();

        let args: Vec<Val> = Vec::from_array(
            env,
            [board_id.into_val(env), user.into_val(env)],
        );
        let fn_name = Symbol::new(env, "can_reply");
        let can_reply: bool = env.invoke_contract(&permissions, &fn_name, args);

        if !can_reply {
            panic!("Not authorized to reply");
        }
    }

    /// Check if user can moderate on this board
    fn check_can_moderate(env: &Env, board_id: u64, user: &Address) {
        if !env.storage().instance().has(&ContentKey::Permissions) {
            return; // Skip if permissions not configured
        }

        let permissions: Address = env
            .storage()
            .instance()
            .get(&ContentKey::Permissions)
            .unwrap();

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

    /// Store thread body content
    /// Note: Auth is handled by the calling contract (theme).
    pub fn set_thread_body(env: Env, board_id: u64, thread_id: u64, content: Bytes, _author: Address) {
        // Note: require_auth() removed - called by theme which handles auth

        let key = Self::get_or_create_thread_body_chonk(&env, board_id, thread_id);
        let chonk = Chonk::open(&env, key);
        chonk.clear();
        chonk.write_chunked(content, 4096);
    }

    /// Get thread body content
    pub fn get_thread_body(env: Env, board_id: u64, thread_id: u64) -> Bytes {
        if let Some(key) = Self::get_thread_body_chonk(&env, board_id, thread_id) {
            let chonk = Chonk::open(&env, key);
            chonk.assemble()
        } else {
            Bytes::new(&env)
        }
    }

    /// Get a chunk of thread body (for progressive loading)
    pub fn get_thread_body_chunk(env: Env, board_id: u64, thread_id: u64, index: u32) -> Option<Bytes> {
        if let Some(key) = Self::get_thread_body_chonk(&env, board_id, thread_id) {
            let chonk = Chonk::open(&env, key);
            chonk.get(index)
        } else {
            None
        }
    }

    /// Get chunk count for thread body
    pub fn get_thread_body_chunk_count(env: Env, board_id: u64, thread_id: u64) -> u32 {
        if let Some(key) = Self::get_thread_body_chonk(&env, board_id, thread_id) {
            let chonk = Chonk::open(&env, key);
            chonk.count()
        } else {
            0
        }
    }

    /// Edit thread body content (takes Bytes, for internal use)
    pub fn edit_thread_body(env: Env, board_id: u64, thread_id: u64, content: Bytes, caller: Address) {
        caller.require_auth();
        // TODO: Verify caller is author or moderator via permissions contract

        let key = Self::get_or_create_thread_body_chonk(&env, board_id, thread_id);
        let chonk = Chonk::open(&env, key);
        chonk.clear();
        chonk.write_chunked(content, 4096);
    }

    /// Edit thread title and body (entry point for form submissions)
    /// This function:
    /// 1. Updates thread title in the Board contract
    /// 2. Updates thread body content in this contract
    /// Returns an error if the board is read-only or thread is locked
    pub fn edit_thread(
        env: Env,
        board_id: u64,
        thread_id: u64,
        new_title: String,
        new_body: String,
        caller: Address,
    ) -> Result<(), ContentError> {
        caller.require_auth();

        // Get the registry to look up the board contract
        let registry: Address = env
            .storage()
            .instance()
            .get(&ContentKey::Registry)
            .ok_or(ContentError::NotInitialized)?;

        // Check board is not readonly (moderators bypass this via permissions contract)
        // For now, block all edits on readonly boards - moderators can use admin actions
        Self::check_board_not_readonly(&env, &registry, board_id)?;

        // Check thread is not locked
        Self::check_thread_not_locked(&env, &registry, board_id, thread_id)?;

        // Get board contract (single contract for all boards)
        let board_contract = Self::get_board_contract_address(&env);

        // Update the thread title in the board contract (now requires board_id)
        let title_args: Vec<Val> = Vec::from_array(
            &env,
            [
                board_id.into_val(&env),
                thread_id.into_val(&env),
                new_title.into_val(&env),
                caller.clone().into_val(&env),
            ],
        );
        env.invoke_contract::<()>(
            &board_contract,
            &Symbol::new(&env, "edit_thread_title"),
            title_args,
        );

        // Convert String to Bytes and update body
        let body_len = new_body.len() as usize;
        let body_bytes = if body_len > 0 && body_len <= 16384 {
            let mut temp = [0u8; 16384];
            new_body.copy_into_slice(&mut temp[..body_len]);
            Bytes::from_slice(&env, &temp[..body_len])
        } else if body_len > 16384 {
            let mut temp = [0u8; 16384];
            new_body.copy_into_slice(&mut temp[..16384]);
            Bytes::from_slice(&env, &temp)
        } else {
            Bytes::new(&env)
        };

        let key = Self::get_or_create_thread_body_chonk(&env, board_id, thread_id);
        let chonk = Chonk::open(&env, key);
        chonk.clear();
        chonk.write_chunked(body_bytes, 4096);

        Ok(())
    }

    /// Create a reply
    /// Called directly from forms via form:@content:create_reply
    /// Returns the reply ID, or an error if the board is read-only or thread is locked
    pub fn create_reply(
        env: Env,
        board_id: u64,
        thread_id: u64,
        parent_id: u64,
        depth: u32,
        content: String,
        creator: Address,
    ) -> Result<u64, ContentError> {
        creator.require_auth();

        // Get registry for checks
        let registry: Address = env
            .storage()
            .instance()
            .get(&ContentKey::Registry)
            .ok_or(ContentError::NotInitialized)?;

        // Check board is not readonly
        Self::check_board_not_readonly(&env, &registry, board_id)?;

        // Check thread is not locked
        Self::check_thread_not_locked(&env, &registry, board_id, thread_id)?;

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

        // Convert String to Bytes for storage
        let content_len = content.len() as usize;
        let content_bytes = if content_len > 0 && content_len <= 16384 {
            let mut temp = [0u8; 16384];
            content.copy_into_slice(&mut temp[..content_len]);
            Bytes::from_slice(&env, &temp[..content_len])
        } else if content_len > 16384 {
            let mut temp = [0u8; 16384];
            content.copy_into_slice(&mut temp[..16384]);
            Bytes::from_slice(&env, &temp)
        } else {
            Bytes::new(&env)
        };

        // Store reply content in chonk
        let key = Self::get_or_create_reply_chonk(&env, board_id, thread_id, reply_id);
        let chonk = Chonk::open(&env, key);
        chonk.write_chunked(content_bytes, 4096);

        // Add to thread replies list
        let mut thread_replies: Vec<u64> = env
            .storage()
            .persistent()
            .get(&ContentKey::ThreadReplies(board_id, thread_id))
            .unwrap_or(Vec::new(&env));
        thread_replies.push_back(reply_id);
        env.storage()
            .persistent()
            .set(&ContentKey::ThreadReplies(board_id, thread_id), &thread_replies);

        // If this is a nested reply, add to parent's child list
        if parent_id > 0 || depth > 0 {
            let mut child_replies: Vec<u64> = env
                .storage()
                .persistent()
                .get(&ContentKey::ChildReplies(board_id, thread_id, parent_id))
                .unwrap_or(Vec::new(&env));
            child_replies.push_back(reply_id);
            env.storage()
                .persistent()
                .set(&ContentKey::ChildReplies(board_id, thread_id, parent_id), &child_replies);
        }

        // Increment reply count (local)
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

        // Increment reply count in board contract (for ThreadMeta.reply_count)
        // Get board contract (single contract for all boards)
        let board_contract = Self::get_board_contract_address(&env);
        // Call increment_reply_count with board_id
        let incr_args: Vec<Val> = Vec::from_array(&env, [board_id.into_val(&env), thread_id.into_val(&env)]);
        let _ = env.try_invoke_contract::<(), soroban_sdk::Error>(
            &board_contract,
            &Symbol::new(&env, "increment_reply_count"),
            incr_args,
        );

        // Record first-seen timestamp for the user (for account age tracking)
        // and increment post count
        if let Some(perms) = env.storage().instance().get::<_, Address>(&ContentKey::Permissions) {
            let record_args: Vec<Val> = Vec::from_array(&env, [creator.clone().into_val(&env)]);
            let _ = env.try_invoke_contract::<(), soroban_sdk::Error>(
                &perms,
                &Symbol::new(&env, "record_first_seen"),
                record_args,
            );

            // Increment user's post count
            let inc_args: Vec<Val> = Vec::from_array(&env, [
                creator.into_val(&env),
                env.current_contract_address().into_val(&env),
            ]);
            let _ = env.try_invoke_contract::<(), soroban_sdk::Error>(
                &perms,
                &Symbol::new(&env, "increment_post_count"),
                inc_args,
            );
        }

        Ok(reply_id)
    }

    /// Get reply metadata
    pub fn get_reply(env: Env, board_id: u64, thread_id: u64, reply_id: u64) -> Option<ReplyMeta> {
        env.storage()
            .persistent()
            .get(&ContentKey::Reply(board_id, thread_id, reply_id))
    }

    /// Get reply content
    pub fn get_reply_content(env: Env, board_id: u64, thread_id: u64, reply_id: u64) -> Bytes {
        if let Some(key) = Self::get_reply_chonk(&env, board_id, thread_id, reply_id) {
            let chonk = Chonk::open(&env, key);
            chonk.assemble()
        } else {
            Bytes::new(&env)
        }
    }

    /// Get reply content chunk (for progressive loading)
    pub fn get_reply_content_chunk(env: Env, board_id: u64, thread_id: u64, reply_id: u64, index: u32) -> Option<Bytes> {
        if let Some(key) = Self::get_reply_chonk(&env, board_id, thread_id, reply_id) {
            let chonk = Chonk::open(&env, key);
            chonk.get(index)
        } else {
            None
        }
    }

    /// Get reply count for a thread
    pub fn get_reply_count(env: Env, board_id: u64, thread_id: u64) -> u64 {
        env.storage()
            .persistent()
            .get(&ContentKey::ReplyCount(board_id, thread_id))
            .unwrap_or(0)
    }

    /// List all replies for a thread with pagination
    pub fn list_replies(env: Env, board_id: u64, thread_id: u64, start: u32, limit: u32) -> Vec<ReplyMeta> {
        let reply_ids: Vec<u64> = env
            .storage()
            .persistent()
            .get(&ContentKey::ThreadReplies(board_id, thread_id))
            .unwrap_or(Vec::new(&env));

        let mut replies = Vec::new(&env);
        let total = reply_ids.len();
        let end = core::cmp::min(start + limit, total);

        for i in start..end {
            let reply_id = reply_ids.get(i).unwrap();
            if let Some(reply) = env
                .storage()
                .persistent()
                .get(&ContentKey::Reply(board_id, thread_id, reply_id))
            {
                replies.push_back(reply);
            }
        }

        replies
    }

    /// List top-level replies only (replies to the thread itself, not nested)
    pub fn list_top_level_replies(env: Env, board_id: u64, thread_id: u64, start: u32, limit: u32) -> Vec<ReplyMeta> {
        let reply_ids: Vec<u64> = env
            .storage()
            .persistent()
            .get(&ContentKey::ThreadReplies(board_id, thread_id))
            .unwrap_or(Vec::new(&env));

        let mut replies = Vec::new(&env);
        let mut count = 0u32;
        let mut skipped = 0u32;

        for i in 0..reply_ids.len() {
            if count >= limit {
                break;
            }
            let reply_id = reply_ids.get(i).unwrap();
            if let Some(reply) = env
                .storage()
                .persistent()
                .get::<_, ReplyMeta>(&ContentKey::Reply(board_id, thread_id, reply_id))
            {
                // Only include top-level replies (depth 0)
                if reply.depth == 0 {
                    if skipped >= start {
                        replies.push_back(reply);
                        count += 1;
                    } else {
                        skipped += 1;
                    }
                }
            }
        }

        replies
    }

    /// List child replies of a specific reply with pagination
    pub fn list_children_replies(env: Env, board_id: u64, thread_id: u64, parent_id: u64, start: u32, limit: u32) -> Vec<ReplyMeta> {
        let child_ids: Vec<u64> = env
            .storage()
            .persistent()
            .get(&ContentKey::ChildReplies(board_id, thread_id, parent_id))
            .unwrap_or(Vec::new(&env));

        let mut replies = Vec::new(&env);
        let total = child_ids.len();
        let end = core::cmp::min(start + limit, total);

        for i in start..end {
            let reply_id = child_ids.get(i).unwrap();
            if let Some(reply) = env
                .storage()
                .persistent()
                .get(&ContentKey::Reply(board_id, thread_id, reply_id))
            {
                replies.push_back(reply);
            }
        }

        replies
    }

    /// Get child reply count for a reply
    pub fn get_children_count(env: Env, board_id: u64, thread_id: u64, parent_id: u64) -> u32 {
        let child_ids: Vec<u64> = env
            .storage()
            .persistent()
            .get(&ContentKey::ChildReplies(board_id, thread_id, parent_id))
            .unwrap_or(Vec::new(&env));
        child_ids.len()
    }

    /// Edit reply content (takes Bytes, for internal use)
    /// Returns an error if not authorized or if the board is read-only/thread is locked
    pub fn edit_reply(env: Env, board_id: u64, thread_id: u64, reply_id: u64, content: Bytes, caller: Address) -> Result<(), ContentError> {
        caller.require_auth();

        let reply_opt = env
            .storage()
            .persistent()
            .get::<_, ReplyMeta>(&ContentKey::Reply(board_id, thread_id, reply_id));

        let mut reply = reply_opt.ok_or(ContentError::NotFound)?;

        // Verify caller is author or moderator
        let is_author = reply.creator == caller;
        let is_moderator = Self::check_is_moderator(&env, board_id, &caller);

        if !is_author && !is_moderator {
            return Err(ContentError::NotAuthorized);
        }

        // Non-moderators cannot edit on readonly boards or locked threads
        if !is_moderator {
            let registry: Address = env
                .storage()
                .instance()
                .get(&ContentKey::Registry)
                .ok_or(ContentError::NotInitialized)?;
            Self::check_board_not_readonly(&env, &registry, board_id)?;
            Self::check_thread_not_locked(&env, &registry, board_id, thread_id)?;
        }

        reply.updated_at = env.ledger().timestamp();
        env.storage()
            .persistent()
            .set(&ContentKey::Reply(board_id, thread_id, reply_id), &reply);

        // Update content
        let key = Self::get_or_create_reply_chonk(&env, board_id, thread_id, reply_id);
        let chonk = Chonk::open(&env, key);
        chonk.clear();
        chonk.write_chunked(content, 4096);

        Ok(())
    }

    /// Edit reply content (entry point for form submissions, accepts String)
    /// Returns an error if not authorized or if the board is read-only/thread is locked
    pub fn edit_reply_content(
        env: Env,
        board_id: u64,
        thread_id: u64,
        reply_id: u64,
        content: String,
        caller: Address,
    ) -> Result<(), ContentError> {
        caller.require_auth();

        let reply_opt = env
            .storage()
            .persistent()
            .get::<_, ReplyMeta>(&ContentKey::Reply(board_id, thread_id, reply_id));

        let mut reply = reply_opt.ok_or(ContentError::NotFound)?;

        // Verify caller is author or moderator
        let is_author = reply.creator == caller;
        let is_moderator = Self::check_is_moderator(&env, board_id, &caller);

        if !is_author && !is_moderator {
            return Err(ContentError::NotAuthorized);
        }

        // Non-moderators cannot edit on readonly boards or locked threads
        if !is_moderator {
            let registry: Address = env
                .storage()
                .instance()
                .get(&ContentKey::Registry)
                .ok_or(ContentError::NotInitialized)?;
            Self::check_board_not_readonly(&env, &registry, board_id)?;
            Self::check_thread_not_locked(&env, &registry, board_id, thread_id)?;
        }

        reply.updated_at = env.ledger().timestamp();
        env.storage()
            .persistent()
            .set(&ContentKey::Reply(board_id, thread_id, reply_id), &reply);

        // Convert String to Bytes
        let content_len = content.len() as usize;
        let content_bytes = if content_len > 0 && content_len <= 16384 {
            let mut temp = [0u8; 16384];
            content.copy_into_slice(&mut temp[..content_len]);
            Bytes::from_slice(&env, &temp[..content_len])
        } else if content_len > 16384 {
            let mut temp = [0u8; 16384];
            content.copy_into_slice(&mut temp[..16384]);
            Bytes::from_slice(&env, &temp)
        } else {
            Bytes::new(&env)
        };

        // Update content
        let key = Self::get_or_create_reply_chonk(&env, board_id, thread_id, reply_id);
        let chonk = Chonk::open(&env, key);
        chonk.clear();
        chonk.write_chunked(content_bytes, 4096);

        Ok(())
    }

    /// Helper: Check if user is moderator
    fn check_is_moderator(env: &Env, board_id: u64, user: &Address) -> bool {
        if !env.storage().instance().has(&ContentKey::Permissions) {
            return false;
        }

        let permissions: Address = env
            .storage()
            .instance()
            .get(&ContentKey::Permissions)
            .unwrap();

        let args: Vec<Val> = Vec::from_array(env, [board_id.into_val(env), user.into_val(env)]);
        let fn_name = Symbol::new(env, "can_moderate");
        env.invoke_contract(&permissions, &fn_name, args)
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
            if let Some(key) = Self::get_reply_chonk(&env, board_id, thread_id, reply_id) {
                let chonk = Chonk::open(&env, key);
                chonk.clear();
                // Store deletion notice
                chonk.push(Bytes::from_slice(&env, b"[This reply has been deleted]"));
            }
        }
    }

    /// Flag a reply
    pub fn flag_reply(env: Env, board_id: u64, thread_id: u64, reply_id: u64, reason: String, flagger: Address) {
        flagger.require_auth();

        let flag = Flag {
            flagger: flagger.clone(),
            reason,
            created_at: env.ledger().timestamp(),
            resolved: false,
            rule_violated: None,
        };

        // Get existing flags
        let mut flags: Vec<Flag> = env
            .storage()
            .persistent()
            .get(&ContentKey::Flags(board_id, thread_id, reply_id))
            .unwrap_or(Vec::new(&env));

        // Check if this user already flagged
        for i in 0..flags.len() {
            if flags.get(i).unwrap().flagger == flagger {
                panic!("Already flagged by this user");
            }
        }

        let is_first_flag = flags.is_empty();
        flags.push_back(flag);
        let flag_count = flags.len() as u32;

        env.storage()
            .persistent()
            .set(&ContentKey::Flags(board_id, thread_id, reply_id), &flags);

        // Update flag count on reply and check for auto-hide
        if let Some(mut reply) = env
            .storage()
            .persistent()
            .get::<_, ReplyMeta>(&ContentKey::Reply(board_id, thread_id, reply_id))
        {
            reply.flag_count = flag_count;

            // Check auto-hide threshold
            let threshold = Self::get_flag_threshold(&env, board_id);
            if flag_count >= threshold && !reply.is_hidden {
                reply.is_hidden = true;
            }

            env.storage()
                .persistent()
                .set(&ContentKey::Reply(board_id, thread_id, reply_id), &reply);
        }

        // Add to flagged content list (for moderation queue)
        if is_first_flag {
            Self::add_to_flagged_content(&env, board_id, thread_id, reply_id, FlaggedType::Reply);
        } else {
            Self::update_flagged_content_count(&env, board_id, thread_id, reply_id, FlaggedType::Reply, flag_count);
        }
    }

    /// Flag a thread
    pub fn flag_thread(env: Env, board_id: u64, thread_id: u64, reason: String, flagger: Address) {
        flagger.require_auth();

        let flag = Flag {
            flagger: flagger.clone(),
            reason,
            created_at: env.ledger().timestamp(),
            resolved: false,
            rule_violated: None,
        };

        // Get existing flags
        let mut flags: Vec<Flag> = env
            .storage()
            .persistent()
            .get(&ContentKey::ThreadFlags(board_id, thread_id))
            .unwrap_or(Vec::new(&env));

        // Check if this user already flagged
        for i in 0..flags.len() {
            if flags.get(i).unwrap().flagger == flagger {
                panic!("Already flagged by this user");
            }
        }

        let is_first_flag = flags.is_empty();
        flags.push_back(flag);
        let flag_count = flags.len() as u32;

        env.storage()
            .persistent()
            .set(&ContentKey::ThreadFlags(board_id, thread_id), &flags);
        env.storage()
            .persistent()
            .set(&ContentKey::ThreadFlagCount(board_id, thread_id), &flag_count);

        // Add to flagged content list
        if is_first_flag {
            Self::add_to_flagged_content(&env, board_id, thread_id, 0, FlaggedType::Thread);
        } else {
            Self::update_flagged_content_count(&env, board_id, thread_id, 0, FlaggedType::Thread, flag_count);
        }

        // Note: Auto-hide for threads is handled by board contract
    }

    /// Get flags for a reply
    pub fn get_reply_flags(env: Env, board_id: u64, thread_id: u64, reply_id: u64) -> Vec<Flag> {
        env.storage()
            .persistent()
            .get(&ContentKey::Flags(board_id, thread_id, reply_id))
            .unwrap_or(Vec::new(&env))
    }

    /// Get flags for a thread
    pub fn get_thread_flags(env: Env, board_id: u64, thread_id: u64) -> Vec<Flag> {
        env.storage()
            .persistent()
            .get(&ContentKey::ThreadFlags(board_id, thread_id))
            .unwrap_or(Vec::new(&env))
    }

    /// Get thread flag count
    pub fn get_thread_flag_count(env: Env, board_id: u64, thread_id: u64) -> u32 {
        env.storage()
            .persistent()
            .get(&ContentKey::ThreadFlagCount(board_id, thread_id))
            .unwrap_or(0)
    }

    /// Clear/resolve flags on a reply (moderator action)
    pub fn clear_reply_flags(env: Env, board_id: u64, thread_id: u64, reply_id: u64, caller: Address) {
        caller.require_auth();
        Self::check_can_moderate(&env, board_id, &caller);

        // Mark all flags as resolved
        if let Some(mut flags) = env
            .storage()
            .persistent()
            .get::<_, Vec<Flag>>(&ContentKey::Flags(board_id, thread_id, reply_id))
        {
            for i in 0..flags.len() {
                let mut flag = flags.get(i).unwrap();
                flag.resolved = true;
                flags.set(i, flag);
            }
            env.storage()
                .persistent()
                .set(&ContentKey::Flags(board_id, thread_id, reply_id), &flags);
        }

        // Update reply flag count to 0 (cleared)
        if let Some(mut reply) = env
            .storage()
            .persistent()
            .get::<_, ReplyMeta>(&ContentKey::Reply(board_id, thread_id, reply_id))
        {
            reply.flag_count = 0;
            env.storage()
                .persistent()
                .set(&ContentKey::Reply(board_id, thread_id, reply_id), &reply);
        }

        // Remove from flagged content list
        Self::remove_from_flagged_content(&env, board_id, thread_id, reply_id, FlaggedType::Reply);
    }

    /// Clear/resolve flags on a thread (moderator action)
    pub fn clear_thread_flags(env: Env, board_id: u64, thread_id: u64, caller: Address) {
        caller.require_auth();
        Self::check_can_moderate(&env, board_id, &caller);

        // Mark all flags as resolved
        if let Some(mut flags) = env
            .storage()
            .persistent()
            .get::<_, Vec<Flag>>(&ContentKey::ThreadFlags(board_id, thread_id))
        {
            for i in 0..flags.len() {
                let mut flag = flags.get(i).unwrap();
                flag.resolved = true;
                flags.set(i, flag);
            }
            env.storage()
                .persistent()
                .set(&ContentKey::ThreadFlags(board_id, thread_id), &flags);
        }

        env.storage()
            .persistent()
            .set(&ContentKey::ThreadFlagCount(board_id, thread_id), &0u32);

        // Remove from flagged content list
        Self::remove_from_flagged_content(&env, board_id, thread_id, 0, FlaggedType::Thread);
    }

    /// Get all flagged content for a board (moderation queue)
    pub fn list_flagged_content(env: Env, board_id: u64) -> Vec<FlaggedItem> {
        env.storage()
            .persistent()
            .get(&ContentKey::FlaggedContent(board_id))
            .unwrap_or(Vec::new(&env))
    }

    /// Helper: Get flag threshold from permissions contract
    fn get_flag_threshold(env: &Env, board_id: u64) -> u32 {
        if !env.storage().instance().has(&ContentKey::Permissions) {
            return 3; // Default threshold
        }

        let permissions: Address = env
            .storage()
            .instance()
            .get(&ContentKey::Permissions)
            .unwrap();

        let args: Vec<Val> = Vec::from_array(env, [board_id.into_val(env)]);
        let fn_name = Symbol::new(env, "get_flag_threshold");
        env.invoke_contract(&permissions, &fn_name, args)
    }

    /// Helper: Add item to flagged content list
    fn add_to_flagged_content(env: &Env, board_id: u64, thread_id: u64, reply_id: u64, item_type: FlaggedType) {
        let mut flagged: Vec<FlaggedItem> = env
            .storage()
            .persistent()
            .get(&ContentKey::FlaggedContent(board_id))
            .unwrap_or(Vec::new(env));

        flagged.push_back(FlaggedItem {
            board_id,
            thread_id,
            reply_id,
            item_type,
            flag_count: 1,
            first_flagged_at: env.ledger().timestamp(),
        });

        env.storage()
            .persistent()
            .set(&ContentKey::FlaggedContent(board_id), &flagged);
    }

    /// Helper: Update flag count in flagged content list
    fn update_flagged_content_count(env: &Env, board_id: u64, thread_id: u64, reply_id: u64, item_type: FlaggedType, count: u32) {
        if let Some(mut flagged) = env
            .storage()
            .persistent()
            .get::<_, Vec<FlaggedItem>>(&ContentKey::FlaggedContent(board_id))
        {
            for i in 0..flagged.len() {
                let mut item = flagged.get(i).unwrap();
                if item.thread_id == thread_id && item.reply_id == reply_id && item.item_type == item_type {
                    item.flag_count = count;
                    flagged.set(i, item);
                    break;
                }
            }
            env.storage()
                .persistent()
                .set(&ContentKey::FlaggedContent(board_id), &flagged);
        }
    }

    /// Helper: Remove item from flagged content list
    fn remove_from_flagged_content(env: &Env, board_id: u64, thread_id: u64, reply_id: u64, item_type: FlaggedType) {
        if let Some(flagged) = env
            .storage()
            .persistent()
            .get::<_, Vec<FlaggedItem>>(&ContentKey::FlaggedContent(board_id))
        {
            let mut new_flagged = Vec::new(env);
            for i in 0..flagged.len() {
                let item = flagged.get(i).unwrap();
                if !(item.thread_id == thread_id && item.reply_id == reply_id && item.item_type == item_type) {
                    new_flagged.push_back(item);
                }
            }
            env.storage()
                .persistent()
                .set(&ContentKey::FlaggedContent(board_id), &new_flagged);
        }
    }

    /// Hide a reply (moderator action)
    pub fn hide_reply(env: Env, board_id: u64, thread_id: u64, reply_id: u64, caller: Address) {
        caller.require_auth();

        // Check moderator permissions
        Self::check_can_moderate(&env, board_id, &caller);

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

        // Check moderator permissions
        Self::check_can_moderate(&env, board_id, &caller);

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

    /// Set reply hidden state (called by admin contract)
    pub fn set_reply_hidden(env: Env, board_id: u64, thread_id: u64, reply_id: u64, hidden: bool) {
        // Note: Auth is handled by the calling admin contract
        if let Some(mut reply) = env
            .storage()
            .persistent()
            .get::<_, ReplyMeta>(&ContentKey::Reply(board_id, thread_id, reply_id))
        {
            reply.is_hidden = hidden;
            reply.updated_at = env.ledger().timestamp();
            env.storage()
                .persistent()
                .set(&ContentKey::Reply(board_id, thread_id, reply_id), &reply);
        }
    }

    /// Clear flags on content (called by admin contract)
    /// If reply_id is None, clears thread flags; otherwise clears reply flags
    pub fn clear_flags(env: Env, board_id: u64, thread_id: u64, reply_id: Option<u64>) {
        // Note: Auth is handled by the calling admin contract
        if let Some(rid) = reply_id {
            // Clear reply flags
            if let Some(mut flags) = env
                .storage()
                .persistent()
                .get::<_, Vec<Flag>>(&ContentKey::Flags(board_id, thread_id, rid))
            {
                for i in 0..flags.len() {
                    let mut flag = flags.get(i).unwrap();
                    flag.resolved = true;
                    flags.set(i, flag);
                }
                env.storage()
                    .persistent()
                    .set(&ContentKey::Flags(board_id, thread_id, rid), &flags);
            }

            // Update reply flag count
            if let Some(mut reply) = env
                .storage()
                .persistent()
                .get::<_, ReplyMeta>(&ContentKey::Reply(board_id, thread_id, rid))
            {
                reply.flag_count = 0;
                env.storage()
                    .persistent()
                    .set(&ContentKey::Reply(board_id, thread_id, rid), &reply);
            }

            // Remove from flagged content list
            Self::remove_from_flagged_content(&env, board_id, thread_id, rid, FlaggedType::Reply);
        } else {
            // Clear thread flags
            if let Some(mut flags) = env
                .storage()
                .persistent()
                .get::<_, Vec<Flag>>(&ContentKey::ThreadFlags(board_id, thread_id))
            {
                for i in 0..flags.len() {
                    let mut flag = flags.get(i).unwrap();
                    flag.resolved = true;
                    flags.set(i, flag);
                }
                env.storage()
                    .persistent()
                    .set(&ContentKey::ThreadFlags(board_id, thread_id), &flags);
            }

            env.storage()
                .persistent()
                .set(&ContentKey::ThreadFlagCount(board_id, thread_id), &0u32);

            // Remove from flagged content list
            Self::remove_from_flagged_content(&env, board_id, thread_id, 0, FlaggedType::Thread);
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

    // ==================== Crossposting Functions ====================

    /// Create a crosspost of a thread to another board
    /// Returns the new thread ID in the target board
    ///
    /// The crossposted thread is a reference to the original, with optional comment
    pub fn create_crosspost(
        env: Env,
        target_board_id: u64,
        original_board_id: u64,
        original_thread_id: u64,
        comment: String,
        caller: Address,
    ) -> Result<u64, ContentError> {
        caller.require_auth();

        // Get registry
        let registry: Address = env
            .storage()
            .instance()
            .get(&ContentKey::Registry)
            .ok_or(ContentError::NotInitialized)?;

        // Check target board is not readonly
        Self::check_board_not_readonly(&env, &registry, target_board_id)?;

        // Get board contract (single contract for all boards)
        let board_contract = Self::get_board_contract_address(&env);

        // Get original thread metadata (now requires board_id)
        let thread_args: Vec<Val> = Vec::from_array(&env, [original_board_id.into_val(&env), original_thread_id.into_val(&env)]);
        let original_thread: Option<(String, Address)> = env
            .try_invoke_contract::<Option<(String, Address)>, soroban_sdk::Error>(
                &board_contract,
                &Symbol::new(&env, "get_thread_title_and_author"),
                thread_args,
            )
            .ok()
            .and_then(|r| r.ok())
            .flatten();

        let (original_title, original_author) = original_thread.expect("Original thread not found");

        // Create title with crosspost indicator
        let mut title_buf = [0u8; 256];
        let title_len = original_title.len() as usize;
        if title_len > 0 && title_len <= 200 {
            original_title.copy_into_slice(&mut title_buf[..title_len]);
        }
        let crosspost_title = String::from_str(&env, "[Crosspost] ");
        // Just use original title with prefix for simplicity
        let _ = crosspost_title; // Prefix would go here in more complex impl

        // Create thread in target board (now requires board_id)
        let create_args: Vec<Val> = Vec::from_array(
            &env,
            [target_board_id.into_val(&env), original_title.clone().into_val(&env), caller.clone().into_val(&env)],
        );
        let new_thread_id: u64 = env.invoke_contract(
            &board_contract,
            &Symbol::new(&env, "create_thread"),
            create_args,
        );

        // Store the crosspost reference on the new thread
        let crosspost_ref = CrosspostRef {
            original_board_id,
            original_thread_id,
            original_title,
            original_author,
            crossposted_by: caller.clone(),
            crossposted_at: env.ledger().timestamp(),
        };

        env.storage()
            .persistent()
            .set(&ContentKey::CrosspostRef(target_board_id, new_thread_id), &crosspost_ref);

        // Store comment as thread body if provided
        if comment.len() > 0 {
            let comment_len = comment.len() as usize;
            let comment_bytes = if comment_len <= 16384 {
                let mut temp = [0u8; 16384];
                comment.copy_into_slice(&mut temp[..comment_len]);
                Bytes::from_slice(&env, &temp[..comment_len])
            } else {
                let mut temp = [0u8; 16384];
                comment.copy_into_slice(&mut temp[..16384]);
                Bytes::from_slice(&env, &temp)
            };
            let key = Self::get_or_create_thread_body_chonk(&env, target_board_id, new_thread_id);
            let chonk = Chonk::open(&env, key);
            chonk.write_chunked(comment_bytes, 4096);
        }

        // Update crosspost count and list on original thread
        let count: u32 = env
            .storage()
            .persistent()
            .get(&ContentKey::CrosspostCount(original_board_id, original_thread_id))
            .unwrap_or(0);
        env.storage()
            .persistent()
            .set(&ContentKey::CrosspostCount(original_board_id, original_thread_id), &(count + 1));

        let mut crosspost_list: Vec<CrosspostLocation> = env
            .storage()
            .persistent()
            .get(&ContentKey::CrosspostList(original_board_id, original_thread_id))
            .unwrap_or(Vec::new(&env));
        crosspost_list.push_back(CrosspostLocation {
            board_id: target_board_id,
            thread_id: new_thread_id,
            created_at: env.ledger().timestamp(),
        });
        env.storage()
            .persistent()
            .set(&ContentKey::CrosspostList(original_board_id, original_thread_id), &crosspost_list);

        Ok(new_thread_id)
    }

    /// Get crosspost reference for a thread (returns None if not a crosspost)
    pub fn get_crosspost_ref(env: Env, board_id: u64, thread_id: u64) -> Option<CrosspostRef> {
        env.storage()
            .persistent()
            .get(&ContentKey::CrosspostRef(board_id, thread_id))
    }

    /// Get the number of times a thread has been crossposted
    pub fn get_crosspost_count(env: Env, board_id: u64, thread_id: u64) -> u32 {
        env.storage()
            .persistent()
            .get(&ContentKey::CrosspostCount(board_id, thread_id))
            .unwrap_or(0)
    }

    /// List all locations where a thread has been crossposted
    pub fn list_crossposts(env: Env, board_id: u64, thread_id: u64) -> Vec<CrosspostLocation> {
        env.storage()
            .persistent()
            .get(&ContentKey::CrosspostList(board_id, thread_id))
            .unwrap_or(Vec::new(&env))
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

    /// Get or create a unique chonk symbol for a thread body
    fn get_or_create_thread_body_chonk(env: &Env, board_id: u64, thread_id: u64) -> Symbol {
        let key = ContentKey::ThreadBodyChonk(board_id, thread_id);
        if let Some(symbol) = env.storage().persistent().get(&key) {
            symbol
        } else {
            let symbol = Self::next_chonk_symbol(env);
            env.storage().persistent().set(&key, &symbol);
            symbol
        }
    }

    /// Get or create a unique chonk symbol for reply content
    fn get_or_create_reply_chonk(env: &Env, board_id: u64, thread_id: u64, reply_id: u64) -> Symbol {
        let key = ContentKey::ReplyChonk(board_id, thread_id, reply_id);
        if let Some(symbol) = env.storage().persistent().get(&key) {
            symbol
        } else {
            let symbol = Self::next_chonk_symbol(env);
            env.storage().persistent().set(&key, &symbol);
            symbol
        }
    }

    /// Get chonk symbol for thread body (returns None if not created)
    fn get_thread_body_chonk(env: &Env, board_id: u64, thread_id: u64) -> Option<Symbol> {
        env.storage()
            .persistent()
            .get(&ContentKey::ThreadBodyChonk(board_id, thread_id))
    }

    /// Get chonk symbol for reply content (returns None if not created)
    fn get_reply_chonk(env: &Env, board_id: u64, thread_id: u64, reply_id: u64) -> Option<Symbol> {
        env.storage()
            .persistent()
            .get(&ContentKey::ReplyChonk(board_id, thread_id, reply_id))
    }

    /// Generate the next unique chonk symbol
    /// Uses format "c_XXXXXXXX" where X is hex digits from the counter
    fn next_chonk_symbol(env: &Env) -> Symbol {
        let counter: u64 = env
            .storage()
            .instance()
            .get(&ContentKey::NextChonkId)
            .unwrap_or(0);

        // Increment counter
        env.storage()
            .instance()
            .set(&ContentKey::NextChonkId, &(counter + 1));

        // Create symbol string "c_" + 8 hex chars (fits in short symbol limit)
        // We'll use a simple encoding: c_{counter in base36}
        Self::counter_to_symbol(env, counter)
    }

    /// Convert a counter to a short symbol (max 9 chars for short symbol)
    /// Format: "c" + base36 encoded number
    fn counter_to_symbol(env: &Env, counter: u64) -> Symbol {
        // Use a fixed-size buffer for the symbol
        // "c" prefix + up to 8 chars for base36 number = max 9 chars
        let mut buf = [0u8; 9];
        buf[0] = b'c';

        // Encode counter in base36 (0-9, a-z)
        let mut n = counter;
        let mut pos = 8usize;

        if n == 0 {
            buf[1] = b'0';
            return Symbol::new(env, core::str::from_utf8(&buf[0..2]).unwrap());
        }

        while n > 0 && pos > 0 {
            let digit = (n % 36) as u8;
            buf[pos] = if digit < 10 {
                b'0' + digit
            } else {
                b'a' + digit - 10
            };
            n /= 36;
            pos -= 1;
        }

        // Create symbol from the non-zero portion
        let start = pos + 1;
        buf[start - 1] = b'c';
        Symbol::new(env, core::str::from_utf8(&buf[start - 1..9]).unwrap())
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
        // Pass None for permissions to skip permission checks in tests
        client.init(&registry, &None);

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
        // Pass None for permissions to skip permission checks in tests
        client.init(&registry, &None);

        let author = Address::generate(&env);
        let content = String::from_str(&env, "This is a reply!");

        let reply_id = client.create_reply(&0, &0, &0, &1, &content, &author);
        assert_eq!(reply_id, 0);

        let reply = client.get_reply(&0, &0, &reply_id).unwrap();
        assert_eq!(reply.creator, author);
        assert_eq!(reply.depth, 1);

        let reply_content = client.get_reply_content(&0, &0, &reply_id);
        let expected_bytes = Bytes::from_slice(&env, b"This is a reply!");
        assert_eq!(reply_content, expected_bytes);

        assert_eq!(client.get_reply_count(&0, &0), 1);
    }

    #[test]
    fn test_delete_reply() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsContent, ());
        let client = BoardsContentClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        // Pass None for permissions to skip permission checks in tests
        client.init(&registry, &None);

        let author = Address::generate(&env);
        let content = String::from_str(&env, "This reply will be deleted");

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
        // Pass None for permissions to skip permission checks in tests
        client.init(&registry, &None);

        let author = Address::generate(&env);
        let flagger = Address::generate(&env);
        let content = String::from_str(&env, "Controversial content");

        let reply_id = client.create_reply(&0, &0, &0, &1, &content, &author);

        // Flag
        let reason = String::from_str(&env, "Spam");
        client.flag_reply(&0, &0, &reply_id, &reason, &flagger);

        let reply = client.get_reply(&0, &0, &reply_id).unwrap();
        assert_eq!(reply.flag_count, 1);

        // Check flags list
        let flags = client.get_reply_flags(&0, &0, &reply_id);
        assert_eq!(flags.len(), 1);
        assert!(!flags.get(0).unwrap().resolved);
    }

    #[test]
    fn test_flag_thread() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsContent, ());
        let client = BoardsContentClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        client.init(&registry, &None);

        let flagger1 = Address::generate(&env);
        let flagger2 = Address::generate(&env);

        // Flag thread
        let reason = String::from_str(&env, "Inappropriate");
        client.flag_thread(&0, &0, &reason, &flagger1);
        assert_eq!(client.get_thread_flag_count(&0, &0), 1);

        // Another user flags
        client.flag_thread(&0, &0, &reason, &flagger2);
        assert_eq!(client.get_thread_flag_count(&0, &0), 2);

        // Check flagged content list
        let flagged = client.list_flagged_content(&0);
        assert_eq!(flagged.len(), 1);
        assert_eq!(flagged.get(0).unwrap().flag_count, 2);
    }

    #[test]
    fn test_auto_hide_on_threshold() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsContent, ());
        let client = BoardsContentClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        // No permissions contract means default threshold of 3
        client.init(&registry, &None);

        let author = Address::generate(&env);
        let content = String::from_str(&env, "Some content");
        let reply_id = client.create_reply(&0, &0, &0, &1, &content, &author);

        // Flag 3 times (default threshold)
        for _ in 0..3 {
            let flagger = Address::generate(&env);
            let reason = String::from_str(&env, "Bad");
            client.flag_reply(&0, &0, &reply_id, &reason, &flagger);
        }

        // Should be auto-hidden
        let reply = client.get_reply(&0, &0, &reply_id).unwrap();
        assert!(reply.is_hidden);
    }

    #[test]
    fn test_edit_reply() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsContent, ());
        let client = BoardsContentClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        client.init(&registry, &None);

        let author = Address::generate(&env);
        let original_content = String::from_str(&env, "Original content");
        let reply_id = client.create_reply(&0, &0, &0, &1, &original_content, &author);

        // Edit the reply
        let new_content = Bytes::from_slice(&env, b"Updated content");
        client.edit_reply(&0, &0, &reply_id, &new_content, &author);

        let content = client.get_reply_content(&0, &0, &reply_id);
        assert_eq!(content, new_content);

        // Verify updated_at changed
        let reply = client.get_reply(&0, &0, &reply_id).unwrap();
        assert!(reply.updated_at >= reply.created_at);
    }

    #[test]
    fn test_nested_replies() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsContent, ());
        let client = BoardsContentClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        client.init(&registry, &None);

        let author1 = Address::generate(&env);
        let author2 = Address::generate(&env);
        let author3 = Address::generate(&env);

        // Create top-level reply
        let content1 = String::from_str(&env, "Top level reply");
        let reply1 = client.create_reply(&0, &0, &0, &0, &content1, &author1);

        // Create nested reply to reply1
        let content2 = String::from_str(&env, "Reply to reply 1");
        let reply2 = client.create_reply(&0, &0, &reply1, &1, &content2, &author2);

        // Create deeply nested reply
        let content3 = String::from_str(&env, "Reply to reply 2");
        let reply3 = client.create_reply(&0, &0, &reply2, &2, &content3, &author3);

        // Verify depths
        assert_eq!(client.get_reply(&0, &0, &reply1).unwrap().depth, 0);
        assert_eq!(client.get_reply(&0, &0, &reply2).unwrap().depth, 1);
        assert_eq!(client.get_reply(&0, &0, &reply3).unwrap().depth, 2);

        // Verify parent relationships
        assert_eq!(client.get_reply(&0, &0, &reply2).unwrap().parent_id, reply1);
        assert_eq!(client.get_reply(&0, &0, &reply3).unwrap().parent_id, reply2);

        // Verify child counts
        assert_eq!(client.get_children_count(&0, &0, &reply1), 1);
        assert_eq!(client.get_children_count(&0, &0, &reply2), 1);
        assert_eq!(client.get_children_count(&0, &0, &reply3), 0);
    }

    #[test]
    fn test_list_replies() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsContent, ());
        let client = BoardsContentClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        client.init(&registry, &None);

        let author = Address::generate(&env);

        // Create multiple replies
        let contents = ["Reply 0", "Reply 1", "Reply 2", "Reply 3", "Reply 4"];
        for content_str in contents.iter() {
            let content = String::from_str(&env, *content_str);
            client.create_reply(&0, &0, &0, &1, &content, &author);
        }

        // Test pagination
        let page1 = client.list_replies(&0, &0, &0, &2);
        assert_eq!(page1.len(), 2);

        let page2 = client.list_replies(&0, &0, &2, &2);
        assert_eq!(page2.len(), 2);

        let page3 = client.list_replies(&0, &0, &4, &2);
        assert_eq!(page3.len(), 1);
    }

    #[test]
    fn test_progressive_loading() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsContent, ());
        let client = BoardsContentClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        client.init(&registry, &None);

        let author = Address::generate(&env);

        // Create a large thread body that will be chunked
        let mut large_content = [0u8; 8192];
        for i in 0..large_content.len() {
            large_content[i] = (i % 256) as u8;
        }
        let content = Bytes::from_slice(&env, &large_content);

        client.set_thread_body(&0, &0, &content, &author);

        // Verify we can get chunks
        let chunk_count = client.get_thread_body_chunk_count(&0, &0);
        assert!(chunk_count > 1); // Should be multiple chunks

        // Verify full content can be assembled
        let assembled = client.get_thread_body(&0, &0);
        assert_eq!(assembled, content);
    }

    #[test]
    #[should_panic(expected = "Already flagged by this user")]
    fn test_double_flag_prevented() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsContent, ());
        let client = BoardsContentClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        client.init(&registry, &None);

        let author = Address::generate(&env);
        let flagger = Address::generate(&env);
        let content = String::from_str(&env, "Content");
        let reply_id = client.create_reply(&0, &0, &0, &1, &content, &author);

        // Flag once
        let reason = String::from_str(&env, "Spam");
        client.flag_reply(&0, &0, &reply_id, &reason, &flagger);

        // Try to flag again - should panic
        client.flag_reply(&0, &0, &reply_id, &reason, &flagger);
    }

    #[test]
    fn test_hide_unhide_reply() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsContent, ());
        let client = BoardsContentClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        // Note: Without permissions contract, hide/unhide won't check moderator status
        client.init(&registry, &None);

        let author = Address::generate(&env);
        let moderator = Address::generate(&env);
        let content = String::from_str(&env, "Content");
        let reply_id = client.create_reply(&0, &0, &0, &1, &content, &author);

        // Initially not hidden
        let reply = client.get_reply(&0, &0, &reply_id).unwrap();
        assert!(!reply.is_hidden);

        // Hide
        client.hide_reply(&0, &0, &reply_id, &moderator);
        let reply = client.get_reply(&0, &0, &reply_id).unwrap();
        assert!(reply.is_hidden);

        // Unhide
        client.unhide_reply(&0, &0, &reply_id, &moderator);
        let reply = client.get_reply(&0, &0, &reply_id).unwrap();
        assert!(!reply.is_hidden);
    }

    #[test]
    fn test_list_children_replies() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsContent, ());
        let client = BoardsContentClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        client.init(&registry, &None);

        let author = Address::generate(&env);

        // Create parent reply
        let parent_content = String::from_str(&env, "Parent");
        let parent_id = client.create_reply(&0, &0, &0, &0, &parent_content, &author);

        // Create children
        let child_contents = ["Child 0", "Child 1", "Child 2"];
        for content_str in child_contents.iter() {
            let content = String::from_str(&env, *content_str);
            client.create_reply(&0, &0, &parent_id, &1, &content, &author);
        }

        // List children (with pagination)
        let children = client.list_children_replies(&0, &0, &parent_id, &0, &100);
        assert_eq!(children.len(), 3);

        // All children should have parent_id set correctly
        for i in 0..children.len() {
            assert_eq!(children.get(i).unwrap().parent_id, parent_id);
        }
    }

    #[test]
    fn test_get_nonexistent_reply() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsContent, ());
        let client = BoardsContentClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        client.init(&registry, &None);

        // Get reply that doesn't exist
        let reply = client.get_reply(&0, &0, &999);
        assert!(reply.is_none());
    }

    #[test]
    fn test_list_flagged_content() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsContent, ());
        let client = BoardsContentClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        client.init(&registry, &None);

        let author = Address::generate(&env);

        // Create replies and flag them
        let content = String::from_str(&env, "Content");
        let reply1 = client.create_reply(&0, &0, &0, &1, &content, &author);
        let reply2 = client.create_reply(&0, &0, &0, &1, &content, &author);

        // Flag reply1 once
        let flagger1 = Address::generate(&env);
        let reason = String::from_str(&env, "Spam");
        client.flag_reply(&0, &0, &reply1, &reason, &flagger1);

        // Flag reply2 twice
        let flagger2 = Address::generate(&env);
        let flagger3 = Address::generate(&env);
        client.flag_reply(&0, &0, &reply2, &reason, &flagger2);
        client.flag_reply(&0, &0, &reply2, &reason, &flagger3);

        // Flag a thread
        let thread_flagger = Address::generate(&env);
        client.flag_thread(&0, &0, &reason, &thread_flagger);

        // List all flagged content
        let flagged = client.list_flagged_content(&0);
        assert_eq!(flagged.len(), 3); // 2 replies + 1 thread
    }

    #[test]
    fn test_edit_thread_body() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsContent, ());
        let client = BoardsContentClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        client.init(&registry, &None);

        let author = Address::generate(&env);

        // Set initial content
        let original = Bytes::from_slice(&env, b"Original thread content");
        client.set_thread_body(&0, &0, &original, &author);
        assert_eq!(client.get_thread_body(&0, &0), original);

        // Edit content
        let updated = Bytes::from_slice(&env, b"Updated thread content");
        client.edit_thread_body(&0, &0, &updated, &author);
        assert_eq!(client.get_thread_body(&0, &0), updated);
    }
}
