#![no_std]

//! boards-main: Application entry point for Soroban Boards
//!
//! This contract serves as the main entry point and ties together
//! the registry, theme, and board contracts. It handles:
//! - Route dispatching to appropriate contracts
//! - Navigation bar rendering with proper return paths
//! - Home page, create board, and help page rendering
//!
//! Contract Responsibilities:
//! - boards-main (this): Application entry, routing, navigation, coordination
//! - boards-registry: Alias lookups only (get_contract_by_alias)
//! - boards-theme: CSS/styling only (styles())
//! - boards-board: Individual board logic
//! - boards-admin: Admin/settings functions
//! - boards-content: Content storage/handling
//! - boards-permissions: Access control logic

use soroban_render_sdk::prelude::*;
use soroban_sdk::{
    contract, contractimpl, contracttype, Address, Bytes, BytesN, Env, IntoVal, String, Symbol,
    Val, Vec,
};

// Declare render capabilities
soroban_render!(markdown, styles);

/// Storage keys for the main contract
#[contracttype]
#[derive(Clone)]
pub enum MainKey {
    /// Registry contract address (for alias lookups)
    Registry,
    /// Theme contract address (for CSS)
    Theme,
    /// Permissions contract address
    Permissions,
    /// Content contract address
    Content,
    /// Admin contract address
    Admin,
    /// Community contract address
    Community,
}

/// Board metadata (same structure as registry for compatibility)
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
pub struct BoardsMain;

#[contractimpl]
impl BoardsMain {
    /// Initialize the main contract with service contract addresses
    pub fn init(env: Env, registry: Address, theme: Address, permissions: Address, content: Address, admin: Address, community: Address) {
        if env.storage().instance().has(&MainKey::Registry) {
            panic!("Already initialized");
        }

        env.storage().instance().set(&MainKey::Registry, &registry);
        env.storage().instance().set(&MainKey::Theme, &theme);
        env.storage().instance().set(&MainKey::Permissions, &permissions);
        env.storage().instance().set(&MainKey::Content, &content);
        env.storage().instance().set(&MainKey::Admin, &admin);
        env.storage().instance().set(&MainKey::Community, &community);
    }

    /// Get community contract address
    pub fn get_community(env: Env) -> Option<Address> {
        env.storage()
            .instance()
            .get(&MainKey::Community)
    }

    /// Set community contract address (for upgrades - requires registry admin auth)
    pub fn set_community(env: Env, community: Address, caller: Address) {
        caller.require_auth();

        // Verify caller is the registry admin
        let registry: Address = env
            .storage()
            .instance()
            .get(&MainKey::Registry)
            .expect("Not initialized");

        let admin_args: Vec<Val> = Vec::new(&env);
        let admin: Address = env.invoke_contract(&registry, &Symbol::new(&env, "get_admin"), admin_args);

        if caller != admin {
            panic!("Only registry admin can set community");
        }

        env.storage().instance().set(&MainKey::Community, &community);
    }

