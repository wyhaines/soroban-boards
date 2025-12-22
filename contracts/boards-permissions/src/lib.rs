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
}
