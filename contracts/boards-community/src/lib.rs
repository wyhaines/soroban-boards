#![no_std]

use soroban_render_sdk::prelude::*;
use soroban_sdk::{contract, contractimpl, contracttype, Address, Bytes, Env, IntoVal, String, Symbol, Val, Vec};

// Declare render capabilities
soroban_render!(markdown);

/// Storage keys for the community contract
#[contracttype]
#[derive(Clone)]
pub enum CommunityKey {
    /// Registry contract address (for cross-contract calls)
    Registry,
    /// Permissions contract address
    Permissions,
    /// Theme contract address
    Theme,
    /// Config contract address
    Config,
    /// Total number of communities
    CommunityCount,
    /// Count of communities created by a user (for threshold limits)
    UserCommunityCount(Address),
    /// Community metadata by ID
    Community(u64),
    /// Community ID by name (case-insensitive lookup)
    CommunityByName(String),
    /// List of board IDs in a community
    CommunityBoards(u64),
    /// Reverse lookup: board_id -> community_id
    BoardCommunity(u64),
    /// Community members list (for private communities)
    CommunityMembers(u64),
    /// Pending join requests
    CommunityJoinRequests(u64),
    /// Community rules (markdown text stored as Bytes)
    CommunityRules(u64),
    /// Community permission defaults
    CommunityPermDefaults(u64),
    /// Whether community is listed publicly
    CommunityListed(u64),
    /// Pending ownership transfer request
    PendingOwnershipTransfer(u64),
}

/// Community metadata
#[contracttype]
#[derive(Clone)]
pub struct CommunityMeta {
    /// Unique community ID
    pub id: u64,
    /// URL-safe name (3-30 chars, lowercase alphanumeric + hyphen)
    pub name: String,
    /// Display name shown in UI
    pub display_name: String,
    /// Description
    pub description: String,
    /// Community owner address
    pub owner: Address,
    /// Creation timestamp
    pub created_at: u64,
    /// Number of boards in community
    pub board_count: u64,
    /// Number of members (for private communities)
    pub member_count: u64,
    /// Whether community is private (requires membership to view)
    pub is_private: bool,
}

/// Community rules and settings
#[contracttype]
#[derive(Clone)]
pub struct CommunityRules {
    /// Rules text in markdown format
    pub rules_text: String,
    /// Auto-approve new members (for private communities)
    pub auto_approve_members: bool,
    /// Minimum account age in days to join
    pub min_account_age_days: u32,
}

/// Board configuration (matches boards-board BoardConfig)
#[contracttype]
#[derive(Clone)]
pub struct BoardConfig {
    pub name: String,
    pub description: String,
    pub is_private: bool,
    pub is_readonly: bool,
    pub max_reply_depth: u32,
    pub reply_chunk_size: u32,
}

/// Board metadata (matches boards-board BoardMeta for cross-contract calls)
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
    pub is_listed: bool,
}

/// Minimal community info for navigation (used by board contract)
#[contracttype]
#[derive(Clone)]
pub struct CommunityInfo {
    pub id: u64,
    pub name: String,
    pub display_name: String,
}

/// Role levels (mirrors permissions contract)
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

/// Default permission settings for boards in this community
#[contracttype]
#[derive(Clone)]
pub struct CommunityPermissionDefaults {
    /// Minimum role to create threads (default: Member)
    pub min_thread_role: Role,
    /// Minimum role to reply (default: Member)
    pub min_reply_role: Role,
    /// New boards private by default
    pub default_board_private: bool,
    /// Maximum role that can be granted by community admins (prevents escalation)
    pub max_grantable_role: Role,
}

/// Join request for private communities
#[contracttype]
#[derive(Clone)]
pub struct JoinRequest {
    pub user: Address,
    pub requested_at: u64,
    pub message: String,
}

/// Pending ownership transfer request
#[contracttype]
#[derive(Clone)]
pub struct PendingOwnershipTransfer {
    /// Community ID being transferred
    pub community_id: u64,
    /// Address of the new owner
    pub new_owner: Address,
    /// Timestamp when transfer was initiated
    pub initiated_at: u64,
    /// Address of the current owner who initiated the transfer
    pub initiator: Address,
}

#[contract]
pub struct BoardsCommunity;

#[contractimpl]
impl BoardsCommunity {
    /// Initialize the community contract with registry address
    pub fn init(env: Env, registry: Address, permissions: Address, theme: Address) {
        if env.storage().instance().has(&CommunityKey::Registry) {
            panic!("Already initialized");
        }

        env.storage().instance().set(&CommunityKey::Registry, &registry);
        env.storage().instance().set(&CommunityKey::Permissions, &permissions);
        env.storage().instance().set(&CommunityKey::Theme, &theme);
        env.storage().instance().set(&CommunityKey::CommunityCount, &0u64);
    }

    /// Set the config contract address (called from registry)
    pub fn set_config(env: Env, config: Address) {
        // Verify caller is the registry
        let registry: Address = env
            .storage()
            .instance()
            .get(&CommunityKey::Registry)
            .expect("Not initialized");
        registry.require_auth();

        env.storage().instance().set(&CommunityKey::Config, &config);
    }

    /// Get the config contract address
    pub fn get_config(env: Env) -> Option<Address> {
        env.storage().instance().get(&CommunityKey::Config)
    }

    // === Admin Helper Functions ===

    /// Check if user is a community admin (via permissions contract)
    fn is_community_admin(env: &Env, community_id: u64, user: &Address) -> bool {
        let perms_opt: Option<Address> = env.storage().instance().get(&CommunityKey::Permissions);
        if let Some(perms) = perms_opt {
            let args: Vec<Val> = Vec::from_array(env, [
                community_id.into_val(env),
                user.clone().into_val(env),
                (Role::Admin as u32).into_val(env),
            ]);
            env.try_invoke_contract::<bool, soroban_sdk::Error>(
                &perms,
                &Symbol::new(env, "has_community_role"),
                args,
            )
            .ok()
            .and_then(|r| r.ok())
            .unwrap_or(false)
        } else {
            false
        }
    }

    /// Check if user is community owner or admin
    fn is_owner_or_admin(env: &Env, community: &CommunityMeta, user: &Address) -> bool {
        if *user == community.owner {
            return true;
        }
        Self::is_community_admin(env, community.id, user)
    }

    /// Get how many communities a user has created
    pub fn count_user_communities(env: Env, user: Address) -> u32 {
        env.storage()
            .persistent()
            .get(&CommunityKey::UserCommunityCount(user))
            .unwrap_or(0)
    }

    /// Create a new community
    /// Note: is_private and is_listed are Strings from form input ("true"/"false")
    pub fn create_community(
        env: Env,
        name: String,
        display_name: String,
        description: String,
        is_private: String,
        is_listed: String,
        caller: Address,
    ) -> u64 {
        caller.require_auth();

        // Validate name format
        Self::validate_community_name(&env, &name);

        // Parse is_private string to bool
        let is_private_bool = is_private == String::from_str(&env, "true");

        // Parse is_listed string to bool (default to true)
        let is_listed_bool = is_listed.len() == 0 || is_listed != String::from_str(&env, "false");

        // Check name not already taken
        let name_lower = Self::to_lowercase(&env, &name);
        if env
            .storage()
            .persistent()
            .has(&CommunityKey::CommunityByName(name_lower.clone()))
        {
            panic!("Community name already exists");
        }

        // Check if caller is site admin (bypass all threshold checks)
        let permissions: Option<Address> = env.storage().instance().get(&CommunityKey::Permissions);
        let is_site_admin = if let Some(ref perms) = permissions {
            let admin_args: Vec<Val> = Vec::from_array(&env, [caller.clone().into_val(&env)]);
            env.try_invoke_contract::<bool, soroban_sdk::Error>(
                perms,
                &Symbol::new(&env, "is_site_admin"),
                admin_args,
            )
            .ok()
            .and_then(|r| r.ok())
            .unwrap_or(false)
        } else {
            false
        };

        // Check creation thresholds if config contract is set (skip for site admins)
        if !is_site_admin {
            if let Some(config) = env
                .storage()
                .instance()
                .get::<_, Address>(&CommunityKey::Config)
            {
                let user_community_count = Self::count_user_communities(env.clone(), caller.clone());

                // Get account age from permissions contract
                let perms: Address = env
                    .storage()
                    .instance()
                    .get(&CommunityKey::Permissions)
                    .expect("Not initialized");
                let account_age_args: Vec<Val> =
                    Vec::from_array(&env, [caller.clone().into_val(&env)]);
                let user_account_age: u64 = env.invoke_contract(
                    &perms,
                    &Symbol::new(&env, "get_account_age"),
                    account_age_args,
                );

                // Check thresholds
                let check_args: Vec<Val> = Vec::from_array(
                    &env,
                    [
                        1u32.into_val(&env),  // CreationType::Community = 1
                        caller.clone().into_val(&env),
                        user_community_count.into_val(&env),
                        0i64.into_val(&env),  // user_karma (TODO: get from voting contract)
                        user_account_age.into_val(&env),
                        0u32.into_val(&env),  // user_post_count (TODO: get from content)
                        false.into_val(&env), // has_profile (TODO: get from profile contract)
                    ],
                );

                // Call config.check_thresholds and check result
                let result: (bool, String) = env.invoke_contract(
                    &config,
                    &Symbol::new(&env, "check_thresholds"),
                    check_args,
                );

                if !result.0 {
                    let mut buf = [0u8; 64];
                    let len = core::cmp::min(result.1.len() as usize, 64);
                    result.1.copy_into_slice(&mut buf[..len]);
                    let reason = core::str::from_utf8(&buf[..len]).unwrap_or("Threshold check failed");
                    panic!("{}", reason);
                }
            }
        }

        // Get next community ID
        let community_id: u64 = env
            .storage()
            .instance()
            .get(&CommunityKey::CommunityCount)
            .unwrap_or(0);

        // Create community metadata
        let community = CommunityMeta {
            id: community_id,
            name: name.clone(),
            display_name,
            description,
            owner: caller.clone(),
            created_at: env.ledger().timestamp(),
            board_count: 0,
            member_count: 1, // Owner is first member
            is_private: is_private_bool,
        };

        // Store community
        env.storage()
            .persistent()
            .set(&CommunityKey::Community(community_id), &community);

        // Store name lookup (lowercase for case-insensitive lookup)
        env.storage()
            .persistent()
            .set(&CommunityKey::CommunityByName(name_lower), &community_id);

        // Store listed status
        env.storage()
            .persistent()
            .set(&CommunityKey::CommunityListed(community_id), &is_listed_bool);

        // Initialize empty boards list
        let boards: Vec<u64> = Vec::new(&env);
        env.storage()
            .persistent()
            .set(&CommunityKey::CommunityBoards(community_id), &boards);

        // Initialize members list with owner
        let mut members: Vec<Address> = Vec::new(&env);
        members.push_back(caller.clone());
        env.storage()
            .persistent()
            .set(&CommunityKey::CommunityMembers(community_id), &members);

        // Initialize default permission settings
        let perm_defaults = CommunityPermissionDefaults {
            min_thread_role: Role::Member,
            min_reply_role: Role::Member,
            default_board_private: false,
            max_grantable_role: Role::Admin,
        };
        env.storage()
            .persistent()
            .set(&CommunityKey::CommunityPermDefaults(community_id), &perm_defaults);

        // Update count
        env.storage()
            .instance()
            .set(&CommunityKey::CommunityCount, &(community_id + 1));

        // Increment user's community count for threshold tracking
        let user_count = Self::count_user_communities(env.clone(), caller.clone());
        env.storage()
            .persistent()
            .set(&CommunityKey::UserCommunityCount(caller.clone()), &(user_count + 1));

        // Record first-seen timestamp for the user (idempotent if already recorded)
        let permissions: Address = env
            .storage()
            .instance()
            .get(&CommunityKey::Permissions)
            .expect("Not initialized");
        let record_args: Vec<Val> = Vec::from_array(&env, [caller.into_val(&env)]);
        let _ = env.try_invoke_contract::<(), soroban_sdk::Error>(
            &permissions,
            &Symbol::new(&env, "record_first_seen"),
            record_args,
        );

        community_id
    }

