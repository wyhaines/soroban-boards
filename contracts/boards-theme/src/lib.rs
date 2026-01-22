#![no_std]

//! boards-theme: Styling contract for Soroban Boards
//!
//! This contract provides CSS/styling ONLY. It has no routing or rendering logic.
//! All rendering and routing is handled by boards-main.
//!
//! Exports:
//! - styles() / render_styles() - CSS stylesheet
//! - init() - initialization
//! - upgrade() - contract upgrade
//! - get_* functions - contract address getters

use soroban_render_sdk::prelude::*;
use soroban_sdk::{
    contract, contractimpl, contracttype, Address, Bytes, BytesN, Env, IntoVal, String, Symbol, Vec,
};

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
    /// Admin contract address
    Admin,
    /// Config contract address (for custom CSS)
    Config,
}

#[contract]
pub struct BoardsTheme;

#[contractimpl]
impl BoardsTheme {
    /// Initialize the theme contract
    pub fn init(
        env: Env,
        registry: Address,
        permissions: Address,
        content: Address,
        admin: Address,
        config: Address,
    ) {
        if env.storage().instance().has(&ThemeKey::Registry) {
            panic!("Already initialized");
        }
        env.storage().instance().set(&ThemeKey::Registry, &registry);
        env.storage()
            .instance()
            .set(&ThemeKey::Permissions, &permissions);
        env.storage().instance().set(&ThemeKey::Content, &content);
        env.storage().instance().set(&ThemeKey::Admin, &admin);
        env.storage().instance().set(&ThemeKey::Config, &config);
    }

    /// Get config contract address
    pub fn get_config(env: Env) -> Option<Address> {
        env.storage().instance().get(&ThemeKey::Config)
    }

