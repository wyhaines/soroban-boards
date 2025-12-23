#![no_std]

use soroban_render_sdk::prelude::*;
use soroban_sdk::{contract, contractimpl, contracttype, Address, Bytes, BytesN, Env, IntoVal, String, Symbol, Val, Vec};

// Declare render capabilities
soroban_render!(markdown, styles);

/// Storage keys for the admin contract
#[contracttype]
#[derive(Clone)]
pub enum AdminKey {
    /// Registry contract address
    Registry,
    /// Permissions contract address
    Permissions,
    /// Content contract address
    Content,
    /// Theme contract address (for shared components if needed)
    Theme,
}

// ============================================================================
// External Types (must match other contracts)
// ============================================================================

/// Board metadata from registry
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

/// Ban record from permissions contract
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

/// Role levels from permissions contract
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

/// Type of flagged content
#[contracttype]
#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum FlaggedType {
    Thread = 0,
    Reply = 1,
}

/// Flagged content item from content contract
#[contracttype]
#[derive(Clone)]
pub struct FlaggedItem {
    pub board_id: u64,
    pub thread_id: u64,
    pub reply_id: u64,
    pub item_type: FlaggedType,
    pub flag_count: u32,
    pub first_flagged_at: u64,
}

/// Permission set from permissions contract
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
pub struct BoardsAdmin;

#[contractimpl]
impl BoardsAdmin {
    /// Initialize the admin contract
    pub fn init(env: Env, registry: Address, permissions: Address, content: Address, theme: Address) {
        if env.storage().instance().has(&AdminKey::Registry) {
            panic!("Already initialized");
        }
        env.storage().instance().set(&AdminKey::Registry, &registry);
        env.storage().instance().set(&AdminKey::Permissions, &permissions);
        env.storage().instance().set(&AdminKey::Content, &content);
        env.storage().instance().set(&AdminKey::Theme, &theme);
    }