    /// Get registry address
    pub fn get_registry(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&MainKey::Registry)
            .expect("Not initialized")
    }

    /// Get theme address
    pub fn get_theme(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&MainKey::Theme)
            .expect("Not initialized")
    }

    // ========================================================================
    // Main Render Entry Point
    // ========================================================================

    /// Main render entry point - routes to appropriate handler
    ///
    /// Routing:
    /// - `/`, `/create`, `/help` → Rendered here (home, create board, help)
    /// - `/communities`, `/c/{name}/*` → Community contract
    /// - `/admin/*`, `/b/{id}/settings`, etc. → Admin contract
    /// - `/b/{id}/*` → Board contract (looked up via Registry)
    pub fn render(env: Env, path: Option<String>, viewer: Option<Address>) -> Bytes {
        Router::new(&env, path.clone())
            // Home page
            .handle(b"/", |_| Self::render_home(&env, &viewer))
            // Create board form
            .or_handle(b"/create", |_| Self::render_create_board(&env, &viewer))
            // Help page
            .or_handle(b"/help", |_| Self::render_help(&env, &viewer))
            // Crosspost form
            .or_handle(b"/crosspost*", |_| Self::render_crosspost(&env, &path, &viewer))
            // Community routes - delegate to community contract
            .or_handle(b"/communities", |_| Self::delegate_to_community(&env, &String::from_str(&env, "/"), &viewer))
            .or_handle(b"/new", |_| Self::delegate_to_community(&env, &String::from_str(&env, "/new"), &viewer))
            .or_handle(b"/c/{name}/*", |req| {
                let name = req.get_var(b"name").unwrap_or(Bytes::new(&env));
                Self::delegate_to_community_by_name(&env, &name, &path, &viewer)
            })
            .or_handle(b"/c/{name}", |req| {
                let name = req.get_var(b"name").unwrap_or(Bytes::new(&env));
                Self::delegate_to_community_by_name(&env, &name, &path, &viewer)
            })
            // Admin routes - delegate to admin contract
            .or_handle(b"/admin/*", |_| Self::delegate_to_admin(&env, &path, &viewer))
            .or_handle(b"/b/{id}/members", |_| Self::delegate_to_admin(&env, &path, &viewer))
            .or_handle(b"/b/{id}/banned", |_| Self::delegate_to_admin(&env, &path, &viewer))
            .or_handle(b"/b/{id}/flags", |_| Self::delegate_to_admin(&env, &path, &viewer))
            .or_handle(b"/b/{id}/settings", |_| Self::delegate_to_admin(&env, &path, &viewer))
            .or_handle(b"/b/{id}/invites", |_| Self::delegate_to_admin(&env, &path, &viewer))
            // Board routes - delegate to board contract
            .or_handle(b"/b/{id}/*", |req| {
                let board_id = req.get_var_u32(b"id").unwrap_or(0) as u64;
                Self::delegate_to_board(&env, board_id, &path, &viewer)
            })
            .or_handle(b"/b/{id}", |req| {
                let board_id = req.get_var_u32(b"id").unwrap_or(0) as u64;
                Self::delegate_to_board(&env, board_id, &path, &viewer)
            })
            // Default - home page
            .or_default(|_| Self::render_home(&env, &viewer))
    }

    // ========================================================================
    // CSS Styles (delegates to theme)
    // ========================================================================

    /// Get CSS from Theme contract
    pub fn styles(env: Env) -> Bytes {
        let theme: Address = env
            .storage()
            .instance()
            .get(&MainKey::Theme)
            .expect("Not initialized");

        env.invoke_contract(&theme, &Symbol::new(&env, "styles"), Vec::new(&env))
    }

    /// Get CSS - named render_styles to follow render_* convention for the viewer
    /// Accepts path/viewer for consistency with render_* convention (unused here)
    pub fn render_styles(env: Env, _path: Option<String>, _viewer: Option<Address>) -> Bytes {
        Self::styles(env)
    }

    // ========================================================================
    // Navigation
    // ========================================================================

    /// Render the navigation bar with profile link
    fn render_nav<'a>(env: &'a Env, viewer: &Option<Address>) -> MarkdownBuilder<'a> {
        let registry: Address = env
            .storage()
            .instance()
            .get(&MainKey::Registry)
            .expect("Not initialized");

        let mut md = MarkdownBuilder::new(env)
            .div_start("nav-bar")
            .render_link("Soroban Boards", "/")
            .render_link("Communities", "/communities")
            .render_link("Help", "/help");

        // Add profile link if profile contract is registered
        let profile_alias = Symbol::new(env, "profile");
        let alias_args: Vec<Val> = Vec::from_array(env, [profile_alias.into_val(env)]);
        let profile_opt: Option<Address> = env.invoke_contract(
            &registry,
            &Symbol::new(env, "get_contract_by_alias"),
            alias_args,
        );

        if let Some(profile_addr) = profile_opt {
            // Build return path using OUR contract ID (not @registry alias)
            // This ensures "Go Back" returns to this contract correctly
            let self_addr = env.current_contract_address();
            let self_id_str = Self::address_to_contract_id_string(env, &self_addr);

            // Format: {CONTRACT_ID}:/
            let mut return_path = self_id_str;
            return_path.append(&Bytes::from_slice(env, b":/"));

            let args: Vec<Val> = Vec::from_array(env, [
                viewer.into_val(env),
                return_path.into_val(env),
            ]);
            let profile_link: Bytes = env.invoke_contract(
                &profile_addr,
                &Symbol::new(env, "render_nav_link_return"),
                args,
            );
            md = md.raw(profile_link);
        } else if viewer.is_some() {
            // No profile contract registered - show a placeholder link
            // Build return path for when profile contract is eventually registered
            let self_addr = env.current_contract_address();
            let self_id_str = Self::address_to_contract_id_string(env, &self_addr);
            let mut return_path = self_id_str;
            return_path.append(&Bytes::from_slice(env, b":/"));

            md = md
                .raw_str("<a href=\"render:@profile:/register/from/")
                .raw(return_path)
                .raw_str("\">Create Profile</a>");
        }

        md.div_end()
    }

    /// Append footer to builder
    fn render_footer_into(md: MarkdownBuilder<'_>) -> MarkdownBuilder<'_> {
        md.div_start("footer")
            .text("Powered by ")
            .link("Soroban Render", "https://github.com/wyhaines/soroban-render")
            .text(" on ")
            .link("Stellar", "https://stellar.org")
            .div_end()
    }

    // ========================================================================
    // Page Rendering
    // ========================================================================

    /// Render the home page with board list
    fn render_home(env: &Env, viewer: &Option<Address>) -> Bytes {
        let registry: Address = env
            .storage()
            .instance()
            .get(&MainKey::Registry)
            .expect("Not initialized");

        let mut md = Self::render_nav(env, viewer)
            .newline()  // Blank line after nav-bar div for markdown parsing
            .h1("Soroban Boards")
            .paragraph("Decentralized discussion forums on Stellar");

        // Show connection status
        if viewer.is_some() {
            md = md.tip("Wallet connected! You can create boards and post.");
        } else {
            md = md.note("Connect your wallet to participate in discussions.");
        }

        md = md.h2("Boards")
            .newline();

        // Get board count from registry
        let count: u64 = env.invoke_contract(
            &registry,
            &Symbol::new(env, "board_count"),
            Vec::new(env),
        );

        if count == 0 {
            md = md.paragraph("No boards yet. Be the first to create one!");
        } else {
            // List boards (up to 20)
            let mut displayed = 0u32;
            md = md.raw_str("<div class=\"board-list\">\n");

            for i in 0..count {
                if displayed >= 20 {
                    break;
                }

                // Check if board is listed
                let listed_args: Vec<Val> = Vec::from_array(env, [i.into_val(env)]);
                let is_listed: bool = env.invoke_contract(
                    &registry,
                    &Symbol::new(env, "get_board_listed"),
                    listed_args,
                );

                if !is_listed {
                    continue;
                }

                // Get board metadata
                let board_args: Vec<Val> = Vec::from_array(env, [i.into_val(env)]);
                let board_opt: Option<BoardMeta> = env.invoke_contract(
                    &registry,
                    &Symbol::new(env, "get_board"),
                    board_args,
                );

                if let Some(board) = board_opt {
                    // Check private status
                    let private_args: Vec<Val> = Vec::from_array(env, [i.into_val(env)]);
                    let is_private: bool = env.invoke_contract(
                        &registry,
                        &Symbol::new(env, "get_board_private"),
                        private_args,
                    );

                    // Board card with link wrapper
                    md = md.raw_str("<a href=\"render:/b/")
                        .number(board.id as u32)
                        .raw_str("\" class=\"board-card\"><span class=\"board-card-title\">")
                        .text_string(&board.name)
                        .raw_str("</span><span class=\"board-card-desc\">")
                        .text_string(&board.description)
                        .raw_str("</span><span class=\"board-card-meta\">")
                        .number(board.thread_count as u32)
                        .text(" threads · ")
                        .raw(Self::format_timestamp(env, board.created_at));
                    if is_private {
                        md = md.raw_str(" <span class=\"badge\">private</span>");
                    }
                    md = md.raw_str("</span></a>\n");
                    displayed += 1;
                }
            }
            md = md.raw_str("</div>\n");

            if displayed == 0 {
                md = md.paragraph("No boards yet. Be the first to create one!");
            }
        }

        md = md.newline()
            .render_link("+ Create New Board", "/create");

        // Show registry admin link if viewer is logged in
        if viewer.is_some() {
            md = md
                .text(" | ")
                .render_link("Registry Admin", "/admin/registry");
        }

        md = md.newline();

        Self::render_footer_into(md).build()
    }

    /// Render create board form
    fn render_create_board(env: &Env, viewer: &Option<Address>) -> Bytes {
        let mut md = Self::render_nav(env, viewer)
            .newline()  // Blank line after nav-bar for markdown parsing
            .h1("Create New Board");

        if viewer.is_none() {
            md = md.warning("Please connect your wallet to create a board.");
            return Self::render_footer_into(md).build();
        }

        md = md
            .paragraph("Create a new discussion board.")
            .newline()
            .redirect("/")  // Return to board list after creating board
            .input("name", "Board name")
            .newline()
            .textarea("description", 3, "Board description")
            .newline()
            // Private board checkbox
            .raw_str("<input type=\"hidden\" name=\"is_private\" value=\"false\" />\n")
            .raw_str("<label class=\"checkbox-label\"><input type=\"checkbox\" name=\"is_private\" value=\"true\" /> Make this board private</label>\n")
            .newline()
            // Listed board checkbox
            .raw_str("<input type=\"hidden\" name=\"is_listed\" value=\"true\" />\n")
            .raw_str("<label class=\"checkbox-label\"><input type=\"checkbox\" name=\"is_listed\" value=\"false\" /> Hide from public board list (unlisted)</label>\n")
            .newline()
            // Caller address for the contract
            .raw_str("<input type=\"hidden\" name=\"caller\" value=\"")
            .text_string(&viewer.as_ref().unwrap().to_string())
            .raw_str("\" />\n")
            .newline()
            // Use form_link_to to target registry contract
            .form_link_to("Create Board", "registry", "create_board")
            .newline()
            .newline()
            .render_link("Cancel", "/");

        Self::render_footer_into(md).build()
    }

    /// Render help page
    fn render_help(env: &Env, viewer: &Option<Address>) -> Bytes {
        let md = Self::render_nav(env, viewer)
            .newline()  // Blank line after nav-bar for markdown parsing
            .raw_str("<h1>Help</h1>\n")
            .raw_str("<h2>What is Soroban Boards?</h2>\n")
            .paragraph("Soroban Boards is a decentralized forum system running on Stellar's Soroban smart contract platform. All content is stored on-chain, and the UI is rendered directly from the smart contracts.")
            .raw_str("<h2>Features</h2>\n")
            .list_item("Create discussion boards")
            .list_item("Post threads and replies")
            .list_item("Nested comment threads")
            .list_item("Role-based permissions (Owner, Admin, Moderator, Member)")
            .list_item("Content moderation (flagging, banning)")
            .list_item("Progressive loading for large threads")
            .raw_str("<h2>How to Use</h2>\n")
            .list_item("Connect your Stellar wallet")
            .list_item("Browse existing boards or create a new one")
            .list_item("Create threads and reply to discussions")
            .list_item("Flag inappropriate content");
        Self::render_footer_into(md).build()
    }

    /// Render crosspost form
    /// Path format: /crosspost?from_board={id}&from_thread={id}
    fn render_crosspost(env: &Env, path: &Option<String>, viewer: &Option<Address>) -> Bytes {
        let mut md = Self::render_nav(env, viewer)
            .newline()
            .h1("Crosspost Thread");

        if viewer.is_none() {
            md = md.warning("Please connect your wallet to crosspost.");
            return Self::render_footer_into(md).build();
        }

        // Parse query params from path
        let (from_board, from_thread) = if let Some(ref p) = path {
            Self::parse_crosspost_params(env, p)
        } else {
            (0u64, 0u64)
        };

        if from_board == 0 || from_thread == 0 {
            md = md.warning("Invalid crosspost parameters.");
            return Self::render_footer_into(md).build();
        }

        // Get registry and content contract
        let registry: Address = env
            .storage()
            .instance()
            .get(&MainKey::Registry)
            .expect("Not initialized");

        // Get original board contract to get thread title
        let board_args: Vec<Val> = Vec::from_array(env, [from_board.into_val(env)]);
        let board_contract_opt: Option<Address> = env.invoke_contract(
            &registry,
            &Symbol::new(env, "get_board_contract"),
            board_args,
        );

        let Some(board_contract) = board_contract_opt else {
            md = md.warning("Original board not found.");
            return Self::render_footer_into(md).build();
        };

        // Get thread title
        let thread_args: Vec<Val> = Vec::from_array(env, [from_thread.into_val(env)]);
        let thread_info: Option<(String, Address)> = env
            .try_invoke_contract::<Option<(String, Address)>, soroban_sdk::Error>(
                &board_contract,
                &Symbol::new(env, "get_thread_title_and_author"),
                thread_args,
            )
            .ok()
            .and_then(|r| r.ok())
            .flatten();

        let Some((thread_title, _author)) = thread_info else {
            md = md.warning("Original thread not found.");
            return Self::render_footer_into(md).build();
        };

        // Get board list for target selection
        let boards_args: Vec<Val> = Vec::from_array(env, [0u64.into_val(env), 100u64.into_val(env)]);
        let boards: Vec<BoardMeta> = env.invoke_contract(
            &registry,
            &Symbol::new(env, "list_boards"),
            boards_args,
        );

        md = md
            .paragraph("Share this thread to another board.")
            .newline()
            .raw_str("<div class=\"crosspost-preview\">")
            .raw_str("<strong>Thread:</strong> ")
            .text_string(&thread_title)
            .raw_str("<br><strong>From:</strong> Board #")
            .number(from_board as u32)
            .raw_str("</div>\n")
            .newline()
            // Target board selection
            .raw_str("<div class=\"form-group\">")
            .raw_str("<label>Target Board:</label>\n")
            .raw_str("<select name=\"target_board_id\">\n");

        // Add board options (excluding the source board)
        for i in 0..boards.len() {
            let board = boards.get(i).unwrap();
            if board.id != from_board {
                md = md.raw_str("<option value=\"")
                    .number(board.id as u32)
                    .raw_str("\">")
                    .text_string(&board.name)
                    .raw_str("</option>\n");
            }
        }

        md = md.raw_str("</select>\n")
            .raw_str("</div>\n")
            .newline()
            // Optional comment
            .textarea("comment", 3, "Add a comment (optional)")
            .newline()
            // Hidden fields
            .raw_str("<input type=\"hidden\" name=\"original_board_id\" value=\"")
            .number(from_board as u32)
            .raw_str("\" />\n")
            .raw_str("<input type=\"hidden\" name=\"original_thread_id\" value=\"")
            .number(from_thread as u32)
            .raw_str("\" />\n")
            .raw_str("<input type=\"hidden\" name=\"caller\" value=\"")
            .text_string(&viewer.as_ref().unwrap().to_string())
            .raw_str("\" />\n")
            // Redirect after crosspost
            .redirect("/")
            .newline()
            // Submit button
            .form_link_to("Crosspost", "content", "create_crosspost")
            .newline()
            .newline()
            .raw_str("[Cancel](render:/b/")
            .number(from_board as u32)
            .raw_str("/t/")
            .number(from_thread as u32)
            .raw_str(")");

        Self::render_footer_into(md).build()
    }

    /// Parse crosspost query parameters from path
    fn parse_crosspost_params(_env: &Env, path: &String) -> (u64, u64) {
        let mut from_board: u64 = 0;
        let mut from_thread: u64 = 0;

        // Simple parsing for ?from_board=X&from_thread=Y
        let path_len = path.len() as usize;
        if path_len > 0 && path_len <= 256 {
            let mut buf = [0u8; 256];
            path.copy_into_slice(&mut buf[..path_len]);
            let path_str = core::str::from_utf8(&buf[..path_len]).unwrap_or("");

            // Find from_board=
            if let Some(start) = path_str.find("from_board=") {
                let start = start + 11;
                let end = path_str[start..].find('&').map(|i| start + i).unwrap_or(path_len);
                if let Ok(val) = path_str[start..end].parse::<u64>() {
                    from_board = val;
                }
            }

            // Find from_thread=
            if let Some(start) = path_str.find("from_thread=") {
                let start = start + 12;
                let end = path_str[start..].find('&').map(|i| start + i).unwrap_or(path_len);
                if let Ok(val) = path_str[start..end].parse::<u64>() {
                    from_thread = val;
                }
            }
        }

        (from_board, from_thread)
    }

    // ========================================================================
    // Delegation to Other Contracts
    // ========================================================================

    /// Delegate rendering to the community contract
    fn delegate_to_community(env: &Env, path: &String, viewer: &Option<Address>) -> Bytes {
        let community: Address = env
            .storage()
            .instance()
            .get(&MainKey::Community)
            .expect("Community contract not initialized");

        let args: Vec<Val> = Vec::from_array(env, [
            path.into_val(env),
            viewer.into_val(env),
        ]);
        env.invoke_contract(&community, &Symbol::new(env, "render"), args)
    }

    /// Delegate rendering to the community contract with the full path
    /// for community-specific routes like /c/{name}/*
    fn delegate_to_community_by_name(env: &Env, _name: &Bytes, path: &Option<String>, viewer: &Option<Address>) -> Bytes {
        let community: Address = env
            .storage()
            .instance()
            .get(&MainKey::Community)
            .expect("Community contract not initialized");

        // Pass the original path to community contract which will parse it
        let community_path = match path {
            Some(p) => p.clone(),
            None => String::from_str(env, "/"),
        };

        let args: Vec<Val> = Vec::from_array(env, [
            community_path.into_val(env),
            viewer.into_val(env),
        ]);
        env.invoke_contract(&community, &Symbol::new(env, "render"), args)
    }

    /// Delegate rendering to the admin contract
    fn delegate_to_admin(env: &Env, path: &Option<String>, viewer: &Option<Address>) -> Bytes {
        let admin: Address = env
            .storage()
            .instance()
            .get(&MainKey::Admin)
            .expect("Admin contract not initialized");

        let args: Vec<Val> = Vec::from_array(env, [
            path.into_val(env),
            viewer.into_val(env),
        ]);
        env.invoke_contract(&admin, &Symbol::new(env, "render"), args)
    }

    /// Delegate rendering to a board contract
    fn delegate_to_board(env: &Env, board_id: u64, path: &Option<String>, viewer: &Option<Address>) -> Bytes {
        let registry: Address = env
            .storage()
            .instance()
            .get(&MainKey::Registry)
            .expect("Not initialized");

        // Get board contract address from registry
        let board_args: Vec<Val> = Vec::from_array(env, [board_id.into_val(env)]);
        let board_contract_opt: Option<Address> = env.invoke_contract(
            &registry,
            &Symbol::new(env, "get_board_contract"),
            board_args,
        );

        let Some(board_contract) = board_contract_opt else {
            // Board not found - return error page
            return MarkdownBuilder::new(env)
                .h1("Board Not Found")
                .paragraph("The requested board does not exist.")
                .render_link("Back to Home", "/")
                .build();
        };

        // Convert path to relative path for board contract
        let relative_path = Self::strip_board_prefix(env, path, board_id);

        let args: Vec<Val> = Vec::from_array(env, [
            relative_path.into_val(env),
            viewer.into_val(env),
        ]);
        env.invoke_contract(&board_contract, &Symbol::new(env, "render"), args)
    }

    // ========================================================================
    // Helper Functions
    // ========================================================================

    /// Convert an Address to its contract ID string as Bytes
    fn address_to_contract_id_string(env: &Env, addr: &Address) -> Bytes {
        // Use the Address's to_string() which returns the C... format
        let addr_str = addr.to_string();
        let len = addr_str.len() as usize;

        // Copy string bytes into a buffer
        let mut buf = [0u8; 56]; // Contract IDs are 56 chars
        let copy_len = core::cmp::min(len, 56);
        addr_str.copy_into_slice(&mut buf[..copy_len]);

        Bytes::from_slice(env, &buf[..copy_len])
    }

    /// Strip the `/b/{id}` prefix from a path to get relative path for board contract
    fn strip_board_prefix(env: &Env, path: &Option<String>, board_id: u64) -> Option<String> {
        let Some(p) = path else {
            return Some(String::from_str(env, "/"));
        };

        let path_len = p.len() as usize;

        // Build the prefix: "/b/{board_id}"
        let mut prefix = [0u8; 32];
        let prefix_start = b"/b/";
        prefix[0..3].copy_from_slice(prefix_start);

        // Convert board_id to string
        let mut id_bytes = [0u8; 20];
        let id_len = Self::u64_to_bytes_buf(board_id, &mut id_bytes);
        prefix[3..3 + id_len].copy_from_slice(&id_bytes[0..id_len]);
        let prefix_len = 3 + id_len;

        // Copy path to buffer for comparison
        let mut path_buf = [0u8; 256];
        let copy_len = if path_len > 256 { 256 } else { path_len };
        p.copy_into_slice(&mut path_buf[0..copy_len]);

        // Check if path starts with prefix
        if copy_len >= prefix_len && &path_buf[0..prefix_len] == &prefix[0..prefix_len] {
            if copy_len == prefix_len {
                // Exact match like "/b/0", return "/"
                return Some(String::from_str(env, "/"));
            } else if path_buf[prefix_len] == b'/' {
                // Path has more after prefix
                let rest_slice = &path_buf[prefix_len..copy_len];
                return Some(String::from_bytes(env, rest_slice));
            }
        }

        // Fallback - return root
        Some(String::from_str(env, "/"))
    }

    /// Convert u64 to byte slice, return number of bytes written
    fn u64_to_bytes_buf(mut n: u64, buf: &mut [u8; 20]) -> usize {
        if n == 0 {
            buf[0] = b'0';
            return 1;
        }

        let mut temp = [0u8; 20];
        let mut len = 0;

        while n > 0 {
            temp[len] = b'0' + (n % 10) as u8;
            n /= 10;
            len += 1;
        }

        // Reverse into buf
        for i in 0..len {
            buf[i] = temp[len - 1 - i];
        }

        len
    }

    /// Format a Unix timestamp as a human-readable date string.
    fn format_timestamp(env: &Env, timestamp: u64) -> Bytes {
        // Handle legacy ledger sequence numbers (small values)
        if timestamp < 1_000_000_000 {
            let mut result = Bytes::from_slice(env, b"Ledger ");
            result.append(&Self::u64_to_bytes(env, timestamp));
            return result;
        }

        let total_seconds = timestamp;
        let total_minutes = total_seconds / 60;
        let total_hours = total_minutes / 60;
        let total_days = total_hours / 24;

        let minutes = (total_minutes % 60) as u8;
        let hours = (total_hours % 24) as u8;

        let (year, month, day) = Self::days_to_date(total_days as i64);

        // Format: "YYYY-MM-DD HH:MM UTC"
        let mut buffer = [0u8; 20];

        buffer[0] = b'0' + ((year / 1000) % 10) as u8;
        buffer[1] = b'0' + ((year / 100) % 10) as u8;
        buffer[2] = b'0' + ((year / 10) % 10) as u8;
        buffer[3] = b'0' + (year % 10) as u8;
        buffer[4] = b'-';
        buffer[5] = b'0' + ((month / 10) % 10) as u8;
        buffer[6] = b'0' + (month % 10) as u8;
        buffer[7] = b'-';
        buffer[8] = b'0' + ((day / 10) % 10) as u8;
        buffer[9] = b'0' + (day % 10) as u8;
        buffer[10] = b' ';
        buffer[11] = b'0' + ((hours / 10) % 10) as u8;
        buffer[12] = b'0' + (hours % 10) as u8;
        buffer[13] = b':';
        buffer[14] = b'0' + ((minutes / 10) % 10) as u8;
        buffer[15] = b'0' + (minutes % 10) as u8;
        buffer[16] = b' ';
        buffer[17] = b'U';
        buffer[18] = b'T';
        buffer[19] = b'C';

        Bytes::from_slice(env, &buffer)
    }

    /// Convert days since Unix epoch to (year, month, day).
    fn days_to_date(days: i64) -> (i32, u8, u8) {
        let z = days + 719468;
        let era = if z >= 0 { z } else { z - 146096 } / 146097;
        let doe = (z - era * 146097) as u32;
        let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
        let y = yoe as i64 + era * 400;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = doy - (153 * mp + 2) / 5 + 1;
        let m = if mp < 10 { mp + 3 } else { mp - 9 };
        let year = if m <= 2 { y + 1 } else { y };

        (year as i32, m as u8, d as u8)
    }

    /// Convert u64 to Bytes for display.
    fn u64_to_bytes(env: &Env, n: u64) -> Bytes {
        if n == 0 {
            return Bytes::from_slice(env, b"0");
        }

        let mut buffer = [0u8; 20];
        let mut idx = 20;
        let mut num = n;

        while num > 0 {
            idx -= 1;
            buffer[idx] = b'0' + (num % 10) as u8;
            num /= 10;
        }

        Bytes::from_slice(env, &buffer[idx..])
    }

    // ========================================================================
    // Upgradability
    // ========================================================================

    /// Upgrade the contract WASM
    pub fn upgrade(env: Env, new_wasm_hash: BytesN<32>) {
        let registry: Address = env
            .storage()
            .instance()
            .get(&MainKey::Registry)
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

        let contract_id = env.register(BoardsMain, ());
        let client = BoardsMainClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        let theme = Address::generate(&env);
        let permissions = Address::generate(&env);
        let content = Address::generate(&env);
        let admin = Address::generate(&env);
        let community = Address::generate(&env);

        client.init(&registry, &theme, &permissions, &content, &admin, &community);

        assert_eq!(client.get_registry(), registry);
        assert_eq!(client.get_theme(), theme);
        assert_eq!(client.get_community(), Some(community));
    }
}
