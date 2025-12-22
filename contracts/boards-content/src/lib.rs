#![no_std]

use soroban_chonk::prelude::*;
use soroban_sdk::{
    contract, contractimpl, contracttype, Address, Bytes, BytesN, Env, IntoVal, String, Symbol, Val, Vec,
};

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

    // Permission check helpers

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
    pub fn set_thread_body(env: Env, board_id: u64, thread_id: u64, content: Bytes, author: Address) {
        author.require_auth();

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

    /// Edit thread body content
    pub fn edit_thread_body(env: Env, board_id: u64, thread_id: u64, content: Bytes, caller: Address) {
        caller.require_auth();
        // TODO: Verify caller is author or moderator via permissions contract

        let key = Self::get_or_create_thread_body_chonk(&env, board_id, thread_id);
        let chonk = Chonk::open(&env, key);
        chonk.clear();
        chonk.write_chunked(content, 4096);
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

        // Check permissions
        Self::check_can_reply(&env, board_id, &creator);

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
        let key = Self::get_or_create_reply_chonk(&env, board_id, thread_id, reply_id);
        let chonk = Chonk::open(&env, key);
        chonk.write_chunked(content, 4096);

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
                // Only include top-level replies (depth 0 or 1, parent_id 0)
                if reply.depth <= 1 && reply.parent_id == 0 {
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

    /// List child replies of a specific reply
    pub fn list_children_replies(env: Env, board_id: u64, thread_id: u64, parent_id: u64) -> Vec<ReplyMeta> {
        let child_ids: Vec<u64> = env
            .storage()
            .persistent()
            .get(&ContentKey::ChildReplies(board_id, thread_id, parent_id))
            .unwrap_or(Vec::new(&env));

        let mut replies = Vec::new(&env);

        for i in 0..child_ids.len() {
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
            let key = Self::get_or_create_reply_chonk(&env, board_id, thread_id, reply_id);
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
        // Pass None for permissions to skip permission checks in tests
        client.init(&registry, &None);

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
        // Pass None for permissions to skip permission checks in tests
        client.init(&registry, &None);

        let author = Address::generate(&env);
        let flagger = Address::generate(&env);
        let content = Bytes::from_slice(&env, b"Controversial content");

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
        let content = Bytes::from_slice(&env, b"Some content");
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
}