    /// Set config contract address (for upgrades - requires registry admin auth)
    pub fn set_config(env: Env, config: Address, caller: Address) {
        caller.require_auth();

        // Verify caller is the registry (admin)
        let registry: Address = env
            .storage()
            .instance()
            .get(&ThemeKey::Registry)
            .expect("Not initialized");

        if caller != registry {
            panic!("Only registry can set config");
        }

        env.storage().instance().set(&ThemeKey::Config, &config);
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

    /// Public styles method for viewer/test access
    pub fn styles(env: Env) -> Bytes {
        Self::render_styles(env, None, None)
    }

    /// Base styles using Stellar Design System colors
    /// Named render_styles to follow the render_* convention for routable content
    /// Accepts path/viewer for consistency with render_* convention (unused here)
    pub fn render_styles(env: Env, _path: Option<String>, _viewer: Option<Address>) -> Bytes {
        let base_css = StyleBuilder::new(&env)
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
            .rule(".nav-bar a", "padding: var(--space-xs) var(--space-sm); background: var(--bg-muted); border-radius: 4px; font-size: 0.875rem; color: var(--text) !important; text-decoration: none !important;")
            .rule(".nav-bar a:hover", "background: var(--border); text-decoration: none !important; color: var(--primary) !important;")
            // Back navigation (community + home links) - use high specificity to override viewer styles
            .rule(".back-nav", "display: flex; flex-wrap: wrap; gap: var(--space-sm); align-items: center; margin-bottom: var(--space-md); font-size: 0.875rem;")
            .rule(".back-nav a.back-link", "background: none !important; padding: 0 !important; color: var(--text-muted) !important; text-decoration: none !important; transition: color 0.15s;")
            .rule(".back-nav a.back-link:hover", "background: none !important; color: var(--primary) !important; text-decoration: none !important;")
            .rule(".back-nav a.back-community", "color: var(--primary) !important; font-weight: 500;")
            .rule(".back-nav a.back-community:hover", "color: var(--primary-hover) !important;")
            .rule(".back-nav a.back-community::after", "content: '·'; margin-left: var(--space-sm); color: var(--text-muted); font-weight: normal;")
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
            // Profile integration
            .rule(".thread-meta", "font-size: 0.875rem; color: var(--text-muted); margin-bottom: var(--space-md);")
            .rule(".reply-header", "font-size: 0.8125rem; color: var(--text-muted); margin-bottom: var(--space-xs);")
            .rule(".reply-header a", "color: var(--primary); font-weight: 500;")
            .rule(".profile-compact", "display: inline;")
            .rule(".profile-compact a", "color: var(--primary); font-weight: 500;")
            .rule(".profile-anonymous", "font-family: monospace; font-size: 0.75rem;")
            .rule(".author", "font-family: monospace; font-size: 0.75rem;")
            // Action links row
            .rule(".actions", "display: flex; flex-wrap: wrap; gap: var(--space-sm); font-size: 0.875rem;")
            // Badge/Tag
            .rule(".badge", "display: inline-block; padding: 0.125rem 0.5rem; background: var(--bg-muted); border-radius: 9999px; font-size: 0.75rem;")
            .rule(".badge-pinned", "background: #ffeeba; color: #856404;")
            .rule(".badge-locked", "background: #f8d7da; color: #721c24;")
            .rule(".badge-private", "background: #e7d4ff; color: #5a3d7a;")
            .rule(".badge-hidden", "background: #ccc; color: #333;")
            .rule(".badge-readonly", "background: #d4edda; color: #155724;")
            .rule(".mod-actions", "margin: var(--space-sm) 0; padding: var(--space-sm); background: var(--bg-muted); border-radius: var(--radius-md); font-size: 0.875rem;")
            // Flair styles
            .rule(".flair", "display: inline-block; padding: 0.125rem 0.5rem; border-radius: 4px; font-size: 0.75rem; font-weight: 600; margin-right: var(--space-xs); vertical-align: middle;")
            .rule(".flair-selector", "margin-bottom: var(--space-md);")
            .rule(".flair-selector label", "display: block; margin-bottom: var(--space-xs); font-size: 0.875rem; color: var(--text-muted);")
            .rule(".flair-selector select", "padding: var(--space-xs) var(--space-sm); border: 1px solid var(--border); border-radius: 4px; font-size: 0.875rem; background: var(--bg); cursor: pointer; min-width: 150px;")
            .rule(".flair-selector select:focus", "outline: none; border-color: var(--primary);")
            .rule(".flair-selector .required", "color: var(--danger);")
            // User flair (badges)
            .rule(".user-flair", "display: inline-block; padding: 0.125rem 0.375rem; border-radius: 3px; font-size: 0.625rem; font-weight: 600; margin-left: var(--space-xs); vertical-align: middle;")
            // Crosspost styles
            .rule(".crosspost-header", "background: var(--bg-muted); padding: var(--space-sm) var(--space-md); border-radius: 6px; margin-bottom: var(--space-md); font-size: 0.875rem; border-left: 3px solid var(--primary);")
            .rule(".crosspost-badge", "display: inline-block; padding: 0.125rem 0.5rem; background: var(--primary); color: white; border-radius: 3px; font-size: 0.75rem; font-weight: 600; margin-right: var(--space-xs);")
            .rule(".crosspost-count", "color: var(--text-muted); font-size: 0.875rem; margin-left: var(--space-xs);")
            .rule(".crosspost-preview", "background: var(--bg-muted); padding: var(--space-md); border-radius: 6px; margin-bottom: var(--space-md);")
            // Board rules styles - green tip box
            .rule(".board-rules", "margin-bottom: var(--space-md); border-left: 4px solid #2da44e; border-radius: 6px; overflow: hidden; background: linear-gradient(to right, rgba(45, 164, 78, 0.08), rgba(45, 164, 78, 0.03)); padding: var(--space-sm);")
            .rule(".board-rules summary", "padding: var(--space-sm) var(--space-md); cursor: pointer; color: #1a7f37; font-weight: 500;")
            .rule(".board-rules summary:hover", "background: rgba(45, 164, 78, 0.08); border-radius: 4px;")
            .rule(".board-rules summary::before", "content: '📋 '; margin-right: var(--space-xs);")
            .rule(".rules-content", "padding: var(--space-xs) var(--space-md) var(--space-sm); white-space: pre-wrap; font-size: 0.875rem; line-height: 1.6; color: var(--text);")
            .rule(".rules-reminder", "margin-bottom: var(--space-md); border: 2px solid var(--warning); border-radius: 6px; overflow: hidden; background: rgba(255, 193, 7, 0.05);")
            .rule(".rules-reminder summary", "padding: var(--space-sm) var(--space-md); background: rgba(255, 193, 7, 0.1); cursor: pointer; color: var(--warning);")
            .rule(".rules-reminder summary:hover", "background: rgba(255, 193, 7, 0.15);")
            // Vote buttons and scoring
            .rule(".vote-buttons", "display: flex; align-items: center; gap: var(--space-sm); margin-bottom: var(--space-md);")
            .rule(".vote-up, .vote-down", "display: inline-flex; align-items: center; justify-content: center; width: 2rem; height: 2rem; border-radius: 4px; background: var(--bg-muted); color: var(--text-muted); text-decoration: none; font-size: 1rem; transition: all 0.15s;")
            .rule(".vote-up:hover", "background: #e8f5e9; color: #4caf50;")
            .rule(".vote-down:hover", "background: #ffebee; color: #f44336;")
            .rule(".vote-up.vote-active", "background: #4caf50; color: white;")
            .rule(".vote-down.vote-active", "background: #f44336; color: white;")
            .rule(".vote-disabled", "cursor: not-allowed; opacity: 0.5;")
            .rule(".vote-score", "font-weight: 600; font-size: 1.125rem; min-width: 2rem; text-align: center;")
            .rule(".vote-score-compact", "display: flex; align-items: center; justify-content: center; min-width: 2.5rem; padding: var(--space-xs); background: var(--primary); color: white; border-radius: 4px 0 0 4px; font-weight: 600; font-size: 0.875rem;")
            .rule(".vote-score-inline", "font-weight: 600; font-size: 0.75rem; min-width: 1.5rem; text-align: center;")
            // Thread card with vote score wrapper
            .rule(".thread-card-wrapper", "display: flex; gap: 0;")
            .rule(".thread-card-wrapper .thread-card", "flex: 1; border-radius: 0 6px 6px 0;")
            // Reply votes (inline)
            .rule(".reply-votes", "display: flex; align-items: center; gap: var(--space-xs); margin-top: var(--space-xs); font-size: 0.75rem;")
            .rule(".reply-votes .vote-up, .reply-votes .vote-down", "width: 1.25rem; height: 1.25rem; font-size: 0.625rem;")
            // Sort order selector
            .rule(".sort-selector", "display: flex; align-items: center; gap: var(--space-sm); margin-bottom: var(--space-md); padding: var(--space-xs) 0;")
            .rule(".sort-label", "color: var(--text-muted); font-size: 0.875rem;")
            .rule(".sort-option", "padding: var(--space-xs) var(--space-sm); background: var(--bg-muted); border-radius: 4px; color: var(--text-muted); text-decoration: none; font-size: 0.875rem; transition: all 0.15s;")
            .rule(".sort-option:hover", "background: var(--primary); color: white;")
            .rule(".sort-active", "background: var(--primary); color: white;")
            // Section spacing
            .rule(".section", "margin-bottom: var(--space-lg);")
            // Footer
            .rule(".footer", "margin-top: var(--space-xl); padding-top: var(--space-md); border-top: 1px solid var(--border); color: var(--text-muted); font-size: 0.875rem;")
            // Quick links (horizontal link row)
            .rule(".quick-links", "display: flex; flex-wrap: wrap; gap: var(--space-sm); margin-top: var(--space-sm);")
            .rule(".quick-links a", "padding: var(--space-xs) var(--space-sm); background: var(--bg-muted); border-radius: 4px; font-size: 0.875rem; color: var(--text) !important; text-decoration: none !important;")
            .rule(".quick-links a:hover", "background: var(--primary); color: white !important; text-decoration: none !important;")
            // Community styles - use a.community-card for higher specificity over viewer's a.soroban-action
            .rule(".community-list", "display: flex; flex-direction: column; gap: var(--space-sm);")
            .rule("a.community-card", "display: flex !important; flex-direction: column; align-items: flex-start !important; background: var(--bg) !important; color: var(--text) !important; border: 1px solid var(--border); border-radius: 6px; padding: var(--space-md) !important; transition: border-color 0.15s, box-shadow 0.15s; text-decoration: none !important;")
            .rule("a.community-card:hover", "border-color: var(--primary); box-shadow: 0 2px 8px rgba(120, 87, 225, 0.1); text-decoration: none !important; background: var(--bg) !important;")
            .rule(".community-card-title", "display: block; font-weight: 600; color: var(--text); margin-bottom: var(--space-xs); text-align: left;")
            .rule(".community-card-desc", "display: block; color: var(--text-muted); font-size: 0.9375rem; margin-bottom: var(--space-xs); text-align: left;")
            .rule(".community-card-meta", "display: block; font-size: 0.8125rem; color: var(--text-muted); text-align: left;")
            .rule(".community-card-meta .badge", "margin-left: var(--space-xs);")
            .rule(".community-header", "margin-bottom: var(--space-lg);")
            .rule(".community-header h1", "margin-bottom: var(--space-xs);")
            .rule(".community-header p", "color: var(--text-muted); margin: 0;")
            .rule(".community-actions", "display: flex; gap: var(--space-sm); margin-top: var(--space-md);")
            // Disabled action buttons (for threshold-restricted users)
            .rule(".action-disabled", "display: inline-block; color: var(--text-muted); cursor: not-allowed; opacity: 0.6; padding: var(--space-xs) var(--space-sm); font-size: 0.875rem;")
            .rule(".action-disabled:hover", "text-decoration: none;")
            // Admin bypass badge
            .rule(".badge-admin", "display: inline-block; background: var(--primary); color: white; padding: 0.125rem 0.375rem; border-radius: 4px; font-size: 0.6875rem; font-weight: 600; margin-left: var(--space-xs); vertical-align: middle;")
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
            // Board rules dark mode - green tip box
            .rule(".board-rules", "background: linear-gradient(to right, rgba(111, 221, 139, 0.1), rgba(111, 221, 139, 0.03));")
            .rule(".board-rules summary", "color: #6fdd8b;")
            .rule(".board-rules summary:hover", "background: rgba(111, 221, 139, 0.1);")
            .rule(".badge-pinned", "background: #3a3019; color: #ffd859;")
            .rule(".badge-locked", "background: #3a1c1c; color: #ff8080;")
            .rule(".badge-private", "background: #3a2d4a; color: #c9a5ff;")
            .rule(".badge-hidden", "background: #2a2a2a; color: #888;")
            .rule(".badge-readonly", "background: #1e3a28; color: #6fdd8b;")
            // Vote buttons dark mode
            .rule(".vote-up:hover", "background: #1e3a28; color: #6fdd8b;")
            .rule(".vote-down:hover", "background: #3a1c1c; color: #ff8080;")
            .rule(".vote-up.vote-active", "background: #2e7d32; color: white;")
            .rule(".vote-down.vote-active", "background: #c62828; color: white;")
            // Community cards dark mode
            .rule("a.community-card:hover", "box-shadow: 0 2px 8px rgba(120, 87, 225, 0.2);")
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
            .build();

        // Append custom CSS from config if available
        let custom_css = Self::get_custom_css_from_config(&env);
        if !custom_css.is_empty() {
            let mut result = base_css;
            result.append(&Bytes::from_slice(&env, b"\n/* Custom CSS */\n"));
            result.append(&custom_css);
            result
        } else {
            base_css
        }
    }

    /// Get custom CSS from config contract, with graceful fallback
    fn get_custom_css_from_config(env: &Env) -> Bytes {
        let config_opt: Option<Address> = env.storage().instance().get(&ThemeKey::Config);

        if let Some(config) = config_opt {
            let result = env.try_invoke_contract::<Bytes, soroban_sdk::Error>(
                &config,
                &Symbol::new(env, "render_custom_css"),
                Vec::from_array(
                    env,
                    [
                        Option::<String>::None.into_val(env),
                        Option::<Address>::None.into_val(env),
                    ],
                ),
            );
            if let Ok(Ok(css)) = result {
                return css;
            }
        }

        // No custom CSS
        Bytes::new(env)
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

    /// Helper to setup a boards-theme contract with all dependencies
    fn setup_theme(
        env: &Env,
    ) -> (
        BoardsThemeClient,
        Address,
        Address,
        Address,
        Address,
        Address,
    ) {
        env.mock_all_auths();

        let contract_id = env.register(BoardsTheme, ());
        let client = BoardsThemeClient::new(env, &contract_id);

        let registry = Address::generate(env);
        let permissions = Address::generate(env);
        let content = Address::generate(env);
        let admin = Address::generate(env);
        let config = Address::generate(env);

        client.init(&registry, &permissions, &content, &admin, &config);

        (client, registry, permissions, content, admin, config)
    }

    #[test]
    fn test_init() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsTheme, ());
        let client = BoardsThemeClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        let permissions = Address::generate(&env);
        let content = Address::generate(&env);
        let admin = Address::generate(&env);
        let config = Address::generate(&env);
        client.init(&registry, &permissions, &content, &admin, &config);

        assert_eq!(client.get_registry(), registry);
        assert_eq!(client.get_permissions(), permissions);
        assert_eq!(client.get_content(), content);
        assert_eq!(client.get_admin(), admin);
        assert_eq!(client.get_config(), Some(config));
    }

