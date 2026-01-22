#![no_std]
#![allow(clippy::too_many_arguments)]

use soroban_chonk::prelude::*;
use soroban_render_sdk::prelude::*;
use soroban_sdk::{
    contract, contractimpl, contracttype, Address, Bytes, BytesN, Env, IntoVal, String, Symbol,
    Val, Vec,
};

// Declare render capabilities
soroban_render!(markdown);

/// Storage keys for the pages contract
#[contracttype]
#[derive(Clone)]
pub enum PagesKey {
    /// Registry contract address
    Registry,
    /// Total number of pages
    PageCount,
    /// Page metadata by ID
    Page(u64),
    /// Page ID by slug (lowercase lookup)
    PageBySlug(String),
    /// Next chonk ID counter
    NextChonkId,
    /// Page content chonk symbol (page_id) -> Symbol
    PageContentChonk(u64),
}

/// Page metadata
#[contracttype]
#[derive(Clone)]
pub struct PageMeta {
    /// Unique page ID
    pub id: u64,
    /// URL slug (e.g., "help", "about")
    pub slug: String,
    /// Admin reference name (for listings)
    pub name: String,
    /// Navigation label (empty = use name)
    pub nav_label: String,
    /// Creator/last editor
    pub author: Address,
    /// Creation timestamp
    pub created_at: u64,
    /// Last update timestamp
    pub updated_at: u64,
    /// Visible/hidden toggle
    pub is_visible: bool,
    /// Show in top navigation
    pub show_in_nav: bool,
    /// Navigation order (lower = first)
    pub nav_order: u32,
}

#[contract]
pub struct BoardsPages;

#[contractimpl]
impl BoardsPages {
    /// Initialize the pages contract with registry address
    /// Also seeds the Help page with default content
    pub fn init(env: Env, registry: Address) {
        if env.storage().instance().has(&PagesKey::Registry) {
            panic!("Already initialized");
        }

        env.storage().instance().set(&PagesKey::Registry, &registry);
        env.storage().instance().set(&PagesKey::PageCount, &0u64);
        env.storage().instance().set(&PagesKey::NextChonkId, &0u64);

        // Seed the Help page
        Self::seed_help_page(&env, &registry);
    }

    /// Seed the default Help page
    fn seed_help_page(env: &Env, registry: &Address) {
        let page_id = 0u64;
        let slug = String::from_str(env, "help");
        let name = String::from_str(env, "Help");
        let nav_label = String::from_str(env, "Help");

        let help_content = r#"# Getting Started

Welcome to Soroban Boards! This is a decentralized message board running on the Stellar blockchain.

## Navigation

- **Home**: Browse all available boards
- **Communities**: Explore or create topic communities
- **Help**: You are here!

## Creating Content

### Boards
Boards are organized discussion spaces. Anyone can create a board (subject to creation thresholds).

### Threads
Start a new discussion by creating a thread within a board. Threads have a title and body content.

### Replies
Participate in discussions by replying to threads. Nested replies are supported.

## Markdown Support

Content supports markdown formatting:
- **Bold** and *italic* text
- `Code snippets`
- Links and images
- Lists and headers

## Connecting Your Wallet

To create content, you'll need a Stellar wallet like Freighter. Connect your wallet using the button in the navigation bar.

## Need More Help?

Contact the site administrators for additional assistance."#;

        // Create page metadata
        let page = PageMeta {
            id: page_id,
            slug: slug.clone(),
            name,
            nav_label,
            author: registry.clone(), // Registry as initial author
            created_at: env.ledger().timestamp(),
            updated_at: env.ledger().timestamp(),
            is_visible: true,
            show_in_nav: true,
            nav_order: 1000, // High number so custom pages appear first
        };

        // Store page metadata
        env.storage()
            .persistent()
            .set(&PagesKey::Page(page_id), &page);

        // Store slug lookup (lowercase)
        let slug_lower = Self::to_lowercase(env, &slug);
        env.storage()
            .persistent()
            .set(&PagesKey::PageBySlug(slug_lower), &page_id);

        // Store content
        let content_bytes = Bytes::from_slice(env, help_content.as_bytes());
        let chonk_key = Self::get_or_create_page_chonk(env, page_id);
        let chonk = Chonk::open(env, chonk_key);
        chonk.write_chunked(content_bytes, 4096);

        // Update page count
        env.storage().instance().set(&PagesKey::PageCount, &1u64);
    }

