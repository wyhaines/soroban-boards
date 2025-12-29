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

/// Invite request from permissions contract
#[contracttype]
#[derive(Clone)]
pub struct InviteRequest {
    pub user: Address,
    pub board_id: u64,
    pub created_at: u64,
}

/// Flair definition from board contract
#[contracttype]
#[derive(Clone)]
pub struct FlairDef {
    pub id: u32,
    pub name: String,
    pub color: String,
    pub bg_color: String,
    pub required: bool,
    pub mod_only: bool,
    pub enabled: bool,
}

/// Voting configuration from voting contract
#[contracttype]
#[derive(Clone)]
pub struct VotingConfig {
    pub enabled: bool,
    pub allow_downvotes: bool,
    pub karma_enabled: bool,
    pub karma_multiplier: u32,
}

/// Community metadata from community contract
#[contracttype]
#[derive(Clone)]
pub struct CommunityMeta {
    pub id: u64,
    pub name: String,
    pub display_name: String,
    pub description: String,
    pub owner: Address,
    pub created_at: u64,
    pub board_count: u64,
    pub member_count: u64,
    pub is_private: bool,
}

#[contract]
pub struct BoardsAdmin;

/// Helper function to parse a Soroban String to u32
/// Panics if the string contains non-digit characters
fn parse_string_to_u32(_env: &Env, s: &String) -> u32 {
    let len = s.len() as usize;
    if len == 0 {
        panic!("Empty string");
    }
    if len > 10 {
        panic!("Number too large");
    }

    let mut buf = [0u8; 10];
    s.copy_into_slice(&mut buf[..len]);

    let mut result: u32 = 0;
    for i in 0..len {
        let byte = buf[i];
        if byte < b'0' || byte > b'9' {
            panic!("Invalid number format");
        }
        result = result * 10 + (byte - b'0') as u32;
    }
    result
}

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
            // Board admin routes (without /admin prefix - for direct access)
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
            .or_handle(b"/b/{id}/invites", |req| {
                let board_id = req.get_var_u32(b"id").unwrap_or(0) as u64;
                Self::render_invites(&env, board_id, &viewer)
            })
            .or_handle(b"/b/{id}/settings", |req| {
                let board_id = req.get_var_u32(b"id").unwrap_or(0) as u64;
                Self::render_settings(&env, board_id, &viewer)
            })
            .or_handle(b"/b/{id}/flairs", |req| {
                let board_id = req.get_var_u32(b"id").unwrap_or(0) as u64;
                Self::render_flairs(&env, board_id, &viewer)
            })
            .or_handle(b"/b/{id}/rules", |req| {
                let board_id = req.get_var_u32(b"id").unwrap_or(0) as u64;
                Self::render_rules(&env, board_id, &viewer)
            })
            .or_handle(b"/b/{id}/voting", |req| {
                let board_id = req.get_var_u32(b"id").unwrap_or(0) as u64;
                Self::render_voting(&env, board_id, &viewer)
            })
            // Board admin routes (with /admin prefix - canonical URLs)
            .or_handle(b"/admin/b/{id}/members", |req| {
                let board_id = req.get_var_u32(b"id").unwrap_or(0) as u64;
                Self::render_members(&env, board_id, &viewer)
            })
            .or_handle(b"/admin/b/{id}/banned", |req| {
                let board_id = req.get_var_u32(b"id").unwrap_or(0) as u64;
                Self::render_banned(&env, board_id, &viewer)
            })
            .or_handle(b"/admin/b/{id}/flags", |req| {
                let board_id = req.get_var_u32(b"id").unwrap_or(0) as u64;
                Self::render_flag_queue(&env, board_id, &viewer)
            })
            .or_handle(b"/admin/b/{id}/invites", |req| {
                let board_id = req.get_var_u32(b"id").unwrap_or(0) as u64;
                Self::render_invites(&env, board_id, &viewer)
            })
            .or_handle(b"/admin/b/{id}/settings", |req| {
                let board_id = req.get_var_u32(b"id").unwrap_or(0) as u64;
                Self::render_settings(&env, board_id, &viewer)
            })
            .or_handle(b"/admin/b/{id}/flairs", |req| {
                let board_id = req.get_var_u32(b"id").unwrap_or(0) as u64;
                Self::render_flairs(&env, board_id, &viewer)
            })
            .or_handle(b"/b/{id}/flairs/{flair_id}/edit", |req| {
                let board_id = req.get_var_u32(b"id").unwrap_or(0) as u64;
                let flair_id = req.get_var_u32(b"flair_id").unwrap_or(0);
                Self::render_flair_edit(&env, board_id, flair_id, &viewer)
            })
            .or_handle(b"/admin/b/{id}/flairs/{flair_id}/edit", |req| {
                let board_id = req.get_var_u32(b"id").unwrap_or(0) as u64;
                let flair_id = req.get_var_u32(b"flair_id").unwrap_or(0);
                Self::render_flair_edit(&env, board_id, flair_id, &viewer)
            })
            .or_handle(b"/admin/b/{id}/rules", |req| {
                let board_id = req.get_var_u32(b"id").unwrap_or(0) as u64;
                Self::render_rules(&env, board_id, &viewer)
            })
            .or_handle(b"/admin/b/{id}/voting", |req| {
                let board_id = req.get_var_u32(b"id").unwrap_or(0) as u64;
                Self::render_voting(&env, board_id, &viewer)
            })
            // Registry admin routes
            .or_handle(b"/registry", |_| {
                Self::render_registry_admin(&env, &viewer)
            })
            .or_handle(b"/admin/registry", |_| {
                Self::render_registry_admin(&env, &viewer)
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

    /// Render the navigation bar for sub-pages (includes Back to Settings link)
    fn render_nav_subpage(env: &Env, board_id: u64) -> MarkdownBuilder<'_> {
        MarkdownBuilder::new(env)
            .render_link("Soroban Boards", "/")
            .text(" | ")
            .raw_str("[Back to Board](render:/b/")
            .number(board_id as u32)
            .raw_str(")")
            .text(" | ")
            .raw_str("[Back to Settings](render:/admin/b/")
            .number(board_id as u32)
            .raw_str("/settings)")
            .newline()
            .hr()
    }

    /// Append footer to builder
    fn render_footer_into(md: MarkdownBuilder<'_>) -> MarkdownBuilder<'_> {
        md.newline()
            .newline()
            .hr()
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

        let mut md = Self::render_nav_subpage(env, board_id)
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
                md = md.text(" ").tx_link_to("[Demote]", "admin", "remove_admin", "");
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
                md = md.text(" ").tx_link_to("[Demote]", "admin", "remove_moderator", "");
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
                md = md.text(" ").tx_link_to("[Promote to Mod]", "admin", "set_moderator", "");
                md = md.text(" ").tx_link_to("[Remove]", "admin", "remove_member", "");
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
            .form_link_to("Add as Member", "admin", "add_member")
            .text(" ")
            .form_link_to("Add as Moderator", "admin", "add_moderator")
            .text(" ")
            .form_link_to("Add as Admin", "admin", "add_admin");

        Self::render_footer_into(md).build()
    }

    /// Render banned users page
    fn render_banned(env: &Env, board_id: u64, viewer: &Option<Address>) -> Bytes {
        let permissions: Address = env
            .storage()
            .instance()
            .get(&AdminKey::Permissions)
            .expect("Not initialized");

        let mut md = Self::render_nav_subpage(env, board_id)
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
                    .tx_link_to("Unban User", "admin", "unban_user", "")
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
            .form_link_to("Ban User", "admin", "ban_user");

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

        let mut md = Self::render_nav_subpage(env, board_id)
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
                    md = md.text(" | ").tx_link_to("Hide Thread", "admin", "hide_thread", "");
                    md = md.text(" | ").tx_link_to("Delete Thread", "admin", "delete_thread", "");
                } else {
                    md = md.text(" | ").tx_link_to("Hide Reply", "admin", "hide_reply", "");
                    md = md.text(" | ").tx_link_to("Delete Reply", "admin", "delete_reply", "");
                }
                md = md.text(" | ").tx_link_to("Clear Flags", "admin", "clear_flags", "");
                md = md.newline();
            }
        }

        Self::render_footer_into(md).build()
    }

    /// Render invite requests page
    fn render_invites(env: &Env, board_id: u64, viewer: &Option<Address>) -> Bytes {
        let permissions: Address = env
            .storage()
            .instance()
            .get(&AdminKey::Permissions)
            .expect("Not initialized");

        let mut md = Self::render_nav_subpage(env, board_id)
            .h1("Invite Requests");

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

        // Fetch pending invite requests
        let requests: Vec<InviteRequest> = env.invoke_contract(
            &permissions,
            &Symbol::new(env, "list_invite_requests"),
            Vec::from_array(env, [board_id.into_val(env)]),
        );

        if requests.is_empty() {
            md = md.tip("No pending invite requests.");
        } else {
            md = md.paragraph("Users requesting to join this board:");

            for i in 0..requests.len() {
                let request = requests.get(i).unwrap();
                let user_str = Self::format_address(env, &request.user);

                md = md.hr()
                    .h3("").text("`").text_string(&user_str).text("`")
                    .newline()
                    .text("**Requested:** ").number(request.created_at as u32).text(" (timestamp)")
                    .newline()
                    // Hidden fields for actions
                    .raw_str("<input type=\"hidden\" name=\"board_id\" value=\"")
                    .number(board_id as u32)
                    .raw_str("\" />\n")
                    .raw_str("<input type=\"hidden\" name=\"user\" value=\"")
                    .text_string(&user_str)
                    .raw_str("\" />\n")
                    // Action buttons
                    .tx_link_to("Accept", "admin", "accept_invite", "")
                    .text(" | ")
                    .tx_link_to("Reject", "admin", "revoke_invite", "")
                    .newline();
            }
        }

        // Direct invite form
        md = md.hr()
            .h3("Directly Invite a User")
            .paragraph("Invite a user without requiring them to request access.")
            .raw_str("<input type=\"hidden\" name=\"board_id\" value=\"")
            .number(board_id as u32)
            .raw_str("\" />\n")
            .input("user", "Wallet address (G...)")
            .newline()
            .form_link_to("Invite as Member", "admin", "invite_member")
            .text(" ")
            .form_link_to("Invite as Moderator", "admin", "invite_moderator")
            .text(" ")
            .form_link_to("Invite as Admin", "admin", "invite_admin");

        // Link back to members page
        md = md.hr()
            .raw_str("[View All Members](render:/admin/b/")
            .number(board_id as u32)
            .raw_str("/members)");

        Self::render_footer_into(md).build()
    }

    /// Render board settings page
    fn render_settings(env: &Env, board_id: u64, viewer: &Option<Address>) -> Bytes {
        let permissions: Address = env
            .storage()
            .instance()
            .get(&AdminKey::Permissions)
            .expect("Not initialized");

        // Get registry for board info (needed for title)
        let registry: Address = env
            .storage()
            .instance()
            .get(&AdminKey::Registry)
            .expect("Not initialized");

        // Get board metadata for title
        let board_opt: Option<BoardMeta> = env.invoke_contract(
            &registry,
            &Symbol::new(env, "get_board"),
            Vec::from_array(env, [board_id.into_val(env)]),
        );

        // Build title with board name
        let mut md = Self::render_nav(env, board_id);
        if let Some(ref board) = board_opt {
            md = md.h1("Settings: ").text_string(&board.name).newline().newline();
        } else {
            md = md.h1("Board Settings");
        }

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

        if let Some(board) = board_opt {
            md = md.h2("Board Name")
                .text("**Current name:** ").text_string(&board.name).newline();

            // Get aliases
            let aliases: Vec<String> = env.invoke_contract(
                &registry,
                &Symbol::new(env, "get_board_aliases"),
                Vec::from_array(env, [board_id.into_val(env)]),
            );

            if !aliases.is_empty() {
                md = md.text("**Aliases:** ");
                for i in 0..aliases.len() {
                    if i > 0 {
                        md = md.text(", ");
                    }
                    md = md.text_string(&aliases.get(i).unwrap());
                }
                md = md.newline();
            }

            md = md.newline()
                .note("Rename the board. The old name will become an alias that continues to work.")
                .raw_str("<input type=\"hidden\" name=\"board_id\" value=\"")
                .number(board_id as u32)
                .raw_str("\" />\n")
                .input("new_name", "New board name (3-50 chars, letters/numbers/-/_)")
                .newline()
                .form_link_to("Rename Board", "admin", "rename_board")
                .newline()
                .newline();
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
            .form_link_to("Update Threshold", "admin", "set_flag_threshold")
            .newline()
            .newline();

        // Get board contract for chunk size and other settings
        let board_contract: Option<Address> = env.invoke_contract(
            &registry,
            &Symbol::new(env, "get_board_contract"),
            Vec::from_array(env, [board_id.into_val(env)]),
        );

        if let Some(board_addr) = board_contract {
            let chunk_size: u32 = env.invoke_contract(
                &board_addr,
                &Symbol::new(env, "get_chunk_size"),
                Vec::new(env),
            );

            md = md.h2("Display Settings")
                .text("**Reply chunk size:** ").number(chunk_size).text(" replies per batch").newline()
                .newline()
                .note("Controls how many replies load at once in waterfall loading. Lower values load faster but require more scrolling.")
                .raw_str("<input type=\"hidden\" name=\"board_id\" value=\"")
                .number(board_id as u32)
                .raw_str("\" />\n")
                .input("chunk_size", "New chunk size (1-20)")
                .newline()
                .form_link_to("Update Chunk Size", "admin", "set_chunk_size")
                .newline()
                .newline();

            // Max reply depth setting
            let max_depth: u32 = env.invoke_contract(
                &board_addr,
                &Symbol::new(env, "get_max_reply_depth"),
                Vec::new(env),
            );

            md = md.h2("Reply Threading")
                .text("**Maximum reply depth:** ").number(max_depth).text(" levels").newline()
                .newline()
                .note("Controls how deeply nested replies can be. Replies at the maximum depth cannot have children. Setting this lower helps keep discussions focused.")
                .raw_str("<input type=\"hidden\" name=\"board_id\" value=\"")
                .number(board_id as u32)
                .raw_str("\" />\n")
                .input("max_depth", "New max depth (1-20)")
                .newline()
                .form_link_to("Update Max Depth", "admin", "set_max_reply_depth")
                .newline()
                .newline();

            // Edit window setting
            let edit_window: u64 = env.invoke_contract(
                &board_addr,
                &Symbol::new(env, "get_edit_window"),
                Vec::new(env),
            );

            // Convert seconds to hours for display
            let edit_hours = if edit_window == 0 { 0 } else { edit_window / 3600 };

            md = md.h2("Content Editing")
                .text("**Edit window:** ");

            if edit_window == 0 {
                md = md.text("No limit (users can always edit their content)").newline();
            } else {
                md = md.number(edit_hours as u32).text(" hours").newline();
            }

            md = md.newline()
                .note("Controls how long users can edit their posts after creation. Moderators can always edit.")
                .raw_str("<input type=\"hidden\" name=\"board_id\" value=\"")
                .number(board_id as u32)
                .raw_str("\" />\n")
                .input("edit_hours", "Edit window in hours (0 = no limit)")
                .newline()
                .form_link_to("Update Edit Window", "admin", "set_edit_window")
                .newline()
                .newline();
        }

        // Board visibility setting
        let is_listed: bool = env.invoke_contract(
            &registry,
            &Symbol::new(env, "get_board_listed"),
            Vec::from_array(env, [board_id.into_val(env)]),
        );

        md = md.h2("Board Visibility")
            .text("**Listed publicly:** ");

        if is_listed {
            md = md.text("Yes (appears on home page)").newline();
        } else {
            md = md.text("No (hidden from home page, accessible via direct link)").newline();
        }

        md = md.newline()
            .note("Controls whether this board appears in the public board list on the home page. Unlisted boards are still accessible via direct link.")
            .raw_str("<input type=\"hidden\" name=\"board_id\" value=\"")
            .number(board_id as u32)
            .raw_str("\" />\n");

        if is_listed {
            md = md.form_link_to("Hide from Public List", "admin", "unlist_board");
        } else {
            md = md.form_link_to("Show on Public List", "admin", "list_board");
        }

        md = md.newline()
            .newline();

        // Board access control (public/private)
        let is_private: bool = env.invoke_contract(
            &registry,
            &Symbol::new(env, "get_board_private"),
            Vec::from_array(env, [board_id.into_val(env)]),
        );

        md = md.h2("Access Control")
            .text("**Board type:** ");

        if is_private {
            md = md.text("Private (members only)").newline();
        } else {
            md = md.text("Public (anyone can view and post)").newline();
        }

        md = md.newline()
            .note("Controls who can access this board. Private boards require membership to view or post content.")
            .raw_str("<input type=\"hidden\" name=\"board_id\" value=\"")
            .number(board_id as u32)
            .raw_str("\" />\n");

        if is_private {
            md = md.form_link_to("Make Public", "admin", "make_public");
        } else {
            md = md.form_link_to("Make Private", "admin", "make_private");
        }

        md = md.newline()
            .newline();

        // Board read-only status - query board contract directly
        let board_contract: Address = env.invoke_contract::<Option<Address>>(
            &registry,
            &Symbol::new(env, "get_board_contract"),
            Vec::from_array(env, [board_id.into_val(env)]),
        ).expect("Board contract not found");

        let is_readonly: bool = env.invoke_contract(
            &board_contract,
            &Symbol::new(env, "is_readonly"),
            Vec::new(env),
        );

        md = md.h2("Posting Status")
            .text("**Current status:** ");

        if is_readonly {
            md = md.text("Read-only (no new posts allowed)").newline();
        } else {
            md = md.text("Open (members can post)").newline();
        }

        md = md.newline()
            .note("Controls whether new threads and replies can be created. Use read-only mode to archive a board.")
            .raw_str("<input type=\"hidden\" name=\"board_id\" value=\"")
            .number(board_id as u32)
            .raw_str("\" />\n");

        if is_readonly {
            md = md.form_link_to("Enable Posting", "admin", "enable_posting");
        } else {
            md = md.form_link_to("Make Read-Only", "admin", "make_readonly");
        }

        md = md.newline()
            .newline();

        // Community Management Section
        md = md.h2("Community");

        // Get community contract address from registry
        let community_contract_opt: Option<Address> = env.invoke_contract(
            &registry,
            &Symbol::new(env, "get_community_contract"),
            Vec::new(env),
        );

        if let Some(community_contract) = community_contract_opt {
            // Get current community association
            let community_id_opt: Option<u64> = env.invoke_contract(
                &registry,
                &Symbol::new(env, "get_board_community"),
                Vec::from_array(env, [board_id.into_val(env)]),
            );

            if let Some(community_id) = community_id_opt {
                // Board is in a community - get community info
                let comm_args: Vec<Val> = Vec::from_array(env, [community_id.into_val(env)]);
                let comm_opt: Option<CommunityMeta> = env.invoke_contract(
                    &community_contract,
                    &Symbol::new(env, "get_community"),
                    comm_args,
                );

                if let Some(community) = comm_opt {
                    md = md.text("**Current Community:** ")
                        .raw_str("<a href=\"render:/c/")
                        .text_string(&community.name)
                        .raw_str("\">")
                        .text_string(&community.display_name)
                        .raw_str("</a>")
                        .newline()
                        .newline()
                        .note("Leaving a community makes this a standalone board.")
                        .raw_str("<input type=\"hidden\" name=\"board_id\" value=\"")
                        .number(board_id as u32)
                        .raw_str("\" />\n")
                        .raw_str("<input type=\"hidden\" name=\"caller\" value=\"")
                        .text_string(&viewer.as_ref().unwrap().to_string())
                        .raw_str("\" />\n")
                        .raw_str("<input type=\"hidden\" name=\"_redirect\" value=\"/b/")
                        .number(board_id as u32)
                        .raw_str("/settings\" />\n")
                        .form_link_to("Leave Community", "registry", "remove_board_from_community")
                        .newline();
                } else {
                    md = md.text("**Status:** In community (ID: ")
                        .number(community_id as u32)
                        .text(")")
                        .newline();
                }
            } else {
                // Board is standalone - show option to move to a community
                md = md.text("**Status:** Standalone board (not in a community)")
                    .newline()
                    .newline();

                // Get communities the user owns
                let count: u64 = env.invoke_contract(
                    &community_contract,
                    &Symbol::new(env, "community_count"),
                    Vec::new(env),
                );

                let mut user_communities: Vec<CommunityMeta> = Vec::new(env);
                for i in 0..count {
                    let comm_args: Vec<Val> = Vec::from_array(env, [i.into_val(env)]);
                    let comm_opt: Option<CommunityMeta> = env.invoke_contract(
                        &community_contract,
                        &Symbol::new(env, "get_community"),
                        comm_args,
                    );
                    if let Some(community) = comm_opt {
                        if community.owner == *viewer.as_ref().unwrap() {
                            user_communities.push_back(community);
                        }
                    }
                }

                if user_communities.is_empty() {
                    md = md.tip("Create a community to organize your boards together.")
                        .raw_str("<a href=\"render:/new\">Create Community</a>\n");
                } else {
                    // Note: Form inputs ordered to match function signature:
                    // move_board_to_community(board_id, community_id, caller)
                    md = md.note("Move this board to one of your communities.")
                        .raw_str("<input type=\"hidden\" name=\"board_id\" value=\"")
                        .number(board_id as u32)
                        .raw_str("\" />\n")
                        .raw_str("<label>Select Community:</label>\n")
                        .raw_str("<select name=\"community_id\">\n");

                    for i in 0..user_communities.len() {
                        let comm = user_communities.get(i).unwrap();
                        md = md.raw_str("<option value=\"")
                            .number(comm.id as u32)
                            .raw_str("\">")
                            .text_string(&comm.display_name)
                            .raw_str("</option>\n");
                    }

                    md = md.raw_str("</select>\n")
                        .raw_str("<input type=\"hidden\" name=\"caller\" value=\"")
                        .text_string(&viewer.as_ref().unwrap().to_string())
                        .raw_str("\" />\n")
                        .raw_str("<input type=\"hidden\" name=\"_redirect\" value=\"/b/")
                        .number(board_id as u32)
                        .raw_str("/settings\" />\n")
                        .newline()
                        .form_link_to("Move to Community", "registry", "move_board_to_community")
                        .newline();
                }
            }
        } else {
            md = md.text("Community features not available.")
                .newline();
        }

        md = md.newline();

        md = md.h2("Quick Links")
            .raw_str("[View Members](render:/admin/b/")
            .number(board_id as u32)
            .raw_str("/members)")
            .text(" | ")
            .raw_str("[View Invites](render:/admin/b/")
            .number(board_id as u32)
            .raw_str("/invites)")
            .text(" | ")
            .raw_str("[View Banned](render:/admin/b/")
            .number(board_id as u32)
            .raw_str("/banned)")
            .text(" | ")
            .raw_str("[View Flag Queue](render:/admin/b/")
            .number(board_id as u32)
            .raw_str("/flags)")
            .newline()
            .raw_str("[Manage Flairs](render:/admin/b/")
            .number(board_id as u32)
            .raw_str("/flairs)")
            .text(" | ")
            .raw_str("[Edit Rules](render:/admin/b/")
            .number(board_id as u32)
            .raw_str("/rules)")
            .text(" | ")
            .raw_str("[Voting Config](render:/admin/b/")
            .number(board_id as u32)
            .raw_str("/voting)");

        Self::render_footer_into(md).build()
    }

    /// Render flair management page
    fn render_flairs(env: &Env, board_id: u64, viewer: &Option<Address>) -> Bytes {
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

        let mut md = Self::render_nav_subpage(env, board_id)
            .h1("Flair Management");

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
            md = md.warning("You must be an admin to manage flairs.");
            return Self::render_footer_into(md).build();
        }

        // Get board contract
        let board_contract: Option<Address> = env.invoke_contract(
            &registry,
            &Symbol::new(env, "get_board_contract"),
            Vec::from_array(env, [board_id.into_val(env)]),
        );

        if let Some(board_addr) = board_contract {
            // Get existing flairs
            let flairs: Vec<FlairDef> = env.invoke_contract(
                &board_addr,
                &Symbol::new(env, "list_flairs"),
                Vec::new(env),
            );

            md = md.h2("Current Flairs");

            if flairs.is_empty() {
                md = md.paragraph("No flairs defined yet. Create one below.");
            } else {
                for i in 0..flairs.len() {
                    let flair = flairs.get(i).unwrap();
                    md = md.hr()
                        .raw_str("<div class=\"flair-preview\" style=\"display: inline-block; padding: 2px 8px; border-radius: 3px; background: ")
                        .text_string(&flair.bg_color)
                        .raw_str("; color: ")
                        .text_string(&flair.color)
                        .raw_str(";\">")
                        .text_string(&flair.name)
                        .raw_str("</div>\n\n")
                        .raw_str("- **ID:** ").number(flair.id).newline()
                        .raw_str("- **Enabled:** ").text(if flair.enabled { "Yes" } else { "No" }).newline()
                        .raw_str("- **Required:** ").text(if flair.required { "Yes" } else { "No" }).newline()
                        .raw_str("- **Mod Only:** ").text(if flair.mod_only { "Yes" } else { "No" }).newline()
                        .newline()
                        // Edit button - links to separate edit page
                        .raw_str("[Edit](render:/admin/b/")
                        .number(board_id as u32)
                        .raw_str("/flairs/")
                        .number(flair.id)
                        .raw_str("/edit)");

                    // Disable button (only for enabled flairs)
                    if flair.enabled {
                        md = md.text(" | ")
                            .raw_str("<input type=\"hidden\" name=\"board_id\" value=\"")
                            .number(board_id as u32)
                            .raw_str("\" />\n")
                            .raw_str("<input type=\"hidden\" name=\"flair_id\" value=\"")
                            .number(flair.id)
                            .raw_str("\" />\n")
                            .form_link_to("Disable", "admin", "disable_flair");
                    }

                    md = md.newline();
                }
            }

            // Create new flair form
            md = md.hr()
                .h2("Create New Flair")
                .raw_str("<input type=\"hidden\" name=\"board_id\" value=\"")
                .number(board_id as u32)
                .raw_str("\" />\n")
                .input("name", "Flair name (max 32 chars)")
                .newline()
                .input("color", "Text color (e.g., #ffffff)")
                .newline()
                .input("bg_color", "Background color (e.g., #ff4500)")
                .newline()
                // Hidden inputs provide default "false" when checkbox is unchecked
                .raw_str("<input type=\"hidden\" name=\"required\" value=\"false\" />\n")
                .raw_str("<label><input type=\"checkbox\" name=\"required\" value=\"true\" /> Required for new posts</label>\n")
                .raw_str("<input type=\"hidden\" name=\"mod_only\" value=\"false\" />\n")
                .raw_str("<label><input type=\"checkbox\" name=\"mod_only\" value=\"true\" /> Moderator only</label>\n")
                .form_link_to("Create Flair", "admin", "create_flair");
        } else {
            md = md.warning("Board contract not found.");
        }

        Self::render_footer_into(md).build()
    }

    /// Render flair edit page
    fn render_flair_edit(env: &Env, board_id: u64, flair_id: u32, viewer: &Option<Address>) -> Bytes {
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

        let mut md = Self::render_nav_subpage(env, board_id)
            .h1("Edit Flair");

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
            md = md.warning("You must be an admin to edit flairs.");
            return Self::render_footer_into(md).build();
        }

        // Get board contract
        let board_contract: Option<Address> = env.invoke_contract(
            &registry,
            &Symbol::new(env, "get_board_contract"),
            Vec::from_array(env, [board_id.into_val(env)]),
        );

        if let Some(board_addr) = board_contract {
            // Get the flair
            let flair_opt: Option<FlairDef> = env.invoke_contract(
                &board_addr,
                &Symbol::new(env, "get_flair"),
                Vec::from_array(env, [flair_id.into_val(env)]),
            );

            if let Some(flair) = flair_opt {
                // Show current flair preview
                md = md
                    .raw_str("<div class=\"flair-preview\" style=\"display: inline-block; padding: 2px 8px; border-radius: 3px; margin-bottom: 16px; background: ")
                    .text_string(&flair.bg_color)
                    .raw_str("; color: ")
                    .text_string(&flair.color)
                    .raw_str(";\">")
                    .text_string(&flair.name)
                    .raw_str("</div>\n\n")
                    .text("Editing flair ID: ").number(flair_id).newline()
                    .newline()
                    // Hidden fields
                    .raw_str("<input type=\"hidden\" name=\"board_id\" value=\"")
                    .number(board_id as u32)
                    .raw_str("\" />\n")
                    .raw_str("<input type=\"hidden\" name=\"flair_id\" value=\"")
                    .number(flair_id)
                    .raw_str("\" />\n")
                    .raw_str("<input type=\"hidden\" name=\"_redirect\" value=\"/admin/b/")
                    .number(board_id as u32)
                    .raw_str("/flairs\" />\n");

                // Pre-fill form with current values
                // Name field - we need to use raw HTML to set the value
                let name_len = flair.name.len() as usize;
                let mut name_buf = [0u8; 64];
                let name_copy_len = if name_len > 64 { 64 } else { name_len };
                flair.name.copy_into_slice(&mut name_buf[..name_copy_len]);

                md = md.raw_str("<div class=\"form-group\">\n")
                    .raw_str("<label for=\"name\">Name</label>\n")
                    .raw_str("<input type=\"text\" name=\"name\" id=\"name\" value=\"");
                for j in 0..name_copy_len {
                    md = md.raw_str(core::str::from_utf8(&name_buf[j..j+1]).unwrap_or(""));
                }
                md = md.raw_str("\" />\n")
                    .raw_str("</div>\n\n");

                // Color field
                let color_len = flair.color.len() as usize;
                let mut color_buf = [0u8; 32];
                let color_copy_len = if color_len > 32 { 32 } else { color_len };
                flair.color.copy_into_slice(&mut color_buf[..color_copy_len]);

                md = md.raw_str("<div class=\"form-group\">\n")
                    .raw_str("<label for=\"color\">Text color</label>\n")
                    .raw_str("<input type=\"text\" name=\"color\" id=\"color\" value=\"");
                for j in 0..color_copy_len {
                    md = md.raw_str(core::str::from_utf8(&color_buf[j..j+1]).unwrap_or(""));
                }
                md = md.raw_str("\" />\n")
                    .raw_str("</div>\n\n");

                // Background color field
                let bg_len = flair.bg_color.len() as usize;
                let mut bg_buf = [0u8; 32];
                let bg_copy_len = if bg_len > 32 { 32 } else { bg_len };
                flair.bg_color.copy_into_slice(&mut bg_buf[..bg_copy_len]);

                md = md.raw_str("<div class=\"form-group\">\n")
                    .raw_str("<label for=\"bg_color\">Background color</label>\n")
                    .raw_str("<input type=\"text\" name=\"bg_color\" id=\"bg_color\" value=\"");
                for j in 0..bg_copy_len {
                    md = md.raw_str(core::str::from_utf8(&bg_buf[j..j+1]).unwrap_or(""));
                }
                md = md.raw_str("\" />\n")
                    .raw_str("</div>\n\n");

                // Checkboxes with current values
                md = md.raw_str("<input type=\"hidden\" name=\"required\" value=\"false\" />\n");
                if flair.required {
                    md = md.raw_str("<label><input type=\"checkbox\" name=\"required\" value=\"true\" checked /> Required for new posts</label>\n");
                } else {
                    md = md.raw_str("<label><input type=\"checkbox\" name=\"required\" value=\"true\" /> Required for new posts</label>\n");
                }

                md = md.raw_str("<input type=\"hidden\" name=\"mod_only\" value=\"false\" />\n");
                if flair.mod_only {
                    md = md.raw_str("<label><input type=\"checkbox\" name=\"mod_only\" value=\"true\" checked /> Moderator only</label>\n");
                } else {
                    md = md.raw_str("<label><input type=\"checkbox\" name=\"mod_only\" value=\"true\" /> Moderator only</label>\n");
                }

                md = md.raw_str("<input type=\"hidden\" name=\"enabled\" value=\"false\" />\n");
                if flair.enabled {
                    md = md.raw_str("<label><input type=\"checkbox\" name=\"enabled\" value=\"true\" checked /> Enabled</label>\n");
                } else {
                    md = md.raw_str("<label><input type=\"checkbox\" name=\"enabled\" value=\"true\" /> Enabled</label>\n");
                }

                md = md.newline()
                    .form_link_to("Update Flair", "admin", "update_flair")
                    .newline()
                    .newline()
                    .raw_str("[Cancel](render:/admin/b/")
                    .number(board_id as u32)
                    .raw_str("/flairs)");
            } else {
                md = md.warning("Flair not found.");
            }
        } else {
            md = md.warning("Board contract not found.");
        }

        Self::render_footer_into(md).build()
    }

    /// Render rules editor page
    fn render_rules(env: &Env, board_id: u64, viewer: &Option<Address>) -> Bytes {
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

        let mut md = Self::render_nav_subpage(env, board_id)
            .h1("Board Rules Editor");

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
            md = md.warning("You must be an admin to edit board rules.");
            return Self::render_footer_into(md).build();
        }

        // Get board contract
        let board_contract: Option<Address> = env.invoke_contract(
            &registry,
            &Symbol::new(env, "get_board_contract"),
            Vec::from_array(env, [board_id.into_val(env)]),
        );

        if let Some(board_addr) = board_contract {
            // Get current rules
            let current_rules: Option<String> = env.invoke_contract(
                &board_addr,
                &Symbol::new(env, "get_rules"),
                Vec::new(env),
            );

            md = md.h2("Current Rules");

            if let Some(ref rules) = current_rules {
                if rules.len() > 0 {
                    md = md.div_start("rules-content")
                        .text_string(rules)
                        .div_end()
                        .newline()
                        .raw_str("<input type=\"hidden\" name=\"board_id\" value=\"")
                        .number(board_id as u32)
                        .raw_str("\" />\n")
                        .form_link_to("Clear Rules", "admin", "clear_rules")
                        .newline();
                } else {
                    md = md.paragraph("No rules set.");
                }
            } else {
                md = md.paragraph("No rules set.");
            }

            // Edit rules form
            md = md.hr()
                .h2("Edit Rules")
                .note("Rules are displayed on the board page and when creating new threads. Use plain text or Markdown.")
                .raw_str("<input type=\"hidden\" name=\"board_id\" value=\"")
                .number(board_id as u32)
                .raw_str("\" />\n")
                .raw_str("<input type=\"hidden\" name=\"_redirect\" value=\"/admin/b/")
                .number(board_id as u32)
                .raw_str("/rules\" />\n")
                .textarea("rules", 10, "Enter board rules here...");

            // Pre-fill with current rules if available
            if let Some(rules) = current_rules {
                if rules.len() > 0 {
                    // Note: Can't pre-fill textarea in this setup, user needs to re-enter
                    md = md.newline()
                        .note("Copy existing rules above and paste into the text area if you want to edit.");
                }
            }

            md = md.newline()
                .form_link_to("Save Rules", "admin", "set_rules");
        } else {
            md = md.warning("Board contract not found.");
        }

        Self::render_footer_into(md).build()
    }

    /// Render voting configuration page
    fn render_voting(env: &Env, board_id: u64, viewer: &Option<Address>) -> Bytes {
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

        let mut md = Self::render_nav_subpage(env, board_id)
            .h1("Voting Configuration");

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
            md = md.warning("You must be an admin to configure voting.");
            return Self::render_footer_into(md).build();
        }

        // Get voting contract from registry
        let voting_contract: Option<Address> = env.invoke_contract(
            &registry,
            &Symbol::new(env, "get_voting_contract"),
            Vec::new(env),
        );

        if let Some(voting_addr) = voting_contract {
            // Get current config
            let config: Option<VotingConfig> = env
                .try_invoke_contract::<VotingConfig, soroban_sdk::Error>(
                    &voting_addr,
                    &Symbol::new(env, "get_voting_config"),
                    Vec::from_array(env, [board_id.into_val(env)]),
                )
                .ok()
                .and_then(|r| r.ok());

            md = md.h2("Current Configuration");

            if let Some(cfg) = config {
                md = md
                    .raw_str("- **Voting enabled:** ").text(if cfg.enabled { "Yes" } else { "No" }).newline()
                    .raw_str("- **Allow downvotes:** ").text(if cfg.allow_downvotes { "Yes" } else { "No" }).newline()
                    .raw_str("- **Karma tracking:** ").text(if cfg.karma_enabled { "Yes" } else { "No" }).newline()
                    .raw_str("- **Karma multiplier:** ").number(cfg.karma_multiplier).newline();
            } else {
                md = md.paragraph("Using default configuration (voting enabled, downvotes allowed).");
            }

            // Update form
            md = md.hr()
                .h2("Update Configuration")
                .raw_str("<input type=\"hidden\" name=\"board_id\" value=\"")
                .number(board_id as u32)
                .raw_str("\" />\n")
                .raw_str("<input type=\"hidden\" name=\"_redirect\" value=\"/admin/b/")
                .number(board_id as u32)
                .raw_str("/voting\" />\n")
                .raw_str("<label><input type=\"checkbox\" name=\"enabled\" value=\"true\" checked /> Enable voting</label>\n")
                .raw_str("<label><input type=\"checkbox\" name=\"allow_downvotes\" value=\"true\" checked /> Allow downvotes</label>\n")
                .raw_str("<label><input type=\"checkbox\" name=\"karma_enabled\" value=\"true\" /> Enable karma tracking</label>\n")
                .input("karma_multiplier", "Karma multiplier (default: 1)")
                .newline()
                .form_link_to("Update Configuration", "admin", "set_voting_config");
        } else {
            md = md.note("Voting contract is not configured for this board. Voting features are disabled.");
        }

        Self::render_footer_into(md).build()
    }

    /// Render registry admin page
    fn render_registry_admin(env: &Env, viewer: &Option<Address>) -> Bytes {
        let registry: Address = env
            .storage()
            .instance()
            .get(&AdminKey::Registry)
            .expect("Not initialized");

        let mut md = MarkdownBuilder::new(env)
            .render_link("Soroban Boards", "/")
            .newline()
            .hr()
            .h1("Registry Administration");

        if viewer.is_none() {
            md = md.warning("Please connect your wallet to access registry admin.");
            return Self::render_footer_into(md).build();
        }

        let viewer_addr = viewer.as_ref().unwrap();

        // Get all admins
        let admins: Vec<Address> = env.invoke_contract(
            &registry,
            &Symbol::new(env, "get_admins"),
            Vec::new(env),
        );

        // Check if viewer is an admin
        let viewer_is_admin: bool = env.invoke_contract(
            &registry,
            &Symbol::new(env, "is_admin"),
            Vec::from_array(env, [viewer_addr.into_val(env)]),
        );

        md = md
            .h2("Current Admins")
            .raw_str("<ul>\n");

        for i in 0..admins.len() {
            let admin = admins.get(i).unwrap();
            md = md
                .raw_str("<li><code>")
                .text_string(&admin.to_string())
                .raw_str("</code></li>\n");
        }

        md = md.raw_str("</ul>\n").newline();

        // If viewer is admin, show add/remove admin forms
        if viewer_is_admin {
            md = md
                .h2("Add Admin")
                .raw_str("<form>\n")
                .raw_str("<input type=\"text\" name=\"new_admin\" placeholder=\"Address (G...)\" style=\"width:100%;max-width:400px;\" />\n")
                .raw_str("<p><a class=\"soroban-action\" href=\"form:@registry:add_admin\">Add Admin</a></p>\n")
                .raw_str("</form>\n")
                .newline();

            if admins.len() > 1 {
                md = md
                    .h2("Remove Admin")
                    .raw_str("<form>\n")
                    .raw_str("<input type=\"text\" name=\"admin_to_remove\" placeholder=\"Address (G...)\" style=\"width:100%;max-width:400px;\" />\n")
                    .raw_str("<p><a class=\"soroban-action\" href=\"form:@registry:remove_admin\">Remove Admin</a></p>\n")
                    .raw_str("</form>\n")
                    .newline();
            }
        }

        // Show viewer's address for reference
        md = md
            .newline()
            .h2("Your Address")
            .raw_str("<p><code>")
            .text_string(&viewer_addr.to_string())
            .raw_str("</code>");

        if viewer_is_admin {
            md = md.raw_str(" <span style=\"color:green;\">(Admin)</span>");
        }

        md = md.raw_str("</p>");

        // Add link back to home
        md = md
            .newline()
            .render_link("← Back to Home", "/");

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
            caller.into_val(&env),
        ]);
        env.invoke_contract::<()>(
            &permissions,
            &Symbol::new(&env, "set_role"),
            args,
        );
    }

    /// Add a user as Member (convenience function for forms)
    pub fn add_member(env: Env, board_id: u64, user_address: Address, caller: Address) {
        Self::set_role(env, board_id, user_address, Role::Member, caller);
    }

    /// Add a user as Moderator (convenience function for forms)
    pub fn add_moderator(env: Env, board_id: u64, user_address: Address, caller: Address) {
        Self::set_role(env, board_id, user_address, Role::Moderator, caller);
    }

    /// Add a user as Admin (convenience function for forms)
    pub fn add_admin(env: Env, board_id: u64, user_address: Address, caller: Address) {
        Self::set_role(env, board_id, user_address, Role::Admin, caller);
    }

    // ========================================================================
    // Invite Operations
    // ========================================================================

    /// Accept a pending invite request (moderator+)
    pub fn accept_invite(env: Env, board_id: u64, user: Address, caller: Address) {
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

        // Accept the invite
        let args: Vec<Val> = Vec::from_array(&env, [
            board_id.into_val(&env),
            user.into_val(&env),
            caller.into_val(&env),
        ]);
        env.invoke_contract::<()>(
            &permissions,
            &Symbol::new(&env, "accept_invite"),
            args,
        );
    }

    /// Revoke/reject a pending invite request (moderator+)
    pub fn revoke_invite(env: Env, board_id: u64, user: Address, caller: Address) {
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

        // Revoke the invite
        let args: Vec<Val> = Vec::from_array(&env, [
            board_id.into_val(&env),
            user.into_val(&env),
            caller.into_val(&env),
        ]);
        env.invoke_contract::<()>(
            &permissions,
            &Symbol::new(&env, "revoke_invite"),
            args,
        );
    }

    /// Directly invite a user as Member (moderator+)
    pub fn invite_member(env: Env, board_id: u64, user: Address, caller: Address) {
        Self::invite_with_role(env, board_id, user, Role::Member, caller);
    }

    /// Directly invite a user as Moderator (admin+)
    pub fn invite_moderator(env: Env, board_id: u64, user: Address, caller: Address) {
        Self::invite_with_role(env, board_id, user, Role::Moderator, caller);
    }

    /// Directly invite a user as Admin (owner only)
    pub fn invite_admin(env: Env, board_id: u64, user: Address, caller: Address) {
        Self::invite_with_role(env, board_id, user, Role::Admin, caller);
    }

    /// Helper function to invite with a specific role
    fn invite_with_role(env: Env, board_id: u64, user: Address, role: Role, caller: Address) {
        caller.require_auth();

        let permissions: Address = env
            .storage()
            .instance()
            .get(&AdminKey::Permissions)
            .expect("Not initialized");

        // The permissions contract handles authorization checks
        let args: Vec<Val> = Vec::from_array(&env, [
            board_id.into_val(&env),
            user.into_val(&env),
            role.into_val(&env),
            caller.into_val(&env),
        ]);
        env.invoke_contract::<()>(
            &permissions,
            &Symbol::new(&env, "invite_member"),
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
        // Permissions expects: (board_id, user, reason, duration_hours, caller)
        let args: Vec<Val> = Vec::from_array(&env, [
            board_id.into_val(&env),
            user.into_val(&env),
            reason.into_val(&env),
            expires_at.into_val(&env),
            caller.into_val(&env),
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
            caller.into_val(&env),
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
    /// Accepts threshold as String since HTML forms submit strings
    pub fn set_flag_threshold(
        env: Env,
        board_id: u64,
        threshold: String,
        caller: Address,
    ) {
        caller.require_auth();

        // Parse string to u32
        let threshold_u32 = parse_string_to_u32(&env, &threshold);

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
            threshold_u32.into_val(&env),
            caller.into_val(&env),
        ]);
        env.invoke_contract::<()>(
            &permissions,
            &Symbol::new(&env, "set_flag_threshold"),
            args,
        );
    }

    /// Update reply chunk size for waterfall loading (admin+)
    /// Accepts chunk_size as String since HTML forms submit strings
    pub fn set_chunk_size(
        env: Env,
        board_id: u64,
        chunk_size: String,
        caller: Address,
    ) {
        caller.require_auth();

        // Parse string to u32
        let chunk_size_u32 = parse_string_to_u32(&env, &chunk_size);

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

        // Validate chunk size (1-20 is reasonable range)
        if chunk_size_u32 < 1 || chunk_size_u32 > 20 {
            panic!("Chunk size must be between 1 and 20");
        }

        // Get board contract
        let registry: Address = env
            .storage()
            .instance()
            .get(&AdminKey::Registry)
            .expect("Not initialized");

        let board_contract: Address = env
            .invoke_contract::<Option<Address>>(
                &registry,
                &Symbol::new(&env, "get_board_contract"),
                Vec::from_array(&env, [board_id.into_val(&env)]),
            )
            .expect("Board contract not found");

        // Set the chunk size
        let args: Vec<Val> = Vec::from_array(&env, [
            chunk_size_u32.into_val(&env),
            caller.into_val(&env),
        ]);
        env.invoke_contract::<()>(
            &board_contract,
            &Symbol::new(&env, "set_chunk_size"),
            args,
        );
    }

    /// Update maximum reply depth for a board (admin+)
    /// Accepts max_depth as String since HTML forms submit strings
    pub fn set_max_reply_depth(
        env: Env,
        board_id: u64,
        max_depth: String,
        caller: Address,
    ) {
        caller.require_auth();

        // Parse string to u32
        let max_depth_u32 = parse_string_to_u32(&env, &max_depth);

        let permissions: Address = env
            .storage()
            .instance()
            .get(&AdminKey::Permissions)
            .expect("Not initialized");

        // Verify caller has admin permissions
        let caller_perms: PermissionSet = env.invoke_contract(
            &permissions,
            &Symbol::new(&env, "get_permissions"),
            Vec::from_array(&env, [board_id.into_val(&env), caller.clone().into_val(&env)]),
        );

        if !caller_perms.can_admin {
            panic!("Caller must be admin or owner");
        }

        // Validate max depth (1-20 is reasonable range)
        if max_depth_u32 < 1 || max_depth_u32 > 20 {
            panic!("Max reply depth must be between 1 and 20");
        }

        // Get board contract
        let registry: Address = env
            .storage()
            .instance()
            .get(&AdminKey::Registry)
            .expect("Not initialized");

        let board_contract: Address = env
            .invoke_contract::<Option<Address>>(
                &registry,
                &Symbol::new(&env, "get_board_contract"),
                Vec::from_array(&env, [board_id.into_val(&env)]),
            )
            .expect("Board contract not found");

        // Set the max reply depth
        let args: Vec<Val> = Vec::from_array(&env, [
            max_depth_u32.into_val(&env),
            caller.into_val(&env),
        ]);
        env.invoke_contract::<()>(
            &board_contract,
            &Symbol::new(&env, "set_max_reply_depth"),
            args,
        );
    }

    /// Update edit window for a board (admin+)
    /// Accepts edit_hours as String since HTML forms submit strings
    /// Input is in hours, stored as seconds
    pub fn set_edit_window(
        env: Env,
        board_id: u64,
        edit_hours: String,
        caller: Address,
    ) {
        caller.require_auth();

        // Parse string to u32 (hours)
        let hours_u32 = parse_string_to_u32(&env, &edit_hours);

        let permissions: Address = env
            .storage()
            .instance()
            .get(&AdminKey::Permissions)
            .expect("Not initialized");

        // Verify caller has admin permissions
        let caller_perms: PermissionSet = env.invoke_contract(
            &permissions,
            &Symbol::new(&env, "get_permissions"),
            Vec::from_array(&env, [board_id.into_val(&env), caller.clone().into_val(&env)]),
        );

        if !caller_perms.can_admin {
            panic!("Caller must be admin or owner");
        }

        // Convert hours to seconds (0 stays 0 for "no limit")
        let seconds: u64 = (hours_u32 as u64) * 3600;

        // Get board contract
        let registry: Address = env
            .storage()
            .instance()
            .get(&AdminKey::Registry)
            .expect("Not initialized");

        let board_contract: Address = env
            .invoke_contract::<Option<Address>>(
                &registry,
                &Symbol::new(&env, "get_board_contract"),
                Vec::from_array(&env, [board_id.into_val(&env)]),
            )
            .expect("Board contract not found");

        // Set the edit window
        let args: Vec<Val> = Vec::from_array(&env, [
            seconds.into_val(&env),
            caller.into_val(&env),
        ]);
        env.invoke_contract::<()>(
            &board_contract,
            &Symbol::new(&env, "set_edit_window"),
            args,
        );
    }

    /// Rename a board (admin+)
    /// The old name becomes an alias that continues to resolve
    pub fn rename_board(
        env: Env,
        board_id: u64,
        new_name: String,
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
            Vec::from_array(&env, [board_id.into_val(&env), caller.clone().into_val(&env)]),
        );

        if !caller_perms.can_admin {
            panic!("Caller must be admin or owner");
        }

        // Call registry's rename_board function
        let registry: Address = env
            .storage()
            .instance()
            .get(&AdminKey::Registry)
            .expect("Not initialized");

        let args: Vec<Val> = Vec::from_array(&env, [
            board_id.into_val(&env),
            new_name.into_val(&env),
            caller.into_val(&env),
        ]);
        env.invoke_contract::<()>(
            &registry,
            &Symbol::new(&env, "rename_board"),
            args,
        );
    }

    /// List a board publicly (admin+)
    /// Makes the board appear on the home page
    pub fn list_board(
        env: Env,
        board_id: u64,
        caller: Address,
    ) {
        caller.require_auth();
        Self::set_board_listed(env, board_id, true, caller);
    }

    /// Unlist a board (admin+)
    /// Hides the board from the home page but keeps it accessible via direct link
    pub fn unlist_board(
        env: Env,
        board_id: u64,
        caller: Address,
    ) {
        caller.require_auth();
        Self::set_board_listed(env, board_id, false, caller);
    }

    /// Helper to set board listed status
    fn set_board_listed(
        env: Env,
        board_id: u64,
        is_listed: bool,
        caller: Address,
    ) {
        let permissions: Address = env
            .storage()
            .instance()
            .get(&AdminKey::Permissions)
            .expect("Not initialized");

        // Verify caller has admin permissions
        let caller_perms: PermissionSet = env.invoke_contract(
            &permissions,
            &Symbol::new(&env, "get_permissions"),
            Vec::from_array(&env, [board_id.into_val(&env), caller.clone().into_val(&env)]),
        );

        if !caller_perms.can_admin {
            panic!("Caller must be admin or owner");
        }

        // Call registry's set_listed function
        let registry: Address = env
            .storage()
            .instance()
            .get(&AdminKey::Registry)
            .expect("Not initialized");

        let args: Vec<Val> = Vec::from_array(&env, [
            board_id.into_val(&env),
            is_listed.into_val(&env),
            caller.into_val(&env),
        ]);
        env.invoke_contract::<()>(
            &registry,
            &Symbol::new(&env, "set_listed"),
            args,
        );
    }

    /// Make a board public (admin+)
    pub fn make_public(
        env: Env,
        board_id: u64,
        caller: Address,
    ) {
        caller.require_auth();
        Self::set_board_private(env, board_id, false, caller);
    }

    /// Make a board private (admin+)
    pub fn make_private(
        env: Env,
        board_id: u64,
        caller: Address,
    ) {
        caller.require_auth();
        Self::set_board_private(env, board_id, true, caller);
    }

    /// Helper to set board private status
    fn set_board_private(
        env: Env,
        board_id: u64,
        is_private: bool,
        caller: Address,
    ) {
        let permissions: Address = env
            .storage()
            .instance()
            .get(&AdminKey::Permissions)
            .expect("Not initialized");

        // Verify caller has admin permissions
        let caller_perms: PermissionSet = env.invoke_contract(
            &permissions,
            &Symbol::new(&env, "get_permissions"),
            Vec::from_array(&env, [board_id.into_val(&env), caller.clone().into_val(&env)]),
        );

        if !caller_perms.can_admin {
            panic!("Caller must be admin or owner");
        }

        // Call registry's set_private function
        let registry: Address = env
            .storage()
            .instance()
            .get(&AdminKey::Registry)
            .expect("Not initialized");

        let args: Vec<Val> = Vec::from_array(&env, [
            board_id.into_val(&env),
            is_private.into_val(&env),
            caller.into_val(&env),
        ]);
        env.invoke_contract::<()>(
            &registry,
            &Symbol::new(&env, "set_private"),
            args,
        );
    }

    /// Enable posting on a board (admin+)
    pub fn enable_posting(
        env: Env,
        board_id: u64,
        caller: Address,
    ) {
        caller.require_auth();
        Self::set_board_readonly(env, board_id, false, caller);
    }

    /// Make a board read-only (admin+)
    pub fn make_readonly(
        env: Env,
        board_id: u64,
        caller: Address,
    ) {
        caller.require_auth();
        Self::set_board_readonly(env, board_id, true, caller);
    }

    /// Helper to set board readonly status
    fn set_board_readonly(
        env: Env,
        board_id: u64,
        is_readonly: bool,
        caller: Address,
    ) {
        let permissions: Address = env
            .storage()
            .instance()
            .get(&AdminKey::Permissions)
            .expect("Not initialized");

        // Verify caller has admin permissions
        let caller_perms: PermissionSet = env.invoke_contract(
            &permissions,
            &Symbol::new(&env, "get_permissions"),
            Vec::from_array(&env, [board_id.into_val(&env), caller.clone().into_val(&env)]),
        );

        if !caller_perms.can_admin {
            panic!("Caller must be admin or owner");
        }

        // Get board contract address from registry
        let registry: Address = env
            .storage()
            .instance()
            .get(&AdminKey::Registry)
            .expect("Not initialized");

        let board_contract: Address = env.invoke_contract::<Option<Address>>(
            &registry,
            &Symbol::new(&env, "get_board_contract"),
            Vec::from_array(&env, [board_id.into_val(&env)]),
        ).expect("Board contract not found");

        // Call board contract's set_readonly function
        let args: Vec<Val> = Vec::from_array(&env, [
            is_readonly.into_val(&env),
            caller.into_val(&env),
        ]);
        env.invoke_contract::<()>(
            &board_contract,
            &Symbol::new(&env, "set_readonly"),
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

    // ========================================================================
    // Flair Operations
    // ========================================================================

    /// Create a new flair (admin+)
    pub fn create_flair(
        env: Env,
        board_id: u64,
        name: String,
        color: String,
        bg_color: String,
        required: Option<String>,
        mod_only: Option<String>,
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

        // Verify caller has admin permissions
        let caller_perms: PermissionSet = env.invoke_contract(
            &permissions,
            &Symbol::new(&env, "get_permissions"),
            Vec::from_array(&env, [board_id.into_val(&env), caller.clone().into_val(&env)]),
        );

        if !caller_perms.can_admin {
            panic!("Caller must be admin or owner");
        }

        // Get board contract
        let board_contract: Address = env
            .invoke_contract::<Option<Address>>(
                &registry,
                &Symbol::new(&env, "get_board_contract"),
                Vec::from_array(&env, [board_id.into_val(&env)]),
            )
            .expect("Board contract not found");

        // Parse checkbox values (check for "true" string value)
        let is_required = required
            .map(|s| {
                let len = s.len() as usize;
                if len != 4 { return false; }
                let mut buf = [0u8; 4];
                s.copy_into_slice(&mut buf);
                &buf == b"true"
            })
            .unwrap_or(false);
        let is_mod_only = mod_only
            .map(|s| {
                let len = s.len() as usize;
                if len != 4 { return false; }
                let mut buf = [0u8; 4];
                s.copy_into_slice(&mut buf);
                &buf == b"true"
            })
            .unwrap_or(false);

        // Create the flair
        let args: Vec<Val> = Vec::from_array(&env, [
            name.into_val(&env),
            color.into_val(&env),
            bg_color.into_val(&env),
            is_required.into_val(&env),
            is_mod_only.into_val(&env),
            caller.into_val(&env),
        ]);
        env.invoke_contract::<u32>(
            &board_contract,
            &Symbol::new(&env, "create_flair"),
            args,
        );
    }

    /// Disable a flair (admin+)
    pub fn disable_flair(
        env: Env,
        board_id: u64,
        flair_id: String,
        caller: Address,
    ) {
        caller.require_auth();

        let flair_id_u32 = parse_string_to_u32(&env, &flair_id);

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

        // Verify caller has admin permissions
        let caller_perms: PermissionSet = env.invoke_contract(
            &permissions,
            &Symbol::new(&env, "get_permissions"),
            Vec::from_array(&env, [board_id.into_val(&env), caller.clone().into_val(&env)]),
        );

        if !caller_perms.can_admin {
            panic!("Caller must be admin or owner");
        }

        // Get board contract
        let board_contract: Address = env
            .invoke_contract::<Option<Address>>(
                &registry,
                &Symbol::new(&env, "get_board_contract"),
                Vec::from_array(&env, [board_id.into_val(&env)]),
            )
            .expect("Board contract not found");

        // Disable the flair
        let args: Vec<Val> = Vec::from_array(&env, [
            flair_id_u32.into_val(&env),
            caller.into_val(&env),
        ]);
        env.invoke_contract::<()>(
            &board_contract,
            &Symbol::new(&env, "disable_flair"),
            args,
        );
    }

    /// Update an existing flair (admin+)
    pub fn update_flair(
        env: Env,
        board_id: u64,
        flair_id: u64,
        name: String,
        color: String,
        bg_color: String,
        required: Option<String>,
        mod_only: Option<String>,
        enabled: Option<String>,
        caller: Address,
    ) {
        caller.require_auth();

        let flair_id_u32 = flair_id as u32;

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

        // Verify caller has admin permissions
        let caller_perms: PermissionSet = env.invoke_contract(
            &permissions,
            &Symbol::new(&env, "get_permissions"),
            Vec::from_array(&env, [board_id.into_val(&env), caller.clone().into_val(&env)]),
        );

        if !caller_perms.can_admin {
            panic!("Caller must be admin or owner");
        }

        // Get board contract
        let board_contract: Address = env
            .invoke_contract::<Option<Address>>(
                &registry,
                &Symbol::new(&env, "get_board_contract"),
                Vec::from_array(&env, [board_id.into_val(&env)]),
            )
            .expect("Board contract not found");

        // Parse checkbox values (check for "true" string value)
        let is_required = required
            .map(|s| {
                let len = s.len() as usize;
                if len != 4 { return false; }
                let mut buf = [0u8; 4];
                s.copy_into_slice(&mut buf);
                &buf == b"true"
            })
            .unwrap_or(false);
        let is_mod_only = mod_only
            .map(|s| {
                let len = s.len() as usize;
                if len != 4 { return false; }
                let mut buf = [0u8; 4];
                s.copy_into_slice(&mut buf);
                &buf == b"true"
            })
            .unwrap_or(false);
        let is_enabled = enabled
            .map(|s| {
                let len = s.len() as usize;
                if len != 4 { return false; }
                let mut buf = [0u8; 4];
                s.copy_into_slice(&mut buf);
                &buf == b"true"
            })
            .unwrap_or(true); // Default to enabled

        // Build FlairDef
        let flair = FlairDef {
            id: flair_id_u32,
            name,
            color,
            bg_color,
            required: is_required,
            mod_only: is_mod_only,
            enabled: is_enabled,
        };

        // Update the flair
        let args: Vec<Val> = Vec::from_array(&env, [
            flair_id_u32.into_val(&env),
            flair.into_val(&env),
            caller.into_val(&env),
        ]);
        env.invoke_contract::<()>(
            &board_contract,
            &Symbol::new(&env, "update_flair"),
            args,
        );
    }

    // ========================================================================
    // Rules Operations
    // ========================================================================

    /// Set board rules (admin+)
    pub fn set_rules(
        env: Env,
        board_id: u64,
        rules: String,
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

        // Verify caller has admin permissions
        let caller_perms: PermissionSet = env.invoke_contract(
            &permissions,
            &Symbol::new(&env, "get_permissions"),
            Vec::from_array(&env, [board_id.into_val(&env), caller.clone().into_val(&env)]),
        );

        if !caller_perms.can_admin {
            panic!("Caller must be admin or owner");
        }

        // Get board contract
        let board_contract: Address = env
            .invoke_contract::<Option<Address>>(
                &registry,
                &Symbol::new(&env, "get_board_contract"),
                Vec::from_array(&env, [board_id.into_val(&env)]),
            )
            .expect("Board contract not found");

        // Set the rules
        let args: Vec<Val> = Vec::from_array(&env, [
            rules.into_val(&env),
            caller.into_val(&env),
        ]);
        env.invoke_contract::<()>(
            &board_contract,
            &Symbol::new(&env, "set_rules"),
            args,
        );
    }

    /// Clear board rules (admin+)
    pub fn clear_rules(
        env: Env,
        board_id: u64,
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

        // Verify caller has admin permissions
        let caller_perms: PermissionSet = env.invoke_contract(
            &permissions,
            &Symbol::new(&env, "get_permissions"),
            Vec::from_array(&env, [board_id.into_val(&env), caller.clone().into_val(&env)]),
        );

        if !caller_perms.can_admin {
            panic!("Caller must be admin or owner");
        }

        // Get board contract
        let board_contract: Address = env
            .invoke_contract::<Option<Address>>(
                &registry,
                &Symbol::new(&env, "get_board_contract"),
                Vec::from_array(&env, [board_id.into_val(&env)]),
            )
            .expect("Board contract not found");

        // Clear the rules
        let args: Vec<Val> = Vec::from_array(&env, [
            caller.into_val(&env),
        ]);
        env.invoke_contract::<()>(
            &board_contract,
            &Symbol::new(&env, "clear_rules"),
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
