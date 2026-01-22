#![no_std]

use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, Vec};

/// Vote direction for threads and replies
#[contracttype]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum VoteDirection {
    None = 0,
    Up = 1,
    Down = 2,
}

/// Aggregated vote tally for a thread or reply
#[contracttype]
#[derive(Clone, Debug)]
pub struct VoteTally {
    pub upvotes: u32,
    pub downvotes: u32,
    pub score: i32,
    pub first_vote_at: u64,
}

impl VoteTally {
    pub fn new() -> Self {
        VoteTally {
            upvotes: 0,
            downvotes: 0,
            score: 0,
            first_vote_at: 0,
        }
    }
}

impl Default for VoteTally {
    fn default() -> Self {
        Self::new()
    }
}

/// Configuration for voting on a board
#[contracttype]
#[derive(Clone, Debug)]
pub struct VotingConfig {
    /// Whether voting is enabled for this board
    pub enabled: bool,
    /// Whether downvotes are allowed (if false, only upvotes)
    pub allow_downvotes: bool,
    /// Whether karma tracking is enabled
    pub karma_enabled: bool,
    /// Multiplier for karma points (default: 1)
    pub karma_multiplier: u32,
}

impl VotingConfig {
    pub fn default_config() -> Self {
        VotingConfig {
            enabled: true,
            allow_downvotes: true,
            karma_enabled: true,
            karma_multiplier: 1,
        }
    }
}

/// Storage keys for voting data
#[contracttype]
#[derive(Clone)]
pub enum VoteKey {
    /// Registry contract address
    Registry,
    /// Permissions contract address
    Permissions,
    /// Vote on a thread: (board_id, thread_id, voter) -> VoteDirection
    ThreadVote(u64, u64, Address),
    /// Vote on a reply: (board_id, thread_id, reply_id, voter) -> VoteDirection
    ReplyVote(u64, u64, u64, Address),
    /// Tally for a thread: (board_id, thread_id) -> VoteTally
    ThreadTally(u64, u64),
    /// Tally for a reply: (board_id, thread_id, reply_id) -> VoteTally
    ReplyTally(u64, u64, u64),
    /// Voting config for a board: board_id -> VotingConfig
    BoardVotingConfig(u64),
    /// Karma for a user on a board: (board_id, user) -> i64
    BoardKarma(u64, Address),
    /// Total karma for a user across all boards: user -> i64
    TotalKarma(Address),
}

/// Role levels (copied from permissions contract for authorization checks)
#[contracttype]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum Role {
    Guest = 0,
    Member = 1,
    Moderator = 2,
    Admin = 3,
    Owner = 4,
}

/// Permission set (copied from permissions contract)
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
pub struct BoardsVoting;

#[contractimpl]
impl BoardsVoting {
    /// Initialize the voting contract with registry address
    pub fn init(env: Env, registry: Address, permissions: Address) {
        if env.storage().instance().has(&VoteKey::Registry) {
            panic!("Already initialized");
        }
        env.storage().instance().set(&VoteKey::Registry, &registry);
        env.storage()
            .instance()
            .set(&VoteKey::Permissions, &permissions);
    }

    /// Set voting configuration for a board (admin+ only)
    pub fn set_voting_config(env: Env, board_id: u64, config: VotingConfig, caller: Address) {
        caller.require_auth();

        // Verify caller has admin permission on this board
        Self::require_admin(&env, board_id, &caller);

        env.storage()
            .persistent()
            .set(&VoteKey::BoardVotingConfig(board_id), &config);
    }

    /// Get voting configuration for a board
    pub fn get_voting_config(env: Env, board_id: u64) -> VotingConfig {
        env.storage()
            .persistent()
            .get(&VoteKey::BoardVotingConfig(board_id))
            .unwrap_or_else(VotingConfig::default_config)
    }

