#![no_std]

use soroban_render_sdk::prelude::*;
use soroban_sdk::{contract, contractimpl, contracttype, Address, BytesN, Env, String};

// Declare render capabilities
soroban_render!(markdown, styles);

/// Storage keys for the theme contract
#[contracttype]
#[derive(Clone)]
pub enum ThemeKey {
    /// Registry contract address
    Registry,
}

#[contract]
pub struct BoardsTheme;

#[contractimpl]
impl BoardsTheme {
    /// Initialize the theme contract
    pub fn init(env: Env, registry: Address) {
        if env.storage().instance().has(&ThemeKey::Registry) {
            panic!("Already initialized");
        }
        env.storage().instance().set(&ThemeKey::Registry, &registry);
    }

    /// Get registry address
    pub fn get_registry(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&ThemeKey::Registry)
            .expect("Not initialized")
    }

    /// Main render entry point
    pub fn render(env: Env, path: Option<String>, viewer: Option<Address>) -> Bytes {
        Router::new(&env, path)
            // Home page - board list
            .handle(b"/", |_| Self::render_home(&env, &viewer))
            // Help page
            .or_handle(b"/help", |_| Self::render_help(&env))
            // Board view (placeholder)
            .or_handle(b"/b/{id}", |req| {
                let _id = req.get_var_u32(b"id").unwrap_or(0);
                Self::render_board_placeholder(&env)
            })
            // Default
            .or_default(|_| Self::render_home(&env, &viewer))
    }

    /// Render the home page with board list
    fn render_home(env: &Env, viewer: &Option<Address>) -> Bytes {
        let mut md = MarkdownBuilder::new(env)
            .render_link("Soroban Boards", "/")
            .text(" | ")
            .render_link("Help", "/help")
            .newline()
            .hr()
            .h1("Soroban Boards")
            .paragraph("*Decentralized discussion forums on Stellar*")
            .hr();

        // Show connection status
        if viewer.is_some() {
            md = md.tip("Wallet connected! You can create boards and post.");
        } else {
            md = md.note("Connect your wallet to participate in discussions.");
        }

        md.newline()
            .h2("Boards")
            .paragraph("No boards yet. Create the first one!")
            .newline()
            .render_link("Create Board", "/create")
            .newline()
            .hr()
            .paragraph("*Powered by Soroban Render on Stellar*")
            .build()
    }

    /// Render help page
    fn render_help(env: &Env) -> Bytes {
        MarkdownBuilder::new(env)
            .render_link("Soroban Boards", "/")
            .text(" | ")
            .render_link("Help", "/help")
            .newline()
            .hr()
            .h1("Help")
            .h2("What is Soroban Boards?")
            .paragraph("Soroban Boards is a decentralized forum system running on Stellar's Soroban smart contract platform. All content is stored on-chain, and the UI is rendered directly from the smart contracts.")
            .h2("Features")
            .list_item("Create discussion boards")
            .list_item("Post threads and replies")
            .list_item("Nested comment threads")
            .list_item("Role-based permissions (Owner, Admin, Moderator, Member)")
            .list_item("Content moderation (flagging, banning)")
            .list_item("Progressive loading for large threads")
            .h2("How to Use")
            .list_item("Connect your Stellar wallet")
            .list_item("Browse existing boards or create a new one")
            .list_item("Create threads and reply to discussions")
            .list_item("Flag inappropriate content")
            .hr()
            .paragraph("*Powered by Soroban Render on Stellar*")
            .build()
    }

    /// Placeholder for board view
    fn render_board_placeholder(env: &Env) -> Bytes {
        MarkdownBuilder::new(env)
            .render_link("Soroban Boards", "/")
            .text(" | ")
            .render_link("Help", "/help")
            .newline()
            .hr()
            .h1("Board")
            .paragraph("Board view coming soon...")
            .render_link("Back to Home", "/")
            .hr()
            .paragraph("*Powered by Soroban Render on Stellar*")
            .build()
    }

    /// Render header component
    pub fn render_header(env: Env, _path: Option<String>, _viewer: Option<Address>) -> Bytes {
        MarkdownBuilder::new(&env)
            .render_link("Soroban Boards", "/")
            .text(" | ")
            .render_link("Help", "/help")
            .newline()
            .hr()
            .build()
    }

    /// Render footer component
    pub fn render_footer(env: Env, _path: Option<String>, _viewer: Option<Address>) -> Bytes {
        MarkdownBuilder::new(&env)
            .hr()
            .paragraph("*Powered by [Soroban Render](https://github.com/wyhaines/soroban-render) on [Stellar](https://stellar.org)*")
            .build()
    }

    /// Base styles using Stellar Design System colors
    pub fn styles(env: Env) -> Bytes {
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
            .var("bg-muted", "#f3f3f3")
            // Border
            .var("border", "#e2e2e2")
            // Status colors
            .var("success", "#30a46c")
            .var("warning", "#ffc53d")
            .var("danger", "#e5484d")
            .root_vars_end()
            // Typography
            .rule("body", "font-family: 'Inter', -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; color: var(--text); background: var(--bg); line-height: 1.6;")
            .rule("h1", "font-size: 1.875rem; font-weight: 600; margin: 0 0 1rem 0;")
            .rule("h2", "font-size: 1.5rem; font-weight: 600; margin: 2rem 0 1rem 0;")
            .rule("h3", "font-size: 1.25rem; font-weight: 600; margin: 1.5rem 0 0.75rem 0;")
            // Links
            .rule("a", "color: var(--primary); text-decoration: none;")
            .rule("a:hover", "color: var(--primary-hover); text-decoration: underline;")
            // Code
            .rule("code", "font-family: 'Inconsolata', 'Monaco', monospace; background: var(--bg-muted); padding: 0.15rem 0.4rem; border-radius: 4px;")
            // Thread items
            .rule(".thread-item", "padding: 1rem; border-bottom: 1px solid var(--border);")
            .rule(".thread-title", "font-weight: 600; color: var(--text);")
            .rule(".thread-meta", "color: var(--text-muted); font-size: 0.875rem;")
            // Replies
            .rule(".reply", "margin-left: 1.5rem; padding: 0.75rem; border-left: 2px solid var(--border);")
            .rule(".reply-nested", "margin-left: 1.5rem;")
            // Forms
            .rule("input, textarea", "width: 100%; padding: 0.5rem; border: 1px solid var(--border); border-radius: 4px; font-size: 1rem;")
            .rule("input:focus, textarea:focus", "outline: none; border-color: var(--primary);")
            // Buttons
            .rule("button, .btn", "background: var(--primary); color: white; padding: 0.5rem 1rem; border: none; border-radius: 4px; cursor: pointer;")
            .rule("button:hover, .btn:hover", "background: var(--primary-hover);")
            // Dark mode
            .dark_mode_start()
            .rule_start(":root")
            .prop("--text", "#e0e0e0")
            .prop("--text-muted", "#a0a0a0")
            .prop("--bg", "#0f0f0f")
            .prop("--bg-muted", "#1c1c1c")
            .prop("--border", "#3e3e3e")
            .rule_end()
            .media_end()
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
    fn test_render_home() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsTheme, ());
        let client = BoardsThemeClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        client.init(&registry);

        // Render without viewer
        let html = client.render(&None, &None);
        assert!(html.len() > 0);

        // Render with viewer
        let viewer = Address::generate(&env);
        let html = client.render(&None, &Some(viewer));
        assert!(html.len() > 0);
    }

    #[test]
    fn test_styles() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsTheme, ());
        let client = BoardsThemeClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        client.init(&registry);

        let css = client.styles();
        assert!(css.len() > 0);
    }
}