    /// Get community metadata by ID
    pub fn get_community(env: Env, community_id: u64) -> Option<CommunityMeta> {
        env.storage()
            .persistent()
            .get(&CommunityKey::Community(community_id))
    }

    /// Get minimal community info for navigation (used by board contract)
    pub fn get_community_info(env: Env, community_id: u64) -> Option<CommunityInfo> {
        let community: CommunityMeta = env
            .storage()
            .persistent()
            .get(&CommunityKey::Community(community_id))?;
        Some(CommunityInfo {
            id: community.id,
            name: community.name,
            display_name: community.display_name,
        })
    }

    /// Get community by name (case-insensitive)
    pub fn get_community_by_name(env: Env, name: String) -> Option<CommunityMeta> {
        let name_lower = Self::to_lowercase(&env, &name);
        if let Some(community_id) = env
            .storage()
            .persistent()
            .get::<_, u64>(&CommunityKey::CommunityByName(name_lower))
        {
            return env
                .storage()
                .persistent()
                .get(&CommunityKey::Community(community_id));
        }
        None
    }

    /// List communities with pagination
    pub fn list_communities(env: Env, start: u64, limit: u64) -> Vec<CommunityMeta> {
        let count: u64 = env
            .storage()
            .instance()
            .get(&CommunityKey::CommunityCount)
            .unwrap_or(0);

        let mut communities = Vec::new(&env);
        let end = core::cmp::min(start + limit, count);

        for i in start..end {
            if let Some(community) = env
                .storage()
                .persistent()
                .get(&CommunityKey::Community(i))
            {
                communities.push_back(community);
            }
        }

        communities
    }

    /// List only publicly listed communities
    pub fn list_listed_communities(env: Env, start: u64, limit: u64) -> Vec<CommunityMeta> {
        let count: u64 = env
            .storage()
            .instance()
            .get(&CommunityKey::CommunityCount)
            .unwrap_or(0);

        let mut communities = Vec::new(&env);
        let mut collected = 0u64;
        let mut skipped = 0u64;

        for i in 0..count {
            let is_listed: bool = env
                .storage()
                .persistent()
                .get(&CommunityKey::CommunityListed(i))
                .unwrap_or(true);

            if is_listed {
                if skipped < start {
                    skipped += 1;
                    continue;
                }
                if collected >= limit {
                    break;
                }
                if let Some(community) = env
                    .storage()
                    .persistent()
                    .get(&CommunityKey::Community(i))
                {
                    communities.push_back(community);
                    collected += 1;
                }
            }
        }

        communities
    }