    /// Vote on a thread
    /// Returns the updated tally
    pub fn vote_thread(
        env: Env,
        board_id: u64,
        thread_id: u64,
        direction: VoteDirection,
        voter: Address,
    ) -> VoteTally {
        voter.require_auth();

        // Check voting is enabled
        let config = Self::get_voting_config(env.clone(), board_id);
        if !config.enabled {
            panic!("Voting is disabled for this board");
        }

        // Check downvotes are allowed if voting down
        if direction == VoteDirection::Down && !config.allow_downvotes {
            panic!("Downvotes are not allowed on this board");
        }

        // Get previous vote
        let vote_key = VoteKey::ThreadVote(board_id, thread_id, voter.clone());
        let previous_vote: VoteDirection = env
            .storage()
            .persistent()
            .get(&vote_key)
            .unwrap_or(VoteDirection::None);

        // Get current tally
        let tally_key = VoteKey::ThreadTally(board_id, thread_id);
        let mut tally: VoteTally = env
            .storage()
            .persistent()
            .get(&tally_key)
            .unwrap_or_default();

        // Set first vote timestamp if this is the first vote
        if tally.first_vote_at == 0 {
            tally.first_vote_at = env.ledger().timestamp();
        }

        // Calculate karma delta for content author
        let karma_delta = Self::calculate_karma_delta(previous_vote, direction);

        // Remove previous vote effect
        match previous_vote {
            VoteDirection::Up => {
                tally.upvotes = tally.upvotes.saturating_sub(1);
                tally.score -= 1;
            }
            VoteDirection::Down => {
                tally.downvotes = tally.downvotes.saturating_sub(1);
                tally.score += 1;
            }
            VoteDirection::None => {}
        }

        // Apply new vote
        match direction {
            VoteDirection::Up => {
                tally.upvotes += 1;
                tally.score += 1;
            }
            VoteDirection::Down => {
                tally.downvotes += 1;
                tally.score -= 1;
            }
            VoteDirection::None => {}
        }

        // Store the new vote (or remove if None)
        if direction == VoteDirection::None {
            env.storage().persistent().remove(&vote_key);
        } else {
            env.storage().persistent().set(&vote_key, &direction);
        }

        // Store updated tally
        env.storage().persistent().set(&tally_key, &tally);

        // Update karma if enabled (we'd need the content author address here)
        // For now, karma is updated when we have author context
        // This would typically be done via cross-contract call to content contract
        if config.karma_enabled && karma_delta != 0 {
            // Note: In a full implementation, we'd get the thread author from the content contract
            // and update their karma. For now, we track the vote but karma update requires author.
            // TODO: Add get_thread_author cross-contract call when integrating with content
        }

        tally
    }

    /// Vote on a reply
    /// Returns the updated tally
    pub fn vote_reply(
        env: Env,
        board_id: u64,
        thread_id: u64,
        reply_id: u64,
        direction: VoteDirection,
        voter: Address,
    ) -> VoteTally {
        voter.require_auth();

        // Check voting is enabled
        let config = Self::get_voting_config(env.clone(), board_id);
        if !config.enabled {
            panic!("Voting is disabled for this board");
        }

        // Check downvotes are allowed if voting down
        if direction == VoteDirection::Down && !config.allow_downvotes {
            panic!("Downvotes are not allowed on this board");
        }

        // Get previous vote
        let vote_key = VoteKey::ReplyVote(board_id, thread_id, reply_id, voter.clone());
        let previous_vote: VoteDirection = env
            .storage()
            .persistent()
            .get(&vote_key)
            .unwrap_or(VoteDirection::None);

        // Get current tally
        let tally_key = VoteKey::ReplyTally(board_id, thread_id, reply_id);
        let mut tally: VoteTally = env
            .storage()
            .persistent()
            .get(&tally_key)
            .unwrap_or_default();

        // Set first vote timestamp if this is the first vote
        if tally.first_vote_at == 0 {
            tally.first_vote_at = env.ledger().timestamp();
        }

        // Calculate karma delta (for future use)
        let _karma_delta = Self::calculate_karma_delta(previous_vote, direction);

        // Remove previous vote effect
        match previous_vote {
            VoteDirection::Up => {
                tally.upvotes = tally.upvotes.saturating_sub(1);
                tally.score -= 1;
            }
            VoteDirection::Down => {
                tally.downvotes = tally.downvotes.saturating_sub(1);
                tally.score += 1;
            }
            VoteDirection::None => {}
        }

        // Apply new vote
        match direction {
            VoteDirection::Up => {
                tally.upvotes += 1;
                tally.score += 1;
            }
            VoteDirection::Down => {
                tally.downvotes += 1;
                tally.score -= 1;
            }
            VoteDirection::None => {}
        }

        // Store the new vote (or remove if None)
        if direction == VoteDirection::None {
            env.storage().persistent().remove(&vote_key);
        } else {
            env.storage().persistent().set(&vote_key, &direction);
        }

        // Store updated tally
        env.storage().persistent().set(&tally_key, &tally);

        tally
    }

