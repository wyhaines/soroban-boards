#![no_std]

use soroban_render_sdk::prelude::*;
use soroban_render_sdk::bytes::string_to_bytes;
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
            // Moderation routes (must be before simpler board routes)
            .or_handle(b"/b/{id}/members", |req| {
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
                        md = md.raw_str("**[")
                            .text_string(&board.name)
                            .raw_str("](render:/b/")
                            .number(board.id as u32)
                            .raw_str(")**")
                            .text(" - ")
                            .text_string(&board.description)
                            .text(" (")
                            .number(board.thread_count as u32)
                            .text(" threads)");
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
            .redirect("/")  // Return to board list after creating board
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
            .raw_str("# ")
            .text_string(&board.name)
            .newline()
            .newline()
            .text_string(&board.description)
            .newline()
            .hr();

        // Show create thread button if logged in
        if viewer.is_some() && !board.is_readonly {
            md = md.raw_str("[+ New Thread](render:/b/")
                .number(board_id as u32)
                .raw_str("/new)")
                .newline()
                .newline();
        }

        if board.is_readonly {
            md = md.note("This board is read-only.");
        }

        md = md.h2("Threads");

        // Fetch board contract address from registry
        let board_contract_args: Vec<Val> = Vec::from_array(env, [board_id.into_val(env)]);
        let board_contract_opt: Option<Address> = env.invoke_contract(
            &registry,
            &Symbol::new(env, "get_board_contract"),
            board_contract_args,
        );

        if let Some(board_contract) = board_contract_opt {
            // Fetch threads from board contract
            let list_args: Vec<Val> = Vec::from_array(env, [0u64.into_val(env), 20u64.into_val(env)]);
            let threads: Vec<ThreadMeta> = env.invoke_contract(
                &board_contract,
                &Symbol::new(env, "list_threads"),
                list_args,
            );

            if threads.is_empty() {
                md = md.paragraph("No threads yet. Be the first to post!");
            } else {
                for i in 0..threads.len() {
                    let thread = threads.get(i).unwrap();
                    // Thread link: /b/{board_id}/t/{thread_id}
                    md = md.raw_str("- **[")
                        .text_string(&thread.title)
                        .raw_str("](render:/b/")
                        .number(board_id as u32)
                        .raw_str("/t/")
                        .number(thread.id as u32)
                        .raw_str(")** - ")
                        .number(thread.reply_count)
                        .raw_str(" replies\n");
                }
            }
        } else {
            // No board contract registered yet
            md = md.paragraph("No threads yet. Be the first to post!");
        }

        Self::render_footer_into(md).build()
    }

    /// Render create thread form
    fn render_create_thread(env: &Env, board_id: u64, viewer: &Option<Address>) -> Bytes {
        let mut md = Self::render_nav(env)
            .raw_str("[< Back to Board](render:/b/")
            .number(board_id as u32)
            .raw_str(")")
            .newline()
            .h1("New Thread");

        if viewer.is_none() {
            md = md.warning("Please connect your wallet to create a thread.");
            return Self::render_footer_into(md).build();
        }

        // Hidden inputs for form metadata
        md = md
            // Redirect to board page after creating thread
            .raw_str("<input type=\"hidden\" name=\"_redirect\" value=\"/b/")
            .number(board_id as u32)
            .raw_str("\" />\n")
            // Board ID for the contract
            .raw_str("<input type=\"hidden\" name=\"board_id\" value=\"")
            .number(board_id as u32)
            .raw_str("\" />\n")
            .input("title", "Thread title")
            .newline()
            .textarea("body", 10, "Write your post content here...")
            .newline()
            .newline()
            .form_link("Create Thread", "create_thread")
            .newline()
            .newline()
            .raw_str("[Cancel](render:/b/")
            .number(board_id as u32)
            .raw_str(")");

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
            .raw_str("[< Back to Board](render:/b/")
            .number(board_id as u32)
            .raw_str(")")
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
            md = md.raw_str("[Reply](render:/b/")
                .number(board_id as u32)
                .raw_str("/t/")
                .number(thread_id as u32)
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
                .raw_str("[Reply](render:/b/")
                .number(board_id as u32)
                .raw_str("/t/")
                .number(thread_id as u32)
                .raw_str("/r/")
                .number(reply.id as u32)
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
            .raw_str("[< Back to Thread](render:/b/")
            .number(board_id as u32)
            .raw_str("/t/")
            .number(thread_id as u32)
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

        // Hidden inputs for board_id, thread_id, parent_id, depth
        // Depth: 0 for top-level replies, 1 for nested (simplified)
        let depth: u32 = if parent_id > 0 { 1 } else { 0 };
        md = md
            // Redirect to thread view after posting reply
            .raw_str("<input type=\"hidden\" name=\"_redirect\" value=\"/b/")
            .number(board_id as u32)
            .raw_str("/t/")
            .number(thread_id as u32)
            .raw_str("\" />\n")
            // Form data for contract
            .raw_str("<input type=\"hidden\" name=\"board_id\" value=\"")
            .number(board_id as u32)
            .raw_str("\" />\n")
            .raw_str("<input type=\"hidden\" name=\"thread_id\" value=\"")
            .number(thread_id as u32)
            .raw_str("\" />\n")
            .raw_str("<input type=\"hidden\" name=\"parent_id\" value=\"")
            .number(parent_id as u32)
            .raw_str("\" />\n")
            .raw_str("<input type=\"hidden\" name=\"depth\" value=\"")
            .number(depth)
            .raw_str("\" />\n")
            .textarea("content_str", 6, "Write your reply...")
            .newline()
            .newline()
            .form_link("Post Reply", "create_reply")
            .newline()
            .newline()
            .raw_str("[Cancel](render:/b/")
            .number(board_id as u32)
            .raw_str("/t/")
            .number(thread_id as u32)
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
            .raw_str("[< Back to Thread](render:/b/")
            .number(board_id as u32)
            .raw_str("/t/")
            .number(thread_id as u32)
            .raw_str(")")
            .newline()
            .raw_str("# Reply #")
            .number(reply_id as u32)
            .newline()
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
                .raw_str("[Reply](render:/b/")
                .number(board_id as u32)
                .raw_str("/t/")
                .number(thread_id as u32)
                .raw_str("/r/")
                .number(reply_id as u32)
                .raw_str("/reply)")
                .text(" | ")
                .tx_link("Flag", "flag_reply", "");
        }

        Self::render_footer_into(md).build()
    }

    /// Render members list page
    fn render_members(env: &Env, board_id: u64, viewer: &Option<Address>) -> Bytes {
        let permissions: Address = env
            .storage()
            .instance()
            .get(&ThemeKey::Permissions)
            .expect("Not initialized");

        let mut md = Self::render_nav(env)
            .raw_str("[< Back to Board](render:/b/")
            .number(board_id as u32)
            .raw_str(")")
            .newline()
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
        if let Some(_owner) = owner_opt {
            // Address display - in production, use address shortener or full display
            md = md.text("- ").code("[owner address]").newline();
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
                let _addr = admins.get(i).unwrap();
                md = md.text("- ").code("[admin ").number(i as u32).code("]").newline();
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
                let _addr = mods.get(i).unwrap();
                md = md.text("- ").code("[moderator ").number(i as u32).code("]").newline();
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
                let _addr = members.get(i).unwrap();
                md = md.text("- ").code("[member ").number(i as u32).code("]").newline();
            }
        }

        Self::render_footer_into(md).build()
    }

    /// Render banned users page
    fn render_banned(env: &Env, board_id: u64, viewer: &Option<Address>) -> Bytes {
        let permissions: Address = env
            .storage()
            .instance()
            .get(&ThemeKey::Permissions)
            .expect("Not initialized");

        let mut md = Self::render_nav(env)
            .raw_str("[< Back to Board](render:/b/")
            .number(board_id as u32)
            .raw_str(")")
            .newline()
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
            md = md.paragraph("No banned users.");
        } else {
            for i in 0..bans.len() {
                let ban = bans.get(i).unwrap();
                md = md.h3("Banned User #").number(i as u32);
                md = md.text("**Reason:** ").text_string(&ban.reason).newline();
                md = md.text("**Issued by:** ").code("[issuer address]").newline();

                if let Some(expires) = ban.expires_at {
                    md = md.text("**Expires:** ").number(expires as u32).text(" (timestamp)").newline();
                } else {
                    md = md.text("**Expires:** *Permanent*").newline();
                }

                // Unban link
                md = md.tx_link("Unban", "unban_user", "").newline().newline();
            }
        }

        Self::render_footer_into(md).build()
    }

    /// Render flag queue page
    fn render_flag_queue(env: &Env, board_id: u64, viewer: &Option<Address>) -> Bytes {
        let permissions: Address = env
            .storage()
            .instance()
            .get(&ThemeKey::Permissions)
            .expect("Not initialized");
        let content: Address = env
            .storage()
            .instance()
            .get(&ThemeKey::Content)
            .expect("Not initialized");

        let mut md = Self::render_nav(env)
            .raw_str("[< Back to Board](render:/b/")
            .number(board_id as u32)
            .raw_str(")")
            .newline()
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

                // Actions
                md = md.text(" | ").tx_link("Hide", "hide_content", "");
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
            .get(&ThemeKey::Permissions)
            .expect("Not initialized");

        let mut md = Self::render_nav(env)
            .raw_str("[< Back to Board](render:/b/")
            .number(board_id as u32)
            .raw_str(")")
            .newline()
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

        md = md.h2("Moderation Settings");
        md = md.text("**Flag threshold for auto-hide:** ").number(threshold).newline();
        md = md.input("threshold", "New threshold")
            .newline()
            .form_link("Update Threshold", "set_flag_threshold")
            .newline()
            .newline();

        md = md.h2("Role Management");
        md = md.paragraph("Assign roles to users on this board.");
        md = md.input("user_address", "User wallet address")
            .newline()
            .text("Select role: ")
            .text("[").tx_link("Member", "set_role_member", "").text("] ")
            .text("[").tx_link("Moderator", "set_role_mod", "").text("] ")
            .text("[").tx_link("Admin", "set_role_admin", "").text("]")
            .newline()
            .newline();

        md = md.h2("Quick Links");
        md = md.raw_str("[View Members](render:/b/")
            .number(board_id as u32)
            .raw_str("/members)")
            .text(" | ");
        md = md.raw_str("[View Banned](render:/b/")
            .number(board_id as u32)
            .raw_str("/banned)")
            .text(" | ");
        md = md.raw_str("[View Flag Queue](render:/b/")
            .number(board_id as u32)
            .raw_str("/flags)");

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
            // Spacing
            .var("space-xs", "0.25rem")
            .var("space-sm", "0.5rem")
            .var("space-md", "1rem")
            .var("space-lg", "1.5rem")
            .var("space-xl", "2rem")
            // Container max width
            .var("container-max", "48rem")
            .root_vars_end()
            // Base styles
            .rule("*", "box-sizing: border-box;")
            .rule("body", "font-family: 'Inter', -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; color: var(--text); background: var(--bg); line-height: 1.6; margin: 0; padding: var(--space-md);")
            // Container
            .rule(".container", "max-width: var(--container-max); margin: 0 auto; padding: 0 var(--space-md);")
            // Typography
            .rule("h1", "font-size: 1.875rem; font-weight: 600; margin: 0 0 var(--space-md) 0; word-wrap: break-word;")
            .rule("h2", "font-size: 1.5rem; font-weight: 600; margin: var(--space-xl) 0 var(--space-md) 0;")
            .rule("h3", "font-size: 1.25rem; font-weight: 600; margin: var(--space-lg) 0 var(--space-sm) 0;")
            .rule("p", "margin: 0 0 var(--space-md) 0;")
            // Links
            .rule("a", "color: var(--primary); text-decoration: none;")
            .rule("a:hover", "color: var(--primary-hover); text-decoration: underline;")
            // Code
            .rule("code", "font-family: 'Inconsolata', 'Monaco', monospace; background: var(--bg-muted); padding: 0.15rem 0.4rem; border-radius: 4px; word-break: break-all;")
            .rule("pre", "overflow-x: auto; padding: var(--space-md); background: var(--bg-muted); border-radius: 4px;")
            // Thread items
            .rule(".thread-item", "padding: var(--space-md); border-bottom: 1px solid var(--border);")
            .rule(".thread-title", "font-weight: 600; color: var(--text);")
            .rule(".thread-meta", "color: var(--text-muted); font-size: 0.875rem;")
            // Replies - responsive nesting
            .rule(".reply", "margin-left: var(--space-lg); padding: var(--space-sm); border-left: 2px solid var(--border);")
            .rule(".reply-nested", "margin-left: var(--space-md);")
            // Blockquotes (for replies)
            .rule("blockquote", "margin: 0 0 var(--space-md) 0; padding: var(--space-sm) var(--space-md); border-left: 3px solid var(--primary); background: var(--bg-muted);")
            // Forms
            .rule("input, textarea", "width: 100%; padding: var(--space-sm); border: 1px solid var(--border); border-radius: 4px; font-size: 1rem; background: var(--bg);")
            .rule("input:focus, textarea:focus", "outline: none; border-color: var(--primary);")
            .rule("textarea", "resize: vertical; min-height: 100px;")
            // Buttons
            .rule("button, .btn", "background: var(--primary); color: white; padding: var(--space-sm) var(--space-md); border: none; border-radius: 4px; cursor: pointer; font-size: 1rem;")
            .rule("button:hover, .btn:hover", "background: var(--primary-hover);")
            .rule(".btn-secondary", "background: var(--bg-muted); color: var(--text);")
            .rule(".btn-secondary:hover", "background: var(--border);")
            // Alerts/Notices
            .rule(".alert", "padding: var(--space-md); border-radius: 4px; margin-bottom: var(--space-md);")
            .rule(".alert-success", "background: #d3f9d8; color: #1e7a34;")
            .rule(".alert-warning", "background: #fff3bf; color: #946c00;")
            .rule(".alert-danger", "background: #ffd8d8; color: #c41d1d;")
            .rule(".alert-info", "background: #e8e4fd; color: #5c4bad;")
            // Navigation
            .rule(".nav", "display: flex; flex-wrap: wrap; gap: var(--space-sm); align-items: center; padding: var(--space-md) 0; border-bottom: 1px solid var(--border);")
            // Cards/Boards
            .rule(".card", "background: var(--bg); border: 1px solid var(--border); border-radius: 4px; padding: var(--space-md); margin-bottom: var(--space-md);")
            // Horizontal rule
            .rule("hr", "border: none; border-top: 1px solid var(--border); margin: var(--space-lg) 0;")
            // Lists
            .rule("ul, ol", "padding-left: var(--space-lg); margin: 0 0 var(--space-md) 0;")
            .rule("li", "margin-bottom: var(--space-xs);")
            // Action links
            .rule(".actions", "display: flex; flex-wrap: wrap; gap: var(--space-sm); font-size: 0.875rem;")
            // Badge/Tag
            .rule(".badge", "display: inline-block; padding: 0.125rem 0.5rem; background: var(--bg-muted); border-radius: 9999px; font-size: 0.75rem;")
            .rule(".badge-pinned", "background: #ffeeba; color: #856404;")
            .rule(".badge-locked", "background: #f8d7da; color: #721c24;")
            // Dark mode
            .dark_mode_start()
            .rule_start(":root")
            .prop("--text", "#e0e0e0")
            .prop("--text-muted", "#a0a0a0")
            .prop("--bg", "#0f0f0f")
            .prop("--bg-muted", "#1c1c1c")
            .prop("--border", "#3e3e3e")
            .rule_end()
            .rule(".alert-success", "background: #1e3a28; color: #6fdd8b;")
            .rule(".alert-warning", "background: #3a3019; color: #ffd859;")
            .rule(".alert-danger", "background: #3a1c1c; color: #ff8080;")
            .rule(".alert-info", "background: #2a2644; color: #b8a8e8;")
            .rule(".badge-pinned", "background: #3a3019; color: #ffd859;")
            .rule(".badge-locked", "background: #3a1c1c; color: #ff8080;")
            .media_end()
            // Mobile responsive styles
            .media_start("(max-width: 640px)")
            .rule_start(":root")
            .prop("--space-lg", "1rem")
            .prop("--space-xl", "1.5rem")
            .rule_end()
            .rule("body", "padding: var(--space-sm);")
            .rule("h1", "font-size: 1.5rem;")
            .rule("h2", "font-size: 1.25rem;")
            .rule("h3", "font-size: 1.125rem;")
            .rule(".reply", "margin-left: var(--space-sm);")
            .rule(".reply-nested", "margin-left: var(--space-xs);")
            .rule("blockquote", "padding: var(--space-xs) var(--space-sm);")
            .rule(".nav", "font-size: 0.875rem;")
            .media_end()
            // Very small screens
            .media_start("(max-width: 375px)")
            .rule("body", "font-size: 0.9375rem;")
            .rule("h1", "font-size: 1.25rem;")
            .rule(".reply", "margin-left: var(--space-xs);")
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

    // ========================================================================
    // Write Operations (proxy methods for forms)
    // ========================================================================

    /// Create a new board (proxies to registry)
    pub fn create_board(
        env: Env,
        name: String,
        description: String,
        caller: Address,
    ) -> u64 {
        caller.require_auth();

        let registry: Address = env
            .storage()
            .instance()
            .get(&ThemeKey::Registry)
            .expect("Not initialized");

        // Call registry.create_board
        let args: Vec<Val> = Vec::from_array(&env, [
            name.into_val(&env),
            description.into_val(&env),
            caller.into_val(&env),
            false.into_val(&env),  // is_private = false by default
        ]);

        env.invoke_contract(&registry, &Symbol::new(&env, "create_board"), args)
    }

    /// Create a new thread (proxies to board contract)
    pub fn create_thread(
        env: Env,
        board_id: u64,
        title: String,
        body: String,
        caller: Address,
    ) -> u64 {
        caller.require_auth();

        let registry: Address = env
            .storage()
            .instance()
            .get(&ThemeKey::Registry)
            .expect("Not initialized");
        let content: Address = env
            .storage()
            .instance()
            .get(&ThemeKey::Content)
            .expect("Not initialized");

        // Get board contract from registry
        let board_args: Vec<Val> = Vec::from_array(&env, [board_id.into_val(&env)]);
        let board_contract: Address = env
            .invoke_contract::<Option<Address>>(
                &registry,
                &Symbol::new(&env, "get_board_contract"),
                board_args,
            )
            .expect("Board contract not found");

        // Create thread on board contract
        let create_args: Vec<Val> = Vec::from_array(&env, [
            title.into_val(&env),
            caller.clone().into_val(&env),
        ]);
        let thread_id: u64 = env.invoke_contract(
            &board_contract,
            &Symbol::new(&env, "create_thread"),
            create_args,
        );

        // Increment thread count in registry
        let inc_args: Vec<Val> = Vec::from_array(&env, [board_id.into_val(&env)]);
        env.invoke_contract::<()>(
            &registry,
            &Symbol::new(&env, "increment_thread_count"),
            inc_args,
        );

        // Set thread body on content contract
        let body_bytes = string_to_bytes(&env, &body);

        let body_args: Vec<Val> = Vec::from_array(&env, [
            board_id.into_val(&env),
            thread_id.into_val(&env),
            body_bytes.into_val(&env),
            caller.into_val(&env),
        ]);
        env.invoke_contract::<()>(
            &content,
            &Symbol::new(&env, "set_thread_body"),
            body_args,
        );

        thread_id
    }

    /// Create a reply (proxies to content contract)
    pub fn create_reply(
        env: Env,
        board_id: u64,
        thread_id: u64,
        parent_id: u64,
        depth: u32,
        content_str: String,
        caller: Address,
    ) -> u64 {
        caller.require_auth();

        let registry: Address = env
            .storage()
            .instance()
            .get(&ThemeKey::Registry)
            .expect("Not initialized");
        let content: Address = env
            .storage()
            .instance()
            .get(&ThemeKey::Content)
            .expect("Not initialized");

        // Convert content string to bytes
        let content_bytes = string_to_bytes(&env, &content_str);

        // Call content.create_reply
        let args: Vec<Val> = Vec::from_array(&env, [
            board_id.into_val(&env),
            thread_id.into_val(&env),
            parent_id.into_val(&env),
            depth.into_val(&env),
            content_bytes.into_val(&env),
            caller.into_val(&env),
        ]);

        let reply_id: u64 = env.invoke_contract(&content, &Symbol::new(&env, "create_reply"), args);

        // Increment reply count in board contract
        let board_args: Vec<Val> = Vec::from_array(&env, [board_id.into_val(&env)]);
        if let Some(board_contract) = env.invoke_contract::<Option<Address>>(
            &registry,
            &Symbol::new(&env, "get_board_contract"),
            board_args,
        ) {
            let inc_args: Vec<Val> = Vec::from_array(&env, [thread_id.into_val(&env)]);
            env.invoke_contract::<()>(
                &board_contract,
                &Symbol::new(&env, "increment_reply_count"),
                inc_args,
            );
        }

        reply_id
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