    /// Check if caller is a site admin
    fn is_admin(env: &Env, user: &Address) -> bool {
        let registry: Address = env
            .storage()
            .instance()
            .get(&PagesKey::Registry)
            .expect("Not initialized");

        let args: Vec<Val> = Vec::from_array(env, [user.clone().into_val(env)]);
        env.try_invoke_contract::<bool, soroban_sdk::Error>(
            &registry,
            &Symbol::new(env, "is_admin"),
            args,
        )
        .ok()
        .and_then(|r| r.ok())
        .unwrap_or(false)
    }

    /// Create a new page
    pub fn create_page(
        env: Env,
        slug: String,
        name: String,
        nav_label: String,
        content: String,
        nav_order: String,
        is_visible: String,
        show_in_nav: String,
        caller: Address,
    ) -> u64 {
        caller.require_auth();

        // Check admin permission
        if !Self::is_admin(&env, &caller) {
            panic!("Only site admins can create pages");
        }

        // Validate slug
        Self::validate_slug(&env, &slug);

        // Check slug not already taken
        let slug_lower = Self::to_lowercase(&env, &slug);
        if env
            .storage()
            .persistent()
            .has(&PagesKey::PageBySlug(slug_lower.clone()))
        {
            panic!("Page slug already exists");
        }

        // Get next page ID
        let page_id: u64 = env
            .storage()
            .instance()
            .get(&PagesKey::PageCount)
            .unwrap_or(0);

        // Parse form strings
        let is_visible_bool = is_visible == String::from_str(&env, "true");
        let show_in_nav_bool = show_in_nav == String::from_str(&env, "true");
        let nav_order_u32 = string_to_u32(&env, &nav_order).unwrap_or(100);

        // Create page metadata
        let page = PageMeta {
            id: page_id,
            slug: slug.clone(),
            name,
            nav_label,
            author: caller,
            created_at: env.ledger().timestamp(),
            updated_at: env.ledger().timestamp(),
            is_visible: is_visible_bool,
            show_in_nav: show_in_nav_bool,
            nav_order: nav_order_u32,
        };

        // Store page
        env.storage()
            .persistent()
            .set(&PagesKey::Page(page_id), &page);

        // Store slug lookup
        env.storage()
            .persistent()
            .set(&PagesKey::PageBySlug(slug_lower), &page_id);

        // Store content
        let content_len = content.len() as usize;
        let content_bytes = if content_len > 0 && content_len <= 16384 {
            let mut temp = [0u8; 16384];
            content.copy_into_slice(&mut temp[..content_len]);
            Bytes::from_slice(&env, &temp[..content_len])
        } else if content_len > 16384 {
            let mut temp = [0u8; 16384];
            content.copy_into_slice(&mut temp[..16384]);
            Bytes::from_slice(&env, &temp)
        } else {
            Bytes::new(&env)
        };

        let chonk_key = Self::get_or_create_page_chonk(&env, page_id);
        let chonk = Chonk::open(&env, chonk_key);
        chonk.write_chunked(content_bytes, 4096);

        // Update count
        env.storage()
            .instance()
            .set(&PagesKey::PageCount, &(page_id + 1));

        page_id
    }

    /// Update page metadata (not content)
    pub fn update_page(
        env: Env,
        page_id: u64,
        name: String,
        nav_label: String,
        nav_order: String,
        is_visible: String,
        show_in_nav: String,
        caller: Address,
    ) {
        caller.require_auth();

        if !Self::is_admin(&env, &caller) {
            panic!("Only site admins can update pages");
        }

        let mut page: PageMeta = env
            .storage()
            .persistent()
            .get(&PagesKey::Page(page_id))
            .expect("Page not found");

        // Parse form strings
        let is_visible_bool = is_visible == String::from_str(&env, "true");
        let show_in_nav_bool = show_in_nav == String::from_str(&env, "true");
        let nav_order_u32 = string_to_u32(&env, &nav_order).unwrap_or(page.nav_order);

        page.name = name;
        page.nav_label = nav_label;
        page.is_visible = is_visible_bool;
        page.show_in_nav = show_in_nav_bool;
        page.nav_order = nav_order_u32;
        page.updated_at = env.ledger().timestamp();
        page.author = caller;

        env.storage()
            .persistent()
            .set(&PagesKey::Page(page_id), &page);
    }