    /// Get the tally for a thread
    pub fn get_thread_tally(env: Env, board_id: u64, thread_id: u64) -> VoteTally {
        env.storage()
            .persistent()
            .get(&VoteKey::ThreadTally(board_id, thread_id))
            .unwrap_or_default()
    }

    /// Get the tally for a reply
    pub fn get_reply_tally(env: Env, board_id: u64, thread_id: u64, reply_id: u64) -> VoteTally {
        env.storage()
            .persistent()
            .get(&VoteKey::ReplyTally(board_id, thread_id, reply_id))
            .unwrap_or_default()
    }

    /// Get a user's vote on a thread
    pub fn get_user_thread_vote(
        env: Env,
        board_id: u64,
        thread_id: u64,
        user: Address,
    ) -> VoteDirection {
        env.storage()
            .persistent()
            .get(&VoteKey::ThreadVote(board_id, thread_id, user))
            .unwrap_or(VoteDirection::None)
    }

    /// Get a user's vote on a reply
    pub fn get_user_reply_vote(
        env: Env,
        board_id: u64,
        thread_id: u64,
        reply_id: u64,
        user: Address,
    ) -> VoteDirection {
        env.storage()
            .persistent()
            .get(&VoteKey::ReplyVote(board_id, thread_id, reply_id, user))
            .unwrap_or(VoteDirection::None)
    }

    // === Karma Functions ===

    /// Get karma for a user on a specific board
    pub fn get_board_karma(env: Env, board_id: u64, user: Address) -> i64 {
        env.storage()
            .persistent()
            .get(&VoteKey::BoardKarma(board_id, user))
            .unwrap_or(0i64)
    }

    /// Get total karma for a user across all boards
    pub fn get_total_karma(env: Env, user: Address) -> i64 {
        env.storage()
            .persistent()
            .get(&VoteKey::TotalKarma(user))
            .unwrap_or(0i64)
    }

    /// Update karma for a user (called when their content is voted on)
    /// This would typically be called by the content contract or via integration
    pub fn update_karma(env: Env, board_id: u64, user: Address, delta: i64, caller: Address) {
        caller.require_auth();

        // Only allow calls from trusted contracts (permissions or content)
        // In practice, you'd verify the caller is a known contract
        // For now, we allow any authenticated caller for testing

        let config = Self::get_voting_config(env.clone(), board_id);
        if !config.karma_enabled {
            return;
        }

        let multiplied_delta = delta * (config.karma_multiplier as i64);

        // Update board-specific karma
        let board_karma_key = VoteKey::BoardKarma(board_id, user.clone());
        let current_board_karma: i64 = env
            .storage()
            .persistent()
            .get(&board_karma_key)
            .unwrap_or(0i64);
        env.storage()
            .persistent()
            .set(&board_karma_key, &(current_board_karma + multiplied_delta));

        // Update total karma
        let total_karma_key = VoteKey::TotalKarma(user);
        let current_total_karma: i64 = env
            .storage()
            .persistent()
            .get(&total_karma_key)
            .unwrap_or(0i64);
        env.storage()
            .persistent()
            .set(&total_karma_key, &(current_total_karma + multiplied_delta));
    }

