#![no_std]

use soroban_render_sdk::prelude::*;
use soroban_sdk::{contract, contractimpl, contracttype, Address, Bytes, BytesN, Env, IntoVal, String, Symbol, Val, Vec};

// Declare render capabilities
soroban_render!(markdown, styles);

/// Storage keys for the theme contract
#[contracttype]
#[derive(Clone)]
pub enum ThemeKey {
    /// Registry contract address
    Registry,
    /// Permissions contract address
    Permissions,
    /// Content contract address
    Content,
    /// Admin contract address (for admin UI delegation)
    Admin,
}

#[contract]
pub struct BoardsTheme;

#[contractimpl]
impl BoardsTheme {
    /// Initialize the theme contract
    pub fn init(env: Env, registry: Address, permissions: Address, content: Address, admin: Address) {
        if env.storage().instance().has(&ThemeKey::Registry) {
            panic!("Already initialized");
        }
        env.storage().instance().set(&ThemeKey::Registry, &registry);
        env.storage().instance().set(&ThemeKey::Permissions, &permissions);
        env.storage().instance().set(&ThemeKey::Content, &content);
        env.storage().instance().set(&ThemeKey::Admin, &admin);
    }

    /// Get admin contract address
    pub fn get_admin(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&ThemeKey::Admin)
            .expect("Not initialized")
    }

    /// Get registry address
    pub fn get_registry(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&ThemeKey::Registry)
            .expect("Not initialized")
    }

    /// Get permissions address
    pub fn get_permissions(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&ThemeKey::Permissions)
            .expect("Not initialized")
    }

    /// Get content address
    pub fn get_content(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&ThemeKey::Content)
            .expect("Not initialized")
    }

    /// Main render entry point - delegates to domain-specific contracts
    ///
    /// Routing:
    /// - `/`, `/create`, `/help` → Registry
    /// - `/admin/*`, `/b/{id}/settings`, etc. → Admin
    /// - `/b/{id}/*` → Board contract (looked up via Registry)
    pub fn render(env: Env, path: Option<String>, viewer: Option<Address>) -> Bytes {
        let registry: Address = env
            .storage()
            .instance()
            .get(&ThemeKey::Registry)
            .expect("Not initialized");

        Router::new(&env, path.clone())
            // Registry routes: home, create, help
            .handle(b"/", |_| Self::delegate_to_registry(&env, &registry, &path, &viewer))
            .or_handle(b"/help", |_| Self::delegate_to_registry(&env, &registry, &path, &viewer))
            .or_handle(b"/create", |_| Self::delegate_to_registry(&env, &registry, &path, &viewer))
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
                Self::delegate_to_board(&env, &registry, board_id, &path, &viewer)
            })
            .or_handle(b"/b/{id}", |req| {
                let board_id = req.get_var_u32(b"id").unwrap_or(0) as u64;
                Self::delegate_to_board(&env, &registry, board_id, &path, &viewer)
            })
            // Default - delegate to registry (home page)
            .or_default(|_| Self::delegate_to_registry(&env, &registry, &None, &viewer))
    }

    /// Delegate rendering to the registry contract
    fn delegate_to_registry(env: &Env, registry: &Address, path: &Option<String>, viewer: &Option<Address>) -> Bytes {
        let args: Vec<Val> = Vec::from_array(env, [
            path.into_val(env),
            viewer.into_val(env),
        ]);
        env.invoke_contract(registry, &Symbol::new(env, "render"), args)
    }

    /// Delegate rendering to the admin contract
    fn delegate_to_admin(env: &Env, path: &Option<String>, viewer: &Option<Address>) -> Bytes {
        let admin: Address = env
            .storage()
            .instance()
            .get(&ThemeKey::Admin)
            .expect("Admin contract not initialized");

        let args: Vec<Val> = Vec::from_array(env, [
            path.into_val(env),
            viewer.into_val(env),
        ]);
        env.invoke_contract(&admin, &Symbol::new(env, "render"), args)
    }

    /// Delegate rendering to a board contract
    ///
    /// Looks up the board contract address from registry, then calls its render()
    /// with the relative path (stripping `/b/{id}` prefix).
    fn delegate_to_board(env: &Env, registry: &Address, board_id: u64, path: &Option<String>, viewer: &Option<Address>) -> Bytes {
        // Get board contract address from registry
        let board_args: Vec<Val> = Vec::from_array(env, [board_id.into_val(env)]);
        let board_contract_opt: Option<Address> = env.invoke_contract(
            registry,
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
        // e.g., "/b/0/t/1" -> "/t/1", "/b/0" -> "/"
        let relative_path = Self::strip_board_prefix(env, path, board_id);

        let args: Vec<Val> = Vec::from_array(env, [
            relative_path.into_val(env),
            viewer.into_val(env),
        ]);
        env.invoke_contract(&board_contract, &Symbol::new(env, "render"), args)
    }

    /// Strip the `/b/{id}` prefix from a path to get relative path for board contract
    fn strip_board_prefix(env: &Env, path: &Option<String>, board_id: u64) -> Option<String> {
        let Some(p) = path else {
            return Some(String::from_str(env, "/"));
        };

        // Build the prefix we need to strip: "/b/{board_id}"
        // For simplicity, we'll use a fixed approach
        let path_len = p.len() as usize;

        // Calculate prefix length: "/b/" + digits + optional rest
        // We need to find where the board_id ends
        let mut prefix = [0u8; 32];
        let prefix_start = b"/b/";
        prefix[0..3].copy_from_slice(prefix_start);

        // Convert board_id to string
        let mut id_bytes = [0u8; 20];
        let id_len = Self::u64_to_bytes(board_id, &mut id_bytes);
        prefix[3..3 + id_len].copy_from_slice(&id_bytes[0..id_len]);
        let prefix_len = 3 + id_len;

        // Copy path to buffer for comparison
        let mut path_buf = [0u8; 256];
        let copy_len = if path_len > 256 { 256 } else { path_len };
        p.copy_into_slice(&mut path_buf[0..copy_len]);

        // Check if path starts with prefix
        if copy_len >= prefix_len && &path_buf[0..prefix_len] == &prefix[0..prefix_len] {
            // Path matches prefix, extract the rest
            if copy_len == prefix_len {
                // Exact match like "/b/0", return "/"
                return Some(String::from_str(env, "/"));
            } else if path_buf[prefix_len] == b'/' {
                // Path has more after prefix, like "/b/0/t/1"
                // Return from the slash onwards
                let rest_len = copy_len - prefix_len;
                let rest_slice = &path_buf[prefix_len..copy_len];
                // Convert slice to String
                return Some(Self::bytes_to_string(env, rest_slice, rest_len));
            }
        }

        // Fallback - return root
        Some(String::from_str(env, "/"))
    }

    /// Convert u64 to byte slice, return number of bytes written
    fn u64_to_bytes(mut n: u64, buf: &mut [u8; 20]) -> usize {
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

    /// Convert byte slice to String
    fn bytes_to_string(env: &Env, bytes: &[u8], len: usize) -> String {
        // Create String from byte slice
        String::from_bytes(env, &bytes[0..len])
    }

    /// Public styles method for viewer/test access
    pub fn styles(env: Env) -> Bytes {
        Self::render_styles(env, None, None)
    }

    /// Base styles using Stellar Design System colors
    /// Named render_styles to follow the render_* convention for routable content
    /// Accepts path/viewer for consistency with render_* convention (unused here)
    pub fn render_styles(env: Env, _path: Option<String>, _viewer: Option<Address>) -> Bytes {
        StyleBuilder::new(&env)
            .root_vars_start()
            // Primary colors (Stellar lilac)
            .var("primary", "#7857e1")
            .var("primary-hover", "#6b4ad1")
            // Text colors
            .var("text", "#171717")
            .var("text-muted", "#6f6f6f")
            // Background colors
            .var("bg", "#ffffff")
            .var("bg-muted", "#f7f7f7")
            // Border
            .var("border", "#e2e2e2")
            // Status colors
            .var("success", "#30a46c")
            .var("warning", "#ffc53d")
            .var("danger", "#e5484d")
            // Spacing
            .var("space-xs", "0.25rem")
            .var("space-sm", "0.5rem")
            .var("space-md", "1rem")
            .var("space-lg", "1.5rem")
            .var("space-xl", "2rem")
            // Container max width
            .var("container-max", "52rem")
            .root_vars_end()
            // Base styles
            .rule("*", "box-sizing: border-box;")
            // Apply max-width to the soroban-render root container
            .rule(".soroban-render", "max-width: var(--container-max); margin: 0 auto; padding: var(--space-md); font-family: 'Inter', -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; color: var(--text); background: var(--bg); line-height: 1.6;")
            .rule("body", "font-family: 'Inter', -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; color: var(--text); background: var(--bg); line-height: 1.6; margin: 0; padding: 0;")
            // Typography
            .rule("h1", "font-size: 1.75rem; font-weight: 600; margin: 0 0 var(--space-sm) 0; word-wrap: break-word;")
            .rule("h2", "font-size: 1.375rem; font-weight: 600; margin: var(--space-lg) 0 var(--space-md) 0;")
            .rule("h3", "font-size: 1.125rem; font-weight: 600; margin: var(--space-md) 0 var(--space-sm) 0;")
            .rule("p", "margin: 0 0 var(--space-md) 0;")
            // Links - inline links styled subtly
            .rule("a", "color: var(--primary); text-decoration: none;")
            .rule("a:hover", "color: var(--primary-hover); text-decoration: underline;")
            // Action links styled as buttons
            .rule("a.action-btn", "display: inline-block; background: var(--primary); color: white; padding: var(--space-xs) var(--space-sm); border-radius: 4px; font-size: 0.875rem; text-decoration: none;")
            .rule("a.action-btn:hover", "background: var(--primary-hover); text-decoration: none;")
            .rule("a.action-btn-secondary", "background: var(--bg-muted); color: var(--text); border: 1px solid var(--border);")
            .rule("a.action-btn-secondary:hover", "background: var(--border);")
            // Code
            .rule("code", "font-family: 'Inconsolata', 'Monaco', monospace; background: var(--bg-muted); padding: 0.15rem 0.4rem; border-radius: 4px; word-break: break-all;")
            .rule("pre", "overflow-x: auto; padding: var(--space-md); background: var(--bg-muted); border-radius: 4px;")
            // Navigation bar
            .rule(".nav-bar", "display: flex; flex-wrap: wrap; gap: var(--space-sm); align-items: center; padding: var(--space-sm) 0; margin-bottom: var(--space-md);")
            .rule(".nav-bar a", "padding: var(--space-xs) var(--space-sm); background: var(--bg-muted); border-radius: 4px; font-size: 0.875rem;")
            .rule(".nav-bar a:hover", "background: var(--border); text-decoration: none;")
            // Page header
            .rule(".page-header", "margin-bottom: var(--space-lg);")
            .rule(".page-header h1", "margin-bottom: var(--space-xs);")
            .rule(".page-header p", "color: var(--text-muted); margin: 0;")
            // Board cards - use a.board-card for higher specificity over viewer's a.soroban-action
            .rule(".board-list", "display: flex; flex-direction: column; gap: var(--space-sm);")
            .rule("a.board-card", "display: flex !important; flex-direction: column; align-items: flex-start !important; background: var(--bg) !important; color: var(--text) !important; border: 1px solid var(--border); border-radius: 6px; padding: var(--space-md) !important; transition: border-color 0.15s, box-shadow 0.15s; text-decoration: none !important;")
            .rule("a.board-card:hover", "border-color: var(--primary); box-shadow: 0 2px 8px rgba(120, 87, 225, 0.1); text-decoration: none !important; background: var(--bg) !important;")
            .rule(".board-card-title", "display: block; font-weight: 600; color: var(--text); margin-bottom: var(--space-xs); text-align: left;")
            .rule(".board-card-desc", "display: block; color: var(--text-muted); font-size: 0.9375rem; margin-bottom: var(--space-xs); text-align: left;")
            .rule(".board-card-meta", "display: block; font-size: 0.8125rem; color: var(--text-muted); text-align: left;")
            .rule(".board-card-meta .badge", "margin-left: var(--space-xs);")
            // Thread list - card layout similar to boards
            .rule(".thread-list", "display: flex; flex-direction: column; gap: var(--space-sm);")
            .rule("a.thread-card", "display: flex !important; flex-direction: column; align-items: flex-start !important; background: var(--bg) !important; color: var(--text) !important; border: 1px solid var(--border); border-radius: 6px; padding: var(--space-md) !important; transition: border-color 0.15s, box-shadow 0.15s; text-decoration: none !important;")
            .rule("a.thread-card:hover", "border-color: var(--primary); box-shadow: 0 2px 8px rgba(120, 87, 225, 0.1); text-decoration: none !important; background: var(--bg) !important;")
            .rule(".thread-card-title", "display: block; font-weight: 600; color: var(--text); margin-bottom: var(--space-xs); text-align: left;")
            .rule(".thread-card-meta", "display: block; font-size: 0.8125rem; color: var(--text-muted); text-align: left;")
            // Thread content
            .rule(".thread-body", "margin-bottom: var(--space-lg); padding: var(--space-md); background: var(--bg-muted); border-radius: 6px;")
            .rule(".thread-actions", "display: flex; gap: var(--space-sm); margin-bottom: var(--space-lg);")
            // Reply containers
            .rule(".reply", "margin-bottom: var(--space-sm); padding: var(--space-sm) var(--space-md); border-left: 3px solid var(--primary); background: var(--bg-muted); border-radius: 0 4px 4px 0;")
            .rule(".reply .reply", "margin-left: var(--space-lg);")
            .rule(".reply-content", "margin-bottom: var(--space-xs);")
            .rule(".reply-meta", "font-size: 0.8125rem; color: var(--text-muted); display: flex; flex-wrap: wrap; gap: var(--space-sm); align-items: center;")
            .rule(".reply-meta a", "padding: var(--space-xs) var(--space-sm); background: var(--primary); color: white; border-radius: 4px; font-size: 0.75rem;")
            .rule(".reply-meta a:hover", "background: var(--primary-hover); text-decoration: none;")
            .rule(".reply-hidden, .reply-deleted", "font-style: italic; color: var(--text-muted);")
            .rule(".reply-more", "margin-top: var(--space-sm); padding: var(--space-xs) 0; font-size: 0.875rem;")
            .rule(".reply-more a", "color: var(--primary); text-decoration: none;")
            .rule(".reply-more a:hover", "text-decoration: underline;")
            // Blockquotes (alerts)
            .rule("blockquote", "margin: 0 0 var(--space-md) 0; padding: var(--space-sm) var(--space-md); border-left: 3px solid var(--border); background: var(--bg-muted); border-radius: 0 4px 4px 0;")
            // Forms
            .rule(".form-group", "margin-bottom: var(--space-md);")
            .rule("input, textarea", "width: 100%; padding: var(--space-sm); border: 1px solid var(--border); border-radius: 4px; font-size: 1rem; background: var(--bg);")
            .rule("input:focus, textarea:focus", "outline: none; border-color: var(--primary);")
            .rule("textarea", "resize: vertical; min-height: 100px;")
            // Buttons
            .rule("button, .btn", "display: inline-block; background: var(--primary); color: white; padding: var(--space-sm) var(--space-md); border: none; border-radius: 4px; cursor: pointer; font-size: 0.9375rem;")
            .rule("button:hover, .btn:hover", "background: var(--primary-hover);")
            .rule(".btn-secondary", "background: var(--bg-muted); color: var(--text); border: 1px solid var(--border);")
            .rule(".btn-secondary:hover", "background: var(--border);")
            // Alerts/Notices - make them more subtle
            .rule(".alert", "padding: var(--space-sm) var(--space-md); border-radius: 4px; margin-bottom: var(--space-md); font-size: 0.9375rem;")
            .rule(".alert-success", "background: #d3f9d8; color: #1e7a34;")
            .rule(".alert-warning", "background: #fff3bf; color: #946c00;")
            .rule(".alert-danger", "background: #ffd8d8; color: #c41d1d;")
            .rule(".alert-info", "background: #e8e4fd; color: #5c4bad;")
            // Horizontal rule - use sparingly
            .rule("hr", "border: none; border-top: 1px solid var(--border); margin: var(--space-md) 0;")
            // Lists
            .rule("ul, ol", "padding-left: var(--space-lg); margin: 0 0 var(--space-md) 0;")
            .rule("li", "margin-bottom: var(--space-xs);")
            // Action links row
            .rule(".actions", "display: flex; flex-wrap: wrap; gap: var(--space-sm); font-size: 0.875rem;")
            // Badge/Tag
            .rule(".badge", "display: inline-block; padding: 0.125rem 0.5rem; background: var(--bg-muted); border-radius: 9999px; font-size: 0.75rem;")
            .rule(".badge-pinned", "background: #ffeeba; color: #856404;")
            .rule(".badge-locked", "background: #f8d7da; color: #721c24;")
            .rule(".badge-private", "background: #e7d4ff; color: #5a3d7a;")
            // Section spacing
            .rule(".section", "margin-bottom: var(--space-lg);")
            // Footer
            .rule(".footer", "margin-top: var(--space-xl); padding-top: var(--space-md); border-top: 1px solid var(--border); color: var(--text-muted); font-size: 0.875rem;")
            // Dark mode
            .dark_mode_start()
            .rule_start(":root")
            .prop("--text", "#e0e0e0")
            .prop("--text-muted", "#a0a0a0")
            .prop("--bg", "#0f0f0f")
            .prop("--bg-muted", "#1a1a1a")
            .prop("--border", "#3e3e3e")
            .rule_end()
            .rule(".board-card:hover", "box-shadow: 0 2px 8px rgba(120, 87, 225, 0.2);")
            .rule(".thread-card:hover", "box-shadow: 0 2px 8px rgba(120, 87, 225, 0.2);")
            .rule(".alert-success", "background: #1e3a28; color: #6fdd8b;")
            .rule(".alert-warning", "background: #3a3019; color: #ffd859;")
            .rule(".alert-danger", "background: #3a1c1c; color: #ff8080;")
            .rule(".alert-info", "background: #2a2644; color: #b8a8e8;")
            .rule(".badge-pinned", "background: #3a3019; color: #ffd859;")
            .rule(".badge-locked", "background: #3a1c1c; color: #ff8080;")
            .rule(".badge-private", "background: #3a2d4a; color: #c9a5ff;")
            .media_end()
            // Mobile responsive styles
            .media_start("(max-width: 640px)")
            .rule_start(":root")
            .prop("--space-lg", "1rem")
            .prop("--space-xl", "1.5rem")
            .rule_end()
            .rule(".soroban-render", "padding: var(--space-sm);")
            .rule("h1", "font-size: 1.375rem;")
            .rule("h2", "font-size: 1.125rem;")
            .rule("h3", "font-size: 1rem;")
            .rule(".reply .reply", "margin-left: var(--space-md);")
            .rule(".reply-meta a", "padding: 0.125rem var(--space-xs); font-size: 0.6875rem;")
            .rule("blockquote", "padding: var(--space-xs) var(--space-sm);")
            .rule(".nav-bar", "font-size: 0.8125rem;")
            .rule(".nav-bar a", "padding: var(--space-xs);")
            .rule(".board-card", "padding: var(--space-sm);")
            .rule(".thread-card", "padding: var(--space-sm);")
            .rule(".thread-body", "padding: var(--space-sm);")
            .rule("button, .btn", "padding: var(--space-xs) var(--space-sm); font-size: 0.875rem;")
            .media_end()
            // Very small screens
            .media_start("(max-width: 375px)")
            .rule(".soroban-render", "font-size: 0.9375rem;")
            .rule("h1", "font-size: 1.25rem;")
            .rule(".reply .reply", "margin-left: var(--space-sm);")
            .rule(".reply-meta", "gap: var(--space-xs);")
            .rule(".reply-meta a", "padding: 0.125rem 0.25rem;")
            .media_end()
            .build()
    }

    /// Render header component
    pub fn render_header(env: Env, _path: Option<String>, _viewer: Option<Address>) -> Bytes {
        MarkdownBuilder::new(&env)
            .div_start("nav-bar")
            .render_link("Soroban Boards", "/")
            .render_link("Help", "/help")
            .div_end()
            .build()
    }

    /// Render footer component
    pub fn render_footer(env: Env, _path: Option<String>, _viewer: Option<Address>) -> Bytes {
        MarkdownBuilder::new(&env)
            .hr()
            .paragraph("*Powered by [Soroban Render](https://github.com/wyhaines/soroban-render) on [Stellar](https://stellar.org)*")
            .build()
    }

    /// Upgrade the contract WASM
    pub fn upgrade(env: Env, new_wasm_hash: BytesN<32>) {
        let registry: Address = env
            .storage()
            .instance()
            .get(&ThemeKey::Registry)
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
    fn test_styles() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsTheme, ());
        let client = BoardsThemeClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        let permissions = Address::generate(&env);
        let content = Address::generate(&env);
        let admin = Address::generate(&env);
        client.init(&registry, &permissions, &content, &admin);

        let css = client.styles();
        assert!(css.len() > 0);
    }
}