    /// Update page content only
    pub fn update_page_content(env: Env, page_id: u64, content: String, caller: Address) {
        caller.require_auth();

        if !Self::is_admin(&env, &caller) {
            panic!("Only site admins can update page content");
        }

        let mut page: PageMeta = env
            .storage()
            .persistent()
            .get(&PagesKey::Page(page_id))
            .expect("Page not found");

        page.updated_at = env.ledger().timestamp();
        page.author = caller;
        env.storage()
            .persistent()
            .set(&PagesKey::Page(page_id), &page);

        // Update content
        let content_len = content.len() as usize;
        let content_bytes = if content_len > 0 && content_len <= 16384 {
            let mut temp = [0u8; 16384];
            content.copy_into_slice(&mut temp[..content_len]);
            Bytes::from_slice(&env, &temp[..content_len])
        } else if content_len > 16384 {
            let mut temp = [0u8; 16384];
            content.copy_into_slice(&mut temp[..16384]);
            Bytes::from_slice(&env, &temp)
        } else {
            Bytes::new(&env)
        };

        let chonk_key = Self::get_or_create_page_chonk(&env, page_id);
        let chonk = Chonk::open(&env, chonk_key);
        chonk.clear();
        chonk.write_chunked(content_bytes, 4096);
    }

    /// Delete a page
    pub fn delete_page(env: Env, page_id: u64, caller: Address) {
        caller.require_auth();

        if !Self::is_admin(&env, &caller) {
            panic!("Only site admins can delete pages");
        }

        let page: PageMeta = env
            .storage()
            .persistent()
            .get(&PagesKey::Page(page_id))
            .expect("Page not found");

        // Remove slug lookup
        let slug_lower = Self::to_lowercase(&env, &page.slug);
        env.storage()
            .persistent()
            .remove(&PagesKey::PageBySlug(slug_lower));

        // Remove page metadata
        env.storage().persistent().remove(&PagesKey::Page(page_id));

        // Clear content
        if let Some(chonk_key) = Self::get_page_chonk(&env, page_id) {
            let chonk = Chonk::open(&env, chonk_key);
            chonk.clear();
        }
        env.storage()
            .persistent()
            .remove(&PagesKey::PageContentChonk(page_id));
    }

    /// Get page by ID
    pub fn get_page(env: Env, page_id: u64) -> Option<PageMeta> {
        env.storage().persistent().get(&PagesKey::Page(page_id))
    }

    /// Get page by slug (case-insensitive)
    pub fn get_page_by_slug(env: Env, slug: String) -> Option<PageMeta> {
        let slug_lower = Self::to_lowercase(&env, &slug);
        if let Some(page_id) = env
            .storage()
            .persistent()
            .get::<_, u64>(&PagesKey::PageBySlug(slug_lower))
        {
            return env.storage().persistent().get(&PagesKey::Page(page_id));
        }
        None
    }

    /// Get page content
    pub fn get_page_content(env: Env, page_id: u64) -> Bytes {
        if let Some(key) = Self::get_page_chonk(&env, page_id) {
            let chonk = Chonk::open(&env, key);
            chonk.assemble()
        } else {
            Bytes::new(&env)
        }
    }

    /// List all pages (for admin)
    pub fn list_pages(env: Env) -> Vec<PageMeta> {
        let count: u64 = env
            .storage()
            .instance()
            .get(&PagesKey::PageCount)
            .unwrap_or(0);

        let mut pages = Vec::new(&env);
        for i in 0..count {
            if let Some(page) = env
                .storage()
                .persistent()
                .get::<_, PageMeta>(&PagesKey::Page(i))
            {
                pages.push_back(page);
            }
        }
        pages
    }

    /// Get visible pages that should appear in navigation
    /// Returns pages ordered by nav_order (ascending)
    pub fn get_nav_pages(env: Env) -> Vec<PageMeta> {
        let count: u64 = env
            .storage()
            .instance()
            .get(&PagesKey::PageCount)
            .unwrap_or(0);

        // Collect all nav pages
        let mut nav_pages: Vec<PageMeta> = Vec::new(&env);
        for i in 0..count {
            if let Some(page) = env
                .storage()
                .persistent()
                .get::<_, PageMeta>(&PagesKey::Page(i))
            {
                if page.is_visible && page.show_in_nav {
                    nav_pages.push_back(page);
                }
            }
        }

        // Sort by nav_order (simple insertion sort since list is typically small)
        let len = nav_pages.len();
        for i in 1..len {
            let current = nav_pages.get(i).unwrap();
            let mut j = i;
            while j > 0 {
                let prev = nav_pages.get(j - 1).unwrap();
                if prev.nav_order <= current.nav_order {
                    break;
                }
                nav_pages.set(j, prev);
                j -= 1;
            }
            nav_pages.set(j, current);
        }

        nav_pages
    }