    /// Get registry address
    pub fn get_registry(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&AdminKey::Registry)
            .expect("Not initialized")
    }

    /// Get permissions address
    pub fn get_permissions(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&AdminKey::Permissions)
            .expect("Not initialized")
    }

    /// Get content address
    pub fn get_content(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&AdminKey::Content)
            .expect("Not initialized")
    }

    /// Get theme address
    pub fn get_theme(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&AdminKey::Theme)
            .expect("Not initialized")
    }

    /// Main render entry point for admin pages
    pub fn render(env: Env, path: Option<String>, viewer: Option<Address>) -> Bytes {
        Router::new(&env, path)
            // Board admin routes
            .handle(b"/b/{id}/members", |req| {
                let board_id = req.get_var_u32(b"id").unwrap_or(0) as u64;
                Self::render_members(&env, board_id, &viewer)
            })
            .or_handle(b"/b/{id}/banned", |req| {
                let board_id = req.get_var_u32(b"id").unwrap_or(0) as u64;
                Self::render_banned(&env, board_id, &viewer)
            })
            .or_handle(b"/b/{id}/flags", |req| {
                let board_id = req.get_var_u32(b"id").unwrap_or(0) as u64;
                Self::render_flag_queue(&env, board_id, &viewer)
            })
            .or_handle(b"/b/{id}/settings", |req| {
                let board_id = req.get_var_u32(b"id").unwrap_or(0) as u64;
                Self::render_settings(&env, board_id, &viewer)
            })
            // Default - show not found
            .or_default(|_| Self::render_not_found(&env))
    }

    /// Render the navigation bar
    fn render_nav(env: &Env, board_id: u64) -> MarkdownBuilder<'_> {
        MarkdownBuilder::new(env)
            .render_link("Soroban Boards", "/")
            .text(" | ")
            .raw_str("[Back to Board](render:/b/")
            .number(board_id as u32)
            .raw_str(")")
            .newline()
            .hr()
    }

    /// Append footer to builder
    fn render_footer_into(md: MarkdownBuilder<'_>) -> MarkdownBuilder<'_> {
        md.hr()
            .paragraph("*Powered by [Soroban Render](https://github.com/wyhaines/soroban-render) on [Stellar](https://stellar.org)*")
    }

    /// Render not found page
    fn render_not_found(env: &Env) -> Bytes {
        MarkdownBuilder::new(env)
            .render_link("Soroban Boards", "/")
            .newline()
            .hr()
            .h1("Page Not Found")
            .paragraph("The admin page you requested does not exist.")
            .render_link("Back to Home", "/")
            .build()
    }

    /// Format an address for display
    /// Returns the full address string (truncation would require copying bytes which is expensive)
    fn format_address(_env: &Env, addr: &Address) -> String {
        // Return the full address string - Stellar addresses are 56 chars
        // Truncation (e.g., GABCD...WXYZ) would require byte-level manipulation
        // which is expensive in Soroban. The UI layer can handle truncation if needed.
        addr.to_string()
    }

    /// Render members list page
    fn render_members(env: &Env, board_id: u64, viewer: &Option<Address>) -> Bytes {
        let permissions: Address = env
            .storage()
            .instance()
            .get(&AdminKey::Permissions)
            .expect("Not initialized");

        let mut md = Self::render_nav(env, board_id)
            .h1("Board Members");

        // Check if viewer has permission to view members (moderator+)
        let can_view = if let Some(user) = viewer {
            let args: Vec<Val> = Vec::from_array(env, [board_id.into_val(env), user.into_val(env)]);
            let perms: PermissionSet = env.invoke_contract(
                &permissions,
                &Symbol::new(env, "get_permissions"),
                args,
            );
            perms.can_moderate
        } else {
            false
        };

        if !can_view {
            md = md.warning("You must be a moderator to view this page.");
            return Self::render_footer_into(md).build();
        }

        // Fetch owner
        let owner_args: Vec<Val> = Vec::from_array(env, [board_id.into_val(env)]);
        let owner_opt: Option<Address> = env.invoke_contract(
            &permissions,
            &Symbol::new(env, "get_board_owner"),
            owner_args,
        );

        md = md.h2("Owner");
        if let Some(owner) = owner_opt {
            let addr_str = Self::format_address(env, &owner);
            md = md.text("- `").text_string(&addr_str).text("`").newline();
        } else {
            md = md.paragraph("*No owner set*");
        }

        // Fetch admins
        md = md.h2("Admins");
        let admins: Vec<Address> = env.invoke_contract(
            &permissions,
            &Symbol::new(env, "list_admins"),
            Vec::from_array(env, [board_id.into_val(env)]),
        );

        if admins.is_empty() {
            md = md.paragraph("*No admins*");
        } else {
            for i in 0..admins.len() {
                let addr = admins.get(i).unwrap();
                let addr_str = Self::format_address(env, &addr);
                md = md.text("- `").text_string(&addr_str).text("`");
                // Demote button (for owner only)
                md = md.text(" ").tx_link("[Demote]", "remove_admin", "");
                md = md.newline();
            }
        }

        // Fetch moderators
        md = md.h2("Moderators");
        let mods: Vec<Address> = env.invoke_contract(
            &permissions,
            &Symbol::new(env, "list_moderators"),
            Vec::from_array(env, [board_id.into_val(env)]),
        );

        if mods.is_empty() {
            md = md.paragraph("*No moderators*");
        } else {
            for i in 0..mods.len() {
                let addr = mods.get(i).unwrap();
                let addr_str = Self::format_address(env, &addr);
                md = md.text("- `").text_string(&addr_str).text("`");
                // Demote button (for admin+)
                md = md.text(" ").tx_link("[Demote]", "remove_moderator", "");
                md = md.newline();
            }
        }

        // Fetch members
        md = md.h2("Members");
        let members: Vec<Address> = env.invoke_contract(
            &permissions,
            &Symbol::new(env, "list_members"),
            Vec::from_array(env, [board_id.into_val(env)]),
        );

        if members.is_empty() {
            md = md.paragraph("*No members*");
        } else {
            for i in 0..members.len() {
                let addr = members.get(i).unwrap();
                let addr_str = Self::format_address(env, &addr);
                md = md.text("- `").text_string(&addr_str).text("`");
                // Promote/Remove buttons
                md = md.text(" ").tx_link("[Promote to Mod]", "set_moderator", "");
                md = md.text(" ").tx_link("[Remove]", "remove_member", "");
                md = md.newline();
            }
        }

        // Add member form
        md = md.hr()
            .h3("Add Member")
            .raw_str("<input type=\"hidden\" name=\"board_id\" value=\"")
            .number(board_id as u32)
            .raw_str("\" />\n")
            .input("user_address", "Wallet address (G...)")
            .newline()
            .form_link("Add as Member", "add_member")
            .text(" ")
            .form_link("Add as Moderator", "add_moderator")
            .text(" ")
            .form_link("Add as Admin", "add_admin");

        Self::render_footer_into(md).build()
    }

    /// Render banned users page
    fn render_banned(env: &Env, board_id: u64, viewer: &Option<Address>) -> Bytes {
        let permissions: Address = env
            .storage()
            .instance()
            .get(&AdminKey::Permissions)
            .expect("Not initialized");

        let mut md = Self::render_nav(env, board_id)
            .h1("Banned Users");

        // Check if viewer has permission (moderator+)
        let can_view = if let Some(user) = viewer {
            let args: Vec<Val> = Vec::from_array(env, [board_id.into_val(env), user.into_val(env)]);
            let perms: PermissionSet = env.invoke_contract(
                &permissions,
                &Symbol::new(env, "get_permissions"),
                args,
            );
            perms.can_moderate
        } else {
            false
        };

        if !can_view {
            md = md.warning("You must be a moderator to view this page.");
            return Self::render_footer_into(md).build();
        }

        // Fetch bans
        let bans: Vec<Ban> = env.invoke_contract(
            &permissions,
            &Symbol::new(env, "list_bans"),
            Vec::from_array(env, [board_id.into_val(env)]),
        );

        if bans.is_empty() {
            md = md.tip("No banned users. Good work!");
        } else {
            for i in 0..bans.len() {
                let ban = bans.get(i).unwrap();
                let user_str = Self::format_address(env, &ban.user);
                let issuer_str = Self::format_address(env, &ban.issuer);

                md = md.hr()
                    .h3("").text("`").text_string(&user_str).text("`")
                    .newline()
                    .text("**Reason:** ").text_string(&ban.reason).newline()
                    .text("**Issued by:** `").text_string(&issuer_str).text("`").newline();

                if let Some(expires) = ban.expires_at {
                    md = md.text("**Expires:** ").number(expires as u32).text(" (timestamp)").newline();
                } else {
                    md = md.text("**Expires:** *Permanent*").newline();
                }

                // Unban link with hidden fields
                md = md.raw_str("<input type=\"hidden\" name=\"board_id\" value=\"")
                    .number(board_id as u32)
                    .raw_str("\" />\n")
                    .tx_link("Unban User", "unban_user", "")
                    .newline();
            }
        }

        // Ban user form
        md = md.hr()
            .h3("Ban a User")
            .raw_str("<input type=\"hidden\" name=\"board_id\" value=\"")
            .number(board_id as u32)
            .raw_str("\" />\n")
            .input("user_address", "Wallet address (G...)")
            .newline()
            .input("reason", "Reason for ban")
            .newline()
            .input("duration_hours", "Duration (hours, 0 = permanent)")
            .newline()
            .form_link("Ban User", "ban_user");

        Self::render_footer_into(md).build()
    }

    /// Render flag queue page
    fn render_flag_queue(env: &Env, board_id: u64, viewer: &Option<Address>) -> Bytes {
        let permissions: Address = env
            .storage()
            .instance()
            .get(&AdminKey::Permissions)
            .expect("Not initialized");
        let content: Address = env
            .storage()
            .instance()
            .get(&AdminKey::Content)
            .expect("Not initialized");

        let mut md = Self::render_nav(env, board_id)
            .h1("Flag Queue");

        // Check if viewer has permission (moderator+)
        let can_view = if let Some(user) = viewer {
            let args: Vec<Val> = Vec::from_array(env, [board_id.into_val(env), user.into_val(env)]);
            let perms: PermissionSet = env.invoke_contract(
                &permissions,
                &Symbol::new(env, "get_permissions"),
                args,
            );
            perms.can_moderate
        } else {
            false
        };

        if !can_view {
            md = md.warning("You must be a moderator to view this page.");
            return Self::render_footer_into(md).build();
        }

        // Fetch flagged content
        let flagged: Vec<FlaggedItem> = env.invoke_contract(
            &content,
            &Symbol::new(env, "list_flagged_content"),
            Vec::from_array(env, [board_id.into_val(env)]),
        );

        if flagged.is_empty() {
            md = md.tip("No flagged content. Good work!");
        } else {
            md = md.paragraph("Review flagged content and take appropriate action.");

            for i in 0..flagged.len() {
                let item = flagged.get(i).unwrap();

                md = md.hr();

                let type_str = if item.item_type == FlaggedType::Thread { "Thread" } else { "Reply" };
                md = md.h3("").text(type_str).text(" #").number(item.thread_id as u32);

                if item.item_type == FlaggedType::Reply {
                    md = md.text(" / Reply #").number(item.reply_id as u32);
                }

                md = md.newline()
                    .text("**Flags:** ").number(item.flag_count).newline();

                // View link
                if item.item_type == FlaggedType::Thread {
                    md = md.raw_str("[View Thread](render:/b/")
                        .number(board_id as u32)
                        .raw_str("/t/")
                        .number(item.thread_id as u32)
                        .raw_str(")");
                } else {
                    md = md.raw_str("[View Reply](render:/b/")
                        .number(board_id as u32)
                        .raw_str("/t/")
                        .number(item.thread_id as u32)
                        .raw_str("/r/")
                        .number(item.reply_id as u32)
                        .raw_str(")");
                }

                // Hidden fields for actions
                md = md.raw_str("<input type=\"hidden\" name=\"board_id\" value=\"")
                    .number(board_id as u32)
                    .raw_str("\" />\n")
                    .raw_str("<input type=\"hidden\" name=\"thread_id\" value=\"")
                    .number(item.thread_id as u32)
                    .raw_str("\" />\n");

                if item.item_type == FlaggedType::Reply {
                    md = md.raw_str("<input type=\"hidden\" name=\"reply_id\" value=\"")
                        .number(item.reply_id as u32)
                        .raw_str("\" />\n");
                }

                // Actions
                if item.item_type == FlaggedType::Thread {
                    md = md.text(" | ").tx_link("Hide Thread", "hide_thread", "");
                    md = md.text(" | ").tx_link("Delete Thread", "delete_thread", "");
                } else {
                    md = md.text(" | ").tx_link("Hide Reply", "hide_reply", "");
                    md = md.text(" | ").tx_link("Delete Reply", "delete_reply", "");
                }
                md = md.text(" | ").tx_link("Clear Flags", "clear_flags", "");
                md = md.newline();
            }
        }

        Self::render_footer_into(md).build()
    }

    /// Render board settings page
    fn render_settings(env: &Env, board_id: u64, viewer: &Option<Address>) -> Bytes {
        let permissions: Address = env
            .storage()
            .instance()
            .get(&AdminKey::Permissions)
            .expect("Not initialized");

        let mut md = Self::render_nav(env, board_id)
            .h1("Board Settings");

        // Check if viewer has admin permission
        let can_admin = if let Some(user) = viewer {
            let args: Vec<Val> = Vec::from_array(env, [board_id.into_val(env), user.into_val(env)]);
            let perms: PermissionSet = env.invoke_contract(
                &permissions,
                &Symbol::new(env, "get_permissions"),
                args,
            );
            perms.can_admin
        } else {
            false
        };

        if !can_admin {
            md = md.warning("You must be an admin to view this page.");
            return Self::render_footer_into(md).build();
        }

        // Get flag threshold
        let threshold: u32 = env.invoke_contract(
            &permissions,
            &Symbol::new(env, "get_flag_threshold"),
            Vec::from_array(env, [board_id.into_val(env)]),
        );

        md = md.h2("Moderation Settings")
            .text("**Flag threshold for auto-hide:** ").number(threshold).newline()
            .raw_str("<input type=\"hidden\" name=\"board_id\" value=\"")
            .number(board_id as u32)
            .raw_str("\" />\n")
            .input("threshold", "New threshold (number)")
            .newline()
            .form_link("Update Threshold", "set_flag_threshold")
            .newline()
            .newline();

        md = md.h2("Quick Links")
            .raw_str("[View Members](render:/b/")
            .number(board_id as u32)
            .raw_str("/members)")
            .text(" | ")
            .raw_str("[View Banned](render:/b/")
            .number(board_id as u32)
            .raw_str("/banned)")
            .text(" | ")
            .raw_str("[View Flag Queue](render:/b/")
            .number(board_id as u32)
            .raw_str("/flags)");

        Self::render_footer_into(md).build()
    }

    // ========================================================================
    // Admin Operations (handlers for forms)
    // ========================================================================

    /// Set a user's role on a board (admin+)
    pub fn set_role(
        env: Env,
        board_id: u64,
        user: Address,
        role: Role,
        caller: Address,
    ) {
        caller.require_auth();

        let permissions: Address = env
            .storage()
            .instance()
            .get(&AdminKey::Permissions)
            .expect("Not initialized");

        // Verify caller has admin permissions
        let caller_perms: PermissionSet = env.invoke_contract(
            &permissions,
            &Symbol::new(&env, "get_permissions"),
            Vec::from_array(&env, [board_id.into_val(&env), caller.into_val(&env)]),
        );

        if !caller_perms.can_admin {
            panic!("Caller must be admin or owner");
        }

        // Set the role
        let args: Vec<Val> = Vec::from_array(&env, [
            board_id.into_val(&env),
            user.into_val(&env),
            role.into_val(&env),
        ]);
        env.invoke_contract::<()>(
            &permissions,
            &Symbol::new(&env, "set_role"),
            args,
        );
    }

    /// Ban a user from a board (moderator+)
    pub fn ban_user(
        env: Env,
        board_id: u64,
        user: Address,
        reason: String,
        duration_hours: u64,
        caller: Address,
    ) {
        caller.require_auth();

        let permissions: Address = env
            .storage()
            .instance()
            .get(&AdminKey::Permissions)
            .expect("Not initialized");

        // Verify caller has moderator permissions
        let caller_perms: PermissionSet = env.invoke_contract(
            &permissions,
            &Symbol::new(&env, "get_permissions"),
            Vec::from_array(&env, [board_id.into_val(&env), caller.into_val(&env)]),
        );

        if !caller_perms.can_moderate {
            panic!("Caller must be moderator or higher");
        }

        // Calculate expiry (0 = permanent)
        let expires_at: Option<u64> = if duration_hours > 0 {
            Some(env.ledger().timestamp() + (duration_hours * 3600))
        } else {
            None
        };

        // Ban the user
        let args: Vec<Val> = Vec::from_array(&env, [
            board_id.into_val(&env),
            user.into_val(&env),
            reason.into_val(&env),
            caller.clone().into_val(&env),
            expires_at.into_val(&env),
        ]);
        env.invoke_contract::<()>(
            &permissions,
            &Symbol::new(&env, "ban_user"),
            args,
        );
    }

    /// Unban a user from a board (moderator+)
    pub fn unban_user(
        env: Env,
        board_id: u64,
        user: Address,
        caller: Address,
    ) {
        caller.require_auth();

        let permissions: Address = env
            .storage()
            .instance()
            .get(&AdminKey::Permissions)
            .expect("Not initialized");

        // Verify caller has moderator permissions
        let caller_perms: PermissionSet = env.invoke_contract(
            &permissions,
            &Symbol::new(&env, "get_permissions"),
            Vec::from_array(&env, [board_id.into_val(&env), caller.into_val(&env)]),
        );

        if !caller_perms.can_moderate {
            panic!("Caller must be moderator or higher");
        }

        // Unban the user
        let args: Vec<Val> = Vec::from_array(&env, [
            board_id.into_val(&env),
            user.into_val(&env),
        ]);
        env.invoke_contract::<()>(
            &permissions,
            &Symbol::new(&env, "unban_user"),
            args,
        );
    }

    /// Hide a thread (moderator+)
    pub fn hide_thread(
        env: Env,
        board_id: u64,
        thread_id: u64,
        caller: Address,
    ) {
        caller.require_auth();

        let permissions: Address = env
            .storage()
            .instance()
            .get(&AdminKey::Permissions)
            .expect("Not initialized");
        let registry: Address = env
            .storage()
            .instance()
            .get(&AdminKey::Registry)
            .expect("Not initialized");

        // Verify caller has moderator permissions
        let caller_perms: PermissionSet = env.invoke_contract(
            &permissions,
            &Symbol::new(&env, "get_permissions"),
            Vec::from_array(&env, [board_id.into_val(&env), caller.into_val(&env)]),
        );

        if !caller_perms.can_moderate {
            panic!("Caller must be moderator or higher");
        }

        // Get board contract
        let board_contract: Address = env
            .invoke_contract::<Option<Address>>(
                &registry,
                &Symbol::new(&env, "get_board_contract"),
                Vec::from_array(&env, [board_id.into_val(&env)]),
            )
            .expect("Board contract not found");

        // Hide the thread
        let args: Vec<Val> = Vec::from_array(&env, [
            thread_id.into_val(&env),
            true.into_val(&env),
        ]);
        env.invoke_contract::<()>(
            &board_contract,
            &Symbol::new(&env, "set_thread_hidden"),
            args,
        );
    }

    /// Unhide a thread (moderator+)
    pub fn unhide_thread(
        env: Env,
        board_id: u64,
        thread_id: u64,
        caller: Address,
    ) {
        caller.require_auth();

        let permissions: Address = env
            .storage()
            .instance()
            .get(&AdminKey::Permissions)
            .expect("Not initialized");
        let registry: Address = env
            .storage()
            .instance()
            .get(&AdminKey::Registry)
            .expect("Not initialized");

        // Verify caller has moderator permissions
        let caller_perms: PermissionSet = env.invoke_contract(
            &permissions,
            &Symbol::new(&env, "get_permissions"),
            Vec::from_array(&env, [board_id.into_val(&env), caller.into_val(&env)]),
        );

        if !caller_perms.can_moderate {
            panic!("Caller must be moderator or higher");
        }

        // Get board contract
        let board_contract: Address = env
            .invoke_contract::<Option<Address>>(
                &registry,
                &Symbol::new(&env, "get_board_contract"),
                Vec::from_array(&env, [board_id.into_val(&env)]),
            )
            .expect("Board contract not found");

        // Unhide the thread
        let args: Vec<Val> = Vec::from_array(&env, [
            thread_id.into_val(&env),
            false.into_val(&env),
        ]);
        env.invoke_contract::<()>(
            &board_contract,
            &Symbol::new(&env, "set_thread_hidden"),
            args,
        );
    }

    /// Hide a reply (moderator+)
    pub fn hide_reply(
        env: Env,
        board_id: u64,
        thread_id: u64,
        reply_id: u64,
        caller: Address,
    ) {
        caller.require_auth();

        let permissions: Address = env
            .storage()
            .instance()
            .get(&AdminKey::Permissions)
            .expect("Not initialized");
        let content: Address = env
            .storage()
            .instance()
            .get(&AdminKey::Content)
            .expect("Not initialized");

        // Verify caller has moderator permissions
        let caller_perms: PermissionSet = env.invoke_contract(
            &permissions,
            &Symbol::new(&env, "get_permissions"),
            Vec::from_array(&env, [board_id.into_val(&env), caller.into_val(&env)]),
        );

        if !caller_perms.can_moderate {
            panic!("Caller must be moderator or higher");
        }

        // Hide the reply
        let args: Vec<Val> = Vec::from_array(&env, [
            board_id.into_val(&env),
            thread_id.into_val(&env),
            reply_id.into_val(&env),
            true.into_val(&env),
        ]);
        env.invoke_contract::<()>(
            &content,
            &Symbol::new(&env, "set_reply_hidden"),
            args,
        );
    }

    /// Unhide a reply (moderator+)
    pub fn unhide_reply(
        env: Env,
        board_id: u64,
        thread_id: u64,
        reply_id: u64,
        caller: Address,
    ) {
        caller.require_auth();

        let permissions: Address = env
            .storage()
            .instance()
            .get(&AdminKey::Permissions)
            .expect("Not initialized");
        let content: Address = env
            .storage()
            .instance()
            .get(&AdminKey::Content)
            .expect("Not initialized");

        // Verify caller has moderator permissions
        let caller_perms: PermissionSet = env.invoke_contract(
            &permissions,
            &Symbol::new(&env, "get_permissions"),
            Vec::from_array(&env, [board_id.into_val(&env), caller.into_val(&env)]),
        );

        if !caller_perms.can_moderate {
            panic!("Caller must be moderator or higher");
        }

        // Unhide the reply
        let args: Vec<Val> = Vec::from_array(&env, [
            board_id.into_val(&env),
            thread_id.into_val(&env),
            reply_id.into_val(&env),
            false.into_val(&env),
        ]);
        env.invoke_contract::<()>(
            &content,
            &Symbol::new(&env, "set_reply_hidden"),
            args,
        );
    }

    /// Clear flags on content (moderator+)
    pub fn clear_flags(
        env: Env,
        board_id: u64,
        thread_id: u64,
        reply_id: Option<u64>,
        caller: Address,
    ) {
        caller.require_auth();

        let permissions: Address = env
            .storage()
            .instance()
            .get(&AdminKey::Permissions)
            .expect("Not initialized");
        let content: Address = env
            .storage()
            .instance()
            .get(&AdminKey::Content)
            .expect("Not initialized");

        // Verify caller has moderator permissions
        let caller_perms: PermissionSet = env.invoke_contract(
            &permissions,
            &Symbol::new(&env, "get_permissions"),
            Vec::from_array(&env, [board_id.into_val(&env), caller.into_val(&env)]),
        );

        if !caller_perms.can_moderate {
            panic!("Caller must be moderator or higher");
        }

        // Clear flags
        let args: Vec<Val> = Vec::from_array(&env, [
            board_id.into_val(&env),
            thread_id.into_val(&env),
            reply_id.into_val(&env),
        ]);
        env.invoke_contract::<()>(
            &content,
            &Symbol::new(&env, "clear_flags"),
            args,
        );
    }

    /// Update flag threshold for a board (admin+)
    pub fn set_flag_threshold(
        env: Env,
        board_id: u64,
        threshold: u32,
        caller: Address,
    ) {
        caller.require_auth();

        let permissions: Address = env
            .storage()
            .instance()
            .get(&AdminKey::Permissions)
            .expect("Not initialized");

        // Verify caller has admin permissions
        let caller_perms: PermissionSet = env.invoke_contract(
            &permissions,
            &Symbol::new(&env, "get_permissions"),
            Vec::from_array(&env, [board_id.into_val(&env), caller.into_val(&env)]),
        );

        if !caller_perms.can_admin {
            panic!("Caller must be admin or owner");
        }

        // Set the threshold
        let args: Vec<Val> = Vec::from_array(&env, [
            board_id.into_val(&env),
            threshold.into_val(&env),
            caller.into_val(&env),
        ]);
        env.invoke_contract::<()>(
            &permissions,
            &Symbol::new(&env, "set_flag_threshold"),
            args,
        );
    }

    /// Pin a thread (moderator+)
    pub fn pin_thread(
        env: Env,
        board_id: u64,
        thread_id: u64,
        caller: Address,
    ) {
        caller.require_auth();

        let permissions: Address = env
            .storage()
            .instance()
            .get(&AdminKey::Permissions)
            .expect("Not initialized");
        let registry: Address = env
            .storage()
            .instance()
            .get(&AdminKey::Registry)
            .expect("Not initialized");

        // Verify caller has moderator permissions
        let caller_perms: PermissionSet = env.invoke_contract(
            &permissions,
            &Symbol::new(&env, "get_permissions"),
            Vec::from_array(&env, [board_id.into_val(&env), caller.into_val(&env)]),
        );

        if !caller_perms.can_moderate {
            panic!("Caller must be moderator or higher");
        }

        // Get board contract
        let board_contract: Address = env
            .invoke_contract::<Option<Address>>(
                &registry,
                &Symbol::new(&env, "get_board_contract"),
                Vec::from_array(&env, [board_id.into_val(&env)]),
            )
            .expect("Board contract not found");

        // Pin the thread
        let args: Vec<Val> = Vec::from_array(&env, [
            thread_id.into_val(&env),
            true.into_val(&env),
        ]);
        env.invoke_contract::<()>(
            &board_contract,
            &Symbol::new(&env, "set_thread_pinned"),
            args,
        );
    }

    /// Unpin a thread (moderator+)
    pub fn unpin_thread(
        env: Env,
        board_id: u64,
        thread_id: u64,
        caller: Address,
    ) {
        caller.require_auth();

        let permissions: Address = env
            .storage()
            .instance()
            .get(&AdminKey::Permissions)
            .expect("Not initialized");
        let registry: Address = env
            .storage()
            .instance()
            .get(&AdminKey::Registry)
            .expect("Not initialized");

        // Verify caller has moderator permissions
        let caller_perms: PermissionSet = env.invoke_contract(
            &permissions,
            &Symbol::new(&env, "get_permissions"),
            Vec::from_array(&env, [board_id.into_val(&env), caller.into_val(&env)]),
        );

        if !caller_perms.can_moderate {
            panic!("Caller must be moderator or higher");
        }

        // Get board contract
        let board_contract: Address = env
            .invoke_contract::<Option<Address>>(
                &registry,
                &Symbol::new(&env, "get_board_contract"),
                Vec::from_array(&env, [board_id.into_val(&env)]),
            )
            .expect("Board contract not found");

        // Unpin the thread
        let args: Vec<Val> = Vec::from_array(&env, [
            thread_id.into_val(&env),
            false.into_val(&env),
        ]);
        env.invoke_contract::<()>(
            &board_contract,
            &Symbol::new(&env, "set_thread_pinned"),
            args,
        );
    }

    /// Lock a thread (moderator+)
    pub fn lock_thread(
        env: Env,
        board_id: u64,
        thread_id: u64,
        caller: Address,
    ) {
        caller.require_auth();

        let permissions: Address = env
            .storage()
            .instance()
            .get(&AdminKey::Permissions)
            .expect("Not initialized");
        let registry: Address = env
            .storage()
            .instance()
            .get(&AdminKey::Registry)
            .expect("Not initialized");

        // Verify caller has moderator permissions
        let caller_perms: PermissionSet = env.invoke_contract(
            &permissions,
            &Symbol::new(&env, "get_permissions"),
            Vec::from_array(&env, [board_id.into_val(&env), caller.into_val(&env)]),
        );

        if !caller_perms.can_moderate {
            panic!("Caller must be moderator or higher");
        }

        // Get board contract
        let board_contract: Address = env
            .invoke_contract::<Option<Address>>(
                &registry,
                &Symbol::new(&env, "get_board_contract"),
                Vec::from_array(&env, [board_id.into_val(&env)]),
            )
            .expect("Board contract not found");

        // Lock the thread
        let args: Vec<Val> = Vec::from_array(&env, [
            thread_id.into_val(&env),
            true.into_val(&env),
        ]);
        env.invoke_contract::<()>(
            &board_contract,
            &Symbol::new(&env, "set_thread_locked"),
            args,
        );
    }

    /// Unlock a thread (moderator+)
    pub fn unlock_thread(
        env: Env,
        board_id: u64,
        thread_id: u64,
        caller: Address,
    ) {
        caller.require_auth();

        let permissions: Address = env
            .storage()
            .instance()
            .get(&AdminKey::Permissions)
            .expect("Not initialized");
        let registry: Address = env
            .storage()
            .instance()
            .get(&AdminKey::Registry)
            .expect("Not initialized");

        // Verify caller has moderator permissions
        let caller_perms: PermissionSet = env.invoke_contract(
            &permissions,
            &Symbol::new(&env, "get_permissions"),
            Vec::from_array(&env, [board_id.into_val(&env), caller.into_val(&env)]),
        );

        if !caller_perms.can_moderate {
            panic!("Caller must be moderator or higher");
        }

        // Get board contract
        let board_contract: Address = env
            .invoke_contract::<Option<Address>>(
                &registry,
                &Symbol::new(&env, "get_board_contract"),
                Vec::from_array(&env, [board_id.into_val(&env)]),
            )
            .expect("Board contract not found");

        // Unlock the thread
        let args: Vec<Val> = Vec::from_array(&env, [
            thread_id.into_val(&env),
            false.into_val(&env),
        ]);
        env.invoke_contract::<()>(
            &board_contract,
            &Symbol::new(&env, "set_thread_locked"),
            args,
        );
    }

    /// Delete a thread (moderator+)
    pub fn delete_thread(
        env: Env,
        board_id: u64,
        thread_id: u64,
        caller: Address,
    ) {
        caller.require_auth();

        let permissions: Address = env
            .storage()
            .instance()
            .get(&AdminKey::Permissions)
            .expect("Not initialized");
        let registry: Address = env
            .storage()
            .instance()
            .get(&AdminKey::Registry)
            .expect("Not initialized");

        // Verify caller has moderator permissions
        let caller_perms: PermissionSet = env.invoke_contract(
            &permissions,
            &Symbol::new(&env, "get_permissions"),
            Vec::from_array(&env, [board_id.into_val(&env), caller.clone().into_val(&env)]),
        );

        if !caller_perms.can_moderate {
            panic!("Caller must be moderator or higher");
        }

        // Get board contract
        let board_contract: Address = env
            .invoke_contract::<Option<Address>>(
                &registry,
                &Symbol::new(&env, "get_board_contract"),
                Vec::from_array(&env, [board_id.into_val(&env)]),
            )
            .expect("Board contract not found");

        // Delete the thread (soft delete)
        let args: Vec<Val> = Vec::from_array(&env, [
            thread_id.into_val(&env),
            caller.into_val(&env),
        ]);
        env.invoke_contract::<()>(
            &board_contract,
            &Symbol::new(&env, "delete_thread"),
            args,
        );
    }

    /// Delete a reply (moderator+)
    pub fn delete_reply(
        env: Env,
        board_id: u64,
        thread_id: u64,
        reply_id: u64,
        caller: Address,
    ) {
        caller.require_auth();

        let permissions: Address = env
            .storage()
            .instance()
            .get(&AdminKey::Permissions)
            .expect("Not initialized");
        let content: Address = env
            .storage()
            .instance()
            .get(&AdminKey::Content)
            .expect("Not initialized");

        // Verify caller has moderator permissions
        let caller_perms: PermissionSet = env.invoke_contract(
            &permissions,
            &Symbol::new(&env, "get_permissions"),
            Vec::from_array(&env, [board_id.into_val(&env), caller.clone().into_val(&env)]),
        );

        if !caller_perms.can_moderate {
            panic!("Caller must be moderator or higher");
        }

        // Delete the reply (soft delete via content contract)
        let args: Vec<Val> = Vec::from_array(&env, [
            board_id.into_val(&env),
            thread_id.into_val(&env),
            reply_id.into_val(&env),
            caller.into_val(&env),
        ]);
        env.invoke_contract::<()>(
            &content,
            &Symbol::new(&env, "delete_reply"),
            args,
        );
    }

    /// Upgrade the contract WASM
    pub fn upgrade(env: Env, new_wasm_hash: BytesN<32>) {
        let registry: Address = env
            .storage()
            .instance()
            .get(&AdminKey::Registry)
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

        let contract_id = env.register(BoardsAdmin, ());
        let client = BoardsAdminClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        let permissions = Address::generate(&env);
        let content = Address::generate(&env);
        let theme = Address::generate(&env);

        client.init(&registry, &permissions, &content, &theme);

        assert_eq!(client.get_registry(), registry);
        assert_eq!(client.get_permissions(), permissions);
        assert_eq!(client.get_content(), content);
        assert_eq!(client.get_theme(), theme);
    }

    #[test]
    #[should_panic(expected = "Already initialized")]
    fn test_double_init() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsAdmin, ());
        let client = BoardsAdminClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        let permissions = Address::generate(&env);
        let content = Address::generate(&env);
        let theme = Address::generate(&env);

        client.init(&registry, &permissions, &content, &theme);
        // Should panic on second init
        client.init(&registry, &permissions, &content, &theme);
    }

    #[test]
    fn test_render_not_found() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsAdmin, ());
        let client = BoardsAdminClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        let permissions = Address::generate(&env);
        let content = Address::generate(&env);
        let theme = Address::generate(&env);

        client.init(&registry, &permissions, &content, &theme);

        // Render unknown path
        let path = String::from_str(&env, "/unknown");
        let html = client.render(&Some(path), &None);
        assert!(html.len() > 0);
    }
}
