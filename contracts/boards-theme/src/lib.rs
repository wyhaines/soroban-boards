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
}

// ============================================================================
// External Types (must match registry/board/content contracts)
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

/// Thread metadata from board contract
#[contracttype]
#[derive(Clone)]
pub struct ThreadMeta {
    pub id: u64,
    pub board_id: u64,
    pub title: String,
    pub creator: Address,
    pub created_at: u64,
    pub updated_at: u64,
    pub reply_count: u32,
    pub is_locked: bool,
    pub is_pinned: bool,
    pub is_hidden: bool,
}

/// Reply metadata from content contract
#[contracttype]
#[derive(Clone)]
pub struct ReplyMeta {
    pub id: u64,
    pub board_id: u64,
    pub thread_id: u64,
    pub parent_id: u64,
    pub depth: u32,
    pub creator: Address,
    pub created_at: u64,
    pub updated_at: u64,
    pub is_hidden: bool,
    pub is_deleted: bool,
    pub flag_count: u32,
}

#[contract]
pub struct BoardsTheme;

#[contractimpl]
impl BoardsTheme {
    /// Initialize the theme contract
    pub fn init(env: Env, registry: Address, permissions: Address, content: Address) {
        if env.storage().instance().has(&ThemeKey::Registry) {
            panic!("Already initialized");
        }
        env.storage().instance().set(&ThemeKey::Registry, &registry);
        env.storage().instance().set(&ThemeKey::Permissions, &permissions);
        env.storage().instance().set(&ThemeKey::Content, &content);
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

    /// Main render entry point
    pub fn render(env: Env, path: Option<String>, viewer: Option<Address>) -> Bytes {
        Router::new(&env, path)
            // Home page - board list
            .handle(b"/", |_| Self::render_home(&env, &viewer))
            // Help page
            .or_handle(b"/help", |_| Self::render_help(&env))
            // Create board form
            .or_handle(b"/create", |_| Self::render_create_board(&env, &viewer))
            // Thread reply form (must be before thread view to match first)
            .or_handle(b"/b/{id}/t/{tid}/reply", |req| {
                let board_id = req.get_var_u32(b"id").unwrap_or(0) as u64;
                let thread_id = req.get_var_u32(b"tid").unwrap_or(0) as u64;
                Self::render_reply_form(&env, board_id, thread_id, 0, &viewer)
            })
            // Nested reply form
            .or_handle(b"/b/{id}/t/{tid}/r/{rid}/reply", |req| {
                let board_id = req.get_var_u32(b"id").unwrap_or(0) as u64;
                let thread_id = req.get_var_u32(b"tid").unwrap_or(0) as u64;
                let reply_id = req.get_var_u32(b"rid").unwrap_or(0) as u64;
                Self::render_reply_form(&env, board_id, thread_id, reply_id, &viewer)
            })
            // Single reply view
            .or_handle(b"/b/{id}/t/{tid}/r/{rid}", |req| {
                let board_id = req.get_var_u32(b"id").unwrap_or(0) as u64;
                let thread_id = req.get_var_u32(b"tid").unwrap_or(0) as u64;
                let reply_id = req.get_var_u32(b"rid").unwrap_or(0) as u64;
                Self::render_reply(&env, board_id, thread_id, reply_id, &viewer)
            })
            // Thread view with replies
            .or_handle(b"/b/{id}/t/{tid}", |req| {
                let board_id = req.get_var_u32(b"id").unwrap_or(0) as u64;
                let thread_id = req.get_var_u32(b"tid").unwrap_or(0) as u64;
                Self::render_thread(&env, board_id, thread_id, &viewer)
            })
            // Create thread form
            .or_handle(b"/b/{id}/new", |req| {
                let board_id = req.get_var_u32(b"id").unwrap_or(0) as u64;
                Self::render_create_thread(&env, board_id, &viewer)
            })
            // Board view with thread list
            .or_handle(b"/b/{id}", |req| {
                let board_id = req.get_var_u32(b"id").unwrap_or(0) as u64;
                Self::render_board(&env, board_id, &viewer)
            })
            // Default
            .or_default(|_| Self::render_home(&env, &viewer))
    }

    /// Render the home page with board list
    fn render_home(env: &Env, viewer: &Option<Address>) -> Bytes {
        let mut md = Self::render_nav(env)
            .h1("Soroban Boards")
            .paragraph("*Decentralized discussion forums on Stellar*")
            .hr();

        // Show connection status
        if viewer.is_some() {
            md = md.tip("Wallet connected! You can create boards and post.");
        } else {
            md = md.note("Connect your wallet to participate in discussions.");
        }

        md = md.newline().h2("Boards");

        // Fetch boards from registry
        if let Some(registry) = env.storage().instance().get::<_, Address>(&ThemeKey::Registry) {
            // Call registry.board_count()
            let count: u64 = env.invoke_contract(
                &registry,
                &Symbol::new(env, "board_count"),
                Vec::new(env),
            );

            if count == 0 {
                md = md.paragraph("No boards yet. Be the first to create one!");
            } else {
                // Call registry.list_boards(0, 20)
                let args: Vec<Val> = Vec::from_array(env, [0u64.into_val(env), 20u64.into_val(env)]);
                let boards: Vec<BoardMeta> = env.invoke_contract(
                    &registry,
                    &Symbol::new(env, "list_boards"),
                    args,
                );

                for i in 0..boards.len() {
                    if let Some(board) = boards.get(i) {
                        // Format: **[Board Name](/b/0)** - Description (X threads)
                        md = md.bold("").raw_str("[");
                        md = md.text_string(&board.name);
                        md = md.raw_str("](render:/b/").number(board.id as u32).raw_str(")");
                        md = md.text(" - ").text_string(&board.description);
                        md = md.text(" (").number(board.thread_count as u32).text(" threads)");
                        if board.is_private {
                            md = md.text(" ").italic("[private]");
                        }
                        md = md.newline().newline();
                    }
                }
            }
        } else {
            md = md.warning("Registry not configured");
        }

        md = md.newline()
            .render_link("+ Create New Board", "/create")
            .newline()
            .newline();

        Self::render_footer_into(md).build()
    }

    /// Render the navigation bar
    fn render_nav(env: &Env) -> MarkdownBuilder<'_> {
        MarkdownBuilder::new(env)
            .render_link("Soroban Boards", "/")
            .text(" | ")
            .render_link("Help", "/help")
            .newline()
            .hr()
    }