    /// Main render entry point
    pub fn render(env: Env, path: String, viewer: Option<Address>) -> Bytes {
        let path_bytes = string_to_bytes(&env, &path);
        let path_len = path_bytes.len() as usize;
        let mut buf = [0u8; 256];
        let copy_len = core::cmp::min(path_len, 256);
        path_bytes.copy_into_slice(&mut buf[..copy_len]);

        // Check for admin routes
        if copy_len >= 6 && &buf[0..6] == b"/admin" {
            // /admin - list pages
            if copy_len == 6 || (copy_len == 7 && buf[6] == b'/') {
                return Self::render_admin_list(&env, viewer);
            }
            // /admin/new - create form
            if copy_len >= 10 && &buf[6..10] == b"/new" {
                return Self::render_admin_new(&env, viewer);
            }
            // /admin/{id}/edit - edit form
            if copy_len > 7 {
                // Extract ID and check for /edit suffix
                let id_start = 7;
                let mut id_end = id_start;
                while id_end < copy_len && buf[id_end] != b'/' {
                    id_end += 1;
                }
                if id_end > id_start {
                    if let Ok(id_str) = core::str::from_utf8(&buf[id_start..id_end]) {
                        if let Ok(page_id) = id_str.parse::<u64>() {
                            // Check if /edit follows
                            if id_end + 5 <= copy_len && &buf[id_end..id_end + 5] == b"/edit" {
                                return Self::render_admin_edit(&env, page_id, viewer);
                            }
                        }
                    }
                }
            }
        }

        // Default: view page by slug
        // Path is either "/" (root) or "/{slug}"
        let slug = if copy_len <= 1 {
            // Root path - show help by default
            String::from_str(&env, "help")
        } else {
            // Skip leading slash
            let slug_slice = &buf[1..copy_len];
            String::from_str(&env, core::str::from_utf8(slug_slice).unwrap_or("help"))
        };

        Self::render_page_view(&env, &slug, viewer)
    }

    /// Render page view
    fn render_page_view(env: &Env, slug: &String, viewer: Option<Address>) -> Bytes {
        let page_opt = Self::get_page_by_slug(env.clone(), slug.clone());

        let page = match page_opt {
            Some(p) => p,
            None => {
                return MarkdownBuilder::new(env)
                    .h1("Page Not Found")
                    .paragraph("The requested page does not exist.")
                    .build();
            }
        };

        // Check visibility
        if !page.is_visible {
            // Only admins can view hidden pages
            let can_view = match &viewer {
                Some(v) => Self::is_admin(env, v),
                None => false,
            };
            if !can_view {
                return MarkdownBuilder::new(env)
                    .h1("Page Not Available")
                    .paragraph("This page is currently hidden.")
                    .build();
            }
        }

        // Get page content
        let content = Self::get_page_content(env.clone(), page.id);

        // Render page
        let mut builder = MarkdownBuilder::new(env);

        // If admin, show edit link
        if let Some(ref v) = viewer {
            if Self::is_admin(env, v) {
                builder = builder.raw_str("<div class=\"page-admin-actions\">");
                builder = builder.raw_str("<a href=\"render:/p/admin/");
                builder = builder.number(page.id as u32);
                builder = builder.raw_str("/edit\" class=\"soroban-action\">Edit Page</a>");
                if !page.is_visible {
                    builder = builder.raw_str(" <span class=\"badge\">Hidden</span>");
                }
                builder = builder.raw_str("</div>\n");
                builder = builder.newline();
            }
        }

        // Render markdown content
        builder = builder.raw(content);

        // Show last updated info
        builder = builder.newline();
        builder = builder.raw_str("<div class=\"page-meta\">");
        builder = builder.raw_str("<small>Last updated: <span class=\"timestamp\" data-ts=\"");
        builder = builder.number(page.updated_at as u32);
        builder = builder.raw_str("\"></span></small>");
        builder = builder.raw_str("</div>\n");

        builder.build()
    }

