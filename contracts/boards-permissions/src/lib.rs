#![no_std]

use soroban_sdk::{contract, contractimpl, contracttype, Address, BytesN, Env, String, Vec};

/// Role levels (hierarchical - higher includes lower permissions)
#[contracttype]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum Role {
    Guest = 0,
    Member = 1,
    Moderator = 2,
    Admin = 3,
    Owner = 4,
}

impl Role {
    pub fn from_u32(val: u32) -> Self {
        match val {
            0 => Role::Guest,
            1 => Role::Member,
            2 => Role::Moderator,
            3 => Role::Admin,
            4 => Role::Owner,
            _ => Role::Guest,
        }
    }
}

/// Ban information
#[contracttype]
#[derive(Clone)]
pub struct Ban {
    pub user: Address,
    pub board_id: u64,
    pub issuer: Address,
    pub reason: String,
    pub created_at: u64,
    pub expires_at: Option<u64>,
}

/// Invite request (user requesting to join a private board)
#[contracttype]
#[derive(Clone)]
pub struct InviteRequest {
    pub user: Address,
    pub board_id: u64,
    pub created_at: u64,
}

/// Community ban information
#[contracttype]
#[derive(Clone)]
pub struct CommunityBan {
    pub user: Address,
    pub community_id: u64,
    pub issuer: Address,
    pub reason: String,
    pub created_at: u64,
    pub expires_at: Option<u64>,
}

/// User flair (badge) for a specific board
/// Displayed next to user's name in posts/replies
#[contracttype]
#[derive(Clone)]
pub struct UserFlair {
    pub text: String,       // max 24 chars
    pub color: String,      // CSS text color
    pub bg_color: String,   // CSS background color
    pub granted_by: Address,
    pub granted_at: u64,
}

/// Storage keys for the permissions contract
#[contracttype]
#[derive(Clone)]
pub enum PermKey {
    /// Registry contract address
    Registry,
    /// Board owner address
    BoardOwner(u64),
    /// User role for a board (board_id, user) -> Role
    BoardRole(u64, Address),
    /// Ban record (board_id, user) -> Ban
    BoardBan(u64, Address),
    /// Global ban (user) -> Ban
    GlobalBan(Address),
    /// Flag threshold for auto-hide (board_id) -> u32
    FlagThreshold(u64),
    /// List of admins for a board
    BoardAdmins(u64),
    /// List of moderators for a board
    BoardModerators(u64),
    /// List of members for a board
    BoardMembers(u64),
    /// List of banned users for a board
    BannedUsers(u64),
    /// Individual invite request (board_id, user) -> InviteRequest
    InviteRequest(u64, Address),
    /// List of users with pending invite requests for a board
    InviteRequests(u64),
    /// Board role override - can only RESTRICT, not expand community role
    /// (board_id, user) -> Role
    BoardRoleOverride(u64, Address),

    // Community-level permission keys
    /// Community owner address (community_id) -> Address
    CommunityOwner(u64),
    /// User role for a community (community_id, user) -> Role
    CommunityRole(u64, Address),
    /// Community ban record (community_id, user) -> CommunityBan
    CommunityBan(u64, Address),
    /// List of admins for a community
    CommunityAdmins(u64),
    /// List of moderators for a community
    CommunityModerators(u64),
    /// List of members for a community
    CommunityMembers(u64),
    /// List of banned users for a community
    CommunityBannedUsers(u64),
    /// User flair for a board (board_id, user) -> UserFlair
    UserFlair(u64, Address),
}

/// Permission check result with all relevant permissions
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
pub struct BoardsPermissions;

#[contractimpl]
impl BoardsPermissions {
    /// Initialize the permissions contract
    pub fn init(env: Env, registry: Address) {
        if env.storage().instance().has(&PermKey::Registry) {
            panic!("Already initialized");
        }
        env.storage().instance().set(&PermKey::Registry, &registry);
    }