    /// Append footer to builder
    fn render_footer_into(md: MarkdownBuilder<'_>) -> MarkdownBuilder<'_> {
        md.hr()
            .paragraph("*Powered by [Soroban Render](https://github.com/wyhaines/soroban-render) on [Stellar](https://stellar.org)*")
    }

    /// Render help page
    fn render_help(env: &Env) -> Bytes {
        let md = Self::render_nav(env)
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
            .list_item("Flag inappropriate content");
        Self::render_footer_into(md).build()
    }

    /// Render create board form
    fn render_create_board(env: &Env, viewer: &Option<Address>) -> Bytes {
        let mut md = Self::render_nav(env)
            .h1("Create New Board");

        if viewer.is_none() {
            md = md.warning("Please connect your wallet to create a board.");
            return Self::render_footer_into(md).build();
        }

        md = md
            .paragraph("Create a new discussion board.")
            .newline()
            .input("name", "Board name")
            .newline()
            .textarea("description", 3, "Board description")
            .newline()
            .newline()
            .form_link("Create Board", "create_board")
            .newline()
            .newline()
            .render_link("Cancel", "/");

        Self::render_footer_into(md).build()
    }

    /// Render a single board with thread list
    fn render_board(env: &Env, board_id: u64, viewer: &Option<Address>) -> Bytes {
        let registry: Address = env
            .storage()
            .instance()
            .get(&ThemeKey::Registry)
            .expect("Not initialized");

        // Fetch board metadata
        let args: Vec<Val> = Vec::from_array(env, [board_id.into_val(env)]);
        let board_opt: Option<BoardMeta> = env.invoke_contract(
            &registry,
            &Symbol::new(env, "get_board"),
            args,
        );

        let Some(board) = board_opt else {
            return Self::render_nav(env)
                .h1("Board Not Found")
                .paragraph("The requested board does not exist.")
                .render_link("Back to Home", "/")
                .build();
        };

        let mut md = Self::render_nav(env)
            .render_link("< Back", "/")
            .newline()
            .h1("").text_string(&board.name)
            .paragraph("").text_string(&board.description)
            .hr();

        // Show create thread button if logged in
        if viewer.is_some() && !board.is_readonly {
            md = md.render_link("+ New Thread", "")
                .raw_str("/b/").number(board_id as u32).raw_str("/new)")
                .newline()
                .newline();
        }

        if board.is_readonly {
            md = md.note("This board is read-only.");
        }

        md = md.h2("Threads");

        // For now, show placeholder since we'd need the board contract address
        // In production, registry would return the board contract address
        if board.thread_count == 0 {
            md = md.paragraph("No threads yet. Be the first to post!");
        } else {
            md = md.paragraph("*Thread list will be loaded from board contract*");
            // In a real implementation, we'd fetch from the board contract:
            // let threads: Vec<ThreadMeta> = env.invoke_contract(&board_contract, ...);
        }

        Self::render_footer_into(md).build()
    }

    /// Render create thread form
    fn render_create_thread(env: &Env, board_id: u64, viewer: &Option<Address>) -> Bytes {
        let mut md = Self::render_nav(env)
            .render_link("< Back to Board", "")
            .raw_str("/b/").number(board_id as u32).raw_str(")")
            .newline()
            .h1("New Thread");

        if viewer.is_none() {
            md = md.warning("Please connect your wallet to create a thread.");
            return Self::render_footer_into(md).build();
        }

        md = md
            .input("title", "Thread title")
            .newline()
            .textarea("body", 10, "Write your post content here...")
            .newline()
            .newline()
            .form_link("Create Thread", "create_thread")
            .newline()
            .newline()
            .render_link("Cancel", "")
            .raw_str("/b/").number(board_id as u32).raw_str(")");

        Self::render_footer_into(md).build()
    }

    /// Render a thread with replies
    fn render_thread(env: &Env, board_id: u64, thread_id: u64, viewer: &Option<Address>) -> Bytes {
        let content: Address = env
            .storage()
            .instance()
            .get(&ThemeKey::Content)
            .expect("Not initialized");

        let mut md = Self::render_nav(env)
            .render_link("< Back to Board", "")
            .raw_str("/b/").number(board_id as u32).raw_str(")")
            .newline()
            .h1("Thread")  // In production, we'd fetch title from board contract
            .hr();

        // Get thread body from content contract
        let args: Vec<Val> = Vec::from_array(env, [board_id.into_val(env), thread_id.into_val(env)]);
        let body: Bytes = env.invoke_contract(
            &content,
            &Symbol::new(env, "get_thread_body"),
            args.clone(),
        );

        if body.len() > 0 {
            md = md.raw(body).newline().newline();
        } else {
            md = md.paragraph("*No content*");
        }

        md = md.hr();

        // Reply button
        if viewer.is_some() {
            md = md.render_link("Reply", "")
                .raw_str("/b/").number(board_id as u32)
                .raw_str("/t/").number(thread_id as u32)
                .raw_str("/reply)")
                .newline()
                .newline();
        }

        md = md.h2("Replies");

        // Fetch reply count
        let reply_count: u64 = env.invoke_contract(
            &content,
            &Symbol::new(env, "get_reply_count"),
            args.clone(),
        );

        if reply_count == 0 {
            md = md.paragraph("No replies yet. Be the first to respond!");
        } else {
            // Fetch top-level replies
            let list_args: Vec<Val> = Vec::from_array(env, [
                board_id.into_val(env),
                thread_id.into_val(env),
                0u32.into_val(env),
                20u32.into_val(env),
            ]);
            let replies: Vec<ReplyMeta> = env.invoke_contract(
                &content,
                &Symbol::new(env, "list_top_level_replies"),
                list_args,
            );

            for i in 0..replies.len() {
                if let Some(reply) = replies.get(i) {
                    md = Self::render_reply_item(env, md, &content, &reply, board_id, thread_id, viewer);
                }
            }

            // Progressive loading marker if more replies exist
            if reply_count > 20 {
                md = md.continuation("replies", 20, Some(reply_count as u32));
            }
        }

        Self::render_footer_into(md).build()
    }

    /// Render a single reply item with nested children
    fn render_reply_item<'a>(
        env: &Env,
        mut md: MarkdownBuilder<'a>,
        content: &Address,
        reply: &ReplyMeta,
        board_id: u64,
        thread_id: u64,
        viewer: &Option<Address>,
    ) -> MarkdownBuilder<'a> {
        // Show reply metadata
        if reply.is_hidden {
            md = md.blockquote("*[This reply has been hidden by a moderator]*");
        } else if reply.is_deleted {
            md = md.blockquote("*[This reply has been deleted]*");
        } else {
            // Get reply content
            let args: Vec<Val> = Vec::from_array(env, [
                board_id.into_val(env),
                thread_id.into_val(env),
                reply.id.into_val(env),
            ]);
            let content_bytes: Bytes = env.invoke_contract(
                content,
                &Symbol::new(env, "get_reply_content"),
                args,
            );

            md = md.blockquote("").raw(content_bytes);
        }