    /// Render admin page list
    fn render_admin_list(env: &Env, viewer: Option<Address>) -> Bytes {
        // Check admin permission
        let is_admin = match &viewer {
            Some(v) => Self::is_admin(env, v),
            None => false,
        };

        if !is_admin {
            return MarkdownBuilder::new(env)
                .warning("Only site admins can access page management.")
                .build();
        }

        let pages = Self::list_pages(env.clone());

        let mut builder = MarkdownBuilder::new(env);
        builder = builder.h1("Manage Pages");

        builder = builder.raw_str(
            "<p><a class=\"soroban-action\" href=\"render:/p/admin/new\">+ New Page</a></p>\n",
        );
        builder = builder.newline();

        if pages.is_empty() {
            builder = builder.paragraph("No pages yet.");
        } else {
            builder = builder.raw_str("<table class=\"page-list\">\n");
            builder = builder.raw_str("<tr><th>Name</th><th>Slug</th><th>Nav</th><th>Order</th><th>Visible</th><th>Actions</th></tr>\n");

            for page in pages.iter() {
                let p: PageMeta = page;
                builder = builder.raw_str("<tr>");

                // Name
                builder = builder.raw_str("<td>");
                builder = builder.text_string(&p.name);
                builder = builder.raw_str("</td>");

                // Slug
                builder = builder.raw_str("<td><code>");
                builder = builder.text_string(&p.slug);
                builder = builder.raw_str("</code></td>");

                // In Nav
                builder = builder.raw_str("<td>");
                if p.show_in_nav {
                    builder = builder.text("Yes");
                } else {
                    builder = builder.text("No");
                }
                builder = builder.raw_str("</td>");

                // Nav Order
                builder = builder.raw_str("<td>");
                builder = builder.number(p.nav_order);
                builder = builder.raw_str("</td>");

                // Visible
                builder = builder.raw_str("<td>");
                if p.is_visible {
                    builder = builder.text("Yes");
                } else {
                    builder = builder.raw_str("<span class=\"badge\">Hidden</span>");
                }
                builder = builder.raw_str("</td>");

                // Actions
                builder = builder.raw_str("<td>");
                builder = builder.raw_str("<a href=\"render:/p/");
                builder = builder.text_string(&p.slug);
                builder = builder.raw_str("\">View</a> | ");
                builder = builder.raw_str("<a href=\"render:/p/admin/");
                builder = builder.number(p.id as u32);
                builder = builder.raw_str("/edit\">Edit</a>");
                builder = builder.raw_str("</td>");

                builder = builder.raw_str("</tr>\n");
            }

            builder = builder.raw_str("</table>\n");
        }

        builder.build()
    }

    /// Render create page form
    fn render_admin_new(env: &Env, viewer: Option<Address>) -> Bytes {
        let is_admin = match &viewer {
            Some(v) => Self::is_admin(env, v),
            None => false,
        };

        if !is_admin {
            return MarkdownBuilder::new(env)
                .warning("Only site admins can create pages.")
                .build();
        }

        let mut builder = MarkdownBuilder::new(env);

        // Back nav
        builder = builder.raw_str("<div class=\"back-nav\">");
        builder =
            builder.raw_str("<a href=\"render:/p/admin\" class=\"back-link\">← Back to Pages</a>");
        builder = builder.raw_str("</div>\n");
        builder = builder.newline();

        builder = builder.h1("Create New Page");

        // Form fields
        builder = builder.raw_str("<label>URL Slug (lowercase, no spaces):</label>\n");
        builder = builder.raw_str("<input type=\"text\" name=\"slug\" placeholder=\"about-us\" pattern=\"[a-z0-9\\-]+\" />\n");

        builder = builder.raw_str("<label>Page Name (for admin list):</label>\n");
        builder =
            builder.raw_str("<input type=\"text\" name=\"name\" placeholder=\"About Us\" />\n");

        builder = builder.raw_str("<label>Navigation Label (leave empty to use name):</label>\n");
        builder = builder.raw_str("<input type=\"text\" name=\"nav_label\" placeholder=\"\" />\n");

        builder = builder.raw_str("<label>Content (Markdown):</label>\n");
        builder = builder.textarea_markdown("content", 10, "Write your page content here...");

        builder = builder.raw_str("<label>Navigation Order (lower = appears first):</label>\n");
        builder = builder.raw_str(
            "<input type=\"number\" name=\"nav_order\" value=\"100\" min=\"0\" max=\"9999\" />\n",
        );

        // Visibility checkboxes
        builder =
            builder.raw_str("<input type=\"hidden\" name=\"is_visible\" value=\"false\" />\n");
        builder = builder.raw_str("<label><input type=\"checkbox\" name=\"is_visible\" value=\"true\" checked /> Visible</label>\n");

        builder =
            builder.raw_str("<input type=\"hidden\" name=\"show_in_nav\" value=\"false\" />\n");
        builder = builder.raw_str("<label><input type=\"checkbox\" name=\"show_in_nav\" value=\"true\" checked /> Show in Navigation</label>\n");

        // Hidden fields
        builder = builder.raw_str("<input type=\"hidden\" name=\"caller\" value=\"");
        builder = builder.text_string(&viewer.as_ref().unwrap().to_string());
        builder = builder.raw_str("\" />\n");
        builder =
            builder.raw_str("<input type=\"hidden\" name=\"_redirect\" value=\"/p/admin\" />\n");

        builder = builder.newline();
        builder = builder.form_link_to("Create Page", "pages", "create_page");
        builder = builder.text(" | ");
        builder = builder.raw_str("<a href=\"render:/p/admin\">Cancel</a>\n");

        builder.build()
    }