    // === Score Algorithms ===

    /// Calculate hot score for ranking
    /// Higher scores = hotter content
    /// Formula uses integer math to avoid floating point
    pub fn hot_score(score: i32, created_at: u64, now: u64) -> i64 {
        // Age in hours, minimum 2 to avoid division by zero and give new posts a chance
        let age_hours = ((now.saturating_sub(created_at)) / 3600) as i64 + 2;

        // Decay factor: increases with age squared (faster decay for old content)
        // age + age^2/100 gives reasonable decay curve
        let decay = age_hours + (age_hours * age_hours / 100);

        // Scale score and divide by decay
        // Multiply by 10000 for precision
        (score as i64 * 10000) / decay.max(1)
    }

    /// Calculate controversial score
    /// Content with many votes split between up and down is more controversial
    pub fn controversial_score(upvotes: u32, downvotes: u32) -> i64 {
        let total = upvotes as i64 + downvotes as i64;
        if total == 0 {
            return 0;
        }

        let min_votes = core::cmp::min(upvotes, downvotes) as i64;
        let max_votes = core::cmp::max(upvotes, downvotes) as i64;

        if max_votes == 0 {
            return 0;
        }

        // Controversial = volume * balance
        // balance = min/max (how evenly split the votes are)
        // Scale by 1000 for precision
        (total * min_votes * 1000) / max_votes
    }

    /// Calculate top score (simple net votes, optionally time-weighted)
    pub fn top_score(score: i32, _created_at: u64) -> i64 {
        // Simple net score - can be extended with time decay if needed
        score as i64
    }

    /// Get sorted thread IDs by hot score
    /// Note: This is a simplified version - in production you'd want
    /// to maintain sorted indices for efficiency
    pub fn get_hot_threads(
        env: Env,
        board_id: u64,
        thread_ids: Vec<u64>,
        now: u64,
    ) -> Vec<(u64, i64)> {
        let mut scored: Vec<(u64, i64)> = Vec::new(&env);

        for thread_id in thread_ids.iter() {
            let tally = Self::get_thread_tally(env.clone(), board_id, thread_id);
            let created_at = tally.first_vote_at;
            let hot = Self::hot_score(tally.score, created_at, now);
            scored.push_back((thread_id, hot));
        }

        // Note: Vec doesn't have built-in sort in Soroban SDK
        // In production, you'd maintain pre-sorted indices or use a different approach
        scored
    }

    // === Internal Helpers ===

    /// Calculate karma change from a vote change
    fn calculate_karma_delta(previous: VoteDirection, new: VoteDirection) -> i64 {
        let prev_val: i64 = match previous {
            VoteDirection::Up => 1,
            VoteDirection::Down => -1,
            VoteDirection::None => 0,
        };
        let new_val: i64 = match new {
            VoteDirection::Up => 1,
            VoteDirection::Down => -1,
            VoteDirection::None => 0,
        };
        new_val - prev_val
    }

    /// Require that caller has admin+ permissions on the board
    fn require_admin(env: &Env, board_id: u64, caller: &Address) {
        // Get permissions contract
        let permissions: Address = env
            .storage()
            .instance()
            .get(&VoteKey::Permissions)
            .expect("Permissions not set");

        // Get caller's permissions
        use soroban_sdk::{IntoVal, Symbol, Val};
        let args: Vec<Val> =
            Vec::from_array(env, [board_id.into_val(env), caller.clone().into_val(env)]);
        let perms: PermissionSet =
            env.invoke_contract(&permissions, &Symbol::new(env, "get_permissions"), args);

        if !perms.can_admin {
            panic!("Caller must be admin or owner");
        }
    }