        // Reply meta and actions
        md = md.text("*Reply #").number(reply.id as u32).text("*");

        if viewer.is_some() {
            md = md.text(" | ")
                .render_link("Reply", "")
                .raw_str("/b/").number(board_id as u32)
                .raw_str("/t/").number(thread_id as u32)
                .raw_str("/r/").number(reply.id as u32)
                .raw_str("/reply)");

            md = md.text(" | ")
                .tx_link("Flag", "flag_reply", "");
        }

        md = md.newline().newline();

        // Fetch and render children (limited depth for performance)
        if reply.depth < 3 {
            let children_args: Vec<Val> = Vec::from_array(env, [
                board_id.into_val(env),
                thread_id.into_val(env),
                reply.id.into_val(env),
            ]);
            let children: Vec<ReplyMeta> = env.invoke_contract(
                content,
                &Symbol::new(env, "list_children_replies"),
                children_args,
            );

            for i in 0..children.len() {
                if let Some(child) = children.get(i) {
                    // Indent nested replies
                    md = md.raw_str("  ");
                    md = Self::render_reply_item(env, md, content, &child, board_id, thread_id, viewer);
                }
            }
        }

        md
    }

    /// Render reply form
    fn render_reply_form(env: &Env, board_id: u64, thread_id: u64, parent_id: u64, viewer: &Option<Address>) -> Bytes {
        let mut md = Self::render_nav(env)
            .render_link("< Back to Thread", "")
            .raw_str("/b/").number(board_id as u32)
            .raw_str("/t/").number(thread_id as u32)
            .raw_str(")")
            .newline();

        if parent_id > 0 {
            md = md.h1("Reply to Comment");
        } else {
            md = md.h1("Reply to Thread");
        }

        if viewer.is_none() {
            md = md.warning("Please connect your wallet to reply.");
            return Self::render_footer_into(md).build();
        }

        md = md
            .textarea("content", 6, "Write your reply...")
            .newline()
            .newline()
            .form_link("Post Reply", "create_reply")
            .newline()
            .newline()
            .render_link("Cancel", "")
            .raw_str("/b/").number(board_id as u32)
            .raw_str("/t/").number(thread_id as u32)
            .raw_str(")");

        Self::render_footer_into(md).build()
    }

    /// Render a single reply view
    fn render_reply(env: &Env, board_id: u64, thread_id: u64, reply_id: u64, viewer: &Option<Address>) -> Bytes {
        let content: Address = env
            .storage()
            .instance()
            .get(&ThemeKey::Content)
            .expect("Not initialized");

        let mut md = Self::render_nav(env)
            .render_link("< Back to Thread", "")
            .raw_str("/b/").number(board_id as u32)
            .raw_str("/t/").number(thread_id as u32)
            .raw_str(")")
            .newline()
            .h1("Reply #").number(reply_id as u32)
            .hr();

        // Fetch reply metadata
        let args: Vec<Val> = Vec::from_array(env, [
            board_id.into_val(env),
            thread_id.into_val(env),
            reply_id.into_val(env),
        ]);
        let reply_opt: Option<ReplyMeta> = env.invoke_contract(
            &content,
            &Symbol::new(env, "get_reply"),
            args.clone(),
        );

        let Some(reply) = reply_opt else {
            return md.warning("Reply not found.").build();
        };

        if reply.is_hidden {
            md = md.warning("This reply has been hidden by a moderator.");
        } else if reply.is_deleted {
            md = md.note("This reply has been deleted.");
        } else {
            // Get reply content
            let content_bytes: Bytes = env.invoke_contract(
                &content,
                &Symbol::new(env, "get_reply_content"),
                args,
            );
            md = md.raw(content_bytes).newline().newline();
        }

        // Actions
        if viewer.is_some() {
            md = md.hr()
                .render_link("Reply", "")
                .raw_str("/b/").number(board_id as u32)
                .raw_str("/t/").number(thread_id as u32)
                .raw_str("/r/").number(reply_id as u32)
                .raw_str("/reply)")
                .text(" | ")
                .tx_link("Flag", "flag_reply", "");
        }

        Self::render_footer_into(md).build()
    }

    /// Render header component
    pub fn render_header(env: Env, _path: Option<String>, _viewer: Option<Address>) -> Bytes {
        Self::render_nav(&env).build()
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
    fn test_render_help() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsTheme, ());
        let client = BoardsThemeClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        let permissions = Address::generate(&env);
        let content = Address::generate(&env);
        client.init(&registry, &permissions, &content);

        // Render help page (doesn't require external contracts)
        let help_path = String::from_str(&env, "/help");
        let html = client.render(&Some(help_path), &None);
        assert!(html.len() > 0);
    }

    #[test]
    fn test_render_create_board() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsTheme, ());
        let client = BoardsThemeClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        let permissions = Address::generate(&env);
        let content = Address::generate(&env);
        client.init(&registry, &permissions, &content);

        // Render create board form (doesn't require external contracts)
        let path = String::from_str(&env, "/create");
        let viewer = Address::generate(&env);
        let html = client.render(&Some(path), &Some(viewer));
        assert!(html.len() > 0);
    }

    #[test]
    fn test_styles() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsTheme, ());
        let client = BoardsThemeClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        let permissions = Address::generate(&env);
        let content = Address::generate(&env);
        client.init(&registry, &permissions, &content);

        let css = client.styles();
        assert!(css.len() > 0);
    }
}