    /// Render edit page form
    fn render_admin_edit(env: &Env, page_id: u64, viewer: Option<Address>) -> Bytes {
        let is_admin = match &viewer {
            Some(v) => Self::is_admin(env, v),
            None => false,
        };

        if !is_admin {
            return MarkdownBuilder::new(env)
                .warning("Only site admins can edit pages.")
                .build();
        }

        let page_opt = Self::get_page(env.clone(), page_id);
        let page = match page_opt {
            Some(p) => p,
            None => {
                return MarkdownBuilder::new(env)
                    .h1("Page Not Found")
                    .paragraph("The requested page does not exist.")
                    .build();
            }
        };

        // Get current content
        let content = Self::get_page_content(env.clone(), page_id);
        let content_len = content.len() as usize;
        let mut content_buf = [0u8; 16384];
        let content_copy_len = core::cmp::min(content_len, 16384);
        content.copy_into_slice(&mut content_buf[..content_copy_len]);
        let content_str = core::str::from_utf8(&content_buf[..content_copy_len]).unwrap_or("");

        let mut builder = MarkdownBuilder::new(env);

        // Back nav
        builder = builder.raw_str("<div class=\"back-nav\">");
        builder =
            builder.raw_str("<a href=\"render:/p/admin\" class=\"back-link\">← Back to Pages</a>");
        builder = builder.raw_str("</div>\n");
        builder = builder.newline();

        builder = builder.h1("Edit Page");

        // Show slug (readonly)
        builder = builder.raw_str("<label>URL Slug:</label>\n");
        builder = builder.raw_str("<code>/p/");
        builder = builder.text_string(&page.slug);
        builder = builder.raw_str("</code>\n");
        builder = builder.newline();

        // Metadata form
        builder = builder.h2("Page Settings");
        builder = builder.raw_str("<input type=\"hidden\" name=\"page_id\" value=\"");
        builder = builder.number(page_id as u32);
        builder = builder.raw_str("\" />\n");

        builder = builder.raw_str("<label>Page Name:</label>\n");
        builder = builder.raw_str("<input type=\"text\" name=\"name\" value=\"");
        builder = builder.text_string(&page.name);
        builder = builder.raw_str("\" />\n");

        builder = builder.raw_str("<label>Navigation Label:</label>\n");
        builder = builder.raw_str("<input type=\"text\" name=\"nav_label\" value=\"");
        builder = builder.text_string(&page.nav_label);
        builder = builder.raw_str("\" />\n");

        builder = builder.raw_str("<label>Navigation Order:</label>\n");
        builder = builder.raw_str("<input type=\"number\" name=\"nav_order\" value=\"");
        builder = builder.number(page.nav_order);
        builder = builder.raw_str("\" min=\"0\" max=\"9999\" />\n");

        // Visibility checkboxes
        builder =
            builder.raw_str("<input type=\"hidden\" name=\"is_visible\" value=\"false\" />\n");
        builder =
            builder.raw_str("<label><input type=\"checkbox\" name=\"is_visible\" value=\"true\"");
        if page.is_visible {
            builder = builder.raw_str(" checked");
        }
        builder = builder.raw_str(" /> Visible</label>\n");

        builder =
            builder.raw_str("<input type=\"hidden\" name=\"show_in_nav\" value=\"false\" />\n");
        builder =
            builder.raw_str("<label><input type=\"checkbox\" name=\"show_in_nav\" value=\"true\"");
        if page.show_in_nav {
            builder = builder.raw_str(" checked");
        }
        builder = builder.raw_str(" /> Show in Navigation</label>\n");

        builder = builder.raw_str("<input type=\"hidden\" name=\"caller\" value=\"");
        builder = builder.text_string(&viewer.as_ref().unwrap().to_string());
        builder = builder.raw_str("\" />\n");
        builder = builder.raw_str("<input type=\"hidden\" name=\"_redirect\" value=\"/p/admin/");
        builder = builder.number(page_id as u32);
        builder = builder.raw_str("/edit\" />\n");

        builder = builder.newline();
        builder = builder.form_link_to("Save Settings", "pages", "update_page");

        // Content form (separate)
        builder = builder.newline();
        builder = builder.hr();
        builder = builder.h2("Page Content");

        builder = builder.raw_str("<input type=\"hidden\" name=\"page_id\" value=\"");
        builder = builder.number(page_id as u32);
        builder = builder.raw_str("\" />\n");

        builder = builder.raw_str("<label>Content (Markdown):</label>\n");
        builder =
            builder.raw_str("<textarea class=\"markdown-editor\" name=\"content\" rows=\"15\">");
        builder = builder.text(content_str);
        builder = builder.raw_str("</textarea>\n");

        builder = builder.raw_str("<input type=\"hidden\" name=\"caller\" value=\"");
        builder = builder.text_string(&viewer.as_ref().unwrap().to_string());
        builder = builder.raw_str("\" />\n");
        builder = builder.raw_str("<input type=\"hidden\" name=\"_redirect\" value=\"/p/admin/");
        builder = builder.number(page_id as u32);
        builder = builder.raw_str("/edit\" />\n");

        builder = builder.newline();
        builder = builder.form_link_to("Save Content", "pages", "update_page_content");

        // Danger zone
        builder = builder.newline();
        builder = builder.hr();
        builder = builder.h2("Danger Zone");
        builder = builder.warning("Deleting a page is permanent and cannot be undone.");
        builder = builder.newline();

        builder = builder.raw_str("<input type=\"hidden\" name=\"page_id\" value=\"");
        builder = builder.number(page_id as u32);
        builder = builder.raw_str("\" />\n");
        builder = builder.raw_str("<input type=\"hidden\" name=\"caller\" value=\"");
        builder = builder.text_string(&viewer.as_ref().unwrap().to_string());
        builder = builder.raw_str("\" />\n");
        builder =
            builder.raw_str("<input type=\"hidden\" name=\"_redirect\" value=\"/p/admin\" />\n");

        builder = builder.newline();
        builder = builder.form_link_to("Delete Page", "pages", "delete_page");

        builder.build()
    }