    #[test]
    #[should_panic(expected = "Already initialized")]
    fn test_double_init() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsTheme, ());
        let client = BoardsThemeClient::new(&env, &contract_id);

        let registry = Address::generate(&env);
        let permissions = Address::generate(&env);
        let content = Address::generate(&env);
        let admin = Address::generate(&env);
        let config = Address::generate(&env);

        client.init(&registry, &permissions, &content, &admin, &config);
        // Second init should panic
        client.init(&registry, &permissions, &content, &admin, &config);
    }

    #[test]
    fn test_styles() {
        let env = Env::default();
        let (client, _, _, _, _, config) = setup_theme(&env);

        let css = client.styles();
        assert!(css.len() > 0);
        assert_eq!(client.get_config(), Some(config));
    }

    #[test]
    fn test_render_styles() {
        let env = Env::default();
        let (client, _, _, _, _, _) = setup_theme(&env);

        // render_styles should return same as styles
        let css = client.render_styles(&None, &None);
        assert!(css.len() > 0);
    }

    #[test]
    fn test_get_registry() {
        let env = Env::default();
        let (client, registry, _, _, _, _) = setup_theme(&env);
        assert_eq!(client.get_registry(), registry);
    }

    #[test]
    fn test_get_permissions() {
        let env = Env::default();
        let (client, _, permissions, _, _, _) = setup_theme(&env);
        assert_eq!(client.get_permissions(), permissions);
    }

    #[test]
    fn test_get_content() {
        let env = Env::default();
        let (client, _, _, content, _, _) = setup_theme(&env);
        assert_eq!(client.get_content(), content);
    }

    #[test]
    fn test_get_admin() {
        let env = Env::default();
        let (client, _, _, _, admin, _) = setup_theme(&env);
        assert_eq!(client.get_admin(), admin);
    }

    #[test]
    fn test_get_config() {
        let env = Env::default();
        let (client, _, _, _, _, config) = setup_theme(&env);
        assert_eq!(client.get_config(), Some(config));
    }

    #[test]
    fn test_styles_contains_css() {
        let env = Env::default();
        let (client, _, _, _, _, _) = setup_theme(&env);

        let css = client.styles();
        // CSS should be reasonably sized (more than just a few bytes)
        assert!(css.len() > 100);
    }

    // Note: set_config requires caller to be registry which needs
    // a cross-contract call to verify. Skipping as it requires
    // full integration setup.
}