    /// Get total community count
    pub fn community_count(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&CommunityKey::CommunityCount)
            .unwrap_or(0)
    }

    /// Add a board to a community
    /// Caller must be either the community owner, the board owner, or a registry admin
    pub fn add_board(env: Env, community_id: u64, board_id: u64, caller: Address) {
        caller.require_auth();

        // Verify community exists
        let community_opt: Option<CommunityMeta> = env
            .storage()
            .persistent()
            .get(&CommunityKey::Community(community_id));

        if community_opt.is_none() {
            panic!("Community {} does not exist", community_id);
        }
        let mut community = community_opt.unwrap();

        // Get registry for checks
        let registry_opt: Option<Address> = env
            .storage()
            .instance()
            .get(&CommunityKey::Registry);

        if registry_opt.is_none() {
            panic!("Community contract not initialized (no registry)");
        }
        let registry = registry_opt.unwrap();

        // Check if caller is a registry admin
        let admin_args: Vec<Val> = Vec::from_array(&env, [caller.clone().into_val(&env)]);
        let is_registry_admin: bool = env
            .try_invoke_contract::<bool, soroban_sdk::Error>(
                &registry,
                &Symbol::new(&env, "is_admin"),
                admin_args,
            )
            .ok()
            .and_then(|r| r.ok())
            .unwrap_or(false);

        // Verify caller is community owner/admin OR board owner OR registry admin
        let is_community_owner_or_admin = Self::is_owner_or_admin(&env, &community, &caller);
        let is_board_owner = {
            let alias_args: Vec<Val> = Vec::from_array(&env, [Symbol::new(&env, "board").into_val(&env)]);
            let board_contract: Option<Address> = env.invoke_contract(
                &registry,
                &Symbol::new(&env, "get_contract_by_alias"),
                alias_args,
            );
            if let Some(ref bc) = board_contract {
                // Get board metadata to check owner
                let board_args: Vec<Val> = Vec::from_array(&env, [board_id.into_val(&env)]);
                let board_meta: Option<BoardMeta> = env.try_invoke_contract::<BoardMeta, soroban_sdk::Error>(
                    bc,
                    &Symbol::new(&env, "get_board"),
                    board_args,
                ).ok().and_then(|r| r.ok());
                board_meta.map_or(false, |b| b.creator == caller)
            } else {
                false
            }
        };

        if !is_community_owner_or_admin && !is_board_owner && !is_registry_admin {
            panic!("Only community owner/admin, board owner, or site admin can add boards");
        }

        // Check if board is already in a community
        if env.storage().persistent().has(&CommunityKey::BoardCommunity(board_id)) {
            panic!("Board is already in a community");
        }

        // Add board to community's board list
        let mut boards: Vec<u64> = env
            .storage()
            .persistent()
            .get(&CommunityKey::CommunityBoards(community_id))
            .unwrap_or_else(|| Vec::new(&env));
        boards.push_back(board_id);
        env.storage()
            .persistent()
            .set(&CommunityKey::CommunityBoards(community_id), &boards);

        // Store reverse lookup
        env.storage()
            .persistent()
            .set(&CommunityKey::BoardCommunity(board_id), &community_id);

        // Update board count
        community.board_count += 1;
        env.storage()
            .persistent()
            .set(&CommunityKey::Community(community_id), &community);
    }

    /// Remove a board from a community
    pub fn remove_board(env: Env, community_id: u64, board_id: u64, caller: Address) {
        caller.require_auth();

        // Verify community exists
        let mut community: CommunityMeta = env
            .storage()
            .persistent()
            .get(&CommunityKey::Community(community_id))
            .expect("Community does not exist");

        // Verify caller is community owner or admin
        if !Self::is_owner_or_admin(&env, &community, &caller) {
            panic!("Only community owner or admin can remove boards");
        }

        // Remove board from community's board list
        let boards: Vec<u64> = env
            .storage()
            .persistent()
            .get(&CommunityKey::CommunityBoards(community_id))
            .unwrap_or_else(|| Vec::new(&env));

        let mut new_boards: Vec<u64> = Vec::new(&env);
        for b in boards.iter() {
            if b != board_id {
                new_boards.push_back(b);
            }
        }
        env.storage()
            .persistent()
            .set(&CommunityKey::CommunityBoards(community_id), &new_boards);

        // Remove reverse lookup
        env.storage()
            .persistent()
            .remove(&CommunityKey::BoardCommunity(board_id));

        // Update board count
        if community.board_count > 0 {
            community.board_count -= 1;
        }
        env.storage()
            .persistent()
            .set(&CommunityKey::Community(community_id), &community);
    }

    /// Get boards in a community
    pub fn get_community_boards(env: Env, community_id: u64) -> Vec<u64> {
        env.storage()
            .persistent()
            .get(&CommunityKey::CommunityBoards(community_id))
            .unwrap_or_else(|| Vec::new(&env))
    }

    /// Get community for a board (reverse lookup)
    pub fn get_board_community(env: Env, board_id: u64) -> Option<u64> {
        env.storage()
            .persistent()
            .get(&CommunityKey::BoardCommunity(board_id))
    }

    /// Fix board_count to match actual boards list (owner/admin only)
    /// Use this to repair data inconsistencies
    pub fn fix_board_count(env: Env, community_id: u64, caller: Address) {
        caller.require_auth();

        let mut community: CommunityMeta = env
            .storage()
            .persistent()
            .get(&CommunityKey::Community(community_id))
            .expect("Community does not exist");

        // Only owner or admin can fix
        if !Self::is_owner_or_admin(&env, &community, &caller) {
            panic!("Only community owner or admin can fix board count");
        }

        // Get actual boards list
        let boards: Vec<u64> = env
            .storage()
            .persistent()
            .get(&CommunityKey::CommunityBoards(community_id))
            .unwrap_or_else(|| Vec::new(&env));

        // Update count to match reality
        let actual_count = boards.len() as u64;
        if community.board_count != actual_count {
            community.board_count = actual_count;
            env.storage()
                .persistent()
                .set(&CommunityKey::Community(community_id), &community);
        }
    }

    /// Set community rules (owner/admin only)
    pub fn set_rules(env: Env, community_id: u64, rules: CommunityRules, caller: Address) {
        caller.require_auth();

        let community: CommunityMeta = env
            .storage()
            .persistent()
            .get(&CommunityKey::Community(community_id))
            .expect("Community does not exist");

        // Only owner or admin can set rules
        if !Self::is_owner_or_admin(&env, &community, &caller) {
            panic!("Only community owner or admin can set rules");
        }

        env.storage()
            .persistent()
            .set(&CommunityKey::CommunityRules(community_id), &rules);
    }

    /// Get community rules
    pub fn get_rules(env: Env, community_id: u64) -> Option<CommunityRules> {
        env.storage()
            .persistent()
            .get(&CommunityKey::CommunityRules(community_id))
    }

    /// Set permission defaults for boards in this community (owner/admin only)
    pub fn set_permission_defaults(
        env: Env,
        community_id: u64,
        defaults: CommunityPermissionDefaults,
        caller: Address,
    ) {
        caller.require_auth();

        let community: CommunityMeta = env
            .storage()
            .persistent()
            .get(&CommunityKey::Community(community_id))
            .expect("Community does not exist");

        if !Self::is_owner_or_admin(&env, &community, &caller) {
            panic!("Only community owner or admin can set permission defaults");
        }

        env.storage()
            .persistent()
            .set(&CommunityKey::CommunityPermDefaults(community_id), &defaults);
    }

    /// Get permission defaults
    pub fn get_permission_defaults(env: Env, community_id: u64) -> Option<CommunityPermissionDefaults> {
        env.storage()
            .persistent()
            .get(&CommunityKey::CommunityPermDefaults(community_id))
    }

    /// Request to join a private community
    pub fn request_join(env: Env, community_id: u64, message: String, caller: Address) {
        caller.require_auth();

        let community: CommunityMeta = env
            .storage()
            .persistent()
            .get(&CommunityKey::Community(community_id))
            .expect("Community does not exist");

        if !community.is_private {
            panic!("Community is public, no join request needed");
        }

        // Check if already a member
        let members: Vec<Address> = env
            .storage()
            .persistent()
            .get(&CommunityKey::CommunityMembers(community_id))
            .unwrap_or_else(|| Vec::new(&env));

        for member in members.iter() {
            if member == caller {
                panic!("Already a member");
            }
        }

        // Add join request
        let mut requests: Vec<JoinRequest> = env
            .storage()
            .persistent()
            .get(&CommunityKey::CommunityJoinRequests(community_id))
            .unwrap_or_else(|| Vec::new(&env));

        requests.push_back(JoinRequest {
            user: caller,
            requested_at: env.ledger().timestamp(),
            message,
        });

        env.storage()
            .persistent()
            .set(&CommunityKey::CommunityJoinRequests(community_id), &requests);
    }

    /// Accept a join request (owner/admin only)
    pub fn accept_join(env: Env, community_id: u64, user: Address, caller: Address) {
        caller.require_auth();

        let mut community: CommunityMeta = env
            .storage()
            .persistent()
            .get(&CommunityKey::Community(community_id))
            .expect("Community does not exist");

        if !Self::is_owner_or_admin(&env, &community, &caller) {
            panic!("Only community owner or admin can accept join requests");
        }

        // Remove from pending requests
        let requests: Vec<JoinRequest> = env
            .storage()
            .persistent()
            .get(&CommunityKey::CommunityJoinRequests(community_id))
            .unwrap_or_else(|| Vec::new(&env));

        let mut found = false;
        let mut new_requests: Vec<JoinRequest> = Vec::new(&env);
        for req in requests.iter() {
            if req.user == user {
                found = true;
            } else {
                new_requests.push_back(req);
            }
        }

        if !found {
            panic!("No pending join request from this user");
        }

        env.storage()
            .persistent()
            .set(&CommunityKey::CommunityJoinRequests(community_id), &new_requests);

        // Add to members
        let mut members: Vec<Address> = env
            .storage()
            .persistent()
            .get(&CommunityKey::CommunityMembers(community_id))
            .unwrap_or_else(|| Vec::new(&env));
        members.push_back(user);
        env.storage()
            .persistent()
            .set(&CommunityKey::CommunityMembers(community_id), &members);

        // Update member count
        community.member_count += 1;
        env.storage()
            .persistent()
            .set(&CommunityKey::Community(community_id), &community);
    }

    /// Check if user is a member of community
    pub fn is_member(env: Env, community_id: u64, user: Address) -> bool {
        let members: Vec<Address> = env
            .storage()
            .persistent()
            .get(&CommunityKey::CommunityMembers(community_id))
            .unwrap_or_else(|| Vec::new(&env));

        for member in members.iter() {
            if member == user {
                return true;
            }
        }
        false
    }

    // === Community Admin Management ===

    /// Add a community admin (owner or existing admin can call)
    pub fn add_admin(env: Env, community_id: u64, user: Address, caller: Address) {
        caller.require_auth();

        let community = Self::get_community(env.clone(), community_id)
            .expect("Community not found");

        // Owner or existing admin can add admins
        if !Self::is_owner_or_admin(&env, &community, &caller) {
            panic!("Only owner or admin can add admins");
        }

        // Call permissions contract to set role
        let perms: Address = env
            .storage()
            .instance()
            .get(&CommunityKey::Permissions)
            .expect("Permissions not initialized");

        let args: Vec<Val> = Vec::from_array(&env, [
            community_id.into_val(&env),
            user.into_val(&env),
            (Role::Admin as u32).into_val(&env),
            caller.into_val(&env),
        ]);
        env.invoke_contract::<()>(&perms, &Symbol::new(&env, "set_community_role"), args);
    }

    /// Remove a community admin (owner only)
    pub fn remove_admin(env: Env, community_id: u64, user: Address, caller: Address) {
        caller.require_auth();

        let community = Self::get_community(env.clone(), community_id)
            .expect("Community not found");

        // Only owner can remove admins
        if caller != community.owner {
            panic!("Only owner can remove admins");
        }

        // Call permissions contract to set role to Guest (removes admin)
        let perms: Address = env
            .storage()
            .instance()
            .get(&CommunityKey::Permissions)
            .expect("Permissions not initialized");

        let args: Vec<Val> = Vec::from_array(&env, [
            community_id.into_val(&env),
            user.into_val(&env),
            (Role::Guest as u32).into_val(&env),
            caller.into_val(&env),
        ]);
        env.invoke_contract::<()>(&perms, &Symbol::new(&env, "set_community_role"), args);
    }

    /// Get list of community admins
    pub fn get_admins(env: Env, community_id: u64) -> Vec<Address> {
        let perms_opt: Option<Address> = env.storage().instance().get(&CommunityKey::Permissions);
        if let Some(perms) = perms_opt {
            let args: Vec<Val> = Vec::from_array(&env, [community_id.into_val(&env)]);
            env.try_invoke_contract::<Vec<Address>, soroban_sdk::Error>(
                &perms,
                &Symbol::new(&env, "list_community_admins"),
                args,
            )
            .ok()
            .and_then(|r| r.ok())
            .unwrap_or(Vec::new(&env))
        } else {
            Vec::new(&env)
        }
    }

    /// Get communities where user is owner or admin
    pub fn get_manageable_communities(env: Env, user: Address) -> Vec<CommunityInfo> {
        let mut result: Vec<CommunityInfo> = Vec::new(&env);
        let count = Self::community_count(env.clone());

        for id in 0..count {
            if let Some(community) = Self::get_community(env.clone(), id) {
                if Self::is_owner_or_admin(&env, &community, &user) {
                    result.push_back(CommunityInfo {
                        id: community.id,
                        name: community.name.clone(),
                        display_name: community.display_name.clone(),
                    });
                }
            }
        }

        result
    }

    /// Get pending join requests (owner/admin only)
    pub fn get_join_requests(env: Env, community_id: u64, caller: Address) -> Vec<JoinRequest> {
        caller.require_auth();

        let community: CommunityMeta = env
            .storage()
            .persistent()
            .get(&CommunityKey::Community(community_id))
            .expect("Community does not exist");

        if !Self::is_owner_or_admin(&env, &community, &caller) {
            panic!("Only community owner or admin can view join requests");
        }

        env.storage()
            .persistent()
            .get(&CommunityKey::CommunityJoinRequests(community_id))
            .unwrap_or_else(|| Vec::new(&env))
    }

    /// Update community metadata (owner/admin only)
    /// Accepts is_private and is_listed as strings ("true"/"false") from form input
    pub fn update_community(
        env: Env,
        community_id: u64,
        display_name: String,
        description: String,
        is_private: String,
        is_listed: String,
        caller: Address,
    ) {
        caller.require_auth();

        let mut community: CommunityMeta = env
            .storage()
            .persistent()
            .get(&CommunityKey::Community(community_id))
            .expect("Community does not exist");

        if !Self::is_owner_or_admin(&env, &community, &caller) {
            panic!("Only community owner or admin can update community");
        }

        community.display_name = display_name;
        community.description = description;

        // Parse is_private string to bool
        community.is_private = is_private == String::from_str(&env, "true");

        env.storage()
            .persistent()
            .set(&CommunityKey::Community(community_id), &community);

        // Parse and update is_listed
        let is_listed_bool = is_listed.len() == 0 || is_listed != String::from_str(&env, "false");

        env.storage()
            .persistent()
            .set(&CommunityKey::CommunityListed(community_id), &is_listed_bool);
    }

    /// Initiate ownership transfer (current owner only)
    /// Creates a pending transfer request that the new owner must accept
    pub fn initiate_transfer(
        env: Env,
        community_id: u64,
        new_owner: Address,
        caller: Address,
    ) {
        caller.require_auth();

        let community: CommunityMeta = env
            .storage()
            .persistent()
            .get(&CommunityKey::Community(community_id))
            .expect("Community does not exist");

        if caller != community.owner {
            panic!("Only community owner can initiate transfer");
        }

        // Can't transfer to self
        if new_owner == caller {
            panic!("Cannot transfer to yourself");
        }

        let pending = PendingOwnershipTransfer {
            community_id,
            new_owner,
            initiated_at: env.ledger().timestamp(),
            initiator: caller,
        };

        env.storage()
            .persistent()
            .set(&CommunityKey::PendingOwnershipTransfer(community_id), &pending);
    }

    /// Cancel pending ownership transfer (current owner only)
    pub fn cancel_transfer(env: Env, community_id: u64, caller: Address) {
        caller.require_auth();

        let community: CommunityMeta = env
            .storage()
            .persistent()
            .get(&CommunityKey::Community(community_id))
            .expect("Community does not exist");

        if caller != community.owner {
            panic!("Only community owner can cancel transfer");
        }

        if !env
            .storage()
            .persistent()
            .has(&CommunityKey::PendingOwnershipTransfer(community_id))
        {
            panic!("No pending transfer to cancel");
        }

        env.storage()
            .persistent()
            .remove(&CommunityKey::PendingOwnershipTransfer(community_id));
    }

    /// Accept pending ownership transfer (new owner only)
    /// Completes the transfer and makes the caller the new owner
    pub fn accept_transfer(env: Env, community_id: u64, caller: Address) {
        caller.require_auth();

        let pending: PendingOwnershipTransfer = env
            .storage()
            .persistent()
            .get(&CommunityKey::PendingOwnershipTransfer(community_id))
            .expect("No pending transfer");

        if caller != pending.new_owner {
            panic!("Only designated new owner can accept transfer");
        }

        let mut community: CommunityMeta = env
            .storage()
            .persistent()
            .get(&CommunityKey::Community(community_id))
            .expect("Community does not exist");

        // Add new owner to members if not already
        let mut members: Vec<Address> = env
            .storage()
            .persistent()
            .get(&CommunityKey::CommunityMembers(community_id))
            .unwrap_or_else(|| Vec::new(&env));

        let mut is_member = false;
        for member in members.iter() {
            if member == caller {
                is_member = true;
                break;
            }
        }
        if !is_member {
            members.push_back(caller.clone());
            community.member_count += 1;
            env.storage()
                .persistent()
                .set(&CommunityKey::CommunityMembers(community_id), &members);
        }

        // Transfer ownership
        community.owner = caller;
        env.storage()
            .persistent()
            .set(&CommunityKey::Community(community_id), &community);

        // Remove pending transfer
        env.storage()
            .persistent()
            .remove(&CommunityKey::PendingOwnershipTransfer(community_id));
    }

    /// Get pending ownership transfer if any
    pub fn get_pending_transfer(env: Env, community_id: u64) -> Option<PendingOwnershipTransfer> {
        env.storage()
            .persistent()
            .get(&CommunityKey::PendingOwnershipTransfer(community_id))
    }

    /// Delete a community (owner only, requires board_count == 0)
    pub fn delete_community(env: Env, community_id: u64, caller: Address) {
        caller.require_auth();

        let community: CommunityMeta = env
            .storage()
            .persistent()
            .get(&CommunityKey::Community(community_id))
            .expect("Community does not exist");

        if caller != community.owner {
            panic!("Only community owner can delete community");
        }

        if community.board_count > 0 {
            panic!("Cannot delete community with boards. Remove all boards first.");
        }

        // Remove all associated storage
        let name_lower = Self::to_lowercase(&env, &community.name);

        env.storage()
            .persistent()
            .remove(&CommunityKey::Community(community_id));
        env.storage()
            .persistent()
            .remove(&CommunityKey::CommunityByName(name_lower));
        env.storage()
            .persistent()
            .remove(&CommunityKey::CommunityBoards(community_id));
        env.storage()
            .persistent()
            .remove(&CommunityKey::CommunityMembers(community_id));
        env.storage()
            .persistent()
            .remove(&CommunityKey::CommunityJoinRequests(community_id));
        env.storage()
            .persistent()
            .remove(&CommunityKey::CommunityRules(community_id));
        env.storage()
            .persistent()
            .remove(&CommunityKey::CommunityPermDefaults(community_id));
        env.storage()
            .persistent()
            .remove(&CommunityKey::CommunityListed(community_id));
        env.storage()
            .persistent()
            .remove(&CommunityKey::PendingOwnershipTransfer(community_id));

        // Note: We don't decrement CommunityCount as IDs are not reused
    }

    /// Render community page
    pub fn render(env: Env, path: String, viewer: Option<Address>) -> Bytes {
        // Parse path to determine what to render
        // "/" or "" -> community listing
        // "/c/{name}" -> community home page
        // "/c/{name}/boards" -> list boards in community
        // "/new" -> create community form

        let path_bytes = string_to_bytes(&env, &path);
        let path_len = path_bytes.len() as usize;
        let mut buf = [0u8; 256];
        let copy_len = core::cmp::min(path_len, 256);
        path_bytes.copy_into_slice(&mut buf[..copy_len]);

        // Check for /new route
        if copy_len >= 4 && &buf[0..4] == b"/new" {
            return Self::render_create_form(&env, viewer);
        }

        // Check for /c/{name} route
        if copy_len >= 3 && &buf[0..3] == b"/c/" {
            // Extract community name from path
            let name_end = Self::find_next_slash(&buf, 3, copy_len);
            if name_end > 3 {
                let name_slice = &buf[3..name_end];
                let community_name = Self::bytes_to_string(&env, name_slice);

                // Check if there's a subpath
                if name_end < copy_len {
                    let sub_path = &buf[name_end..copy_len];
                    if sub_path.starts_with(b"/boards") {
                        return Self::render_community_boards(&env, &community_name, viewer);
                    }
                    if sub_path.starts_with(b"/settings") {
                        return Self::render_settings(&env, &community_name, viewer);
                    }
                    if sub_path.starts_with(b"/delete") {
                        return Self::render_delete(&env, &community_name, viewer);
                    }
                    if sub_path.starts_with(b"/fix-count") {
                        return Self::render_fix_count(&env, &community_name, viewer);
                    }
                }

                return Self::render_community_home(&env, &community_name, viewer);
            }
        }

        // Default: render community listing
        Self::render_community_list(&env, viewer)
    }

    // === Render Helper Functions ===

    fn render_community_list(env: &Env, _viewer: Option<Address>) -> Bytes {
        let communities = Self::list_listed_communities(env.clone(), 0, 20);

        let mut builder = MarkdownBuilder::new(env);
        builder = builder.h1("Communities");
        builder = builder.paragraph("Explore communities or create your own.");

        // Create community button
        builder = builder.raw_str("<p><a class=\"soroban-action\" href=\"render:/new\">Create Community</a></p>\n");

        if communities.is_empty() {
            builder = builder.paragraph("No communities yet. Be the first to create one!");
        } else {
            builder = builder.raw_str("<div class=\"community-list\">\n");
            for community in communities.iter() {
                let meta: CommunityMeta = community;
                builder = Self::append_community_card(env, builder, &meta);
            }
            builder = builder.raw_str("</div>\n");
        }

        builder.build()
    }

    fn append_community_card<'a>(_env: &'a Env, mut builder: MarkdownBuilder<'a>, community: &CommunityMeta) -> MarkdownBuilder<'a> {
        // Build the URL for the link
        let mut url_buf = [0u8; 64];
        let prefix = b"render:/c/";
        url_buf[0..10].copy_from_slice(prefix);
        let name_len = community.name.len() as usize;
        let name_copy_len = core::cmp::min(name_len, 50);
        community.name.copy_into_slice(&mut url_buf[10..10 + name_copy_len]);
        let url = core::str::from_utf8(&url_buf[0..10 + name_copy_len]).unwrap_or("");

        // Wrap entire card in an <a> tag like board-card and thread-card
        builder = builder.raw_str("<a href=\"");
        builder = builder.text(url);
        builder = builder.raw_str("\" class=\"community-card\">");

        // Display name as title span
        builder = builder.raw_str("<span class=\"community-card-title\">");
        builder = builder.text_string(&community.display_name);
        builder = builder.raw_str("</span>");

        // Description as desc span
        builder = builder.raw_str("<span class=\"community-card-desc\">");
        builder = builder.text_string(&community.description);
        builder = builder.raw_str("</span>");

        // Stats as meta span
        builder = builder.raw_str("<span class=\"community-card-meta\">");
        builder = builder.number(community.board_count as u32);
        builder = builder.raw_str(" boards · ");
        builder = builder.number(community.member_count as u32);
        builder = builder.raw_str(" members");
        if community.is_private {
            builder = builder.raw_str(" <span class=\"badge\">Private</span>");
        }
        builder = builder.raw_str("</span>");

        builder = builder.raw_str("</a>\n");
        builder
    }

    /// Render a list of boards as cards (fetches metadata from board contracts)
    /// Admins and board owners can see hidden boards with a "hidden" badge
    fn render_board_cards<'a>(
        env: &'a Env,
        mut builder: MarkdownBuilder<'a>,
        board_ids: &Vec<u64>,
        viewer: &Option<Address>,
        community: &CommunityMeta,
    ) -> MarkdownBuilder<'a> {
        if board_ids.is_empty() {
            return builder.paragraph("No boards in this community yet.");
        }

        // Get registry to look up board contract
        let registry: Address = match env.storage().instance().get(&CommunityKey::Registry) {
            Some(r) => r,
            None => {
                return builder.paragraph("Registry not configured.");
            }
        };

        // Check if viewer is a community owner/admin or registry admin
        let viewer_is_community_admin = if let Some(ref v) = viewer {
            Self::is_owner_or_admin(env, community, v)
        } else {
            false
        };

        let viewer_is_registry_admin = if let Some(ref v) = viewer {
            let admin_args: Vec<Val> = Vec::from_array(env, [v.clone().into_val(env)]);
            env.try_invoke_contract::<bool, soroban_sdk::Error>(
                &registry,
                &Symbol::new(env, "is_admin"),
                admin_args,
            ).ok().and_then(|r| r.ok()).unwrap_or(false)
        } else {
            false
        };

        // Get single board contract via registry alias
        let alias_args: Vec<Val> = Vec::from_array(env, [Symbol::new(env, "board").into_val(env)]);
        let board_contract_opt: Option<Address> = env.try_invoke_contract::<Option<Address>, soroban_sdk::Error>(
            &registry,
            &Symbol::new(env, "get_contract_by_alias"),
            alias_args,
        ).ok().and_then(|r| r.ok()).flatten();

        let board_contract = match board_contract_opt {
            Some(c) => c,
            None => {
                return builder.paragraph("Board contract not configured.");
            }
        };

        builder = builder.raw_str("<div class=\"board-list\">\n");

        for board_id in board_ids.iter() {
            // Get board metadata (includes is_listed and thread_count)
            let board_opt: Option<BoardMeta> = env.try_invoke_contract::<BoardMeta, soroban_sdk::Error>(
                &board_contract,
                &Symbol::new(env, "get_board"),
                Vec::from_array(env, [board_id.into_val(env)]),
            ).ok().and_then(|r| r.ok());

            if let Some(board) = board_opt {
                // Check if viewer can see hidden boards
                let viewer_is_board_owner = if let Some(ref v) = viewer {
                    *v == board.creator
                } else {
                    false
                };

                let can_see_hidden = viewer_is_community_admin || viewer_is_registry_admin || viewer_is_board_owner;

                // Skip hidden boards unless viewer has permission
                if !board.is_listed && !can_see_hidden {
                    continue;
                }

                // Board card with link wrapper - use render:/b/ to go through main contract
                builder = builder.raw_str("<a href=\"render:/b/")
                    .number(board_id as u32)
                    .raw_str("\" class=\"board-card\"><span class=\"board-card-title\">")
                    .text_string(&board.name)
                    .raw_str("</span><span class=\"board-card-desc\">")
                    .text_string(&board.description)
                    .raw_str("</span><span class=\"board-card-meta\">")
                    .number(board.thread_count as u32)
                    .text(" threads");
                if board.is_private {
                    builder = builder.raw_str(" <span class=\"badge\">private</span>");
                }
                if !board.is_listed {
                    builder = builder.raw_str(" <span class=\"badge\">hidden</span>");
                }
                builder = builder.raw_str("</span></a>\n");
            }
        }

        builder = builder.raw_str("</div>\n");
        builder
    }

    fn render_community_home(env: &Env, name: &String, viewer: Option<Address>) -> Bytes {
        let community = match Self::get_community_by_name(env.clone(), name.clone()) {
            Some(c) => c,
            None => {
                return MarkdownBuilder::new(env)
                    .paragraph("Community not found")
                    .build();
            }
        };

        // Check if private and viewer is not a member
        if community.is_private {
            if let Some(ref v) = viewer {
                if !Self::is_member(env.clone(), community.id, v.clone()) && *v != community.owner {
                    return MarkdownBuilder::new(env)
                        .paragraph("This is a private community. Join to view content.")
                        .build();
                }
            } else {
                return MarkdownBuilder::new(env)
                    .paragraph("This is a private community. Sign in to request access.")
                    .build();
            }
        }

        // Display name for title
        let mut display_buf = [0u8; 128];
        let display_len = community.display_name.len() as usize;
        let display_copy_len = core::cmp::min(display_len, 128);
        community.display_name.copy_into_slice(&mut display_buf[0..display_copy_len]);
        let display = core::str::from_utf8(&display_buf[0..display_copy_len]).unwrap_or("Community");

        // Description
        let mut desc_buf = [0u8; 256];
        let desc_len = community.description.len() as usize;
        let desc_copy_len = core::cmp::min(desc_len, 256);
        community.description.copy_into_slice(&mut desc_buf[0..desc_copy_len]);
        let desc = core::str::from_utf8(&desc_buf[0..desc_copy_len]).unwrap_or("");

        let mut builder = MarkdownBuilder::new(env);

        // Back navigation
        builder = builder.raw_str("<div class=\"back-nav\">");
        builder = builder.raw_str("<a href=\"render:/communities\" class=\"back-link\">← Communities</a>");
        builder = builder.raw_str("</div>\n");

        builder = builder.newline();
        builder = builder.h1(display);
        builder = builder.paragraph(desc);

        // Community stats
        builder = builder.raw_str("<div class=\"community-stats\"><span>");
        builder = builder.number(community.board_count as u32);
        builder = builder.raw_str(" boards</span> <span>");
        builder = builder.number(community.member_count as u32);
        builder = builder.raw_str(" members</span></div>\n");

        // Actions based on viewer (owner or admin)
        if let Some(ref v) = viewer {
            if Self::is_owner_or_admin(env, &community, v) {
                // Owner/Admin actions - build settings URL
                let mut settings_url_buf = [0u8; 80];
                let settings_prefix = b"render:/c/";
                settings_url_buf[0..10].copy_from_slice(settings_prefix);
                let name_len = community.name.len() as usize;
                let name_copy_len = core::cmp::min(name_len, 50);
                community.name.copy_into_slice(&mut settings_url_buf[10..10 + name_copy_len]);
                let settings_suffix = b"/settings";
                let suffix_start = 10 + name_copy_len;
                settings_url_buf[suffix_start..suffix_start + 9].copy_from_slice(settings_suffix);
                let settings_url = core::str::from_utf8(&settings_url_buf[0..suffix_start + 9]).unwrap_or("");

                builder = builder.raw_str("<div class=\"community-actions\">");
                builder = builder.raw_str("<a class=\"soroban-action\" href=\"");
                builder = builder.raw_str(settings_url);
                builder = builder.raw_str("\">Settings</a>");

                // Create Board link - goes to /create?community=ID
                builder = builder.raw_str(" | <a class=\"soroban-action\" href=\"render:/create?community=");
                builder = builder.number(community.id as u32);
                builder = builder.raw_str("\">+ Create Board</a>");

                builder = builder.raw_str("</div>\n");
            }
        }

        // List boards in community
        builder = builder.newline();
        builder = builder.h2("Boards");
        let board_ids = Self::get_community_boards(env.clone(), community.id);
        builder = Self::render_board_cards(env, builder, &board_ids, &viewer, &community);

        // Rules if set
        if let Some(rules) = Self::get_rules(env.clone(), community.id) {
            let mut rules_buf = [0u8; 512];
            let rules_len = rules.rules_text.len() as usize;
            let rules_copy_len = core::cmp::min(rules_len, 512);
            rules.rules_text.copy_into_slice(&mut rules_buf[0..rules_copy_len]);
            let rules_text = core::str::from_utf8(&rules_buf[0..rules_copy_len]).unwrap_or("");
            builder = builder.h2("Community Rules");
            builder = builder.paragraph(rules_text);
        }

        builder.build()
    }

    fn render_community_boards(env: &Env, name: &String, viewer: Option<Address>) -> Bytes {
        let community = match Self::get_community_by_name(env.clone(), name.clone()) {
            Some(c) => c,
            None => {
                return MarkdownBuilder::new(env)
                    .paragraph("Community not found")
                    .build();
            }
        };

        // Display name for title
        let mut display_buf = [0u8; 128];
        let display_len = community.display_name.len() as usize;
        let display_copy_len = core::cmp::min(display_len, 128);
        community.display_name.copy_into_slice(&mut display_buf[0..display_copy_len]);
        let display = core::str::from_utf8(&display_buf[0..display_copy_len]).unwrap_or("Community");

        let mut builder = MarkdownBuilder::new(env);
        builder = builder.h1(display);
        builder = builder.h2("Boards");

        let board_ids = Self::get_community_boards(env.clone(), community.id);
        builder = Self::render_board_cards(env, builder, &board_ids, &viewer, &community);

        builder.build()
    }

    fn render_settings(env: &Env, name: &String, viewer: Option<Address>) -> Bytes {
        use soroban_sdk::{IntoVal, Symbol, Val, Vec};

        let community = match Self::get_community_by_name(env.clone(), name.clone()) {
            Some(c) => c,
            None => {
                return MarkdownBuilder::new(env)
                    .paragraph("Community not found")
                    .build();
            }
        };

        // Check if viewer is owner
        let is_owner = match &viewer {
            Some(v) => *v == community.owner,
            None => false,
        };

        // Check if viewer is a site admin (bypass owner check)
        let is_site_admin = if let Some(ref user) = viewer {
            // Check permissions contract first
            let permissions: Option<Address> = env.storage().instance().get(&CommunityKey::Permissions);
            let perms_admin = if let Some(ref perms) = permissions {
                let admin_args: Vec<Val> = Vec::from_array(env, [user.clone().into_val(env)]);
                env.try_invoke_contract::<bool, soroban_sdk::Error>(
                    perms,
                    &Symbol::new(env, "is_site_admin"),
                    admin_args,
                )
                .ok()
                .and_then(|r| r.ok())
                .unwrap_or(false)
            } else {
                false
            };

            // Also check registry's is_admin for backwards compatibility
            if perms_admin {
                true
            } else if let Some(registry) = env.storage().instance().get::<_, Address>(&CommunityKey::Registry) {
                let admin_args: Vec<Val> = Vec::from_array(env, [user.clone().into_val(env)]);
                env.try_invoke_contract::<bool, soroban_sdk::Error>(
                    &registry,
                    &Symbol::new(env, "is_admin"),
                    admin_args,
                )
                .ok()
                .and_then(|r| r.ok())
                .unwrap_or(false)
            } else {
                false
            }
        } else {
            false
        };

        // Check if viewer is a community admin
        let is_community_admin = if let Some(ref user) = viewer {
            Self::is_community_admin(env, community.id, user)
        } else {
            false
        };

        if !is_owner && !is_community_admin && !is_site_admin {
            return MarkdownBuilder::new(env)
                .warning("Only community owners or admins can access settings.")
                .build();
        }

        // Get current listed status
        let is_listed: bool = env
            .storage()
            .persistent()
            .get(&CommunityKey::CommunityListed(community.id))
            .unwrap_or(true);

        let mut builder = MarkdownBuilder::new(env);

        // Back navigation to community
        builder = builder.raw_str("<div class=\"back-nav\">");
        builder = builder.raw_str("<a href=\"render:/c/");
        builder = builder.text_string(&community.name);
        builder = builder.raw_str("\" class=\"back-link\">← Back to Community</a>");
        builder = builder.raw_str("</div>\n");

        // Title
        builder = builder.newline();
        builder = builder.h1("Community Settings");

        // Basic Info Section
        builder = builder.newline();
        builder = builder.h2("Basic Information");
        builder = builder.raw_str("<input type=\"hidden\" name=\"community_id\" value=\"");
        builder = builder.number(community.id as u32);
        builder = builder.raw_str("\" />\n");

        // Display name field with current value
        builder = builder.raw_str("<label>Display Name:</label>\n");
        builder = builder.raw_str("<input type=\"text\" name=\"display_name\" value=\"");
        builder = builder.text_string(&community.display_name);
        builder = builder.raw_str("\" />\n");

        // Description field with current value
        builder = builder.raw_str("<label>Description:</label>\n");
        builder = builder.textarea_markdown_with_value_string(
            "description",
            3,
            "Describe your community...",
            &community.description,
        );

        // Visibility Section
        builder = builder.newline();
        builder = builder.h2("Visibility");

        // Private checkbox
        builder = builder.raw_str("<input type=\"hidden\" name=\"is_private\" value=\"false\" />\n");
        builder = builder.raw_str("<label><input type=\"checkbox\" name=\"is_private\" value=\"true\"");
        if community.is_private {
            builder = builder.raw_str(" checked");
        }
        builder = builder.raw_str(" /> Private (members only)</label>\n");

        // Listed checkbox (inverted - checkbox means unlisted)
        builder = builder.raw_str("<input type=\"hidden\" name=\"is_listed\" value=\"true\" />\n");
        builder = builder.raw_str("<label><input type=\"checkbox\" name=\"is_listed\" value=\"false\"");
        if !is_listed {
            builder = builder.raw_str(" checked");
        }
        builder = builder.raw_str(" /> Hide from public directory (unlisted)</label>\n");

        // Caller and redirect
        builder = builder.raw_str("<input type=\"hidden\" name=\"caller\" value=\"");
        builder = builder.text_string(&viewer.as_ref().unwrap().to_string());
        builder = builder.raw_str("\" />\n");
        builder = builder.raw_str("<input type=\"hidden\" name=\"_redirect\" value=\"/c/");
        builder = builder.text_string(&community.name);
        builder = builder.raw_str("/settings\" />\n");

        builder = builder.newline();
        builder = builder.raw_str("<a href=\"form:update_community\" class=\"soroban-action\">Save Changes</a>\n");

        // Ownership Transfer Section
        builder = builder.newline();
        builder = builder.h2("Ownership Transfer");

        // Check for pending transfer
        if let Some(pending) = Self::get_pending_transfer(env.clone(), community.id) {
            builder = builder.note("Pending transfer to:");
            builder = builder.newline();
            builder = builder.raw_str("<code>");
            builder = builder.text_string(&pending.new_owner.to_string());
            builder = builder.raw_str("</code>\n");
            builder = builder.newline();

            // Cancel form
            builder = builder.raw_str("<input type=\"hidden\" name=\"community_id\" value=\"");
            builder = builder.number(community.id as u32);
            builder = builder.raw_str("\" />\n");
            builder = builder.raw_str("<input type=\"hidden\" name=\"caller\" value=\"");
            builder = builder.text_string(&viewer.as_ref().unwrap().to_string());
            builder = builder.raw_str("\" />\n");
            builder = builder.raw_str("<input type=\"hidden\" name=\"_redirect\" value=\"/c/");
            builder = builder.text_string(&community.name);
            builder = builder.raw_str("/settings\" />\n");
            builder = builder.raw_str("<a href=\"form:cancel_transfer\" class=\"soroban-action\">Cancel Transfer</a>\n");
        } else {
            builder = builder.paragraph("Transfer ownership to another user. They will need to accept the transfer.");

            builder = builder.raw_str("<input type=\"hidden\" name=\"community_id\" value=\"");
            builder = builder.number(community.id as u32);
            builder = builder.raw_str("\" />\n");
            builder = builder.raw_str("<label>New Owner Address:</label>\n");
            builder = builder.raw_str("<input type=\"text\" name=\"new_owner\" placeholder=\"G...\" />\n");
            builder = builder.raw_str("<input type=\"hidden\" name=\"caller\" value=\"");
            builder = builder.text_string(&viewer.as_ref().unwrap().to_string());
            builder = builder.raw_str("\" />\n");
            builder = builder.raw_str("<input type=\"hidden\" name=\"_redirect\" value=\"/c/");
            builder = builder.text_string(&community.name);
            builder = builder.raw_str("/settings\" />\n");
            builder = builder.newline();
            builder = builder.raw_str("<a href=\"form:initiate_transfer\" class=\"soroban-action\">Initiate Transfer</a>\n");
        }

        // Admin Management Section
        builder = builder.newline();
        builder = builder.h2("Community Admins");
        builder = builder.paragraph("Admins can manage community settings, boards, and members. Only the owner can remove admins.");

        let admins = Self::get_admins(env.clone(), community.id);
        if admins.is_empty() {
            builder = builder.paragraph("No admins assigned yet.");
        } else {
            builder = builder.raw_str("<h3>Current Admins</h3>\n");
            builder = builder.raw_str("<ul class=\"admin-list\">\n");
            for i in 0..admins.len() {
                if let Some(admin) = admins.get(i) {
                    builder = builder.raw_str("<li><code>");
                    builder = builder.text_string(&admin.to_string());
                    builder = builder.raw_str("</code></li>\n");
                }
            }
            builder = builder.raw_str("</ul>\n");
        }

        // Add admin form (owner and admins can add)
        builder = builder.newline();
        builder = builder.h3("Add Admin");
        builder = builder.raw_str("<input type=\"hidden\" name=\"community_id\" value=\"");
        builder = builder.number(community.id as u32);
        builder = builder.raw_str("\" />\n");
        builder = builder.raw_str("<label>Wallet Address:</label>\n");
        builder = builder.raw_str("<input type=\"text\" name=\"user\" placeholder=\"G...\" />\n");
        builder = builder.raw_str("<input type=\"hidden\" name=\"caller\" value=\"");
        builder = builder.text_string(&viewer.as_ref().unwrap().to_string());
        builder = builder.raw_str("\" />\n");
        builder = builder.raw_str("<input type=\"hidden\" name=\"_redirect\" value=\"/c/");
        builder = builder.text_string(&community.name);
        builder = builder.raw_str("/settings\" />\n");
        builder = builder.newline();
        builder = builder.form_link_to("Add Admin", "community", "add_admin");

        // Remove admin form (owner only)
        if is_owner && !admins.is_empty() {
            builder = builder.newline();
            builder = builder.h3("Remove Admin");
            builder = builder.raw_str("<input type=\"hidden\" name=\"community_id\" value=\"");
            builder = builder.number(community.id as u32);
            builder = builder.raw_str("\" />\n");
            builder = builder.raw_str("<label>Admin Address to Remove:</label>\n");
            builder = builder.raw_str("<input type=\"text\" name=\"user\" placeholder=\"G...\" />\n");
            builder = builder.raw_str("<input type=\"hidden\" name=\"caller\" value=\"");
            builder = builder.text_string(&viewer.as_ref().unwrap().to_string());
            builder = builder.raw_str("\" />\n");
            builder = builder.raw_str("<input type=\"hidden\" name=\"_redirect\" value=\"/c/");
            builder = builder.text_string(&community.name);
            builder = builder.raw_str("/settings\" />\n");
            builder = builder.newline();
            builder = builder.form_link_to("Remove Admin", "community", "remove_admin");
        }

        // Danger Zone
        builder = builder.newline();
        builder = builder.h2("Danger Zone");

        // Fix board count - useful for data inconsistencies
        builder = builder.paragraph("If your board count appears incorrect, you can recalculate it:");
        builder = builder.raw_str("<a href=\"render:/c/");
        builder = builder.text_string(&community.name);
        builder = builder.raw_str("/fix-count\">Fix Board Count</a>\n");
        builder = builder.newline();

        builder = builder.warning("Deleting a community is permanent and cannot be undone.");
        builder = builder.newline();
        builder = builder.raw_str("<a href=\"render:/c/");
        builder = builder.text_string(&community.name);
        builder = builder.raw_str("/delete\" class=\"soroban-action\">Delete Community</a>\n");

        // Back link
        builder = builder.newline();
        builder = builder.hr();
        builder = builder.raw_str("<a href=\"render:/c/");
        builder = builder.text_string(&community.name);
        builder = builder.raw_str("\">Back to Community</a>\n");

        builder.build()
    }

    fn render_delete(env: &Env, name: &String, viewer: Option<Address>) -> Bytes {
        let community = match Self::get_community_by_name(env.clone(), name.clone()) {
            Some(c) => c,
            None => {
                return MarkdownBuilder::new(env)
                    .paragraph("Community not found")
                    .build();
            }
        };

        // Check if viewer is owner
        let is_owner = match &viewer {
            Some(v) => *v == community.owner,
            None => false,
        };

        if !is_owner {
            return MarkdownBuilder::new(env)
                .warning("Only the community owner can delete the community.")
                .build();
        }

        let mut builder = MarkdownBuilder::new(env);

        // Back navigation to settings
        builder = builder.raw_str("<div class=\"back-nav\">");
        builder = builder.raw_str("<a href=\"render:/c/");
        builder = builder.text_string(&community.name);
        builder = builder.raw_str("/settings\" class=\"back-link\">← Back to Settings</a>");
        builder = builder.raw_str("</div>\n");

        builder = builder.newline();
        builder = builder.h1("Delete Community");

        if community.board_count > 0 {
            builder = builder.warning("Cannot delete community with boards.");
            builder = builder.paragraph("This community has ");
            builder = builder.number(community.board_count as u32);
            builder = builder.text(" board(s). Remove all boards from this community before deleting it.");
            builder = builder.newline();
            builder = builder.newline();
            builder = builder.raw_str("<a href=\"render:/c/");
            builder = builder.text_string(&community.name);
            builder = builder.raw_str("/settings\">Back to Settings</a>\n");
        } else {
            builder = builder.warning("This action is permanent and cannot be undone!");
            builder = builder.newline();
            builder = builder.paragraph("You are about to delete the community:");
            builder = builder.raw_str("<strong>");
            builder = builder.text_string(&community.display_name);
            builder = builder.raw_str("</strong>\n");

            builder = builder.newline();
            builder = builder.raw_str("<input type=\"hidden\" name=\"community_id\" value=\"");
            builder = builder.number(community.id as u32);
            builder = builder.raw_str("\" />\n");
            builder = builder.raw_str("<input type=\"hidden\" name=\"caller\" value=\"");
            builder = builder.text_string(&viewer.as_ref().unwrap().to_string());
            builder = builder.raw_str("\" />\n");
            builder = builder.raw_str("<input type=\"hidden\" name=\"_redirect\" value=\"/communities\" />\n");

            builder = builder.newline();
            builder = builder.raw_str("<a href=\"form:delete_community\" class=\"soroban-action\">Delete Community</a>\n");
            builder = builder.text(" | ");
            builder = builder.raw_str("<a href=\"render:/c/");
            builder = builder.text_string(&community.name);
            builder = builder.raw_str("/settings\">Cancel</a>\n");
        }

        builder.build()
    }

    fn render_fix_count(env: &Env, name: &String, viewer: Option<Address>) -> Bytes {
        let community = match Self::get_community_by_name(env.clone(), name.clone()) {
            Some(c) => c,
            None => {
                return MarkdownBuilder::new(env)
                    .paragraph("Community not found")
                    .build();
            }
        };

        // Check if viewer is owner
        let is_owner = match &viewer {
            Some(v) => *v == community.owner,
            None => false,
        };

        if !is_owner {
            return MarkdownBuilder::new(env)
                .warning("Only the community owner can fix the board count.")
                .build();
        }

        // Get actual boards
        let boards: Vec<u64> = env
            .storage()
            .persistent()
            .get(&CommunityKey::CommunityBoards(community.id))
            .unwrap_or_else(|| Vec::new(env));

        let actual_count = boards.len() as u64;

        let mut builder = MarkdownBuilder::new(env);

        // Back navigation
        builder = builder.div_start("back-nav");
        builder = builder.raw_str("<a href=\"render:/c/");
        builder = builder.text_string(&community.name);
        builder = builder.raw_str("/settings\" class=\"back-link\">← Back to Settings</a>");
        builder = builder.div_end();
        builder = builder.newline();

        builder = builder.h1("Fix Board Count");

        builder = builder.paragraph("Current status:");
        builder = builder.raw_str("<ul>\n");
        builder = builder.raw_str("<li>Stored board count: <strong>");
        builder = builder.number(community.board_count as u32);
        builder = builder.raw_str("</strong></li>\n");
        builder = builder.raw_str("<li>Actual boards: <strong>");
        builder = builder.number(actual_count as u32);
        builder = builder.raw_str("</strong></li>\n");
        builder = builder.raw_str("</ul>\n");
        builder = builder.newline();

        if community.board_count == actual_count {
            builder = builder.tip("Board count is already correct. No fix needed.");
            builder = builder.newline();
            builder = builder.raw_str("<a href=\"render:/c/");
            builder = builder.text_string(&community.name);
            builder = builder.raw_str("/settings\">Back to Settings</a>\n");
        } else {
            builder = builder.note("Click below to update the stored count to match reality.");
            builder = builder.newline();
            builder = builder.raw_str("<input type=\"hidden\" name=\"community_id\" value=\"");
            builder = builder.number(community.id as u32);
            builder = builder.raw_str("\" />\n");
            builder = builder.raw_str("<input type=\"hidden\" name=\"caller\" value=\"");
            builder = builder.text_string(&viewer.as_ref().unwrap().to_string());
            builder = builder.raw_str("\" />\n");
            builder = builder.raw_str("<input type=\"hidden\" name=\"_redirect\" value=\"/c/");
            builder = builder.text_string(&community.name);
            builder = builder.raw_str("/settings\" />\n");
            builder = builder.newline();
            builder = builder.form_link_to("Fix Board Count", "community", "fix_board_count");
            builder = builder.text(" | ");
            builder = builder.raw_str("<a href=\"render:/c/");
            builder = builder.text_string(&community.name);
            builder = builder.raw_str("/settings\">Cancel</a>\n");
        }

        builder.build()
    }

    fn render_create_form(env: &Env, viewer: Option<Address>) -> Bytes {
        if viewer.is_none() {
            return MarkdownBuilder::new(env)
                .paragraph("Please connect your wallet to create a community.")
                .build();
        }

        let mut builder = MarkdownBuilder::new(env);
        builder = builder.h1("Create Community");

        // Form inputs - no <form> wrapper since DOMPurify strips form tags for security
        // Instead, use a form: link which collects all inputs on the page

        // Name field - escape hyphen in pattern for unicode regex compatibility
        builder = builder.raw_str("<label for=\"name\">URL Name (lowercase, no spaces):</label>\n");
        builder = builder.raw_str("<input type=\"text\" name=\"name\" placeholder=\"my-community\" pattern=\"[a-z0-9\\-]+\" minlength=\"3\" maxlength=\"30\" />\n");

        // Display name field
        builder = builder.raw_str("<label for=\"display_name\">Display Name:</label>\n");
        builder = builder.raw_str("<input type=\"text\" name=\"display_name\" placeholder=\"My Community\" />\n");

        // Description field
        builder = builder.raw_str("<label for=\"description\">Description:</label>\n");
        builder = builder.textarea_markdown("description", 3, "Describe your community...");

        // Private checkbox - hidden default + checkbox override
        builder = builder.raw_str("<input type=\"hidden\" name=\"is_private\" value=\"false\" />\n");
        builder = builder.raw_str("<label><input type=\"checkbox\" name=\"is_private\" value=\"true\" /> Private (members only)</label>\n");

        // Listed checkbox - hidden default + checkbox override
        builder = builder.raw_str("<input type=\"hidden\" name=\"is_listed\" value=\"true\" />\n");
        builder = builder.raw_str("<label><input type=\"checkbox\" name=\"is_listed\" value=\"false\" /> Hide from public directory (unlisted)</label>\n");

        // Caller address for authentication
        builder = builder.raw_str("<input type=\"hidden\" name=\"caller\" value=\"");
        builder = builder.text_string(&viewer.as_ref().unwrap().to_string());
        builder = builder.raw_str("\" />\n");

        // Hidden redirect - go to communities list after creation
        builder = builder.raw_str("<input type=\"hidden\" name=\"_redirect\" value=\"/communities\" />\n");

        // Submit button using form: protocol link (collects all inputs on page)
        builder = builder.newline();
        builder = builder.raw_str("<a href=\"form:create_community\" class=\"soroban-action\">Create Community</a>\n");

        // Cancel link
        builder = builder.raw_str(" | ");
        builder = builder.render_link("Cancel", "/communities");

        builder.build()
    }

    // === Utility Functions ===

    /// Validate community name format
    fn validate_community_name(env: &Env, name: &String) {
        let len = name.len() as usize;
        if len < 3 || len > 30 {
            panic!("Community name must be 3-30 characters");
        }

        let mut buf = [0u8; 30];
        let copy_len = core::cmp::min(len, 30);
        name.copy_into_slice(&mut buf[..copy_len]);

        // First character must be lowercase letter
        let first = buf[0];
        if !(first >= b'a' && first <= b'z') {
            panic!("Community name must start with lowercase letter");
        }

        // All characters must be lowercase alphanumeric or hyphen
        for i in 0..copy_len {
            let c = buf[i];
            let valid = (c >= b'a' && c <= b'z')
                || (c >= b'0' && c <= b'9')
                || c == b'-';
            if !valid {
                panic!("Community name can only contain lowercase letters, numbers, and hyphens");
            }
        }

        // Cannot end with hyphen
        if buf[copy_len - 1] == b'-' {
            panic!("Community name cannot end with hyphen");
        }

        let _ = env;
    }

    /// Convert string to lowercase for case-insensitive lookup
    fn to_lowercase(env: &Env, s: &String) -> String {
        let len = s.len() as usize;
        if len == 0 {
            return s.clone();
        }

        let mut buf = [0u8; 256];
        let copy_len = core::cmp::min(len, 256);
        s.copy_into_slice(&mut buf[..copy_len]);

        for i in 0..copy_len {
            if buf[i] >= b'A' && buf[i] <= b'Z' {
                buf[i] = buf[i] - b'A' + b'a';
            }
        }

        String::from_str(env, core::str::from_utf8(&buf[..copy_len]).unwrap())
    }

    fn bytes_to_string(env: &Env, bytes: &[u8]) -> String {
        String::from_str(env, core::str::from_utf8(bytes).unwrap_or(""))
    }

    fn find_next_slash(buf: &[u8], start: usize, end: usize) -> usize {
        for i in start..end {
            if buf[i] == b'/' {
                return i;
            }
        }
        end
    }

    /// Upgrade the contract WASM
    pub fn upgrade(env: Env, new_wasm_hash: soroban_sdk::BytesN<32>) {
        // Verify caller is the registry (trusted upgrader)
        let registry: Address = env
            .storage()
            .instance()
            .get(&CommunityKey::Registry)
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
    fn test_init() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsCommunity, ());
        let client = BoardsCommunityClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        let permissions = Address::generate(&env);
        let theme = Address::generate(&env);

        client.init(&registry, &permissions, &theme);
        assert_eq!(client.community_count(), 0);
    }

    #[test]
    fn test_create_community() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsCommunity, ());
        let client = BoardsCommunityClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        let permissions = Address::generate(&env);
        let theme = Address::generate(&env);
        client.init(&registry, &permissions, &theme);

        let owner = Address::generate(&env);
        let name = String::from_str(&env, "stellar-dev");
        let display_name = String::from_str(&env, "Stellar Development");
        let description = String::from_str(&env, "Community for Stellar developers");
        let is_private = String::from_str(&env, "false");
        let is_listed = String::from_str(&env, "true");

        let community_id = client.create_community(
            &name,
            &display_name,
            &description,
            &is_private,
            &is_listed,
            &owner,
        );
        assert_eq!(community_id, 0);

        let community = client.get_community(&community_id).unwrap();
        assert_eq!(community.name, name);
        assert_eq!(community.display_name, display_name);
        assert_eq!(community.owner, owner);
        assert_eq!(community.board_count, 0);
        assert_eq!(community.member_count, 1);
        assert!(!community.is_private);
    }

    #[test]
    fn test_get_community_by_name() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsCommunity, ());
        let client = BoardsCommunityClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        let permissions = Address::generate(&env);
        let theme = Address::generate(&env);
        client.init(&registry, &permissions, &theme);

        let owner = Address::generate(&env);
        let name = String::from_str(&env, "test-community");
        let display_name = String::from_str(&env, "Test Community");
        let description = String::from_str(&env, "A test");
        let is_private = String::from_str(&env, "false");
        let is_listed = String::from_str(&env, "true");

        client.create_community(&name, &display_name, &description, &is_private, &is_listed, &owner);

        // Lookup by exact name
        let community = client.get_community_by_name(&name).unwrap();
        assert_eq!(community.display_name, display_name);

        // Lookup by uppercase should work (case-insensitive)
        let upper_name = String::from_str(&env, "TEST-COMMUNITY");
        let community2 = client.get_community_by_name(&upper_name).unwrap();
        assert_eq!(community2.id, community.id);
    }

    #[test]
    fn test_list_communities() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsCommunity, ());
        let client = BoardsCommunityClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        let permissions = Address::generate(&env);
        let theme = Address::generate(&env);
        client.init(&registry, &permissions, &theme);

        let owner = Address::generate(&env);
        let is_private = String::from_str(&env, "false");
        let is_listed = String::from_str(&env, "true");

        // Create multiple communities
        for i in 0..5 {
            let name = String::from_str(&env, match i {
                0 => "community-a",
                1 => "community-b",
                2 => "community-c",
                3 => "community-d",
                _ => "community-e",
            });
            let display = String::from_str(&env, "Community");
            let desc = String::from_str(&env, "Description");
            client.create_community(&name, &display, &desc, &is_private, &is_listed, &owner);
        }

        // List with pagination
        let communities = client.list_communities(&0, &3);
        assert_eq!(communities.len(), 3);

        let communities = client.list_communities(&3, &10);
        assert_eq!(communities.len(), 2);

        assert_eq!(client.community_count(), 5);
    }

    #[test]
    #[should_panic(expected = "Community name already exists")]
    fn test_duplicate_name() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsCommunity, ());
        let client = BoardsCommunityClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        let permissions = Address::generate(&env);
        let theme = Address::generate(&env);
        client.init(&registry, &permissions, &theme);

        let owner = Address::generate(&env);
        let name = String::from_str(&env, "same-name");
        let display = String::from_str(&env, "Display");
        let desc = String::from_str(&env, "Description");
        let is_private = String::from_str(&env, "false");
        let is_listed = String::from_str(&env, "true");

        client.create_community(&name, &display, &desc, &is_private, &is_listed, &owner);
        // Second create with same name should panic
        client.create_community(&name, &display, &desc, &is_private, &is_listed, &owner);
    }

    #[test]
    fn test_private_community() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsCommunity, ());
        let client = BoardsCommunityClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        let permissions = Address::generate(&env);
        let theme = Address::generate(&env);
        client.init(&registry, &permissions, &theme);

        let owner = Address::generate(&env);
        let name = String::from_str(&env, "private-club");
        let display = String::from_str(&env, "Private Club");
        let desc = String::from_str(&env, "Members only");
        let is_private = String::from_str(&env, "true");
        let is_listed = String::from_str(&env, "false");

        let id = client.create_community(&name, &display, &desc, &is_private, &is_listed, &owner);

        let community = client.get_community(&id).unwrap();
        assert!(community.is_private);

        // Owner should be a member
        assert!(client.is_member(&id, &owner));

        // Random user should not be a member
        let random_user = Address::generate(&env);
        assert!(!client.is_member(&id, &random_user));
    }
}