    /// Validate slug format
    fn validate_slug(env: &Env, slug: &String) {
        let len = slug.len() as usize;
        if !(1..=50).contains(&len) {
            panic!("Slug must be 1-50 characters");
        }

        let mut buf = [0u8; 50];
        let copy_len = core::cmp::min(len, 50);
        slug.copy_into_slice(&mut buf[..copy_len]);

        // Reserved slugs
        if &buf[..copy_len] == b"admin" {
            panic!("Slug 'admin' is reserved");
        }

        // Must start with letter
        let first = buf[0];
        if !(first.is_ascii_lowercase() || first.is_ascii_uppercase()) {
            panic!("Slug must start with a letter");
        }

        // All characters must be alphanumeric or hyphen
        for &c in buf.iter().take(copy_len) {
            let valid = c.is_ascii_lowercase()
                || c.is_ascii_uppercase()
                || c.is_ascii_digit()
                || c == b'-';
            if !valid {
                panic!("Slug can only contain letters, numbers, and hyphens");
            }
        }

        let _ = env;
    }

    /// Convert string to lowercase
    fn to_lowercase(env: &Env, s: &String) -> String {
        let len = s.len() as usize;
        if len == 0 {
            return s.clone();
        }

        let mut buf = [0u8; 256];
        let copy_len = core::cmp::min(len, 256);
        s.copy_into_slice(&mut buf[..copy_len]);

        for c in buf.iter_mut().take(copy_len) {
            if c.is_ascii_uppercase() {
                *c = *c - b'A' + b'a';
            }
        }

        String::from_str(env, core::str::from_utf8(&buf[..copy_len]).unwrap())
    }

    /// Get or create a unique chonk symbol for page content
    fn get_or_create_page_chonk(env: &Env, page_id: u64) -> Symbol {
        let key = PagesKey::PageContentChonk(page_id);
        if let Some(symbol) = env.storage().persistent().get(&key) {
            symbol
        } else {
            let symbol = Self::next_chonk_symbol(env);
            env.storage().persistent().set(&key, &symbol);
            symbol
        }
    }

    /// Get chonk symbol for page content (returns None if not created)
    fn get_page_chonk(env: &Env, page_id: u64) -> Option<Symbol> {
        env.storage()
            .persistent()
            .get(&PagesKey::PageContentChonk(page_id))
    }

    /// Generate the next unique chonk symbol
    fn next_chonk_symbol(env: &Env) -> Symbol {
        let counter: u64 = env
            .storage()
            .instance()
            .get(&PagesKey::NextChonkId)
            .unwrap_or(0);

        env.storage()
            .instance()
            .set(&PagesKey::NextChonkId, &(counter + 1));

        Self::counter_to_symbol(env, counter)
    }