    /// Upgrade the contract WASM (admin only via registry)
    pub fn upgrade(env: Env, new_wasm_hash: soroban_sdk::BytesN<32>) {
        // Only allow upgrade from registry
        let registry: Address = env
            .storage()
            .instance()
            .get(&VoteKey::Registry)
            .expect("Not initialized");
        registry.require_auth();

        env.deployer().update_current_contract_wasm(new_wasm_hash);
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::testutils::{Address as _, Ledger as _};
    use soroban_sdk::Env;

    fn setup_contract(env: &Env) -> (Address, BoardsVotingClient<'_>) {
        let contract_id = env.register(BoardsVoting, ());
        let client = BoardsVotingClient::new(env, &contract_id);

        let registry = Address::generate(env);
        let permissions = Address::generate(env);
        client.init(&registry, &permissions);

        (contract_id, client)
    }

    #[test]
    fn test_init() {
        let env = Env::default();
        env.mock_all_auths();

        let (_contract_id, client) = setup_contract(&env);

        // Default config should be returned for any board
        let config = client.get_voting_config(&0);
        assert!(config.enabled);
        assert!(config.allow_downvotes);
        assert!(config.karma_enabled);
        assert_eq!(config.karma_multiplier, 1);
    }

    #[test]
    fn test_vote_thread_upvote() {
        let env = Env::default();
        env.mock_all_auths();

        // Set ledger timestamp for the test
        env.ledger().set_timestamp(1000000);

        let (_contract_id, client) = setup_contract(&env);

        let voter = Address::generate(&env);
        let board_id = 0u64;
        let thread_id = 1u64;

        // Cast upvote
        let tally = client.vote_thread(&board_id, &thread_id, &VoteDirection::Up, &voter);

        assert_eq!(tally.upvotes, 1);
        assert_eq!(tally.downvotes, 0);
        assert_eq!(tally.score, 1);
        assert!(tally.first_vote_at > 0);

        // Verify user's vote is recorded
        let vote = client.get_user_thread_vote(&board_id, &thread_id, &voter);
        assert_eq!(vote, VoteDirection::Up);
    }

    #[test]
    fn test_vote_thread_downvote() {
        let env = Env::default();
        env.mock_all_auths();

        let (_contract_id, client) = setup_contract(&env);

        let voter = Address::generate(&env);
        let board_id = 0u64;
        let thread_id = 1u64;

        // Cast downvote
        let tally = client.vote_thread(&board_id, &thread_id, &VoteDirection::Down, &voter);

        assert_eq!(tally.upvotes, 0);
        assert_eq!(tally.downvotes, 1);
        assert_eq!(tally.score, -1);
    }

    #[test]
    fn test_vote_thread_change_vote() {
        let env = Env::default();
        env.mock_all_auths();

        let (_contract_id, client) = setup_contract(&env);

        let voter = Address::generate(&env);
        let board_id = 0u64;
        let thread_id = 1u64;

        // Cast upvote
        client.vote_thread(&board_id, &thread_id, &VoteDirection::Up, &voter);

        // Change to downvote
        let tally = client.vote_thread(&board_id, &thread_id, &VoteDirection::Down, &voter);

        // Should have removed upvote and added downvote
        assert_eq!(tally.upvotes, 0);
        assert_eq!(tally.downvotes, 1);
        assert_eq!(tally.score, -1);
    }

    #[test]
    fn test_vote_thread_remove_vote() {
        let env = Env::default();
        env.mock_all_auths();

        let (_contract_id, client) = setup_contract(&env);

        let voter = Address::generate(&env);
        let board_id = 0u64;
        let thread_id = 1u64;

        // Cast upvote
        client.vote_thread(&board_id, &thread_id, &VoteDirection::Up, &voter);

        // Remove vote
        let tally = client.vote_thread(&board_id, &thread_id, &VoteDirection::None, &voter);

        assert_eq!(tally.upvotes, 0);
        assert_eq!(tally.downvotes, 0);
        assert_eq!(tally.score, 0);

        // Verify vote is removed
        let vote = client.get_user_thread_vote(&board_id, &thread_id, &voter);
        assert_eq!(vote, VoteDirection::None);
    }

    #[test]
    fn test_multiple_voters() {
        let env = Env::default();
        env.mock_all_auths();

        let (_contract_id, client) = setup_contract(&env);

        let voter1 = Address::generate(&env);
        let voter2 = Address::generate(&env);
        let voter3 = Address::generate(&env);
        let board_id = 0u64;
        let thread_id = 1u64;

        // Three upvotes
        client.vote_thread(&board_id, &thread_id, &VoteDirection::Up, &voter1);
        client.vote_thread(&board_id, &thread_id, &VoteDirection::Up, &voter2);
        let tally = client.vote_thread(&board_id, &thread_id, &VoteDirection::Down, &voter3);

        assert_eq!(tally.upvotes, 2);
        assert_eq!(tally.downvotes, 1);
        assert_eq!(tally.score, 1);
    }

    #[test]
    fn test_vote_reply() {
        let env = Env::default();
        env.mock_all_auths();

        let (_contract_id, client) = setup_contract(&env);

        let voter = Address::generate(&env);
        let board_id = 0u64;
        let thread_id = 1u64;
        let reply_id = 2u64;

        // Cast upvote on reply
        let tally = client.vote_reply(&board_id, &thread_id, &reply_id, &VoteDirection::Up, &voter);

        assert_eq!(tally.upvotes, 1);
        assert_eq!(tally.downvotes, 0);
        assert_eq!(tally.score, 1);

        // Verify vote is recorded
        let vote = client.get_user_reply_vote(&board_id, &thread_id, &reply_id, &voter);
        assert_eq!(vote, VoteDirection::Up);
    }

    #[test]
    fn test_get_tally_no_votes() {
        let env = Env::default();
        env.mock_all_auths();

        let (_contract_id, client) = setup_contract(&env);

        let board_id = 0u64;
        let thread_id = 999u64;

        // Get tally for thread with no votes
        let tally = client.get_thread_tally(&board_id, &thread_id);

        assert_eq!(tally.upvotes, 0);
        assert_eq!(tally.downvotes, 0);
        assert_eq!(tally.score, 0);
        assert_eq!(tally.first_vote_at, 0);
    }

    #[test]
    fn test_karma_tracking() {
        let env = Env::default();
        env.mock_all_auths();

        let (_contract_id, client) = setup_contract(&env);

        let user = Address::generate(&env);
        let updater = Address::generate(&env);
        let board_id = 0u64;

        // Initially zero karma
        assert_eq!(client.get_board_karma(&board_id, &user), 0);
        assert_eq!(client.get_total_karma(&user), 0);

        // Add karma
        client.update_karma(&board_id, &user, &5, &updater);

        assert_eq!(client.get_board_karma(&board_id, &user), 5);
        assert_eq!(client.get_total_karma(&user), 5);

        // Add more karma from different board
        let board_id_2 = 1u64;
        client.update_karma(&board_id_2, &user, &3, &updater);

        assert_eq!(client.get_board_karma(&board_id_2, &user), 3);
        assert_eq!(client.get_total_karma(&user), 8);

        // Subtract karma
        client.update_karma(&board_id, &user, &-2, &updater);

        assert_eq!(client.get_board_karma(&board_id, &user), 3);
        assert_eq!(client.get_total_karma(&user), 6);
    }

    #[test]
    fn test_karma_multiplier() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsVoting, ());
        let client = BoardsVotingClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        let permissions = Address::generate(&env);
        client.init(&registry, &permissions);

