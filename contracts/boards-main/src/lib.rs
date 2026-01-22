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
use soroban_render_sdk::router::Request;
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
    /// Config contract address (for branding/settings)
    Config,
    /// Pages contract address (for static content pages)
    Pages,
}

/// Chunk metadata for progressive loading
#[contracttype]
#[derive(Clone)]
pub struct ChunkMeta {
    /// Number of chunks
    pub count: u32,
    /// Total bytes across all chunks
    pub total_bytes: u32,
    /// Version number (for cache invalidation)
    pub version: u32,
}

/// Board metadata (same structure as boards-board for compatibility)
#[contracttype]
#[derive(Clone)]
pub struct BoardMeta {
    pub id: u64,
    pub slug: String,
    pub name: String,
    pub description: String,
    pub creator: Address,
    pub created_at: u64,
    pub thread_count: u64,
    pub is_readonly: bool,
    pub is_private: bool,
    pub is_listed: bool,
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

/// Board configuration (same structure as boards-board for cross-contract calls)
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

/// Community metadata (same structure as community contract for compatibility)
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

/// Result of checking if a user can create a board or community
/// (duplicated from permissions contract for cross-contract calls)
#[contracttype]
#[derive(Clone)]
pub struct CanCreateResult {
    pub allowed: bool,
    pub is_bypass: bool,
    pub reason: String,
}

/// Minimal community info for board creation dropdown
/// (matches boards-community CommunityInfo)
#[contracttype]
#[derive(Clone)]
pub struct CommunityInfo {
    pub id: u64,
    pub name: String,
    pub display_name: String,
}

/// Page metadata (matches boards-pages PageMeta)
#[contracttype]
#[derive(Clone)]
pub struct PageMeta {
    pub id: u64,
    pub slug: String,
    pub name: String,
    pub nav_label: String,
    pub author: Address,
    pub created_at: u64,
    pub updated_at: u64,
    pub is_visible: bool,
    pub show_in_nav: bool,
    pub nav_order: u32,
}

#[contract]
pub struct BoardsMain;

#[contractimpl]
impl BoardsMain {
    /// Initialize the main contract with service contract addresses
    pub fn init(env: Env, registry: Address, theme: Address, permissions: Address, content: Address, admin: Address, community: Address, config: Address) {
        if env.storage().instance().has(&MainKey::Registry) {
            panic!("Already initialized");
        }

        env.storage().instance().set(&MainKey::Registry, &registry);
        env.storage().instance().set(&MainKey::Theme, &theme);
        env.storage().instance().set(&MainKey::Permissions, &permissions);
        env.storage().instance().set(&MainKey::Content, &content);
        env.storage().instance().set(&MainKey::Admin, &admin);
        env.storage().instance().set(&MainKey::Community, &community);
        env.storage().instance().set(&MainKey::Config, &config);
    }

    /// Get config contract address
    pub fn get_config(env: Env) -> Option<Address> {
        env.storage().instance().get(&MainKey::Config)
    }

    /// Set config contract address (for upgrades - requires registry admin auth)
    pub fn set_config(env: Env, config: Address, caller: Address) {
        caller.require_auth();

        // Verify caller is a registry admin
        let registry: Address = env
            .storage()
            .instance()
            .get(&MainKey::Registry)
            .expect("Not initialized");

        let admin_args: Vec<Val> = Vec::from_array(&env, [caller.clone().into_val(&env)]);
        let is_admin: bool = env.invoke_contract(&registry, &Symbol::new(&env, "is_admin"), admin_args);

        if !is_admin {
            panic!("Only registry admin can set config");
        }

        env.storage().instance().set(&MainKey::Config, &config);
    }

    /// Get community contract address
    pub fn get_community(env: Env) -> Option<Address> {
        env.storage()
            .instance()
            .get(&MainKey::Community)
    }

    /// Proxy function to create a community
    /// This allows forms on community pages (rendered via main) to call create_community
    pub fn create_community(
        env: Env,
        name: String,
        display_name: String,
        description: String,
        is_private: String,
        is_listed: String,
        caller: Address,
    ) -> u64 {
        // Authenticate at the root level so nested call doesn't fail
        caller.require_auth();

        let community: Address = env
            .storage()
            .instance()
            .get(&MainKey::Community)
            .expect("Community contract not initialized");

        let args: Vec<Val> = Vec::from_array(&env, [
            name.into_val(&env),
            display_name.into_val(&env),
            description.into_val(&env),
            is_private.into_val(&env),
            is_listed.into_val(&env),
            caller.into_val(&env),
        ]);

        env.invoke_contract(&community, &Symbol::new(&env, "create_community"), args)
    }

    /// Proxy function to update a community
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

        let community: Address = env
            .storage()
            .instance()
            .get(&MainKey::Community)
            .expect("Community contract not initialized");

        let args: Vec<Val> = Vec::from_array(&env, [
            community_id.into_val(&env),
            display_name.into_val(&env),
            description.into_val(&env),
            is_private.into_val(&env),
            is_listed.into_val(&env),
            caller.into_val(&env),
        ]);

        env.invoke_contract::<()>(&community, &Symbol::new(&env, "update_community"), args);
    }

    /// Proxy function to initiate ownership transfer
    pub fn initiate_transfer(
        env: Env,
        community_id: u64,
        new_owner: Address,
        caller: Address,
    ) {
        caller.require_auth();

        let community: Address = env
            .storage()
            .instance()
            .get(&MainKey::Community)
            .expect("Community contract not initialized");

        let args: Vec<Val> = Vec::from_array(&env, [
            community_id.into_val(&env),
            new_owner.into_val(&env),
            caller.into_val(&env),
        ]);

        env.invoke_contract::<()>(&community, &Symbol::new(&env, "initiate_transfer"), args);
    }

    /// Proxy function to cancel ownership transfer
    pub fn cancel_transfer(env: Env, community_id: u64, caller: Address) {
        caller.require_auth();

        let community: Address = env
            .storage()
            .instance()
            .get(&MainKey::Community)
            .expect("Community contract not initialized");

        let args: Vec<Val> = Vec::from_array(&env, [
            community_id.into_val(&env),
            caller.into_val(&env),
        ]);

        env.invoke_contract::<()>(&community, &Symbol::new(&env, "cancel_transfer"), args);
    }

    /// Proxy function to accept ownership transfer
    pub fn accept_transfer(env: Env, community_id: u64, caller: Address) {
        caller.require_auth();

        let community: Address = env
            .storage()
            .instance()
            .get(&MainKey::Community)
            .expect("Community contract not initialized");

        let args: Vec<Val> = Vec::from_array(&env, [
            community_id.into_val(&env),
            caller.into_val(&env),
        ]);

        env.invoke_contract::<()>(&community, &Symbol::new(&env, "accept_transfer"), args);
    }

    /// Proxy function to delete a community
    pub fn delete_community(env: Env, community_id: u64, caller: Address) {
        caller.require_auth();

        let community: Address = env
            .storage()
            .instance()
            .get(&MainKey::Community)
            .expect("Community contract not initialized");

        let args: Vec<Val> = Vec::from_array(&env, [
            community_id.into_val(&env),
            caller.into_val(&env),
        ]);

        env.invoke_contract::<()>(&community, &Symbol::new(&env, "delete_community"), args);
    }

    /// Proxy function to create a board (standalone or in a community)
    ///
    /// Delegates to the single boards-board contract to create a new board entry.
    /// If community is provided (non-empty), also adds the board to that community.
    ///
    /// NOTE: Parameter order matches form field DOM order (name, slug, description, community, is_private, is_listed)
    /// NOTE: Field is named 'community' not 'community_id' to avoid viewer's auto-conversion of _id fields to u64
    pub fn create_board(
        env: Env,
        name: String,
        slug: String,
        description: String,
        community: String,
        is_private: String,
        is_listed: String,
        caller: Address,
    ) -> u64 {
        caller.require_auth();

        let registry: Address = env
            .storage()
            .instance()
            .get(&MainKey::Registry)
            .expect("Not initialized");

        // Get board contract via registry alias
        let alias_args: Vec<Val> = Vec::from_array(&env, [Symbol::new(&env, "board").into_val(&env)]);
        let board_contract: Option<Address> = env.invoke_contract(
            &registry,
            &Symbol::new(&env, "get_contract_by_alias"),
            alias_args,
        );
        let board_contract = board_contract.expect("Board contract not registered");

        // Convert slug to Option - empty string or "-" sentinel means auto-generate from name
        // (viewer SDK strips empty values, so "-" is used as sentinel in hidden form field)
        let slug_opt: Option<String> = if slug.len() > 0 && slug != String::from_str(&env, "-") {
            Some(slug)
        } else {
            None
        };

        // Delegate to boards-board.create_board_with_slug()
        let create_args: Vec<Val> = Vec::from_array(&env, [
            name.into_val(&env),
            description.into_val(&env),
            is_private.into_val(&env),
            is_listed.into_val(&env),
            slug_opt.into_val(&env),
            caller.clone().into_val(&env),
        ]);
        let board_id: u64 = env.invoke_contract(&board_contract, &Symbol::new(&env, "create_board_with_slug"), create_args);

        // If community is specified, add board to community
        let community_id_parsed = string_to_u64(&env, &community);
        if let Some(cid) = community_id_parsed {
            let community_contract: Address = env
                .storage()
                .instance()
                .get(&MainKey::Community)
                .expect("Community contract not initialized");

            let add_args: Vec<Val> = Vec::from_array(&env, [
                cid.into_val(&env),
                board_id.into_val(&env),
                caller.into_val(&env),
            ]);
            env.invoke_contract::<()>(&community_contract, &Symbol::new(&env, "add_board"), add_args);
        }

        board_id
    }