    /// Get registry address
    pub fn get_registry(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&PermKey::Registry)
            .expect("Not initialized")
    }

    /// Set board owner (called by registry when board is created)
    pub fn set_board_owner(env: Env, board_id: u64, owner: Address) {
        // TODO: Verify caller is registry
        env.storage()
            .persistent()
            .set(&PermKey::BoardOwner(board_id), &owner);
        env.storage()
            .persistent()
            .set(&PermKey::BoardRole(board_id, owner.clone()), &Role::Owner);
    }

    /// Get board owner
    pub fn get_board_owner(env: Env, board_id: u64) -> Option<Address> {
        env.storage()
            .persistent()
            .get(&PermKey::BoardOwner(board_id))
    }

    /// Set a user's role for a board
    pub fn set_role(env: Env, board_id: u64, user: Address, role: Role, caller: Address) {
        caller.require_auth();

        // Verify caller has authority to set this role
        let caller_role = Self::get_role(env.clone(), board_id, caller.clone());

        // Only owner can set admin, only admin+ can set moderator, etc.
        match role {
            Role::Owner => {
                if caller_role != Role::Owner {
                    panic!("Only owner can transfer ownership");
                }
            }
            Role::Admin => {
                if caller_role != Role::Owner {
                    panic!("Only owner can set admin");
                }
            }
            Role::Moderator => {
                if caller_role != Role::Owner && caller_role != Role::Admin {
                    panic!("Only owner or admin can set moderator");
                }
            }
            Role::Member => {
                if caller_role != Role::Owner
                    && caller_role != Role::Admin
                    && caller_role != Role::Moderator
                {
                    panic!("Only moderator+ can set member");
                }
            }
            Role::Guest => {
                // Anyone with moderator+ can demote to guest
                if caller_role != Role::Owner
                    && caller_role != Role::Admin
                    && caller_role != Role::Moderator
                {
                    panic!("Only moderator+ can remove member");
                }
            }
        }

        // Get old role to update membership lists
        let old_role = Self::get_role(env.clone(), board_id, user.clone());

        // Remove from old role list
        Self::remove_from_role_list(&env, board_id, &user, old_role);

        // Add to new role list
        Self::add_to_role_list(&env, board_id, &user, role.clone());

        env.storage()
            .persistent()
            .set(&PermKey::BoardRole(board_id, user), &role);
    }

    /// Add user to appropriate role list
    fn add_to_role_list(env: &Env, board_id: u64, user: &Address, role: Role) {
        let key = match role {
            Role::Admin => PermKey::BoardAdmins(board_id),
            Role::Moderator => PermKey::BoardModerators(board_id),
            Role::Member => PermKey::BoardMembers(board_id),
            _ => return, // Owner and Guest don't have lists
        };

        let mut list: Vec<Address> = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or(Vec::new(env));

        // Check if already in list
        let mut found = false;
        for i in 0..list.len() {
            if list.get(i).unwrap() == *user {
                found = true;
                break;
            }
        }

        if !found {
            list.push_back(user.clone());
            env.storage().persistent().set(&key, &list);
        }
    }

    /// Remove user from role list
    fn remove_from_role_list(env: &Env, board_id: u64, user: &Address, role: Role) {
        let key = match role {
            Role::Admin => PermKey::BoardAdmins(board_id),
            Role::Moderator => PermKey::BoardModerators(board_id),
            Role::Member => PermKey::BoardMembers(board_id),
            _ => return, // Owner and Guest don't have lists
        };

        if let Some(list) = env.storage().persistent().get::<_, Vec<Address>>(&key) {
            let mut new_list = Vec::new(env);
            for i in 0..list.len() {
                let addr = list.get(i).unwrap();
                if addr != *user {
                    new_list.push_back(addr);
                }
            }
            env.storage().persistent().set(&key, &new_list);
        }
    }

    /// Get a user's role for a board
    pub fn get_role(env: Env, board_id: u64, user: Address) -> Role {
        env.storage()
            .persistent()
            .get(&PermKey::BoardRole(board_id, user))
            .unwrap_or(Role::Guest)
    }

    /// Check if user has at least the specified role
    pub fn has_role(env: Env, board_id: u64, user: Address, min_role: Role) -> bool {
        let role = Self::get_role(env, board_id, user);
        role as u32 >= min_role as u32
    }

    /// Get full permission set for a user on a board
    pub fn get_permissions(env: Env, board_id: u64, user: Address) -> PermissionSet {
        let role = Self::get_role(env.clone(), board_id, user.clone());
        let is_banned = Self::is_banned(env.clone(), board_id, user);

        PermissionSet {
            role: role.clone(),
            can_view: !is_banned,
            can_post: !is_banned && role as u32 >= Role::Member as u32,
            can_moderate: !is_banned && role as u32 >= Role::Moderator as u32,
            can_admin: !is_banned && role as u32 >= Role::Admin as u32,
            is_banned,
        }
    }

    /// Ban a user from a board
    pub fn ban_user(
        env: Env,
        board_id: u64,
        user: Address,
        reason: String,
        duration_hours: Option<u64>,
        caller: Address,
    ) {
        caller.require_auth();

        // Check caller has authority
        let caller_role = Self::get_role(env.clone(), board_id, caller.clone());
        if caller_role != Role::Owner
            && caller_role != Role::Admin
            && caller_role != Role::Moderator
        {
            panic!("Only moderator+ can ban users");
        }

        // Can't ban higher roles
        let user_role = Self::get_role(env.clone(), board_id, user.clone());
        if user_role as u32 >= caller_role as u32 {
            panic!("Cannot ban user with equal or higher role");
        }

        let expires_at = duration_hours.map(|h| env.ledger().timestamp() + h * 3600);

        let ban = Ban {
            user: user.clone(),
            board_id,
            issuer: caller,
            reason,
            created_at: env.ledger().timestamp(),
            expires_at,
        };

        env.storage()
            .persistent()
            .set(&PermKey::BoardBan(board_id, user.clone()), &ban);

        // Add to banned users list
        let mut banned: Vec<Address> = env
            .storage()
            .persistent()
            .get(&PermKey::BannedUsers(board_id))
            .unwrap_or(Vec::new(&env));

        // Check if already in list
        let mut found = false;
        for i in 0..banned.len() {
            if banned.get(i).unwrap() == user {
                found = true;
                break;
            }
        }
        if !found {
            banned.push_back(user);
            env.storage()
                .persistent()
                .set(&PermKey::BannedUsers(board_id), &banned);
        }
    }

    /// Unban a user from a board
    pub fn unban_user(env: Env, board_id: u64, user: Address, caller: Address) {
        caller.require_auth();

        // Check caller has authority
        let caller_role = Self::get_role(env.clone(), board_id, caller);
        if caller_role != Role::Owner
            && caller_role != Role::Admin
            && caller_role != Role::Moderator
        {
            panic!("Only moderator+ can unban users");
        }

        env.storage()
            .persistent()
            .remove(&PermKey::BoardBan(board_id, user.clone()));

        // Remove from banned users list
        if let Some(banned) = env
            .storage()
            .persistent()
            .get::<_, Vec<Address>>(&PermKey::BannedUsers(board_id))
        {
            let mut new_banned = Vec::new(&env);
            for i in 0..banned.len() {
                let addr = banned.get(i).unwrap();
                if addr != user {
                    new_banned.push_back(addr);
                }
            }
            env.storage()
                .persistent()
                .set(&PermKey::BannedUsers(board_id), &new_banned);
        }
    }

    /// Check if a user is banned from a board
    pub fn is_banned(env: Env, board_id: u64, user: Address) -> bool {
        // Check global ban first
        if let Some(ban) = env
            .storage()
            .persistent()
            .get::<_, Ban>(&PermKey::GlobalBan(user.clone()))
        {
            if let Some(expires_at) = ban.expires_at {
                if env.ledger().timestamp() < expires_at {
                    return true;
                }
            } else {
                return true; // Permanent ban
            }
        }

        // Check board-specific ban
        if let Some(ban) = env
            .storage()
            .persistent()
            .get::<_, Ban>(&PermKey::BoardBan(board_id, user))
        {
            if let Some(expires_at) = ban.expires_at {
                if env.ledger().timestamp() < expires_at {
                    return true;
                }
            } else {
                return true; // Permanent ban
            }
        }

        false
    }

    /// Get ban info for a user
    pub fn get_ban(env: Env, board_id: u64, user: Address) -> Option<Ban> {
        env.storage()
            .persistent()
            .get(&PermKey::BoardBan(board_id, user))
    }

    /// Set flag threshold for a board
    pub fn set_flag_threshold(env: Env, board_id: u64, threshold: u32, caller: Address) {
        caller.require_auth();

        let caller_role = Self::get_role(env.clone(), board_id, caller);
        if caller_role != Role::Owner && caller_role != Role::Admin {
            panic!("Only admin+ can set flag threshold");
        }

        env.storage()
            .persistent()
            .set(&PermKey::FlagThreshold(board_id), &threshold);
    }

    /// Get flag threshold for a board (default: 3)
    pub fn get_flag_threshold(env: Env, board_id: u64) -> u32 {
        env.storage()
            .persistent()
            .get(&PermKey::FlagThreshold(board_id))
            .unwrap_or(3)
    }

    // Invite system functions

    /// Request an invite to join a board (user-initiated)
    /// Only allowed for non-members who are not banned
    pub fn request_invite(env: Env, board_id: u64, caller: Address) {
        caller.require_auth();

        // Check user is not already a member
        let role = Self::get_role(env.clone(), board_id, caller.clone());
        if role as u32 >= Role::Member as u32 {
            panic!("Already a member of this board");
        }

        // Check user is not banned
        if Self::is_banned(env.clone(), board_id, caller.clone()) {
            panic!("Banned users cannot request invites");
        }

        // Check for existing request
        if env
            .storage()
            .persistent()
            .has(&PermKey::InviteRequest(board_id, caller.clone()))
        {
            panic!("Invite request already pending");
        }

        // Create the invite request
        let request = InviteRequest {
            user: caller.clone(),
            board_id,
            created_at: env.ledger().timestamp(),
        };

        // Store individual request
        env.storage()
            .persistent()
            .set(&PermKey::InviteRequest(board_id, caller.clone()), &request);

        // Add to requests list
        let mut requests: Vec<Address> = env
            .storage()
            .persistent()
            .get(&PermKey::InviteRequests(board_id))
            .unwrap_or(Vec::new(&env));

        requests.push_back(caller);
        env.storage()
            .persistent()
            .set(&PermKey::InviteRequests(board_id), &requests);
    }

    /// Accept an invite request (promotes user to Member)
    /// Only Moderator+ can accept invites
    pub fn accept_invite(env: Env, board_id: u64, user: Address, caller: Address) {
        caller.require_auth();

        // Check caller has authority (Moderator+)
        let caller_role = Self::get_role(env.clone(), board_id, caller.clone());
        if (caller_role as u32) < (Role::Moderator as u32) {
            panic!("Only moderator+ can accept invite requests");
        }

        // Check invite request exists
        if !env
            .storage()
            .persistent()
            .has(&PermKey::InviteRequest(board_id, user.clone()))
        {
            panic!("No invite request found for this user");
        }

        // Remove the invite request
        Self::remove_invite_request(&env, board_id, &user);

        // Set user role to Member
        Self::add_to_role_list(&env, board_id, &user, Role::Member);
        env.storage()
            .persistent()
            .set(&PermKey::BoardRole(board_id, user), &Role::Member);
    }

    /// Revoke/reject an invite request
    /// Only Moderator+ can revoke invites
    pub fn revoke_invite(env: Env, board_id: u64, user: Address, caller: Address) {
        caller.require_auth();

        // Check caller has authority (Moderator+)
        let caller_role = Self::get_role(env.clone(), board_id, caller);
        if (caller_role as u32) < (Role::Moderator as u32) {
            panic!("Only moderator+ can revoke invite requests");
        }

        // Check invite request exists
        if !env
            .storage()
            .persistent()
            .has(&PermKey::InviteRequest(board_id, user.clone()))
        {
            panic!("No invite request found for this user");
        }

        // Remove the invite request
        Self::remove_invite_request(&env, board_id, &user);
    }

    /// Directly invite a user with a specific role (admin-initiated)
    /// Authorization rules:
    /// - Moderator+ can invite as Member or Guest
    /// - Admin+ can invite as Moderator
    /// - Owner can invite as Admin or Owner
    pub fn invite_member(
        env: Env,
        board_id: u64,
        user: Address,
        role: Role,
        caller: Address,
    ) {
        caller.require_auth();

        let caller_role = Self::get_role(env.clone(), board_id, caller.clone());

        // Check authorization based on role being assigned
        match role {
            Role::Owner | Role::Admin => {
                if caller_role != Role::Owner {
                    panic!("Only owner can invite admins or owners");
                }
            }
            Role::Moderator => {
                if (caller_role as u32) < (Role::Admin as u32) {
                    panic!("Only admin+ can invite moderators");
                }
            }
            Role::Member | Role::Guest => {
                if (caller_role as u32) < (Role::Moderator as u32) {
                    panic!("Only moderator+ can invite members");
                }
            }
        }

        // Check user is not already a member with higher role
        let current_role = Self::get_role(env.clone(), board_id, user.clone());
        if current_role as u32 >= role as u32 && current_role != Role::Guest {
            panic!("User already has equal or higher role");
        }

        // Check user is not banned
        if Self::is_banned(env.clone(), board_id, user.clone()) {
            panic!("Cannot invite banned users");
        }

        // Remove any pending invite request for this user
        if env
            .storage()
            .persistent()
            .has(&PermKey::InviteRequest(board_id, user.clone()))
        {
            Self::remove_invite_request(&env, board_id, &user);
        }

        // Remove from old role list if applicable
        Self::remove_from_role_list(&env, board_id, &user, current_role);

        // Add to new role list and set role
        Self::add_to_role_list(&env, board_id, &user, role.clone());
        env.storage()
            .persistent()
            .set(&PermKey::BoardRole(board_id, user), &role);
    }

    /// Helper to remove an invite request
    fn remove_invite_request(env: &Env, board_id: u64, user: &Address) {
        // Remove individual request
        env.storage()
            .persistent()
            .remove(&PermKey::InviteRequest(board_id, user.clone()));

        // Remove from requests list
        if let Some(requests) = env
            .storage()
            .persistent()
            .get::<_, Vec<Address>>(&PermKey::InviteRequests(board_id))
        {
            let mut new_requests = Vec::new(env);
            for i in 0..requests.len() {
                let addr = requests.get(i).unwrap();
                if addr != *user {
                    new_requests.push_back(addr);
                }
            }
            env.storage()
                .persistent()
                .set(&PermKey::InviteRequests(board_id), &new_requests);
        }
    }

    /// List all pending invite requests for a board
    pub fn list_invite_requests(env: Env, board_id: u64) -> Vec<InviteRequest> {
        let request_addrs: Vec<Address> = env
            .storage()
            .persistent()
            .get(&PermKey::InviteRequests(board_id))
            .unwrap_or(Vec::new(&env));

        let mut requests = Vec::new(&env);
        for i in 0..request_addrs.len() {
            let addr = request_addrs.get(i).unwrap();
            if let Some(request) = env
                .storage()
                .persistent()
                .get::<_, InviteRequest>(&PermKey::InviteRequest(board_id, addr))
            {
                requests.push_back(request);
            }
        }
        requests
    }

    /// Check if a user has a pending invite request
    pub fn has_invite_request(env: Env, board_id: u64, user: Address) -> bool {
        env.storage()
            .persistent()
            .has(&PermKey::InviteRequest(board_id, user))
    }

    /// Get a specific invite request
    pub fn get_invite_request(env: Env, board_id: u64, user: Address) -> Option<InviteRequest> {
        env.storage()
            .persistent()
            .get(&PermKey::InviteRequest(board_id, user))
    }

    // Membership list functions

    /// Get list of admins for a board
    pub fn list_admins(env: Env, board_id: u64) -> Vec<Address> {
        env.storage()
            .persistent()
            .get(&PermKey::BoardAdmins(board_id))
            .unwrap_or(Vec::new(&env))
    }

    /// Get list of moderators for a board
    pub fn list_moderators(env: Env, board_id: u64) -> Vec<Address> {
        env.storage()
            .persistent()
            .get(&PermKey::BoardModerators(board_id))
            .unwrap_or(Vec::new(&env))
    }

    /// Get list of members for a board
    pub fn list_members(env: Env, board_id: u64) -> Vec<Address> {
        env.storage()
            .persistent()
            .get(&PermKey::BoardMembers(board_id))
            .unwrap_or(Vec::new(&env))
    }

    /// Get count of members with a specific role
    pub fn role_count(env: Env, board_id: u64, role: Role) -> u32 {
        match role {
            Role::Admin => Self::list_admins(env, board_id).len(),
            Role::Moderator => Self::list_moderators(env, board_id).len(),
            Role::Member => Self::list_members(env, board_id).len(),
            _ => 0,
        }
    }

    /// Get list of banned users for a board (with active bans only)
    pub fn list_banned(env: Env, board_id: u64) -> Vec<Address> {
        let banned: Vec<Address> = env
            .storage()
            .persistent()
            .get(&PermKey::BannedUsers(board_id))
            .unwrap_or(Vec::new(&env));

        // Filter out expired bans
        let mut active_banned = Vec::new(&env);
        let now = env.ledger().timestamp();

        for i in 0..banned.len() {
            let user = banned.get(i).unwrap();
            if let Some(ban) = env
                .storage()
                .persistent()
                .get::<_, Ban>(&PermKey::BoardBan(board_id, user.clone()))
            {
                let is_active = match ban.expires_at {
                    Some(expires) => now < expires,
                    None => true, // Permanent ban
                };
                if is_active {
                    active_banned.push_back(user);
                }
            }
        }

        active_banned
    }

    /// Get list of all bans for a board (with ban details)
    pub fn list_bans(env: Env, board_id: u64) -> Vec<Ban> {
        let banned: Vec<Address> = env
            .storage()
            .persistent()
            .get(&PermKey::BannedUsers(board_id))
            .unwrap_or(Vec::new(&env));

        let mut bans = Vec::new(&env);

        for i in 0..banned.len() {
            let user = banned.get(i).unwrap();
            if let Some(ban) = env
                .storage()
                .persistent()
                .get::<_, Ban>(&PermKey::BoardBan(board_id, user))
            {
                bans.push_back(ban);
            }
        }

        bans
    }

    // Permission check helpers

    /// Check if user can create a thread
    pub fn can_create_thread(env: Env, board_id: u64, user: Address) -> bool {
        let perms = Self::get_permissions(env, board_id, user);
        perms.can_post
    }

    /// Check if user can reply
    pub fn can_reply(env: Env, board_id: u64, user: Address) -> bool {
        let perms = Self::get_permissions(env, board_id, user);
        perms.can_post
    }

    /// Check if user can moderate (delete, hide, lock)
    pub fn can_moderate(env: Env, board_id: u64, user: Address) -> bool {
        let perms = Self::get_permissions(env, board_id, user);
        perms.can_moderate
    }

    /// Check if user can admin (settings, roles)
    pub fn can_admin(env: Env, board_id: u64, user: Address) -> bool {
        let perms = Self::get_permissions(env, board_id, user);
        perms.can_admin
    }

    // ==================== Community Permission Functions ====================

    /// Set community owner (called when community is created)
    pub fn set_community_owner(env: Env, community_id: u64, owner: Address) {
        // TODO: Verify caller is registry or community contract
        env.storage()
            .persistent()
            .set(&PermKey::CommunityOwner(community_id), &owner);
        env.storage()
            .persistent()
            .set(&PermKey::CommunityRole(community_id, owner.clone()), &Role::Owner);
    }

    /// Get community owner
    pub fn get_community_owner(env: Env, community_id: u64) -> Option<Address> {
        env.storage()
            .persistent()
            .get(&PermKey::CommunityOwner(community_id))
    }

    /// Set a user's role for a community
    pub fn set_community_role(
        env: Env,
        community_id: u64,
        user: Address,
        role: Role,
        caller: Address,
    ) {
        caller.require_auth();

        // Verify caller has authority to set this role
        let caller_role = Self::get_community_role(env.clone(), community_id, caller.clone());

        // Only owner can set admin, only admin+ can set moderator, etc.
        match role {
            Role::Owner => {
                if caller_role != Role::Owner {
                    panic!("Only owner can transfer community ownership");
                }
            }
            Role::Admin => {
                if caller_role != Role::Owner {
                    panic!("Only owner can set community admin");
                }
            }
            Role::Moderator => {
                if caller_role != Role::Owner && caller_role != Role::Admin {
                    panic!("Only owner or admin can set community moderator");
                }
            }
            Role::Member => {
                if caller_role != Role::Owner
                    && caller_role != Role::Admin
                    && caller_role != Role::Moderator
                {
                    panic!("Only moderator+ can set community member");
                }
            }
            Role::Guest => {
                // Anyone with moderator+ can demote to guest
                if caller_role != Role::Owner
                    && caller_role != Role::Admin
                    && caller_role != Role::Moderator
                {
                    panic!("Only moderator+ can remove community member");
                }
            }
        }

        // Get old role to update membership lists
        let old_role = Self::get_community_role(env.clone(), community_id, user.clone());

        // Remove from old role list
        Self::remove_from_community_role_list(&env, community_id, &user, old_role);

        // Add to new role list
        Self::add_to_community_role_list(&env, community_id, &user, role.clone());

        env.storage()
            .persistent()
            .set(&PermKey::CommunityRole(community_id, user), &role);
    }

    /// Add user to appropriate community role list
    fn add_to_community_role_list(env: &Env, community_id: u64, user: &Address, role: Role) {
        let key = match role {
            Role::Admin => PermKey::CommunityAdmins(community_id),
            Role::Moderator => PermKey::CommunityModerators(community_id),
            Role::Member => PermKey::CommunityMembers(community_id),
            _ => return, // Owner and Guest don't have lists
        };

        let mut list: Vec<Address> = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or(Vec::new(env));

        // Check if already in list
        let mut found = false;
        for i in 0..list.len() {
            if list.get(i).unwrap() == *user {
                found = true;
                break;
            }
        }

        if !found {
            list.push_back(user.clone());
            env.storage().persistent().set(&key, &list);
        }
    }

    /// Remove user from community role list
    fn remove_from_community_role_list(env: &Env, community_id: u64, user: &Address, role: Role) {
        let key = match role {
            Role::Admin => PermKey::CommunityAdmins(community_id),
            Role::Moderator => PermKey::CommunityModerators(community_id),
            Role::Member => PermKey::CommunityMembers(community_id),
            _ => return, // Owner and Guest don't have lists
        };

        if let Some(list) = env.storage().persistent().get::<_, Vec<Address>>(&key) {
            let mut new_list = Vec::new(env);
            for i in 0..list.len() {
                let addr = list.get(i).unwrap();
                if addr != *user {
                    new_list.push_back(addr);
                }
            }
            env.storage().persistent().set(&key, &new_list);
        }
    }

    /// Get a user's role for a community
    pub fn get_community_role(env: Env, community_id: u64, user: Address) -> Role {
        env.storage()
            .persistent()
            .get(&PermKey::CommunityRole(community_id, user))
            .unwrap_or(Role::Guest)
    }

    /// Check if user has at least the specified role in a community
    pub fn has_community_role(env: Env, community_id: u64, user: Address, min_role: Role) -> bool {
        let role = Self::get_community_role(env, community_id, user);
        role as u32 >= min_role as u32
    }

    /// Ban a user from a community (affects all boards in the community)
    pub fn community_ban_user(
        env: Env,
        community_id: u64,
        user: Address,
        reason: String,
        duration_hours: Option<u64>,
        caller: Address,
    ) {
        caller.require_auth();

        // Check caller has authority
        let caller_role = Self::get_community_role(env.clone(), community_id, caller.clone());
        if caller_role != Role::Owner
            && caller_role != Role::Admin
            && caller_role != Role::Moderator
        {
            panic!("Only community moderator+ can ban users");
        }

        // Can't ban higher roles
        let user_role = Self::get_community_role(env.clone(), community_id, user.clone());
        if user_role as u32 >= caller_role as u32 {
            panic!("Cannot ban user with equal or higher community role");
        }

        let expires_at = duration_hours.map(|h| env.ledger().timestamp() + h * 3600);

        let ban = CommunityBan {
            user: user.clone(),
            community_id,
            issuer: caller,
            reason,
            created_at: env.ledger().timestamp(),
            expires_at,
        };

        env.storage()
            .persistent()
            .set(&PermKey::CommunityBan(community_id, user.clone()), &ban);

        // Add to banned users list
        let mut banned: Vec<Address> = env
            .storage()
            .persistent()
            .get(&PermKey::CommunityBannedUsers(community_id))
            .unwrap_or(Vec::new(&env));

        // Check if already in list
        let mut found = false;
        for i in 0..banned.len() {
            if banned.get(i).unwrap() == user {
                found = true;
                break;
            }
        }
        if !found {
            banned.push_back(user);
            env.storage()
                .persistent()
                .set(&PermKey::CommunityBannedUsers(community_id), &banned);
        }
    }

    /// Unban a user from a community
    pub fn community_unban_user(env: Env, community_id: u64, user: Address, caller: Address) {
        caller.require_auth();

        // Check caller has authority
        let caller_role = Self::get_community_role(env.clone(), community_id, caller);
        if caller_role != Role::Owner
            && caller_role != Role::Admin
            && caller_role != Role::Moderator
        {
            panic!("Only community moderator+ can unban users");
        }

        env.storage()
            .persistent()
            .remove(&PermKey::CommunityBan(community_id, user.clone()));

        // Remove from banned users list
        if let Some(banned) = env
            .storage()
            .persistent()
            .get::<_, Vec<Address>>(&PermKey::CommunityBannedUsers(community_id))
        {
            let mut new_banned = Vec::new(&env);
            for i in 0..banned.len() {
                let addr = banned.get(i).unwrap();
                if addr != user {
                    new_banned.push_back(addr);
                }
            }
            env.storage()
                .persistent()
                .set(&PermKey::CommunityBannedUsers(community_id), &new_banned);
        }
    }

    /// Check if a user is banned from a community
    pub fn is_community_banned(env: Env, community_id: u64, user: Address) -> bool {
        // Check global ban first
        if let Some(ban) = env
            .storage()
            .persistent()
            .get::<_, Ban>(&PermKey::GlobalBan(user.clone()))
        {
            if let Some(expires_at) = ban.expires_at {
                if env.ledger().timestamp() < expires_at {
                    return true;
                }
            } else {
                return true; // Permanent ban
            }
        }

        // Check community-specific ban
        if let Some(ban) = env
            .storage()
            .persistent()
            .get::<_, CommunityBan>(&PermKey::CommunityBan(community_id, user))
        {
            if let Some(expires_at) = ban.expires_at {
                if env.ledger().timestamp() < expires_at {
                    return true;
                }
            } else {
                return true; // Permanent ban
            }
        }

        false
    }

    /// Get community ban info for a user
    pub fn get_community_ban(env: Env, community_id: u64, user: Address) -> Option<CommunityBan> {
        env.storage()
            .persistent()
            .get(&PermKey::CommunityBan(community_id, user))
    }

    /// Get list of admins for a community
    pub fn list_community_admins(env: Env, community_id: u64) -> Vec<Address> {
        env.storage()
            .persistent()
            .get(&PermKey::CommunityAdmins(community_id))
            .unwrap_or(Vec::new(&env))
    }

    /// Get list of moderators for a community
    pub fn list_community_moderators(env: Env, community_id: u64) -> Vec<Address> {
        env.storage()
            .persistent()
            .get(&PermKey::CommunityModerators(community_id))
            .unwrap_or(Vec::new(&env))
    }

    /// Get list of members for a community
    pub fn list_community_members(env: Env, community_id: u64) -> Vec<Address> {
        env.storage()
            .persistent()
            .get(&PermKey::CommunityMembers(community_id))
            .unwrap_or(Vec::new(&env))
    }

    /// Get list of banned users for a community
    pub fn list_community_banned(env: Env, community_id: u64) -> Vec<Address> {
        let banned: Vec<Address> = env
            .storage()
            .persistent()
            .get(&PermKey::CommunityBannedUsers(community_id))
            .unwrap_or(Vec::new(&env));

        // Filter out expired bans
        let mut active_banned = Vec::new(&env);
        let now = env.ledger().timestamp();

        for i in 0..banned.len() {
            let user = banned.get(i).unwrap();
            if let Some(ban) = env
                .storage()
                .persistent()
                .get::<_, CommunityBan>(&PermKey::CommunityBan(community_id, user.clone()))
            {
                let is_active = match ban.expires_at {
                    Some(expires) => now < expires,
                    None => true, // Permanent ban
                };
                if is_active {
                    active_banned.push_back(user);
                }
            }
        }

        active_banned
    }

    // ==================== Board Role Override Functions ====================

    /// Set a board-specific role override for a user
    /// This can only RESTRICT permissions, not expand beyond community role
    /// Used when a community mod should have reduced permissions on a specific board
    pub fn set_board_role_override(
        env: Env,
        board_id: u64,
        user: Address,
        role: Role,
        caller: Address,
    ) {
        caller.require_auth();

        // Caller must be board admin+ or community admin+
        let caller_board_role = Self::get_role(env.clone(), board_id, caller.clone());
        if caller_board_role != Role::Owner && caller_board_role != Role::Admin {
            panic!("Only board admin+ can set role overrides");
        }

        env.storage()
            .persistent()
            .set(&PermKey::BoardRoleOverride(board_id, user), &role);
    }

    /// Get board role override for a user (returns None if no override)
    pub fn get_board_role_override(env: Env, board_id: u64, user: Address) -> Option<Role> {
        env.storage()
            .persistent()
            .get(&PermKey::BoardRoleOverride(board_id, user))
    }

    /// Remove board role override for a user
    pub fn remove_board_role_override(
        env: Env,
        board_id: u64,
        user: Address,
        caller: Address,
    ) {
        caller.require_auth();

        let caller_board_role = Self::get_role(env.clone(), board_id, caller);
        if caller_board_role != Role::Owner && caller_board_role != Role::Admin {
            panic!("Only board admin+ can remove role overrides");
        }

        env.storage()
            .persistent()
            .remove(&PermKey::BoardRoleOverride(board_id, user));
    }

    // ==================== Effective Permissions (Community + Board) ====================

    /// Get effective permissions for a user on a board within a community
    ///
    /// Role inheritance rules:
    /// 1. Community ban overrides everything (banned from all boards in community)
    /// 2. Community role provides baseline permissions
    /// 3. Board can RESTRICT but not EXPAND community role
    /// 4. effective_role = min(community_role, board_override) if override exists
    pub fn get_effective_permissions(
        env: Env,
        board_id: u64,
        community_id: Option<u64>,
        user: Address,
    ) -> PermissionSet {
        // Check global ban first
        if let Some(ban) = env
            .storage()
            .persistent()
            .get::<_, Ban>(&PermKey::GlobalBan(user.clone()))
        {
            let is_banned = match ban.expires_at {
                Some(expires) => env.ledger().timestamp() < expires,
                None => true,
            };
            if is_banned {
                return PermissionSet {
                    role: Role::Guest,
                    can_view: false,
                    can_post: false,
                    can_moderate: false,
                    can_admin: false,
                    is_banned: true,
                };
            }
        }

        // If no community, just return board permissions
        let community_id = match community_id {
            Some(id) => id,
            None => return Self::get_permissions(env, board_id, user),
        };

        // Check community ban
        if Self::is_community_banned(env.clone(), community_id, user.clone()) {
            return PermissionSet {
                role: Role::Guest,
                can_view: false,
                can_post: false,
                can_moderate: false,
                can_admin: false,
                is_banned: true,
            };
        }

        // Check board-specific ban
        if Self::is_banned(env.clone(), board_id, user.clone()) {
            return PermissionSet {
                role: Role::Guest,
                can_view: false,
                can_post: false,
                can_moderate: false,
                can_admin: false,
                is_banned: true,
            };
        }

        // Get community role as baseline
        let community_role = Self::get_community_role(env.clone(), community_id, user.clone());

        // Get board-specific role
        let board_role = Self::get_role(env.clone(), board_id, user.clone());

        // Get board role override (restriction)
        let board_override = Self::get_board_role_override(env.clone(), board_id, user);

        // Calculate effective role:
        // 1. Start with the higher of community_role and board_role (community roles cascade)
        // 2. If there's a board override, take the minimum (restrictions apply)
        let base_role = if community_role as u32 > board_role as u32 {
            community_role
        } else {
            board_role
        };

        let effective_role = match board_override {
            Some(override_role) => {
                if (override_role as u32) < (base_role as u32) {
                    override_role
                } else {
                    base_role
                }
            }
            None => base_role,
        };

        PermissionSet {
            role: effective_role.clone(),
            can_view: true,
            can_post: effective_role as u32 >= Role::Member as u32,
            can_moderate: effective_role as u32 >= Role::Moderator as u32,
            can_admin: effective_role as u32 >= Role::Admin as u32,
            is_banned: false,
        }
    }

    // ==================== User Flair Functions ====================

    /// Set a user's flair (badge) for a board
    /// Only Moderator+ can set user flairs
    pub fn set_user_flair(
        env: Env,
        board_id: u64,
        user: Address,
        text: String,
        color: String,
        bg_color: String,
        caller: Address,
    ) {
        caller.require_auth();

        // Check caller has Moderator+ permissions
        let caller_role = Self::get_role(env.clone(), board_id, caller.clone());
        if (caller_role as u32) < (Role::Moderator as u32) {
            panic!("Only moderator+ can set user flairs");
        }

        // Validate text length (max 24 chars)
        if text.len() > 24 {
            panic!("Flair text too long (max 24 chars)");
        }

        let flair = UserFlair {
            text,
            color,
            bg_color,
            granted_by: caller,
            granted_at: env.ledger().timestamp(),
        };

        env.storage()
            .persistent()
            .set(&PermKey::UserFlair(board_id, user), &flair);
    }

    /// Get a user's flair for a board
    pub fn get_user_flair(env: Env, board_id: u64, user: Address) -> Option<UserFlair> {
        env.storage()
            .persistent()
            .get(&PermKey::UserFlair(board_id, user))
    }

    /// Remove a user's flair from a board
    /// Only Moderator+ can remove user flairs
    pub fn remove_user_flair(env: Env, board_id: u64, user: Address, caller: Address) {
        caller.require_auth();

        // Check caller has Moderator+ permissions
        let caller_role = Self::get_role(env.clone(), board_id, caller);
        if (caller_role as u32) < (Role::Moderator as u32) {
            panic!("Only moderator+ can remove user flairs");
        }

        env.storage()
            .persistent()
            .remove(&PermKey::UserFlair(board_id, user));
    }

    /// Upgrade the contract WASM
    pub fn upgrade(env: Env, new_wasm_hash: BytesN<32>) {
        // Get registry and verify caller is registry admin
        // For now, just require the registry to authorize
        let registry: Address = env
            .storage()
            .instance()
            .get(&PermKey::Registry)
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
    fn test_init_and_set_owner() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsPermissions, ());
        let client = BoardsPermissionsClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        client.init(&registry);

        let owner = Address::generate(&env);
        client.set_board_owner(&0, &owner);

        assert_eq!(client.get_board_owner(&0), Some(owner.clone()));
        assert_eq!(client.get_role(&0, &owner), Role::Owner);
    }

    #[test]
    fn test_role_hierarchy() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsPermissions, ());
        let client = BoardsPermissionsClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        client.init(&registry);

        let owner = Address::generate(&env);
        let admin = Address::generate(&env);
        let moderator = Address::generate(&env);
        let member = Address::generate(&env);

        client.set_board_owner(&0, &owner);

        // Owner sets admin
        client.set_role(&0, &admin, &Role::Admin, &owner);
        assert_eq!(client.get_role(&0, &admin), Role::Admin);

        // Owner sets moderator
        client.set_role(&0, &moderator, &Role::Moderator, &owner);
        assert_eq!(client.get_role(&0, &moderator), Role::Moderator);

        // Admin sets member
        client.set_role(&0, &member, &Role::Member, &admin);
        assert_eq!(client.get_role(&0, &member), Role::Member);
    }

    #[test]
    fn test_ban_user() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsPermissions, ());
        let client = BoardsPermissionsClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        client.init(&registry);

        let owner = Address::generate(&env);
        let user = Address::generate(&env);

        client.set_board_owner(&0, &owner);

        // Ban user
        let reason = String::from_str(&env, "Spam");
        client.ban_user(&0, &user, &reason, &Some(24), &owner);

        assert!(client.is_banned(&0, &user));
        assert!(!client.can_create_thread(&0, &user));

        // Unban
        client.unban_user(&0, &user, &owner);
        assert!(!client.is_banned(&0, &user));
    }

    #[test]
    fn test_permissions() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsPermissions, ());
        let client = BoardsPermissionsClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        client.init(&registry);

        let owner = Address::generate(&env);
        let member = Address::generate(&env);
        let guest = Address::generate(&env);

        client.set_board_owner(&0, &owner);
        client.set_role(&0, &member, &Role::Member, &owner);

        // Owner has all permissions
        let owner_perms = client.get_permissions(&0, &owner);
        assert!(owner_perms.can_view);
        assert!(owner_perms.can_post);
        assert!(owner_perms.can_moderate);
        assert!(owner_perms.can_admin);

        // Member can post but not moderate
        let member_perms = client.get_permissions(&0, &member);
        assert!(member_perms.can_view);
        assert!(member_perms.can_post);
        assert!(!member_perms.can_moderate);
        assert!(!member_perms.can_admin);

        // Guest can only view
        let guest_perms = client.get_permissions(&0, &guest);
        assert!(guest_perms.can_view);
        assert!(!guest_perms.can_post);
        assert!(!guest_perms.can_moderate);
        assert!(!guest_perms.can_admin);
    }

    #[test]
    fn test_membership_lists() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsPermissions, ());
        let client = BoardsPermissionsClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        client.init(&registry);

        let owner = Address::generate(&env);
        let admin = Address::generate(&env);
        let mod1 = Address::generate(&env);
        let mod2 = Address::generate(&env);
        let member = Address::generate(&env);

        client.set_board_owner(&0, &owner);
        client.set_role(&0, &admin, &Role::Admin, &owner);
        client.set_role(&0, &mod1, &Role::Moderator, &owner);
        client.set_role(&0, &mod2, &Role::Moderator, &owner);
        client.set_role(&0, &member, &Role::Member, &owner);

        // Check lists
        let admins = client.list_admins(&0);
        assert_eq!(admins.len(), 1);
        assert_eq!(admins.get(0).unwrap(), admin);

        let mods = client.list_moderators(&0);
        assert_eq!(mods.len(), 2);

        let members = client.list_members(&0);
        assert_eq!(members.len(), 1);
        assert_eq!(members.get(0).unwrap(), member);

        // Promote mod1 to admin - should move from mod list to admin list
        client.set_role(&0, &mod1, &Role::Admin, &owner);
        let admins = client.list_admins(&0);
        assert_eq!(admins.len(), 2);
        let mods = client.list_moderators(&0);
        assert_eq!(mods.len(), 1);
    }

    #[test]
    fn test_banned_list() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsPermissions, ());
        let client = BoardsPermissionsClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        client.init(&registry);

        let owner = Address::generate(&env);
        let user1 = Address::generate(&env);
        let user2 = Address::generate(&env);

        client.set_board_owner(&0, &owner);

        // Ban users
        let reason = String::from_str(&env, "Spam");
        client.ban_user(&0, &user1, &reason, &None, &owner); // Permanent
        client.ban_user(&0, &user2, &reason, &Some(24), &owner); // 24 hours

        // Check banned list
        let banned = client.list_banned(&0);
        assert_eq!(banned.len(), 2);

        // Check bans list with details
        let bans = client.list_bans(&0);
        assert_eq!(bans.len(), 2);

        // Unban one
        client.unban_user(&0, &user1, &owner);
        let banned = client.list_banned(&0);
        assert_eq!(banned.len(), 1);
    }

    #[test]
    fn test_flag_threshold() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsPermissions, ());
        let client = BoardsPermissionsClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        client.init(&registry);

        let owner = Address::generate(&env);
        client.set_board_owner(&0, &owner);

        // Default threshold is 3
        assert_eq!(client.get_flag_threshold(&0), 3);

        // Set a new threshold
        client.set_flag_threshold(&0, &5, &owner);
        assert_eq!(client.get_flag_threshold(&0), 5);
    }

    #[test]
    fn test_role_count() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsPermissions, ());
        let client = BoardsPermissionsClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        client.init(&registry);

        let owner = Address::generate(&env);
        client.set_board_owner(&0, &owner);

        // Initially no one in role lists
        assert_eq!(client.role_count(&0, &Role::Admin), 0);
        assert_eq!(client.role_count(&0, &Role::Moderator), 0);
        assert_eq!(client.role_count(&0, &Role::Member), 0);

        // Add users to roles
        let admin = Address::generate(&env);
        let mod1 = Address::generate(&env);
        let mod2 = Address::generate(&env);
        let member = Address::generate(&env);

        client.set_role(&0, &admin, &Role::Admin, &owner);
        client.set_role(&0, &mod1, &Role::Moderator, &owner);
        client.set_role(&0, &mod2, &Role::Moderator, &owner);
        client.set_role(&0, &member, &Role::Member, &owner);

        assert_eq!(client.role_count(&0, &Role::Admin), 1);
        assert_eq!(client.role_count(&0, &Role::Moderator), 2);
        assert_eq!(client.role_count(&0, &Role::Member), 1);
    }

    #[test]
    fn test_helper_functions() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsPermissions, ());
        let client = BoardsPermissionsClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        client.init(&registry);

        let owner = Address::generate(&env);
        let member = Address::generate(&env);
        let guest = Address::generate(&env);

        client.set_board_owner(&0, &owner);
        client.set_role(&0, &member, &Role::Member, &owner);

        // Test helper permission check functions
        assert!(client.can_create_thread(&0, &owner));
        assert!(client.can_create_thread(&0, &member));
        assert!(!client.can_create_thread(&0, &guest));

        assert!(client.can_reply(&0, &owner));
        assert!(client.can_reply(&0, &member));
        assert!(!client.can_reply(&0, &guest));

        assert!(client.can_moderate(&0, &owner));
        assert!(!client.can_moderate(&0, &member));
        assert!(!client.can_moderate(&0, &guest));

        assert!(client.can_admin(&0, &owner));
        assert!(!client.can_admin(&0, &member));
    }

    #[test]
    fn test_has_role() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsPermissions, ());
        let client = BoardsPermissionsClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        client.init(&registry);

        let owner = Address::generate(&env);
        let moderator = Address::generate(&env);
        let member = Address::generate(&env);
        let guest = Address::generate(&env);

        client.set_board_owner(&0, &owner);
        client.set_role(&0, &moderator, &Role::Moderator, &owner);
        client.set_role(&0, &member, &Role::Member, &owner);

        // Owner has all roles
        assert!(client.has_role(&0, &owner, &Role::Owner));
        assert!(client.has_role(&0, &owner, &Role::Admin));
        assert!(client.has_role(&0, &owner, &Role::Moderator));
        assert!(client.has_role(&0, &owner, &Role::Member));
        assert!(client.has_role(&0, &owner, &Role::Guest));

        // Moderator has moderator and below
        assert!(!client.has_role(&0, &moderator, &Role::Owner));
        assert!(!client.has_role(&0, &moderator, &Role::Admin));
        assert!(client.has_role(&0, &moderator, &Role::Moderator));
        assert!(client.has_role(&0, &moderator, &Role::Member));
        assert!(client.has_role(&0, &moderator, &Role::Guest));

        // Member has member and below
        assert!(!client.has_role(&0, &member, &Role::Moderator));
        assert!(client.has_role(&0, &member, &Role::Member));
        assert!(client.has_role(&0, &member, &Role::Guest));

        // Guest only has guest
        assert!(!client.has_role(&0, &guest, &Role::Member));
        assert!(client.has_role(&0, &guest, &Role::Guest));
    }

    #[test]
    #[should_panic(expected = "Only owner can set admin")]
    fn test_unauthorized_set_admin() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsPermissions, ());
        let client = BoardsPermissionsClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        client.init(&registry);

        let owner = Address::generate(&env);
        let moderator = Address::generate(&env);
        let user = Address::generate(&env);

        client.set_board_owner(&0, &owner);
        client.set_role(&0, &moderator, &Role::Moderator, &owner);

        // Moderator cannot set admin - should panic
        client.set_role(&0, &user, &Role::Admin, &moderator);
    }

    #[test]
    #[should_panic(expected = "Cannot ban user with equal or higher role")]
    fn test_cannot_ban_higher_role() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsPermissions, ());
        let client = BoardsPermissionsClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        client.init(&registry);

        let owner = Address::generate(&env);
        let admin = Address::generate(&env);
        let moderator = Address::generate(&env);

        client.set_board_owner(&0, &owner);
        client.set_role(&0, &admin, &Role::Admin, &owner);
        client.set_role(&0, &moderator, &Role::Moderator, &owner);

        // Moderator cannot ban admin - should panic
        let reason = String::from_str(&env, "Test");
        client.ban_user(&0, &admin, &reason, &None, &moderator);
    }

    #[test]
    fn test_multiple_boards() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsPermissions, ());
        let client = BoardsPermissionsClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        client.init(&registry);

        let owner1 = Address::generate(&env);
        let owner2 = Address::generate(&env);
        let user = Address::generate(&env);

        // Set up two boards with different owners
        client.set_board_owner(&0, &owner1);
        client.set_board_owner(&1, &owner2);

        // Owner1 is owner of board 0 only
        assert_eq!(client.get_role(&0, &owner1), Role::Owner);
        assert_eq!(client.get_role(&1, &owner1), Role::Guest);

        // Owner2 is owner of board 1 only
        assert_eq!(client.get_role(&0, &owner2), Role::Guest);
        assert_eq!(client.get_role(&1, &owner2), Role::Owner);

        // Make user a member of board 0 but not board 1
        client.set_role(&0, &user, &Role::Member, &owner1);
        assert_eq!(client.get_role(&0, &user), Role::Member);
        assert_eq!(client.get_role(&1, &user), Role::Guest);
    }

    #[test]
    fn test_get_ban_details() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsPermissions, ());
        let client = BoardsPermissionsClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        client.init(&registry);

        let owner = Address::generate(&env);
        let user = Address::generate(&env);

        client.set_board_owner(&0, &owner);

        // Ban user with reason
        let reason = String::from_str(&env, "Violating community guidelines");
        client.ban_user(&0, &user, &reason, &Some(48), &owner);

        // Get ban details
        let ban = client.get_ban(&0, &user).unwrap();
        assert_eq!(ban.user, user);
        assert_eq!(ban.issuer, owner);
        assert_eq!(ban.reason, reason);
        assert!(ban.expires_at.is_some());
    }

    #[test]
    fn test_demote_user() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsPermissions, ());
        let client = BoardsPermissionsClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        client.init(&registry);

        let owner = Address::generate(&env);
        let user = Address::generate(&env);

        client.set_board_owner(&0, &owner);

        // Promote user to admin
        client.set_role(&0, &user, &Role::Admin, &owner);
        assert_eq!(client.get_role(&0, &user), Role::Admin);
        assert_eq!(client.list_admins(&0).len(), 1);

        // Demote to member
        client.set_role(&0, &user, &Role::Member, &owner);
        assert_eq!(client.get_role(&0, &user), Role::Member);
        assert_eq!(client.list_admins(&0).len(), 0);
        assert_eq!(client.list_members(&0).len(), 1);

        // Demote to guest (remove from board)
        client.set_role(&0, &user, &Role::Guest, &owner);
        assert_eq!(client.get_role(&0, &user), Role::Guest);
        assert_eq!(client.list_members(&0).len(), 0);
    }

    // Invite system tests

    #[test]
    fn test_request_invite() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsPermissions, ());
        let client = BoardsPermissionsClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        client.init(&registry);

        let owner = Address::generate(&env);
        let user = Address::generate(&env);

        client.set_board_owner(&0, &owner);

        // User requests invite
        client.request_invite(&0, &user);

        // Check request exists
        assert!(client.has_invite_request(&0, &user));
        let requests = client.list_invite_requests(&0);
        assert_eq!(requests.len(), 1);
        assert_eq!(requests.get(0).unwrap().user, user);
    }

    #[test]
    fn test_accept_invite() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsPermissions, ());
        let client = BoardsPermissionsClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        client.init(&registry);

        let owner = Address::generate(&env);
        let user = Address::generate(&env);

        client.set_board_owner(&0, &owner);

        // User requests invite
        client.request_invite(&0, &user);
        assert!(client.has_invite_request(&0, &user));

        // Owner accepts invite
        client.accept_invite(&0, &user, &owner);

        // Check user is now member
        assert_eq!(client.get_role(&0, &user), Role::Member);
        assert!(!client.has_invite_request(&0, &user));
        assert_eq!(client.list_members(&0).len(), 1);
    }

    #[test]
    fn test_revoke_invite() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsPermissions, ());
        let client = BoardsPermissionsClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        client.init(&registry);

        let owner = Address::generate(&env);
        let user = Address::generate(&env);

        client.set_board_owner(&0, &owner);

        // User requests invite
        client.request_invite(&0, &user);
        assert!(client.has_invite_request(&0, &user));

        // Owner revokes invite
        client.revoke_invite(&0, &user, &owner);

        // Check request is gone, user is still guest
        assert!(!client.has_invite_request(&0, &user));
        assert_eq!(client.get_role(&0, &user), Role::Guest);
    }

    #[test]
    fn test_invite_member_directly() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsPermissions, ());
        let client = BoardsPermissionsClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        client.init(&registry);

        let owner = Address::generate(&env);
        let user = Address::generate(&env);

        client.set_board_owner(&0, &owner);

        // Owner directly invites as member
        client.invite_member(&0, &user, &Role::Member, &owner);

        // Check user is now member
        assert_eq!(client.get_role(&0, &user), Role::Member);
        assert_eq!(client.list_members(&0).len(), 1);
    }

    #[test]
    fn test_invite_member_with_role() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsPermissions, ());
        let client = BoardsPermissionsClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        client.init(&registry);

        let owner = Address::generate(&env);
        let new_admin = Address::generate(&env);
        let new_mod = Address::generate(&env);

        client.set_board_owner(&0, &owner);

        // Owner invites as admin
        client.invite_member(&0, &new_admin, &Role::Admin, &owner);
        assert_eq!(client.get_role(&0, &new_admin), Role::Admin);
        assert_eq!(client.list_admins(&0).len(), 1);

        // Owner invites as moderator
        client.invite_member(&0, &new_mod, &Role::Moderator, &owner);
        assert_eq!(client.get_role(&0, &new_mod), Role::Moderator);
        assert_eq!(client.list_moderators(&0).len(), 1);
    }

    #[test]
    fn test_invite_clears_pending_request() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsPermissions, ());
        let client = BoardsPermissionsClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        client.init(&registry);

        let owner = Address::generate(&env);
        let user = Address::generate(&env);

        client.set_board_owner(&0, &owner);

        // User requests invite
        client.request_invite(&0, &user);
        assert!(client.has_invite_request(&0, &user));

        // Owner directly invites (bypasses request-accept flow)
        client.invite_member(&0, &user, &Role::Member, &owner);

        // Request should be cleared
        assert!(!client.has_invite_request(&0, &user));
        assert_eq!(client.get_role(&0, &user), Role::Member);
    }

    #[test]
    #[should_panic(expected = "Already a member of this board")]
    fn test_member_cannot_request_invite() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsPermissions, ());
        let client = BoardsPermissionsClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        client.init(&registry);

        let owner = Address::generate(&env);
        let member = Address::generate(&env);

        client.set_board_owner(&0, &owner);
        client.set_role(&0, &member, &Role::Member, &owner);

        // Member cannot request invite - already a member
        client.request_invite(&0, &member);
    }

    #[test]
    #[should_panic(expected = "Banned users cannot request invites")]
    fn test_banned_cannot_request_invite() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsPermissions, ());
        let client = BoardsPermissionsClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        client.init(&registry);

        let owner = Address::generate(&env);
        let banned_user = Address::generate(&env);

        client.set_board_owner(&0, &owner);
        let reason = String::from_str(&env, "Spam");
        client.ban_user(&0, &banned_user, &reason, &None, &owner);

        // Banned user cannot request invite
        client.request_invite(&0, &banned_user);
    }

    #[test]
    #[should_panic(expected = "Invite request already pending")]
    fn test_duplicate_request_fails() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsPermissions, ());
        let client = BoardsPermissionsClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        client.init(&registry);

        let owner = Address::generate(&env);
        let user = Address::generate(&env);

        client.set_board_owner(&0, &owner);

        // First request succeeds
        client.request_invite(&0, &user);

        // Second request should fail
        client.request_invite(&0, &user);
    }

    #[test]
    #[should_panic(expected = "Only moderator+ can accept invite requests")]
    fn test_member_cannot_accept_invite() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsPermissions, ());
        let client = BoardsPermissionsClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        client.init(&registry);

        let owner = Address::generate(&env);
        let member = Address::generate(&env);
        let user = Address::generate(&env);

        client.set_board_owner(&0, &owner);
        client.set_role(&0, &member, &Role::Member, &owner);

        client.request_invite(&0, &user);

        // Member cannot accept - should panic
        client.accept_invite(&0, &user, &member);
    }

    #[test]
    #[should_panic(expected = "Only admin+ can invite moderators")]
    fn test_mod_cannot_invite_mod() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsPermissions, ());
        let client = BoardsPermissionsClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        client.init(&registry);

        let owner = Address::generate(&env);
        let moderator = Address::generate(&env);
        let user = Address::generate(&env);

        client.set_board_owner(&0, &owner);
        client.set_role(&0, &moderator, &Role::Moderator, &owner);

        // Moderator cannot invite as moderator - should panic
        client.invite_member(&0, &user, &Role::Moderator, &moderator);
    }

    #[test]
    fn test_moderator_can_accept_invite() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsPermissions, ());
        let client = BoardsPermissionsClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        client.init(&registry);

        let owner = Address::generate(&env);
        let moderator = Address::generate(&env);
        let user = Address::generate(&env);

        client.set_board_owner(&0, &owner);
        client.set_role(&0, &moderator, &Role::Moderator, &owner);

        client.request_invite(&0, &user);

        // Moderator can accept
        client.accept_invite(&0, &user, &moderator);
        assert_eq!(client.get_role(&0, &user), Role::Member);
    }

    #[test]
    fn test_get_invite_request() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsPermissions, ());
        let client = BoardsPermissionsClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        client.init(&registry);

        let owner = Address::generate(&env);
        let user = Address::generate(&env);

        client.set_board_owner(&0, &owner);
        client.request_invite(&0, &user);

        // Get the invite request
        let request = client.get_invite_request(&0, &user).unwrap();
        assert_eq!(request.user, user);
        assert_eq!(request.board_id, 0);
    }

    // ==================== Community Permission Tests ====================

    #[test]
    fn test_set_community_owner() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsPermissions, ());
        let client = BoardsPermissionsClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        client.init(&registry);

        let owner = Address::generate(&env);

        // Set community owner
        client.set_community_owner(&0, &owner);

        // Owner should have Owner role
        assert_eq!(client.get_community_owner(&0), Some(owner.clone()));
        assert_eq!(client.get_community_role(&0, &owner), Role::Owner);
    }

    #[test]
    fn test_community_role_hierarchy() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsPermissions, ());
        let client = BoardsPermissionsClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        client.init(&registry);

        let owner = Address::generate(&env);
        let admin = Address::generate(&env);
        let moderator = Address::generate(&env);
        let member = Address::generate(&env);
        let guest = Address::generate(&env);

        // Set community owner
        client.set_community_owner(&0, &owner);

        // Owner sets admin
        client.set_community_role(&0, &admin, &Role::Admin, &owner);
        assert_eq!(client.get_community_role(&0, &admin), Role::Admin);

        // Owner sets moderator
        client.set_community_role(&0, &moderator, &Role::Moderator, &owner);
        assert_eq!(client.get_community_role(&0, &moderator), Role::Moderator);

        // Admin sets member
        client.set_community_role(&0, &member, &Role::Member, &admin);
        assert_eq!(client.get_community_role(&0, &member), Role::Member);

        // Guest should have Guest role by default
        assert_eq!(client.get_community_role(&0, &guest), Role::Guest);
    }

    #[test]
    fn test_community_role_lists() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsPermissions, ());
        let client = BoardsPermissionsClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        client.init(&registry);

        let owner = Address::generate(&env);
        let admin1 = Address::generate(&env);
        let admin2 = Address::generate(&env);
        let mod1 = Address::generate(&env);
        let member1 = Address::generate(&env);

        client.set_community_owner(&0, &owner);

        // Add users to roles
        client.set_community_role(&0, &admin1, &Role::Admin, &owner);
        client.set_community_role(&0, &admin2, &Role::Admin, &owner);
        client.set_community_role(&0, &mod1, &Role::Moderator, &owner);
        client.set_community_role(&0, &member1, &Role::Member, &owner);

        // Check lists
        assert_eq!(client.list_community_admins(&0).len(), 2);
        assert_eq!(client.list_community_moderators(&0).len(), 1);
        assert_eq!(client.list_community_members(&0).len(), 1);
    }

    #[test]
    fn test_community_ban_user() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsPermissions, ());
        let client = BoardsPermissionsClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        client.init(&registry);

        let owner = Address::generate(&env);
        let user = Address::generate(&env);

        client.set_community_owner(&0, &owner);

        // Ban user from community
        let reason = String::from_str(&env, "Spam");
        client.community_ban_user(&0, &user, &reason, &Some(24), &owner);

        // Check ban
        assert!(client.is_community_banned(&0, &user));
        let banned = client.list_community_banned(&0);
        assert_eq!(banned.len(), 1);

        // Get ban details
        let ban = client.get_community_ban(&0, &user).unwrap();
        assert_eq!(ban.user, user);
        assert_eq!(ban.issuer, owner);
        assert_eq!(ban.reason, reason);
    }

    #[test]
    fn test_community_unban_user() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsPermissions, ());
        let client = BoardsPermissionsClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        client.init(&registry);

        let owner = Address::generate(&env);
        let user = Address::generate(&env);

        client.set_community_owner(&0, &owner);

        // Ban and then unban
        let reason = String::from_str(&env, "Spam");
        client.community_ban_user(&0, &user, &reason, &None, &owner);
        assert!(client.is_community_banned(&0, &user));

        client.community_unban_user(&0, &user, &owner);
        assert!(!client.is_community_banned(&0, &user));
    }

    #[test]
    #[should_panic(expected = "Cannot ban user with equal or higher community role")]
    fn test_community_cannot_ban_higher_role() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsPermissions, ());
        let client = BoardsPermissionsClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        client.init(&registry);

        let owner = Address::generate(&env);
        let admin = Address::generate(&env);
        let moderator = Address::generate(&env);

        client.set_community_owner(&0, &owner);
        client.set_community_role(&0, &admin, &Role::Admin, &owner);
        client.set_community_role(&0, &moderator, &Role::Moderator, &owner);

        // Moderator cannot ban admin - should panic
        let reason = String::from_str(&env, "Test");
        client.community_ban_user(&0, &admin, &reason, &None, &moderator);
    }

    #[test]
    fn test_board_role_override() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsPermissions, ());
        let client = BoardsPermissionsClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        client.init(&registry);

        let owner = Address::generate(&env);
        let user = Address::generate(&env);

        client.set_board_owner(&0, &owner);

        // Set a role override
        client.set_board_role_override(&0, &user, &Role::Member, &owner);

        // Check override exists
        assert_eq!(client.get_board_role_override(&0, &user), Some(Role::Member));

        // Remove override
        client.remove_board_role_override(&0, &user, &owner);
        assert_eq!(client.get_board_role_override(&0, &user), None);
    }

    #[test]
    fn test_effective_permissions_community_role_cascades() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsPermissions, ());
        let client = BoardsPermissionsClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        client.init(&registry);

        let community_owner = Address::generate(&env);
        let community_mod = Address::generate(&env);
        let board_owner = Address::generate(&env);

        // Set up community
        client.set_community_owner(&0, &community_owner);
        client.set_community_role(&0, &community_mod, &Role::Moderator, &community_owner);

        // Set up board
        client.set_board_owner(&0, &board_owner);

        // Community mod should have Moderator permissions on the board
        let perms = client.get_effective_permissions(&0, &Some(0), &community_mod);
        assert_eq!(perms.role, Role::Moderator);
        assert!(perms.can_view);
        assert!(perms.can_post);
        assert!(perms.can_moderate);
        assert!(!perms.can_admin);
    }

    #[test]
    fn test_effective_permissions_board_can_restrict() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsPermissions, ());
        let client = BoardsPermissionsClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        client.init(&registry);

        let community_owner = Address::generate(&env);
        let community_mod = Address::generate(&env);
        let board_owner = Address::generate(&env);

        // Set up community
        client.set_community_owner(&0, &community_owner);
        client.set_community_role(&0, &community_mod, &Role::Moderator, &community_owner);

        // Set up board
        client.set_board_owner(&0, &board_owner);

        // Restrict the community mod on this specific board
        client.set_board_role_override(&0, &community_mod, &Role::Member, &board_owner);

        // Community mod should only have Member permissions due to override
        let perms = client.get_effective_permissions(&0, &Some(0), &community_mod);
        assert_eq!(perms.role, Role::Member);
        assert!(perms.can_view);
        assert!(perms.can_post);
        assert!(!perms.can_moderate); // Restricted!
        assert!(!perms.can_admin);
    }

    #[test]
    fn test_effective_permissions_community_ban_blocks_all_boards() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsPermissions, ());
        let client = BoardsPermissionsClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        client.init(&registry);

        let community_owner = Address::generate(&env);
        let board_owner = Address::generate(&env);
        let user = Address::generate(&env);

        // Set up community
        client.set_community_owner(&0, &community_owner);

        // Set up board and add user as member
        client.set_board_owner(&0, &board_owner);
        client.set_role(&0, &user, &Role::Member, &board_owner);

        // User should have Member permissions
        let perms = client.get_effective_permissions(&0, &Some(0), &user);
        assert_eq!(perms.role, Role::Member);
        assert!(!perms.is_banned);

        // Ban user from community
        let reason = String::from_str(&env, "Spam");
        client.community_ban_user(&0, &user, &reason, &None, &community_owner);

        // User should now be banned from board even though they have Member role
        let perms = client.get_effective_permissions(&0, &Some(0), &user);
        assert!(perms.is_banned);
        assert!(!perms.can_view);
        assert!(!perms.can_post);
    }

    #[test]
    fn test_effective_permissions_no_community() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsPermissions, ());
        let client = BoardsPermissionsClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        client.init(&registry);

        let board_owner = Address::generate(&env);
        let member = Address::generate(&env);

        // Set up standalone board (no community)
        client.set_board_owner(&0, &board_owner);
        client.set_role(&0, &member, &Role::Member, &board_owner);

        // Should work the same as regular get_permissions
        let perms = client.get_effective_permissions(&0, &None, &member);
        assert_eq!(perms.role, Role::Member);
        assert!(perms.can_view);
        assert!(perms.can_post);
        assert!(!perms.can_moderate);
    }

    #[test]
    fn test_has_community_role() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsPermissions, ());
        let client = BoardsPermissionsClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        client.init(&registry);

        let owner = Address::generate(&env);
        let moderator = Address::generate(&env);
        let guest = Address::generate(&env);

        client.set_community_owner(&0, &owner);
        client.set_community_role(&0, &moderator, &Role::Moderator, &owner);

        // Owner has all roles
        assert!(client.has_community_role(&0, &owner, &Role::Owner));
        assert!(client.has_community_role(&0, &owner, &Role::Moderator));
        assert!(client.has_community_role(&0, &owner, &Role::Guest));

        // Moderator has moderator and below
        assert!(!client.has_community_role(&0, &moderator, &Role::Owner));
        assert!(client.has_community_role(&0, &moderator, &Role::Moderator));
        assert!(client.has_community_role(&0, &moderator, &Role::Member));
        assert!(client.has_community_role(&0, &moderator, &Role::Guest));

        // Guest only has guest
        assert!(!client.has_community_role(&0, &guest, &Role::Member));
        assert!(client.has_community_role(&0, &guest, &Role::Guest));
    }

    #[test]
    #[should_panic(expected = "Only owner can set community admin")]
    fn test_community_unauthorized_set_admin() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsPermissions, ());
        let client = BoardsPermissionsClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        client.init(&registry);

        let owner = Address::generate(&env);
        let moderator = Address::generate(&env);
        let user = Address::generate(&env);

        client.set_community_owner(&0, &owner);
        client.set_community_role(&0, &moderator, &Role::Moderator, &owner);

        // Moderator cannot set admin - should panic
        client.set_community_role(&0, &user, &Role::Admin, &moderator);
    }

    #[test]
    fn test_community_role_promotion_updates_lists() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsPermissions, ());
        let client = BoardsPermissionsClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        client.init(&registry);

        let owner = Address::generate(&env);
        let user = Address::generate(&env);

        client.set_community_owner(&0, &owner);

        // Add user as moderator
        client.set_community_role(&0, &user, &Role::Moderator, &owner);
        assert_eq!(client.list_community_moderators(&0).len(), 1);
        assert_eq!(client.list_community_admins(&0).len(), 0);

        // Promote to admin
        client.set_community_role(&0, &user, &Role::Admin, &owner);
        assert_eq!(client.list_community_moderators(&0).len(), 0);
        assert_eq!(client.list_community_admins(&0).len(), 1);
    }

    #[test]
    fn test_multiple_communities() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsPermissions, ());
        let client = BoardsPermissionsClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        client.init(&registry);

        let owner1 = Address::generate(&env);
        let owner2 = Address::generate(&env);
        let user = Address::generate(&env);

        // Set up two communities
        client.set_community_owner(&0, &owner1);
        client.set_community_owner(&1, &owner2);

        // Owner1 is owner of community 0 only
        assert_eq!(client.get_community_role(&0, &owner1), Role::Owner);
        assert_eq!(client.get_community_role(&1, &owner1), Role::Guest);

        // Owner2 is owner of community 1 only
        assert_eq!(client.get_community_role(&0, &owner2), Role::Guest);
        assert_eq!(client.get_community_role(&1, &owner2), Role::Owner);

        // User is guest in both by default
        assert_eq!(client.get_community_role(&0, &user), Role::Guest);
        assert_eq!(client.get_community_role(&1, &user), Role::Guest);

        // Make user a member of community 0 only
        client.set_community_role(&0, &user, &Role::Member, &owner1);
        assert_eq!(client.get_community_role(&0, &user), Role::Member);
        assert_eq!(client.get_community_role(&1, &user), Role::Guest);
    }
}