    /// Convert a counter to a short symbol
    fn counter_to_symbol(env: &Env, counter: u64) -> Symbol {
        let mut buf = [0u8; 9];
        buf[0] = b'p'; // Use 'p' for pages

        let mut n = counter;
        let mut pos = 8usize;

        if n == 0 {
            buf[1] = b'0';
            return Symbol::new(env, core::str::from_utf8(&buf[0..2]).unwrap());
        }

        while n > 0 && pos > 0 {
            let digit = (n % 36) as u8;
            buf[pos] = if digit < 10 {
                b'0' + digit
            } else {
                b'a' + digit - 10
            };
            n /= 36;
            pos -= 1;
        }

        let start = pos + 1;
        buf[start - 1] = b'p';
        Symbol::new(env, core::str::from_utf8(&buf[start - 1..9]).unwrap())
    }

    /// Upgrade the contract WASM
    pub fn upgrade(env: Env, new_wasm_hash: BytesN<32>) {
        let registry: Address = env
            .storage()
            .instance()
            .get(&PagesKey::Registry)
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

    /// Helper to setup a boards-pages contract
    fn setup_pages(env: &Env) -> (BoardsPagesClient, Address) {
        env.mock_all_auths();

        let contract_id = env.register(BoardsPages, ());
        let client = BoardsPagesClient::new(env, &contract_id);

        let registry = Address::generate(env);
        client.init(&registry);

        (client, registry)
    }

    #[test]
    fn test_init() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsPages, ());
        let client = BoardsPagesClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        client.init(&registry);

        // Should have help page seeded
        let pages = client.list_pages();
        assert_eq!(pages.len(), 1);

        let help_page = pages.get(0).unwrap();
        assert_eq!(help_page.slug, String::from_str(&env, "help"));
        assert!(help_page.is_visible);
        assert!(help_page.show_in_nav);
    }

    #[test]
    #[should_panic(expected = "Already initialized")]
    fn test_double_init() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsPages, ());
        let client = BoardsPagesClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        client.init(&registry);
        // Second init should panic
        client.init(&registry);
    }

    #[test]
    fn test_get_page_by_slug() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsPages, ());
        let client = BoardsPagesClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        client.init(&registry);

        // Get help page by slug
        let slug = String::from_str(&env, "help");
        let page = client.get_page_by_slug(&slug).unwrap();
        assert_eq!(page.id, 0);

        // Case insensitive
        let upper_slug = String::from_str(&env, "HELP");
        let page2 = client.get_page_by_slug(&upper_slug).unwrap();
        assert_eq!(page2.id, 0);
    }

    #[test]
    fn test_get_page_by_slug_not_found() {
        let env = Env::default();
        let (client, _) = setup_pages(&env);

        let slug = String::from_str(&env, "nonexistent");
        assert!(client.get_page_by_slug(&slug).is_none());
    }

    #[test]
    fn test_get_nav_pages() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsPages, ());
        let client = BoardsPagesClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        client.init(&registry);

        // Should return help page in nav
        let nav_pages = client.get_nav_pages();
        assert_eq!(nav_pages.len(), 1);
        assert_eq!(
            nav_pages.get(0).unwrap().slug,
            String::from_str(&env, "help")
        );
    }

    #[test]
    fn test_get_page_content() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsPages, ());
        let client = BoardsPagesClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        client.init(&registry);

        // Help page should have content
        let content = client.get_page_content(&0);
        assert!(content.len() > 0);
    }

    #[test]
    fn test_get_page_by_id() {
        let env = Env::default();
        let (client, _) = setup_pages(&env);

        // Get help page by ID
        let page = client.get_page(&0);
        assert!(page.is_some());
        let page = page.unwrap();
        assert_eq!(page.slug, String::from_str(&env, "help"));
    }

    #[test]
    fn test_get_page_by_id_not_found() {
        let env = Env::default();
        let (client, _) = setup_pages(&env);

        // Nonexistent page ID
        assert!(client.get_page(&999).is_none());
    }

    // Note: create_page, update_page, delete_page require cross-contract calls
    // to registry.is_admin() which need fully initialized dependency contracts.
    // These tests are skipped as they would require a full integration test setup.

    #[test]
    fn test_render_help_page() {
        let env = Env::default();
        let (client, _) = setup_pages(&env);

        let path = String::from_str(&env, "/help");
        let html = client.render(&path, &None);
        assert!(html.len() > 0);
    }

    #[test]
    fn test_render_nonexistent_page() {
        let env = Env::default();
        let (client, _) = setup_pages(&env);

        let path = String::from_str(&env, "/nonexistent");
        let html = client.render(&path, &None);
        // Should still return something (error message or redirect)
        assert!(html.len() > 0);
    }

    // Note: update_page, update_page_content, delete_page
    // require cross-contract calls to registry.is_admin()
    // which need fully initialized dependency contracts to test.
}