    /// Set community contract address (for upgrades - requires registry admin auth)
    pub fn set_community(env: Env, community: Address, caller: Address) {
        caller.require_auth();

        // Verify caller is a registry admin
        let registry: Address = env
            .storage()
            .instance()
            .get(&MainKey::Registry)
            .expect("Not initialized");

        let admin_args: Vec<Val> = Vec::from_array(&env, [caller.clone().into_val(&env)]);
        let is_admin: bool = env.invoke_contract(&registry, &Symbol::new(&env, "is_admin"), admin_args);

        if !is_admin {
            panic!("Only registry admin can set community");
        }

        env.storage().instance().set(&MainKey::Community, &community);
    }

    /// Get pages contract address
    pub fn get_pages(env: Env) -> Option<Address> {
        env.storage().instance().get(&MainKey::Pages)
    }

    /// Set pages contract address (for upgrades - requires registry admin auth)
    pub fn set_pages(env: Env, pages: Address, caller: Address) {
        caller.require_auth();

        // Verify caller is a registry admin
        let registry: Address = env
            .storage()
            .instance()
            .get(&MainKey::Registry)
            .expect("Not initialized");

        let admin_args: Vec<Val> = Vec::from_array(&env, [caller.clone().into_val(&env)]);
        let is_admin: bool = env.invoke_contract(&registry, &Symbol::new(&env, "is_admin"), admin_args);

        if !is_admin {
            panic!("Only registry admin can set pages");
        }

        env.storage().instance().set(&MainKey::Pages, &pages);
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
            // My Account page
            .or_handle(b"/account", |_| Self::render_account(&env, &viewer))
            // Create board form (supports ?community=ID query param)
            // Router automatically strips query params for matching, so /create matches /create?community=5
            .or_handle(b"/create", |req| Self::render_create_board_from_request(&env, &req, &viewer))
            // Help page - delegate to pages contract (shows /p/help)
            .or_handle(b"/help", |_| {
                // Delegate to pages contract with /help path
                Self::delegate_to_pages(&env, &Some(String::from_str(&env, "/help")), &viewer)
            })
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
            // Admin routes - delegate to admin contract (keep numeric IDs for admin)
            .or_handle(b"/admin/settings", |_| Self::delegate_to_admin(&env, &path, &viewer))
            .or_handle(b"/admin/settings/*", |_| Self::delegate_to_admin(&env, &path, &viewer))
            .or_handle(b"/admin/*", |_| Self::delegate_to_admin(&env, &path, &viewer))
            .or_handle(b"/b/{id}/members", |_| Self::delegate_to_admin(&env, &path, &viewer))
            .or_handle(b"/b/{id}/banned", |_| Self::delegate_to_admin(&env, &path, &viewer))
            .or_handle(b"/b/{id}/flags", |_| Self::delegate_to_admin(&env, &path, &viewer))
            .or_handle(b"/b/{id}/settings", |_| Self::delegate_to_admin(&env, &path, &viewer))
            .or_handle(b"/b/{id}/invites", |_| Self::delegate_to_admin(&env, &path, &viewer))
            // Pages routes - delegate to pages contract
            .or_handle(b"/p/*", |_| Self::delegate_to_pages(&env, &path, &viewer))
            .or_handle(b"/p", |_| Self::delegate_to_pages(&env, &path, &viewer))
            // Board routes - delegate to board contract using slug
            // For standalone boards: /b/{slug}/*
            // For community boards accessed via /b/{slug}: redirect to /c/{community}/b/{slug}
            .or_handle(b"/b/{slug}/*", |req| {
                let slug = req.get_var(b"slug").unwrap_or(Bytes::new(&env));
                Self::delegate_to_board_by_slug(&env, &slug, &path, &viewer)
            })
            .or_handle(b"/b/{slug}", |req| {
                let slug = req.get_var(b"slug").unwrap_or(Bytes::new(&env));
                Self::delegate_to_board_by_slug(&env, &slug, &path, &viewer)
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
    // Include Functions (for {{include}} tags from other contracts)
    // ========================================================================

    /// Render navigation bar for inclusion via {{include}} tag.
    /// Returns just the nav-bar div without meta tags or aliases.
    ///
    /// Usage: {{include contract=@main func="render_nav_include" viewer return_path="/b/1"}}
    /// - viewer: tells the viewer to pass the current viewer address
    /// - return_path: optional path for profile "Go Back" link (defaults to "/")
    pub fn render_nav_include(
        env: Env,
        viewer: Option<Address>,
        return_path: Option<String>,
    ) -> Bytes {
        Self::render_nav_content(&env, &viewer, return_path).build()
    }

    /// Render footer for inclusion via {{include}} tag.
    /// Returns the footer div with content from config contract.
    ///
    /// Usage: {{include contract=@main func="render_footer_include"}}
    pub fn render_footer_include(env: Env) -> Bytes {
        let footer_include = Self::config_include(&env, b"footer_text");
        MarkdownBuilder::new(&env)
            .div_start("footer")
            .raw(footer_include)
            .div_end()
            .build()
    }

    // ========================================================================
    // Navigation
    // ========================================================================

    /// Build an include tag for config contract: {{include contract=CONTRACT_ID func="name"}}
    fn config_include(env: &Env, func_name: &[u8]) -> Bytes {
        let config_opt: Option<Address> = env.storage().instance().get(&MainKey::Config);

        if let Some(config) = config_opt {
            let config_id = Self::address_to_contract_id_string(env, &config);
            let mut result = Bytes::from_slice(env, b"{{include contract=");
            result.append(&config_id);
            result.append(&Bytes::from_slice(env, b" func=\""));
            result.append(&Bytes::from_slice(env, func_name));
            result.append(&Bytes::from_slice(env, b"\"}}"));
            result
        } else {
            // Fallback if config not set
            match func_name {
                b"site_name" => Bytes::from_slice(env, b"Soroban Boards"),
                b"tagline" => Bytes::from_slice(env, b"Decentralized discussion forums on Stellar"),
                b"footer_text" => Bytes::from_slice(env, b"Powered by Soroban Render on Stellar"),
                _ => Bytes::new(env),
            }
        }
    }

    /// Emit {{aliases ...}} tag with all known contract alias-to-ID mappings.
    /// This allows content to use friendly names like `{{include contract=config func="logo"}}`
    /// instead of full 56-character contract IDs.
    fn emit_aliases(env: &Env) -> Bytes {
        let mut result = Bytes::from_slice(env, b"{{aliases ");

        // Helper closure to add an alias entry
        let add_alias = |result: &mut Bytes, name: &[u8], addr: &Address| {
            result.append(&Bytes::from_slice(env, name));
            result.append(&Bytes::from_slice(env, b"="));
            result.append(&Self::address_to_contract_id_string(env, addr));
            result.append(&Bytes::from_slice(env, b" "));
        };

        // Add main contract (self)
        let self_addr = env.current_contract_address();
        add_alias(&mut result, b"main", &self_addr);

        // Add stored contract references
        if let Some(config) = env.storage().instance().get::<_, Address>(&MainKey::Config) {
            add_alias(&mut result, b"config", &config);
        }
        if let Some(registry) = env.storage().instance().get::<_, Address>(&MainKey::Registry) {
            add_alias(&mut result, b"registry", &registry);
        }
        if let Some(theme) = env.storage().instance().get::<_, Address>(&MainKey::Theme) {
            add_alias(&mut result, b"theme", &theme);
        }
        if let Some(perms) = env.storage().instance().get::<_, Address>(&MainKey::Permissions) {
            add_alias(&mut result, b"perms", &perms);
        }
        if let Some(content) = env.storage().instance().get::<_, Address>(&MainKey::Content) {
            add_alias(&mut result, b"content", &content);
        }
        if let Some(admin) = env.storage().instance().get::<_, Address>(&MainKey::Admin) {
            add_alias(&mut result, b"admin", &admin);
        }
        if let Some(community) = env.storage().instance().get::<_, Address>(&MainKey::Community) {
            add_alias(&mut result, b"community", &community);
        }
        if let Some(pages) = env.storage().instance().get::<_, Address>(&MainKey::Pages) {
            add_alias(&mut result, b"pages", &pages);
        }

        // Look up additional aliases from registry (board, voting, profile)
        if let Some(registry) = env.storage().instance().get::<_, Address>(&MainKey::Registry) {
            // Board contract
            let board_args: Vec<Val> = Vec::from_array(env, [Symbol::new(env, "board").into_val(env)]);
            if let Some(board) = env.try_invoke_contract::<Option<Address>, soroban_sdk::Error>(
                &registry,
                &Symbol::new(env, "get_contract_by_alias"),
                board_args,
            ).ok().and_then(|r| r.ok()).flatten() {
                add_alias(&mut result, b"board", &board);
            }

            // Voting contract
            let voting_args: Vec<Val> = Vec::from_array(env, [Symbol::new(env, "voting").into_val(env)]);
            if let Some(voting) = env.try_invoke_contract::<Option<Address>, soroban_sdk::Error>(
                &registry,
                &Symbol::new(env, "get_contract_by_alias"),
                voting_args,
            ).ok().and_then(|r| r.ok()).flatten() {
                add_alias(&mut result, b"voting", &voting);
            }

            // Profile contract
            let profile_args: Vec<Val> = Vec::from_array(env, [Symbol::new(env, "profile").into_val(env)]);
            if let Some(profile) = env.try_invoke_contract::<Option<Address>, soroban_sdk::Error>(
                &registry,
                &Symbol::new(env, "get_contract_by_alias"),
                profile_args,
            ).ok().and_then(|r| r.ok()).flatten() {
                add_alias(&mut result, b"profile", &profile);
            }
        }

        result.append(&Bytes::from_slice(env, b"}}"));
        result
    }

    /// Render the navigation bar with profile link (for use within boards-main)
    /// Includes meta tags and aliases at the start for document setup.
    fn render_nav<'a>(env: &'a Env, viewer: &Option<Address>) -> MarkdownBuilder<'a> {
        // Meta tags for document head (favicon, title, theme-color)
        let meta_include = Self::config_include(env, b"meta");

        // Emit aliases for include resolution
        let aliases_tag = Self::emit_aliases(env);

        // Start with meta and aliases, then add nav content
        let mut md = MarkdownBuilder::new(env)
            .raw(meta_include)
            .raw(aliases_tag);

        // Build return path for profile links: {CONTRACT_ID}:/
        let self_addr = env.current_contract_address();
        let self_id_str = Self::address_to_contract_id_string(env, &self_addr);
        let mut return_path_bytes = self_id_str;
        return_path_bytes.append(&Bytes::from_slice(env, b":/"));

        // Convert to String for render_nav_content
        let return_path = Self::bytes_to_string(env, &return_path_bytes);

        // Append the nav content
        md = Self::render_nav_content_into(env, md, viewer, Some(return_path));
        md
    }

    /// Core navigation bar content (shared between render_nav and render_nav_include)
    /// Returns just the nav-bar div without meta tags or aliases.
    fn render_nav_content<'a>(
        env: &'a Env,
        viewer: &Option<Address>,
        return_path: Option<String>,
    ) -> MarkdownBuilder<'a> {
        Self::render_nav_content_into(env, MarkdownBuilder::new(env), viewer, return_path)
    }

    /// Build nav content into an existing MarkdownBuilder
    fn render_nav_content_into<'a>(
        env: &'a Env,
        md: MarkdownBuilder<'a>,
        viewer: &Option<Address>,
        return_path: Option<String>,
    ) -> MarkdownBuilder<'a> {
        let registry: Address = env
            .storage()
            .instance()
            .get(&MainKey::Registry)
            .expect("Not initialized");

        let site_name_include = Self::config_include(env, b"site_name");
        let mut md = md
            .div_start("nav-bar")
            .raw_str("<a href=\"render:/\">")
            .raw(site_name_include)
            .raw_str("</a>")
            .render_link("Communities", "/communities");

        // Add dynamic page links from pages contract
        let pages_opt: Option<Address> = env.storage().instance().get(&MainKey::Pages);
        if let Some(pages_addr) = pages_opt {
            let nav_pages: Vec<PageMeta> = env
                .try_invoke_contract::<Vec<PageMeta>, soroban_sdk::Error>(
                    &pages_addr,
                    &Symbol::new(env, "get_nav_pages"),
                    Vec::new(env),
                )
                .ok()
                .and_then(|r| r.ok())
                .unwrap_or_else(|| Vec::new(env));

            for page in nav_pages.iter() {
                let label = if page.nav_label.len() > 0 {
                    page.nav_label.clone()
                } else {
                    page.name.clone()
                };
                md = md.raw_str("<a href=\"render:/p/")
                    .text_string(&page.slug)
                    .raw_str("\">")
                    .text_string(&label)
                    .raw_str("</a>");
            }
        } else {
            md = md.render_link("Help", "/help");
        }

        // Add My Account link for logged-in users
        if viewer.is_some() {
            md = md.render_link("My Account", "/account");
        }

        // Add profile link if profile contract is registered
        let profile_alias = Symbol::new(env, "profile");
        let alias_args: Vec<Val> = Vec::from_array(env, [profile_alias.into_val(env)]);
        let profile_opt: Option<Address> = env.invoke_contract(
            &registry,
            &Symbol::new(env, "get_contract_by_alias"),
            alias_args,
        );

        // Build return path bytes for profile link
        let return_path_bytes = match &return_path {
            Some(path) => Self::string_to_bytes(env, path),
            None => {
                // Default to @main:/
                Bytes::from_slice(env, b"@main:/")
            }
        };

        if let Some(profile_addr) = profile_opt {
            let args: Vec<Val> = Vec::from_array(env, [
                viewer.into_val(env),
                return_path_bytes.into_val(env),
            ]);
            let profile_link: Bytes = env.invoke_contract(
                &profile_addr,
                &Symbol::new(env, "render_nav_link_return"),
                args,
            );
            md = md.raw(profile_link);
        } else if viewer.is_some() {
            md = md
                .raw_str("<a href=\"render:@profile:/register/from/")
                .raw(return_path_bytes)
                .raw_str("\">Create Profile</a>");
        }

        md.div_end()
    }

    /// Convert Bytes to String
    fn bytes_to_string(env: &Env, bytes: &Bytes) -> String {
        let len = bytes.len() as usize;
        let mut buf = [0u8; 128];
        let copy_len = core::cmp::min(len, 128);
        bytes.copy_into_slice(&mut buf[..copy_len]);
        String::from_str(env, core::str::from_utf8(&buf[..copy_len]).unwrap_or(""))
    }

    /// Convert String to Bytes
    fn string_to_bytes(env: &Env, s: &String) -> Bytes {
        let len = s.len() as usize;
        let mut buf = [0u8; 128];
        let copy_len = core::cmp::min(len, 128);
        s.copy_into_slice(&mut buf[..copy_len]);
        Bytes::from_slice(env, &buf[..copy_len])
    }

    /// Try to parse a String as a u64 (for handling numeric board IDs in URLs)
    fn try_parse_u64(s: &String) -> Option<u64> {
        let len = s.len() as usize;
        if len == 0 || len > 20 {
            return None;
        }
        let mut buf = [0u8; 20];
        s.copy_into_slice(&mut buf[..len]);

        let mut result: u64 = 0;
        for i in 0..len {
            let c = buf[i];
            if c < b'0' || c > b'9' {
                return None;
            }
            result = result.checked_mul(10)?.checked_add((c - b'0') as u64)?;
        }
        Some(result)
    }

    /// Append footer to builder - uses include for progressive loading
    fn render_footer_into<'a>(env: &'a Env, md: MarkdownBuilder<'a>) -> MarkdownBuilder<'a> {
        let footer_include = Self::config_include(env, b"footer_text");
        md.div_start("footer")
            .raw(footer_include)
            .div_end()
    }

    // ========================================================================
    // Page Rendering
    // ========================================================================

    /// Render the home page with communities and standalone boards
    fn render_home(env: &Env, viewer: &Option<Address>) -> Bytes {
        let registry: Address = env
            .storage()
            .instance()
            .get(&MainKey::Registry)
            .expect("Not initialized");

        // Branding loaded via includes for progressive loading
        let site_name_include = Self::config_include(env, b"site_name");
        let tagline_include = Self::config_include(env, b"tagline");

        let mut md = Self::render_nav(env, viewer)
            .newline()  // Blank line after nav-bar div for markdown parsing
            .raw_str("<h1>")
            .raw(site_name_include)
            .raw_str("</h1>\n<p>")
            .raw(tagline_include)
            .raw_str("</p>\n")
            .newline();  // Blank line before callout for markdown parsing

        // Show connection status
        if viewer.is_some() {
            md = md.tip("Wallet connected! You can create boards and post.");
        } else {
            md = md.note("Connect your wallet to participate in discussions.");
        }

        // === Communities Section ===
        md = md.newline()
            .h2("Communities");

        // Get community contract
        let community_contract_opt: Option<Address> = env
            .storage()
            .instance()
            .get(&MainKey::Community);

        if let Some(ref community_contract) = community_contract_opt {
            // Fetch listed communities
            let list_args: Vec<Val> = Vec::from_array(env, [0u64.into_val(env), 50u64.into_val(env)]);
            let communities: Vec<CommunityMeta> = env.invoke_contract(
                community_contract,
                &Symbol::new(env, "list_listed_communities"),
                list_args,
            );

            if communities.is_empty() {
                md = md.paragraph("No communities yet.");
            } else {
                // Sort communities alphabetically by name
                let sorted_communities = Self::sort_communities_by_name(env, communities);

                md = md.raw_str("<div class=\"community-list\">\n");
                for community in sorted_communities.iter() {
                    md = Self::render_community_card(env, md, &community);
                }
                md = md.raw_str("</div>\n");
            }

            // Show community creation link based on permissions
            md = md.newline();
            if let Some(ref v) = viewer {
                // Check if user can create communities
                if let Some(permissions) = env.storage().instance().get::<_, Address>(&MainKey::Permissions) {
                    let can_create_args: Vec<Val> = Vec::from_array(env, [v.clone().into_val(env)]);
                    let can_create: CanCreateResult = env
                        .try_invoke_contract::<CanCreateResult, soroban_sdk::Error>(
                            &permissions,
                            &Symbol::new(env, "can_create_community"),
                            can_create_args,
                        )
                        .unwrap_or_else(|_| Ok(CanCreateResult {
                            allowed: true,
                            is_bypass: false,
                            reason: String::from_str(env, ""),
                        }))
                        .unwrap_or_else(|_| CanCreateResult {
                            allowed: true,
                            is_bypass: false,
                            reason: String::from_str(env, ""),
                        });

                    if can_create.allowed {
                        md = md.render_link("+ Create New Community", "/new");
                        if can_create.is_bypass {
                            md = md.raw_str(" <span class=\"badge-admin\">Admin</span>");
                        }
                    } else {
                        md = md.raw_str("<span class=\"action-disabled\" title=\"")
                            .text_string(&can_create.reason)
                            .raw_str("\">+ Create New Community</span>");
                    }
                } else {
                    // No permissions contract - allow by default
                    md = md.render_link("+ Create New Community", "/new");
                }
            } else {
                // Not logged in - show link anyway (will be blocked at form)
                md = md.render_link("+ Create New Community", "/new");
            }
        } else {
            md = md.paragraph("Community features not configured.");
        }

        // === Standalone Boards Section ===
        md = md.newline()
            .h2("Standalone Boards");

        // Get board contract via registry alias
        let board_alias_args: Vec<Val> = Vec::from_array(env, [Symbol::new(env, "board").into_val(env)]);
        let board_contract_opt: Option<Address> = env.invoke_contract(
            &registry,
            &Symbol::new(env, "get_contract_by_alias"),
            board_alias_args,
        );

        if let Some(board_contract) = board_contract_opt {
            // Get listed boards directly from board contract
            let list_args: Vec<Val> = Vec::from_array(env, [0u64.into_val(env), 50u64.into_val(env)]);
            let listed_boards: Vec<BoardMeta> = env
                .try_invoke_contract::<Vec<BoardMeta>, soroban_sdk::Error>(
                    &board_contract,
                    &Symbol::new(env, "list_listed_boards"),
                    list_args,
                )
                .ok()
                .and_then(|r| r.ok())
                .unwrap_or_else(|| Vec::new(env));

            // Filter out boards that are in communities
            let mut standalone_boards: Vec<BoardMeta> = Vec::new(env);
            for board in listed_boards.iter() {
                // Check if board is in a community
                let community_id: Option<u64> = if let Some(ref community_contract) = community_contract_opt {
                    let community_args: Vec<Val> = Vec::from_array(env, [board.id.into_val(env)]);
                    env.try_invoke_contract::<Option<u64>, soroban_sdk::Error>(
                        community_contract,
                        &Symbol::new(env, "get_board_community"),
                        community_args,
                    )
                    .ok()
                    .and_then(|r| r.ok())
                    .flatten()
                } else {
                    None
                };

                // Only include standalone boards (not in any community)
                if community_id.is_none() {
                    standalone_boards.push_back(board);
                }

                // Limit to 20 boards
                if standalone_boards.len() >= 20 {
                    break;
                }
            }

            if standalone_boards.is_empty() {
                md = md.paragraph("No standalone boards yet.");
            } else {
                // Sort boards alphabetically by name
                let sorted_boards = Self::sort_boards_by_name(env, standalone_boards);

                md = md.raw_str("<div class=\"board-list\">\n");
                for board in sorted_boards.iter() {
                    // Board card with link wrapper - use slug-based URL: /b/{slug}
                    md = md.raw_str("<a href=\"render:/b/")
                        .text_string(&board.slug)
                        .raw_str("\" class=\"board-card\"><span class=\"board-card-title\">")
                        .text_string(&board.name)
                        .raw_str("</span><span class=\"board-card-desc\">")
                        .text_string(&board.description)
                        .raw_str("</span><span class=\"board-card-meta\">")
                        .number(board.thread_count as u32)
                        .text(" threads · ")
                        .raw(Self::format_timestamp(env, board.created_at));
                    if board.is_private {
                        md = md.raw_str(" <span class=\"badge\">private</span>");
                    }
                    md = md.raw_str("</span></a>\n");
                }
                md = md.raw_str("</div>\n");
            }
        } else {
            md = md.paragraph("Board service not configured.");
        }

        // Show board creation link based on permissions
        md = md.newline();
        if let Some(ref v) = viewer {
            // Check if user can create boards
            if let Some(permissions) = env.storage().instance().get::<_, Address>(&MainKey::Permissions) {
                let can_create_args: Vec<Val> = Vec::from_array(env, [v.clone().into_val(env)]);
                let can_create: CanCreateResult = env
                    .try_invoke_contract::<CanCreateResult, soroban_sdk::Error>(
                        &permissions,
                        &Symbol::new(env, "can_create_board"),
                        can_create_args,
                    )
                    .unwrap_or_else(|_| Ok(CanCreateResult {
                        allowed: false,
                        is_bypass: false,
                        reason: String::from_str(env, "Permission check unavailable"),
                    }))
                    .unwrap_or_else(|_| CanCreateResult {
                        allowed: false,
                        is_bypass: false,
                        reason: String::from_str(env, "Permission check failed"),
                    });

                if can_create.allowed {
                    md = md.render_link("+ Create New Board", "/create");
                    if can_create.is_bypass {
                        md = md.raw_str(" <span class=\"badge-admin\">Admin</span>");
                    }
                } else {
                    md = md.raw_str("<span class=\"action-disabled\" title=\"")
                        .text_string(&can_create.reason)
                        .raw_str("\">+ Create New Board</span>");
                }
            } else {
                // No permissions contract - deny with warning
                md = md.raw_str("<span class=\"action-disabled\" title=\"System not fully configured\">+ Create New Board</span>");
            }
        } else {
            // Not logged in - show link anyway (will be blocked at form)
            md = md.render_link("+ Create New Board", "/create");
        }

        // Check if viewer is a site admin before showing admin links
        let is_admin = if let Some(ref user) = viewer {
            if let Some(registry) = env.storage().instance().get::<_, Address>(&MainKey::Registry) {
                let admin_args: Vec<Val> = Vec::from_array(env, [user.clone().into_val(env)]);
                env.try_invoke_contract::<bool, soroban_sdk::Error>(
                    &registry,
                    &Symbol::new(env, "is_admin"),
                    admin_args,
                )
                .ok()
                .and_then(|r| r.ok())
                .unwrap_or(false) // Fail-closed: if check fails, don't show admin links
            } else {
                false
            }
        } else {
            false
        };

        // Show admin links only if viewer is an admin
        if is_admin {
            md = md
                .text(" | ")
                .render_link("Site Settings", "/admin/settings")
                .text(" | ")
                .render_link("Registry Admin", "/admin/registry");
        }

        md = md.newline();

        Self::render_footer_into(env, md).build()
    }

    /// Render create board form using Router Request (new version with query param support)
    fn render_create_board_from_request(env: &Env, req: &Request, viewer: &Option<Address>) -> Bytes {
        let mut md = Self::render_nav(env, viewer)
            .newline()  // Blank line after nav-bar for markdown parsing
            .h1("Create New Board");

        if viewer.is_none() {
            md = md.warning("Please connect your wallet to create a board.");
            return Self::render_footer_into(env, md).build();
        }

        let user = viewer.as_ref().unwrap();

        // Get pre-selected community from query param using Request accessor
        let preselected_community = req.get_query_param_u64(b"community");

        // Get communities user can manage
        let communities = Self::get_manageable_communities_for_user(env, user);

        md = md
            .paragraph("Create a new discussion board.")
            .newline()
            .redirect("/")  // Return to board list after creating board
            .input("name", "Board name")
            .newline()
            // Slug field - use "-" as default value (sentinel for auto-generate)
            // Empty values get stripped by viewer SDK, so we use "-" as a non-empty sentinel
            // User can replace "-" with their own slug, or leave it for auto-generation
            .raw_str("<div class=\"form-group\">\n")
            .raw_str("<label for=\"slug\">URL slug (optional):</label>\n")
            .raw_str("<input type=\"text\" name=\"slug\" id=\"slug\" value=\"-\" placeholder=\"auto-generated-from-name\" pattern=\"-|[a-z][a-z0-9-]{2,29}\" title=\"Leave as - for auto-generate, or enter 3-30 lowercase letters, numbers, and hyphens starting with a letter.\" />\n")
            .raw_str("<small>Leave as '-' to auto-generate from board name, or enter your own slug</small>\n")
            .raw_str("</div>\n")
            .newline()
            .textarea_markdown("description", 3, "Board description")
            .newline();

        // Community selection dropdown (if user has any manageable communities)
        if !communities.is_empty() {
            // Hidden fallback with preselected value (select elements may not be captured correctly)
            md = md.raw_str("<input type=\"hidden\" name=\"community\" value=\"");
            if let Some(cid) = preselected_community {
                md = md.number(cid as u32);
            } else {
                md = md.raw_str("none");
            }
            md = md.raw_str("\" />\n");

            md = md.raw_str("<div class=\"form-group\">\n")
                .raw_str("<label>Community (optional):</label>\n")
                .raw_str("<select name=\"community\">\n")
                .raw_str("<option value=\"none\">None (standalone board)</option>\n");

            for i in 0..communities.len() {
                if let Some(community) = communities.get(i) {
                    let is_selected = preselected_community == Some(community.id);

                    md = md.raw_str("<option value=\"");
                    md = md.number(community.id as u32);
                    md = md.raw_str("\"");
                    if is_selected {
                        md = md.raw_str(" selected");
                    }
                    md = md.raw_str(">");
                    md = md.text_string(&community.display_name);
                    md = md.raw_str("</option>\n");
                }
            }

            md = md.raw_str("</select>\n</div>\n").newline();
        } else {
            // Hidden non-empty value for form consistency (viewer skips empty values)
            // "none" will fail to parse as u64, resulting in None
            md = md.raw_str("<input type=\"hidden\" name=\"community\" value=\"none\" />\n");
        }

        md = md
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
            .text_string(&user.to_string())
            .raw_str("\" />\n")
            .newline()
            // Use form_link_to to target main contract (proxies to board creation)
            .form_link_to("Create Board", "main", "create_board")
            .newline()
            .newline()
            .render_link("Cancel", "/");

        Self::render_footer_into(env, md).build()
    }

    /// Render My Account page
    fn render_account(env: &Env, viewer: &Option<Address>) -> Bytes {
        let mut md = Self::render_nav(env, viewer)
            .newline()  // Blank line after nav-bar for markdown parsing
            .h1("My Account");

        let Some(user) = viewer else {
            md = md.warning("Please connect your wallet to view your account information.");
            return Self::render_footer_into(env, md).build();
        };

        let registry: Address = env
            .storage()
            .instance()
            .get(&MainKey::Registry)
            .expect("Not initialized");

        // Get permissions contract for account age
        let perms_args: Vec<Val> = Vec::from_array(env, [Symbol::new(env, "perms").into_val(env)]);
        let perms_opt: Option<Address> = env.invoke_contract(
            &registry,
            &Symbol::new(env, "get_contract_by_alias"),
            perms_args,
        );

        // Get profile contract (if available)
        let profile_args: Vec<Val> = Vec::from_array(env, [Symbol::new(env, "profile").into_val(env)]);
        let profile_opt: Option<Address> = env.invoke_contract(
            &registry,
            &Symbol::new(env, "get_contract_by_alias"),
            profile_args,
        );

        // === User Identity Section ===
        md = md.h2("Identity");

        // Show wallet address
        md = md
            .text("**Wallet Address:** ")
            .raw_str("<code class=\"address\">")
            .text_string(&user.to_string())
            .raw_str("</code>")
            .newline()
            .newline();

        // Use the profile contract's nav link rendering (same as header)
        // This handles profile detection internally and returns appropriate HTML
        if let Some(ref profile_addr) = profile_opt {
            // Build return path for the profile page's "Go Back" button
            let self_addr = env.current_contract_address();
            let self_id_str = Self::address_to_contract_id_string(env, &self_addr);
            let mut return_path = self_id_str;
            return_path.append(&Bytes::from_slice(env, b":/account"));

            let args: Vec<Val> = Vec::from_array(env, [
                viewer.into_val(env),
                return_path.into_val(env),
            ]);

            // Call render_nav_link_return - it returns either:
            // - "Create Profile" link if no profile
            // - "@username" link if profile exists
            let profile_link_opt: Option<Bytes> = env
                .try_invoke_contract::<Bytes, soroban_sdk::Error>(
                    profile_addr,
                    &Symbol::new(env, "render_nav_link_return"),
                    args,
                )
                .ok()
                .and_then(|r| r.ok());

            if let Some(profile_link) = profile_link_opt {
                md = md
                    .text("**Profile:** ")
                    .raw(profile_link)
                    .newline()
                    .newline();
            }
        }

        // === Account Stats Section ===
        md = md.h2("Account Statistics");

        if let Some(perms_addr) = perms_opt {
            // Get account age
            let age_args: Vec<Val> = Vec::from_array(env, [user.into_val(env)]);
            let account_age_secs: u64 = env
                .try_invoke_contract::<u64, soroban_sdk::Error>(
                    &perms_addr,
                    &Symbol::new(env, "get_account_age"),
                    age_args.clone(),
                )
                .ok()
                .and_then(|r| r.ok())
                .unwrap_or(0);

            // Get first seen timestamp
            let first_seen_opt: Option<u64> = env
                .try_invoke_contract::<Option<u64>, soroban_sdk::Error>(
                    &perms_addr,
                    &Symbol::new(env, "get_first_seen"),
                    age_args,
                )
                .ok()
                .and_then(|r| r.ok())
                .flatten();

            if let Some(first_seen) = first_seen_opt {
                md = md.text("**First Seen:** ").raw(Self::format_timestamp(env, first_seen)).newline().newline();

                // Format account age nicely
                let days = account_age_secs / 86400;
                let hours = (account_age_secs % 86400) / 3600;
                let minutes = (account_age_secs % 3600) / 60;

                md = md.text("**Account Age:** ");
                if days > 0 {
                    md = md.number(days as u32).text(" days, ");
                }
                if hours > 0 || days > 0 {
                    md = md.number(hours as u32).text(" hours, ");
                }
                md = md.number(minutes as u32).text(" minutes");
                md = md.newline().newline();
            } else {
                md = md.text("**Account Age:** New account (not yet recorded)").newline().newline();
            }
        } else {
            md = md.text("*Account statistics not available*").newline().newline();
        }

        // === Quick Links Section ===
        md = md.h2("Quick Links")
            .raw_str("<div class=\"quick-links\">\n")
            .render_link("Home", "/")
            .render_link("Communities", "/communities")
            .render_link("Help", "/help")
            .raw_str("</div>\n");

        Self::render_footer_into(env, md).build()
    }

    /// Render crosspost form
    /// Path format: /crosspost?from_board={id}&from_thread={id}
    fn render_crosspost(env: &Env, path: &Option<String>, viewer: &Option<Address>) -> Bytes {
        let mut md = Self::render_nav(env, viewer)
            .newline()
            .h1("Crosspost Thread");

        if viewer.is_none() {
            md = md.warning("Please connect your wallet to crosspost.");
            return Self::render_footer_into(env, md).build();
        }

        // Parse query params from path
        let (from_board, from_thread) = if let Some(ref p) = path {
            Self::parse_crosspost_params(env, p)
        } else {
            (0u64, 0u64)
        };

        if from_board == 0 || from_thread == 0 {
            md = md.warning("Invalid crosspost parameters.");
            return Self::render_footer_into(env, md).build();
        }

        // Get registry
        let registry: Address = env
            .storage()
            .instance()
            .get(&MainKey::Registry)
            .expect("Not initialized");

        // Get board contract via registry alias
        let alias_args: Vec<Val> = Vec::from_array(env, [Symbol::new(env, "board").into_val(env)]);
        let board_contract_opt: Option<Address> = env.invoke_contract(
            &registry,
            &Symbol::new(env, "get_contract_by_alias"),
            alias_args,
        );

        let Some(board_contract) = board_contract_opt else {
            md = md.warning("Board service not configured.");
            return Self::render_footer_into(env, md).build();
        };

        // Get thread title from original board
        let thread_args: Vec<Val> = Vec::from_array(env, [from_board.into_val(env), from_thread.into_val(env)]);
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
            return Self::render_footer_into(env, md).build();
        };

        // Get all boards for target selection
        let list_args: Vec<Val> = Vec::from_array(env, [0u64.into_val(env), 50u64.into_val(env)]);
        let boards: Vec<BoardMeta> = env
            .try_invoke_contract::<Vec<BoardMeta>, soroban_sdk::Error>(
                &board_contract,
                &Symbol::new(env, "list_boards"),
                list_args,
            )
            .ok()
            .and_then(|r| r.ok())
            .unwrap_or_else(|| Vec::new(env));

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
        for board in boards.iter() {
            if board.id == from_board {
                continue;
            }

            md = md.raw_str("<option value=\"")
                .number(board.id as u32)
                .raw_str("\">")
                .text_string(&board.name)
                .raw_str("</option>\n");
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

        Self::render_footer_into(env, md).build()
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

    /// Get communities the user can manage (calls community contract)
    fn get_manageable_communities_for_user(env: &Env, user: &Address) -> Vec<CommunityInfo> {
        let community_contract: Option<Address> = env.storage().instance().get(&MainKey::Community);

        if let Some(community) = community_contract {
            let args: Vec<Val> = Vec::from_array(env, [user.clone().into_val(env)]);
            env.try_invoke_contract::<Vec<CommunityInfo>, soroban_sdk::Error>(
                &community,
                &Symbol::new(env, "get_manageable_communities"),
                args,
            )
            .ok()
            .and_then(|r| r.ok())
            .unwrap_or(Vec::new(env))
        } else {
            Vec::new(env)
        }
    }

    // ========================================================================
    // Delegation to Other Contracts
    // ========================================================================

    /// Delegate rendering to the community contract with consistent nav/footer
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
        let content: Bytes = env.invoke_contract(&community, &Symbol::new(env, "render"), args);

        // Wrap with nav and footer for consistent UI
        Self::wrap_with_nav_footer(env, viewer, content)
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
            community_path.clone().into_val(env),
            viewer.into_val(env),
        ]);
        let content: Bytes = env.invoke_contract(&community, &Symbol::new(env, "render"), args);

        // Check if this is a board route (/c/{name}/b/*)
        // Board routes are wrapped by the board contract itself via include tags,
        // so don't double-wrap with nav/footer
        if Self::path_contains_board_segment(env, &community_path) {
            content
        } else {
            // Wrap community pages with nav and footer for consistent UI
            Self::wrap_with_nav_footer(env, viewer, content)
        }
    }

    /// Check if path contains a board segment (/b/) indicating a board route
    fn path_contains_board_segment(_env: &Env, path: &String) -> bool {
        let path_len = path.len() as usize;
        if path_len < 5 {
            // Path too short to contain "/c/x/b/"
            return false;
        }

        let mut buf = [0u8; 256];
        let copy_len = if path_len > 256 { 256 } else { path_len };
        path.copy_into_slice(&mut buf[0..copy_len]);

        // Search for "/b/" pattern after the community name
        for i in 0..copy_len.saturating_sub(2) {
            if buf[i] == b'/' && buf[i + 1] == b'b' && buf[i + 2] == b'/' {
                return true;
            }
        }

        false
    }

    /// Wrap content with navigation bar and footer for consistent UI
    fn wrap_with_nav_footer(env: &Env, viewer: &Option<Address>, content: Bytes) -> Bytes {
        let md = Self::render_nav(env, viewer)
            .newline()  // Blank line after nav-bar for markdown parsing
            .raw(content);
        Self::render_footer_into(env, md).build()
    }

    /// Delegate rendering to the pages contract
    fn delegate_to_pages(env: &Env, path: &Option<String>, viewer: &Option<Address>) -> Bytes {
        let pages_opt: Option<Address> = env.storage().instance().get(&MainKey::Pages);

        let Some(pages) = pages_opt else {
            // Pages contract not configured - show message
            return Self::wrap_with_nav_footer(
                env,
                viewer,
                MarkdownBuilder::new(env)
                    .h1("Pages Unavailable")
                    .paragraph("The pages feature is not configured.")
                    .render_link("Back to Home", "/")
                    .build(),
            );
        };

        // Strip "/p" prefix, pass rest to pages contract
        let pages_path = Self::strip_pages_prefix(env, path);

        let args: Vec<Val> = Vec::from_array(env, [
            pages_path.into_val(env),
            viewer.into_val(env),
        ]);
        let content: Bytes = env.invoke_contract(&pages, &Symbol::new(env, "render"), args);

        // Wrap with nav and footer for consistent UI
        Self::wrap_with_nav_footer(env, viewer, content)
    }

    /// Strip the `/p` prefix from a path to get relative path for pages contract
    fn strip_pages_prefix(env: &Env, path: &Option<String>) -> Option<String> {
        let Some(p) = path else {
            return Some(String::from_str(env, "/"));
        };

        let path_len = p.len() as usize;
        if path_len == 0 {
            return Some(String::from_str(env, "/"));
        }

        // Copy path to buffer for processing
        let mut path_buf = [0u8; 256];
        let copy_len = if path_len > 256 { 256 } else { path_len };
        p.copy_into_slice(&mut path_buf[0..copy_len]);

        // Check if path starts with "/p"
        if copy_len >= 2 && &path_buf[0..2] == b"/p" {
            if copy_len == 2 {
                // Exact match "/p", return "/"
                return Some(String::from_str(env, "/"));
            } else if path_buf[2] == b'/' {
                // Path has more after "/p/", return the rest (starting with /)
                let rest_slice = &path_buf[2..copy_len];
                return Some(String::from_bytes(env, rest_slice));
            }
        }

        // Fallback - return root
        Some(String::from_str(env, "/"))
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

    /// Delegate rendering to the board contract by numeric ID
    /// (Kept for backwards compatibility during migration)
    #[allow(dead_code)]
    fn delegate_to_board(env: &Env, board_id: u64, path: &Option<String>, viewer: &Option<Address>) -> Bytes {
        let registry: Address = env
            .storage()
            .instance()
            .get(&MainKey::Registry)
            .expect("Not initialized");

        // Get single board contract via registry alias
        let alias_args: Vec<Val> = Vec::from_array(env, [Symbol::new(env, "board").into_val(env)]);
        let board_contract_opt: Option<Address> = env.invoke_contract(
            &registry,
            &Symbol::new(env, "get_contract_by_alias"),
            alias_args,
        );

        let Some(board_contract) = board_contract_opt else {
            // Board contract not registered - return error page
            return MarkdownBuilder::new(env)
                .h1("Board Service Unavailable")
                .paragraph("The board contract is not configured.")
                .render_link("Back to Home", "/")
                .build();
        };

        // Convert path to relative path for board contract
        let relative_path = Self::strip_board_prefix(env, path, board_id);

        // Check if this board belongs to a community and get the community slug
        let community_slug = Self::get_board_community_slug(env, board_id);

        // Call render with board_id, path, viewer, and community_slug
        let args: Vec<Val> = Vec::from_array(env, [
            board_id.into_val(env),
            relative_path.into_val(env),
            viewer.into_val(env),
            community_slug.into_val(env),
        ]);
        env.invoke_contract(&board_contract, &Symbol::new(env, "render"), args)
    }

    /// Get the community slug for a board (if it belongs to a community)
    fn get_board_community_slug(env: &Env, board_id: u64) -> Option<String> {
        let registry: Address = env
            .storage()
            .instance()
            .get(&MainKey::Registry)
            .expect("Not initialized");

        // Get board contract
        let alias_args: Vec<Val> = Vec::from_array(env, [Symbol::new(env, "board").into_val(env)]);
        let board_contract_opt: Option<Address> = env.invoke_contract(
            &registry,
            &Symbol::new(env, "get_contract_by_alias"),
            alias_args,
        );

        if let Some(board_contract) = board_contract_opt {
            let args: Vec<Val> = Vec::from_array(env, [board_id.into_val(env)]);
            env.try_invoke_contract::<Option<String>, soroban_sdk::Error>(
                &board_contract,
                &Symbol::new(env, "get_board_community_slug"),
                args,
            ).ok().and_then(|r| r.ok()).flatten()
        } else {
            None
        }
    }

    /// Delegate rendering to the board contract using slug lookup.
    /// For community boards accessed via /b/{slug}, redirects to canonical /c/{community}/b/{slug} URL.
    /// For standalone boards, delegates directly to the board contract.
    fn delegate_to_board_by_slug(env: &Env, slug_bytes: &Bytes, path: &Option<String>, viewer: &Option<Address>) -> Bytes {
        let registry: Address = env
            .storage()
            .instance()
            .get(&MainKey::Registry)
            .expect("Not initialized");

        // Get board contract via registry alias
        let alias_args: Vec<Val> = Vec::from_array(env, [Symbol::new(env, "board").into_val(env)]);
        let board_contract_opt: Option<Address> = env.invoke_contract(
            &registry,
            &Symbol::new(env, "get_contract_by_alias"),
            alias_args,
        );

        let Some(board_contract) = board_contract_opt else {
            return MarkdownBuilder::new(env)
                .h1("Board Service Unavailable")
                .paragraph("The board contract is not configured.")
                .render_link("Back to Home", "/")
                .build();
        };

        // Convert slug bytes to String for lookup
        let slug = Self::bytes_to_string(env, slug_bytes);

        // Try to parse as numeric ID first, then fall back to slug lookup
        let board_id_opt: Option<u64> = Self::try_parse_u64(&slug)
            .or_else(|| {
                // Not a numeric ID, try slug lookup
                let lookup_args: Vec<Val> = Vec::from_array(env, [slug.clone().into_val(env)]);
                env.try_invoke_contract::<Option<u64>, soroban_sdk::Error>(
                    &board_contract,
                    &Symbol::new(env, "get_board_id_by_slug"),
                    lookup_args,
                )
                .ok()
                .and_then(|r| r.ok())
                .flatten()
            });

        let Some(board_id) = board_id_opt else {
            return MarkdownBuilder::new(env)
                .h1("Board Not Found")
                .paragraph("The requested board does not exist.")
                .render_link("Back to Home", "/")
                .build();
        };

        // If we parsed as numeric ID, get the actual board slug for redirects
        let actual_slug = if Self::try_parse_u64(&slug).is_some() {
            // Slug was a numeric ID - get the real slug from board contract
            let slug_args: Vec<Val> = Vec::from_array(env, [board_id.into_val(env)]);
            env.try_invoke_contract::<Option<String>, soroban_sdk::Error>(
                &board_contract,
                &Symbol::new(env, "get_board_slug"),
                slug_args,
            )
            .ok()
            .and_then(|r| r.ok())
            .flatten()
            .unwrap_or(slug.clone())
        } else {
            slug.clone()
        };

        // Check if board is in a community
        let community_contract_opt: Option<Address> = env.storage().instance().get(&MainKey::Community);

        if let Some(ref community_contract) = community_contract_opt {
            let community_args: Vec<Val> = Vec::from_array(env, [board_id.into_val(env)]);
            let community_id_opt: Option<u64> = env
                .try_invoke_contract::<Option<u64>, soroban_sdk::Error>(
                    community_contract,
                    &Symbol::new(env, "get_board_community"),
                    community_args,
                )
                .ok()
                .and_then(|r| r.ok())
                .flatten();

            if let Some(community_id) = community_id_opt {
                // Board is in a community - get community info for redirect
                let info_args: Vec<Val> = Vec::from_array(env, [community_id.into_val(env)]);
                let community_info: Option<CommunityMeta> = env
                    .try_invoke_contract::<Option<CommunityMeta>, soroban_sdk::Error>(
                        community_contract,
                        &Symbol::new(env, "get_community"),
                        info_args,
                    )
                    .ok()
                    .and_then(|r| r.ok())
                    .flatten();

                if let Some(community) = community_info {
                    // Board is in a community - render directly with community context
                    let relative_path = Self::strip_board_slug_prefix_as_option(env, path, &slug);
                    let community_slug: Option<String> = Some(community.name);

                    let args: Vec<Val> = Vec::from_array(env, [
                        board_id.into_val(env),
                        relative_path.into_val(env),
                        viewer.into_val(env),
                        community_slug.into_val(env),
                    ]);
                    return env.invoke_contract(&board_contract, &Symbol::new(env, "render"), args);
                }
            }
        }

        // Standalone board - delegate directly (no community slug)
        let relative_path = Self::strip_board_slug_prefix_as_option(env, path, &slug);
        let community_slug: Option<String> = None;

        let args: Vec<Val> = Vec::from_array(env, [
            board_id.into_val(env),
            relative_path.into_val(env),
            viewer.into_val(env),
            community_slug.into_val(env),
        ]);
        env.invoke_contract(&board_contract, &Symbol::new(env, "render"), args)
    }

    /// Strip the `/b/{slug}` prefix from a path to get remaining path as Bytes
    fn strip_board_slug_prefix(env: &Env, path: &Option<String>, slug: &String) -> Bytes {
        let Some(p) = path else {
            return Bytes::new(env);
        };

        let path_len = p.len() as usize;
        let slug_len = slug.len() as usize;

        // Expected prefix: "/b/{slug}"
        let prefix_len = 3 + slug_len; // "/b/" + slug

        // Copy path to buffer
        let mut path_buf = [0u8; 256];
        let copy_len = if path_len > 256 { 256 } else { path_len };
        p.copy_into_slice(&mut path_buf[0..copy_len]);

        // Copy slug to buffer for comparison
        let mut slug_buf = [0u8; 64];
        let slug_copy_len = if slug_len > 64 { 64 } else { slug_len };
        slug.copy_into_slice(&mut slug_buf[0..slug_copy_len]);

        // Check if path starts with "/b/{slug}"
        if copy_len >= prefix_len
            && &path_buf[0..3] == b"/b/"
            && &path_buf[3..prefix_len] == &slug_buf[0..slug_copy_len]
        {
            if copy_len == prefix_len {
                // Exact match, no remaining path
                return Bytes::new(env);
            } else if path_buf[prefix_len] == b'/' {
                // Return rest (starting from /)
                return Bytes::from_slice(env, &path_buf[prefix_len..copy_len]);
            }
        }

        Bytes::new(env)
    }

    /// Strip the `/b/{slug}` prefix from a path to get remaining path as Option<String>
    fn strip_board_slug_prefix_as_option(env: &Env, path: &Option<String>, slug: &String) -> Option<String> {
        let remaining = Self::strip_board_slug_prefix(env, path, slug);
        if remaining.len() == 0 {
            Some(String::from_str(env, "/"))
        } else {
            Some(Self::bytes_to_string(env, &remaining))
        }
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
    /// (Kept for backwards compatibility during migration)
    #[allow(dead_code)]
    fn strip_board_prefix(env: &Env, path: &Option<String>, board_id: u64) -> Option<String> {
        let Some(p) = path else {
            return Some(String::from_str(env, "/"));
        };

        let path_len = p.len() as usize;

        // Build the prefix: "/b/{board_id}"
        let mut prefix = [0u8; 32];
        let prefix_start = b"/b/";
        prefix[0..3].copy_from_slice(prefix_start);

        // Convert board_id to string using SDK function
        let id_bytes = u64_to_bytes(env, board_id);
        let id_len = id_bytes.len() as usize;
        id_bytes.copy_into_slice(&mut prefix[3..3 + id_len]);
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

    /// Format a Unix timestamp as a human-readable date string.
    fn format_timestamp(env: &Env, timestamp: u64) -> Bytes {
        // Handle legacy ledger sequence numbers (small values)
        if timestamp < 1_000_000_000 {
            let mut result = Bytes::from_slice(env, b"Ledger ");
            result.append(&u64_to_bytes(env, timestamp));
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

    // ========================================================================
    // Sorting Helpers
    // ========================================================================

    /// Compare two Soroban Strings alphabetically (case-insensitive).
    /// Returns: -1 if a < b, 0 if a == b, 1 if a > b
    fn compare_strings(a: &String, b: &String) -> i32 {
        let a_len = a.len() as usize;
        let b_len = b.len() as usize;

        // Copy each string into its own buffer (copy_into_slice requires exact length match)
        let mut a_buf = [0u8; 64];
        let mut b_buf = [0u8; 64];

        let a_copy_len = if a_len > 64 { 64 } else { a_len };
        let b_copy_len = if b_len > 64 { 64 } else { b_len };

        if a_copy_len > 0 {
            a.copy_into_slice(&mut a_buf[..a_copy_len]);
        }
        if b_copy_len > 0 {
            b.copy_into_slice(&mut b_buf[..b_copy_len]);
        }

        // Compare byte by byte up to the shorter length
        let min_len = if a_copy_len < b_copy_len { a_copy_len } else { b_copy_len };

        for i in 0..min_len {
            // Convert to lowercase for comparison
            let a_char = if a_buf[i] >= b'A' && a_buf[i] <= b'Z' {
                a_buf[i] + 32
            } else {
                a_buf[i]
            };
            let b_char = if b_buf[i] >= b'A' && b_buf[i] <= b'Z' {
                b_buf[i] + 32
            } else {
                b_buf[i]
            };

            if a_char < b_char {
                return -1;
            }
            if a_char > b_char {
                return 1;
            }
        }

        // If all compared bytes are equal, shorter string comes first
        if a_len < b_len {
            -1
        } else if a_len > b_len {
            1
        } else {
            0
        }
    }

    /// Sort communities alphabetically by name using selection sort.
    /// Uses Soroban Vec operations to avoid fixed-size arrays.
    fn sort_communities_by_name(env: &Env, communities: Vec<CommunityMeta>) -> Vec<CommunityMeta> {
        let len = communities.len();
        if len <= 1 {
            return communities;
        }

        // Build result by repeatedly finding the minimum
        let mut result: Vec<CommunityMeta> = Vec::new(env);
        let mut used: Vec<bool> = Vec::new(env);

        // Initialize used flags
        for _ in 0..len {
            used.push_back(false);
        }

        // Selection sort: find minimum each iteration
        for _ in 0..len {
            let mut min_idx: Option<u32> = None;

            for j in 0..len {
                if used.get(j).unwrap_or(true) {
                    continue;
                }

                match min_idx {
                    None => min_idx = Some(j),
                    Some(current_min) => {
                        let current = communities.get(current_min).unwrap();
                        let candidate = communities.get(j).unwrap();
                        if Self::compare_strings(&candidate.name, &current.name) < 0 {
                            min_idx = Some(j);
                        }
                    }
                }
            }

            if let Some(idx) = min_idx {
                result.push_back(communities.get(idx).unwrap());
                used.set(idx, true);
            }
        }

        result
    }

    /// Sort boards alphabetically by name using selection sort.
    /// Uses Soroban Vec operations to avoid fixed-size arrays.
    fn sort_boards_by_name(env: &Env, boards: Vec<BoardMeta>) -> Vec<BoardMeta> {
        let len = boards.len();
        if len <= 1 {
            return boards;
        }

        // Build result by repeatedly finding the minimum
        let mut result: Vec<BoardMeta> = Vec::new(env);
        let mut used: Vec<bool> = Vec::new(env);

        // Initialize used flags
        for _ in 0..len {
            used.push_back(false);
        }

        // Selection sort: find minimum each iteration
        for _ in 0..len {
            let mut min_idx: Option<u32> = None;

            for j in 0..len {
                if used.get(j).unwrap_or(true) {
                    continue;
                }

                match min_idx {
                    None => min_idx = Some(j),
                    Some(current_min) => {
                        let current = boards.get(current_min).unwrap();
                        let candidate = boards.get(j).unwrap();
                        if Self::compare_strings(&candidate.name, &current.name) < 0 {
                            min_idx = Some(j);
                        }
                    }
                }
            }

            if let Some(idx) = min_idx {
                result.push_back(boards.get(idx).unwrap());
                used.set(idx, true);
            }
        }

        result
    }

    /// Render a single community card (same format as communities page).
    fn render_community_card<'a>(_env: &'a Env, mut md: MarkdownBuilder<'a>, community: &CommunityMeta) -> MarkdownBuilder<'a> {
        // Build the URL for the link
        let mut url_buf = [0u8; 64];
        let prefix = b"render:/c/";
        url_buf[0..10].copy_from_slice(prefix);
        let name_len = community.name.len() as usize;
        let name_copy_len = if name_len > 50 { 50 } else { name_len };
        community.name.copy_into_slice(&mut url_buf[10..10 + name_copy_len]);
        let url = core::str::from_utf8(&url_buf[0..10 + name_copy_len]).unwrap_or("");

        // Wrap entire card in an <a> tag like board-card
        md = md.raw_str("<a href=\"");
        md = md.text(url);
        md = md.raw_str("\" class=\"community-card\">");

        // Display name as title span
        md = md.raw_str("<span class=\"community-card-title\">");
        md = md.text_string(&community.display_name);
        md = md.raw_str("</span>");

        // Description as desc span
        md = md.raw_str("<span class=\"community-card-desc\">");
        md = md.text_string(&community.description);
        md = md.raw_str("</span>");

        // Stats as meta span
        md = md.raw_str("<span class=\"community-card-meta\">");
        md = md.number(community.board_count as u32);
        md = md.raw_str(" boards · ");
        md = md.number(community.member_count as u32);
        md = md.raw_str(" members");
        if community.is_private {
            md = md.raw_str(" <span class=\"badge\">Private</span>");
        }
        md = md.raw_str("</span>");

        md = md.raw_str("</a>\n");
        md
    }

    // ========================================================================
    // Progressive Loading (delegates to config contract)
    // ========================================================================

    /// Get a chunk from a collection (delegates to config contract)
    /// Used by the viewer's progressive loader for footer, tagline, etc.
    pub fn get_chunk(env: Env, collection: Symbol, index: u32) -> Option<Bytes> {
        let config_opt: Option<Address> = env.storage().instance().get(&MainKey::Config);
        if let Some(config) = config_opt {
            let args: Vec<Val> = Vec::from_array(&env, [
                collection.into_val(&env),
                index.into_val(&env),
            ]);
            env.try_invoke_contract::<Option<Bytes>, soroban_sdk::Error>(
                &config,
                &Symbol::new(&env, "get_chunk"),
                args,
            )
            .ok()
            .and_then(|r| r.ok())
            .flatten()
        } else {
            None
        }
    }

    /// Get chunk metadata for a collection (delegates to config contract)
    pub fn get_chunk_meta(env: Env, collection: Symbol) -> Option<ChunkMeta> {
        let config_opt: Option<Address> = env.storage().instance().get(&MainKey::Config);
        if let Some(config) = config_opt {
            let args: Vec<Val> = Vec::from_array(&env, [collection.into_val(&env)]);
            env.try_invoke_contract::<ChunkMeta, soroban_sdk::Error>(
                &config,
                &Symbol::new(&env, "get_chunk_meta"),
                args,
            )
            .ok()
            .and_then(|r| r.ok())
        } else {
            None
        }
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

    /// Helper to setup a boards-main contract with all dependencies
    fn setup_main(env: &Env) -> (BoardsMainClient, Address, Address, Address, Address, Address, Address, Address) {
        env.mock_all_auths();

        let contract_id = env.register(BoardsMain, ());
        let client = BoardsMainClient::new(env, &contract_id);

        let registry = Address::generate(env);
        let theme = Address::generate(env);
        let permissions = Address::generate(env);
        let content = Address::generate(env);
        let admin = Address::generate(env);
        let community = Address::generate(env);
        let config = Address::generate(env);

        client.init(&registry, &theme, &permissions, &content, &admin, &community, &config);

        (client, registry, theme, permissions, content, admin, community, config)
    }

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
        let config = Address::generate(&env);

        client.init(&registry, &theme, &permissions, &content, &admin, &community, &config);

        assert_eq!(client.get_registry(), registry);
        assert_eq!(client.get_theme(), theme);
        assert_eq!(client.get_community(), Some(community));
        assert_eq!(client.get_config(), Some(config));
    }

    #[test]
    #[should_panic(expected = "Already initialized")]
    fn test_double_init() {
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
        let config = Address::generate(&env);

        client.init(&registry, &theme, &permissions, &content, &admin, &community, &config);
        // Second init should panic
        client.init(&registry, &theme, &permissions, &content, &admin, &community, &config);
    }

    #[test]
    fn test_get_registry() {
        let env = Env::default();
        let (client, registry, _, _, _, _, _, _) = setup_main(&env);
        assert_eq!(client.get_registry(), registry);
    }

    #[test]
    fn test_get_theme() {
        let env = Env::default();
        let (client, _, theme, _, _, _, _, _) = setup_main(&env);
        assert_eq!(client.get_theme(), theme);
    }

    #[test]
    fn test_get_community() {
        let env = Env::default();
        let (client, _, _, _, _, _, community, _) = setup_main(&env);
        assert_eq!(client.get_community(), Some(community));
    }

    #[test]
    fn test_get_config() {
        let env = Env::default();
        let (client, _, _, _, _, _, _, config) = setup_main(&env);
        assert_eq!(client.get_config(), Some(config));
    }

    #[test]
    fn test_get_pages_initially_none() {
        let env = Env::default();
        let (client, _, _, _, _, _, _, _) = setup_main(&env);
        // Pages is not set during init, should be None
        assert_eq!(client.get_pages(), None);
    }

    // Note: set_pages, set_config, set_community, styles, render_styles
    // require cross-contract calls to registry.is_admin() or theme.styles()
    // which need fully initialized dependency contracts to test.
    // These are integration-level tests that would require setting up
    // the entire contract ecosystem.
}