        // We can't set config without admin permissions, but we can test the default
        let user = Address::generate(&env);
        let updater = Address::generate(&env);
        let board_id = 0u64;

        // With default multiplier of 1
        client.update_karma(&board_id, &user, &10, &updater);
        assert_eq!(client.get_board_karma(&board_id, &user), 10);
    }

    #[test]
    fn test_hot_score_algorithm() {
        // Test hot score calculation
        // Use a large enough timestamp to avoid underflow
        let now = 1000000u64;

        // Fresh post with score 10
        let hot1 = BoardsVoting::hot_score(10, now - 3600, now); // 1 hour old

        // Older post with same score
        let hot2 = BoardsVoting::hot_score(10, now - 36000, now); // 10 hours old

        // Fresh post should have higher hot score
        assert!(hot1 > hot2);

        // Negative score
        let hot_neg = BoardsVoting::hot_score(-5, now - 3600, now);
        assert!(hot_neg < 0);

        // Very old post
        let hot_old = BoardsVoting::hot_score(100, now - 360000, now); // 100 hours old
        assert!(hot_old < hot1);
    }

    #[test]
    fn test_controversial_score_algorithm() {
        // Even split = high controversy
        let score1 = BoardsVoting::controversial_score(50, 50);

        // Lopsided = low controversy
        let score2 = BoardsVoting::controversial_score(100, 10);

        // Even split should be more controversial
        assert!(score1 > score2);

        // No votes = 0
        let score3 = BoardsVoting::controversial_score(0, 0);
        assert_eq!(score3, 0);

        // Only upvotes = 0 controversy
        let score4 = BoardsVoting::controversial_score(100, 0);
        assert_eq!(score4, 0);
    }

    #[test]
    fn test_top_score_algorithm() {
        let score1 = BoardsVoting::top_score(100, 0);
        let score2 = BoardsVoting::top_score(-50, 0);

        assert_eq!(score1, 100);
        assert_eq!(score2, -50);
    }

    #[test]
    fn test_separate_thread_and_reply_tallies() {
        let env = Env::default();
        env.mock_all_auths();

        let (_contract_id, client) = setup_contract(&env);

        let voter = Address::generate(&env);
        let board_id = 0u64;
        let thread_id = 1u64;
        let reply_id = 1u64;

        // Vote on thread
        client.vote_thread(&board_id, &thread_id, &VoteDirection::Up, &voter);

        // Vote on reply with same IDs
        client.vote_reply(
            &board_id,
            &thread_id,
            &reply_id,
            &VoteDirection::Down,
            &voter,
        );

        // Tallies should be separate
        let thread_tally = client.get_thread_tally(&board_id, &thread_id);
        let reply_tally = client.get_reply_tally(&board_id, &thread_id, &reply_id);

        assert_eq!(thread_tally.upvotes, 1);
        assert_eq!(thread_tally.downvotes, 0);
        assert_eq!(reply_tally.upvotes, 0);
        assert_eq!(reply_tally.downvotes, 1);
    }

    #[test]
    fn test_votes_across_boards() {
        let env = Env::default();
        env.mock_all_auths();

        let (_contract_id, client) = setup_contract(&env);

        let voter = Address::generate(&env);
        let board_id_1 = 0u64;
        let board_id_2 = 1u64;
        let thread_id = 1u64;

        // Vote on same thread_id in different boards
        client.vote_thread(&board_id_1, &thread_id, &VoteDirection::Up, &voter);
        client.vote_thread(&board_id_2, &thread_id, &VoteDirection::Down, &voter);

        // Tallies should be separate per board
        let tally1 = client.get_thread_tally(&board_id_1, &thread_id);
        let tally2 = client.get_thread_tally(&board_id_2, &thread_id);

        assert_eq!(tally1.score, 1);
        assert_eq!(tally2.score, -1);
    }

    #[test]
    fn test_calculate_karma_delta() {
        // None -> Up = +1
        assert_eq!(
            BoardsVoting::calculate_karma_delta(VoteDirection::None, VoteDirection::Up),
            1
        );
        // None -> Down = -1
        assert_eq!(
            BoardsVoting::calculate_karma_delta(VoteDirection::None, VoteDirection::Down),
            -1
        );
        // Up -> Down = -2
        assert_eq!(
            BoardsVoting::calculate_karma_delta(VoteDirection::Up, VoteDirection::Down),
            -2
        );
        // Down -> Up = +2
        assert_eq!(
            BoardsVoting::calculate_karma_delta(VoteDirection::Down, VoteDirection::Up),
            2
        );
        // Up -> None = -1
        assert_eq!(
            BoardsVoting::calculate_karma_delta(VoteDirection::Up, VoteDirection::None),
            -1
        );
        // Down -> None = +1
        assert_eq!(
            BoardsVoting::calculate_karma_delta(VoteDirection::Down, VoteDirection::None),
            1
        );
        // Same vote = 0
        assert_eq!(
            BoardsVoting::calculate_karma_delta(VoteDirection::Up, VoteDirection::Up),
            0
        );
    }

    #[test]
    #[should_panic(expected = "Already initialized")]
    fn test_double_init() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsVoting, ());
        let client = BoardsVotingClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        let permissions = Address::generate(&env);

        client.init(&registry, &permissions);
        // Second init should panic
        client.init(&registry, &permissions);
    }

    #[test]
    fn test_vote_idempotent() {
        let env = Env::default();
        env.mock_all_auths();

        let (_contract_id, client) = setup_contract(&env);

        let voter = Address::generate(&env);
        let board_id = 0u64;
        let thread_id = 1u64;

        // Cast upvote
        client.vote_thread(&board_id, &thread_id, &VoteDirection::Up, &voter);

        // Cast same vote again - should be idempotent
        let tally = client.vote_thread(&board_id, &thread_id, &VoteDirection::Up, &voter);

        // Should still be 1 upvote (not 2)
        assert_eq!(tally.upvotes, 1);
        assert_eq!(tally.downvotes, 0);
        assert_eq!(tally.score, 1);
    }

    #[test]
    fn test_get_user_karma_empty() {
        let env = Env::default();
        env.mock_all_auths();

        let (_contract_id, client) = setup_contract(&env);

        let user = Address::generate(&env);

        // User with no karma should return 0
        assert_eq!(client.get_total_karma(&user), 0);
        assert_eq!(client.get_board_karma(&0, &user), 0);
    }

    #[test]
    fn test_karma_can_go_negative() {
        let env = Env::default();
        env.mock_all_auths();

        let (_contract_id, client) = setup_contract(&env);

        let user = Address::generate(&env);
        let updater = Address::generate(&env);
        let board_id = 0u64;

        // Start with some karma
        client.update_karma(&board_id, &user, &5, &updater);
        assert_eq!(client.get_board_karma(&board_id, &user), 5);

        // Subtract more than current karma
        client.update_karma(&board_id, &user, &-10, &updater);

        // Karma should be negative
        assert_eq!(client.get_board_karma(&board_id, &user), -5);
        assert_eq!(client.get_total_karma(&user), -5);
    }

    #[test]
    fn test_get_voting_config_default() {
        let env = Env::default();
        env.mock_all_auths();

        let (_contract_id, client) = setup_contract(&env);

        // Get config for any board - should return defaults
        let config = client.get_voting_config(&999);
        assert!(config.enabled);
        assert!(config.allow_downvotes);
        assert!(config.karma_enabled);
        assert_eq!(config.karma_multiplier, 1);
    }

    #[test]
    fn test_get_reply_tally_no_votes() {
        let env = Env::default();
        env.mock_all_auths();

        let (_contract_id, client) = setup_contract(&env);

        // Get tally for reply with no votes
        let tally = client.get_reply_tally(&0, &999, &888);

        assert_eq!(tally.upvotes, 0);
        assert_eq!(tally.downvotes, 0);
        assert_eq!(tally.score, 0);
    }
}
