#![no_std]

//! boards-config: Centralized configuration for Soroban Boards deployments
//!
//! This contract stores:
//! - Creation thresholds for boards and communities (karma, age, limits)
//! - Branding settings (site name, tagline, logo)
//! - Operational settings (reply depth, edit windows, name limits)
//! - Custom CSS overrides
//!
//! Render functions are provided for use with soroban-render's include system:
//! `{{include contract=CONFIG_ID func="site_name"}}`

use soroban_chonk::prelude::*;
use soroban_render_sdk::prelude::*;
use soroban_sdk::{
    contract, contractimpl, contracttype, Address, Bytes, BytesN, Env, IntoVal, String, Symbol,
    Val, Vec,
};

soroban_render!(markdown);

// =============================================================================
// Storage Keys
// =============================================================================

#[contracttype]
#[derive(Clone)]
pub enum ConfigKey {
    /// Registry contract address (for admin verification)
    Registry,

    // === Creation Thresholds ===
    /// Board creation requirements
    BoardThresholds,
    /// Community creation requirements
    CommunityThresholds,

    // === Branding ===
    /// Instance branding settings
    Branding,
    /// Custom CSS overrides (stored as Bytes)
    CustomCss,

    // === Operational Settings ===
    /// Maximum reply depth for threads
    MaxReplyDepth,
    /// Chunk size for paginated replies
    ReplyChunkSize,
    /// Default edit window in seconds
    DefaultEditWindow,
    /// Thread body max bytes
    ThreadBodyMaxBytes,
    /// Board name min/max length
    BoardNameLimits,
    /// Community name min/max length
    CommunityNameLimits,

    // === Voting Defaults ===
    /// Default voting config for new boards
    DefaultVotingConfig,

    // === Chonk Storage ===
    /// Footer text stored in chonk for unlimited size
    FooterTextChonk,
    /// Tagline stored in chonk for unlimited size
    TaglineChonk,
}

// =============================================================================
// Data Structures
// =============================================================================

/// Creation requirements for boards or communities
#[contracttype]
#[derive(Clone)]
pub struct CreationThresholds {
    /// Minimum karma required to create (0 = no requirement)
    pub min_karma: i64,
    /// Minimum account age in seconds (0 = no requirement)
    pub min_account_age_secs: u64,
    /// Minimum post count (0 = no requirement)
    pub min_post_count: u32,
    /// Whether a user profile is required
    pub require_profile: bool,
    /// Maximum creations per user (0 = unlimited)
    pub per_user_limit: u32,
    /// XLM value to lock in stroops (0 = no lock required)
    pub xlm_lock_stroops: i128,
}

impl CreationThresholds {
    /// Create permissive defaults (no restrictions)
    pub fn permissive() -> Self {
        CreationThresholds {
            min_karma: 0,
            min_account_age_secs: 0,
            min_post_count: 0,
            require_profile: false,
            per_user_limit: 0,
            xlm_lock_stroops: 0,
        }
    }
}

/// Instance branding configuration
#[contracttype]
#[derive(Clone)]
pub struct Branding {
    /// Instance name (e.g., "Soroban Boards")
    pub site_name: String,
    /// Short tagline
    pub tagline: String,
    /// URL to logo image
    pub logo_url: String,
    /// URL to favicon
    pub favicon_url: String,
    /// Footer text
    pub footer_text: String,
    /// Primary color (CSS color value)
    pub primary_color: String,
}

/// Name length limits
#[contracttype]
#[derive(Clone)]
pub struct NameLimits {
    pub min_length: u32,
    pub max_length: u32,
}

/// Result of threshold check
#[contracttype]
#[derive(Clone)]
pub struct ThresholdResult {
    /// Whether the user passed all threshold checks
    pub passed: bool,
    /// Reason for failure (empty if passed)
    pub reason: String,
}

/// Type of creation being checked
#[contracttype]
#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum CreationType {
    Board = 0,
    Community = 1,
}

/// Default voting configuration for new boards
#[contracttype]
#[derive(Clone)]
pub struct DefaultVotingConfig {
    pub enabled: bool,
    pub allow_downvotes: bool,
    pub karma_enabled: bool,
    pub karma_multiplier: u32,
}

impl DefaultVotingConfig {
    pub fn default_config() -> Self {
        DefaultVotingConfig {
            enabled: true,
            allow_downvotes: true,
            karma_enabled: true,
            karma_multiplier: 1,
        }
    }
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

// =============================================================================
// Contract Implementation
// =============================================================================

#[contract]
pub struct BoardsConfig;

#[contractimpl]
impl BoardsConfig {
    // =========================================================================
    // Initialization
    // =========================================================================

    /// Initialize the config contract with registry address
    pub fn init(env: Env, registry: Address) {
        if env.storage().instance().has(&ConfigKey::Registry) {
            panic!("Already initialized");
        }

        env.storage().instance().set(&ConfigKey::Registry, &registry);

        // Set all default values
        Self::set_default_values(&env);
    }

    fn set_default_values(env: &Env) {
        // Board thresholds - initially permissive
        env.storage()
            .instance()
            .set(&ConfigKey::BoardThresholds, &CreationThresholds::permissive());

        // Community thresholds - initially permissive
        env.storage()
            .instance()
            .set(&ConfigKey::CommunityThresholds, &CreationThresholds::permissive());

        // Branding
        let branding = Branding {
            site_name: String::from_str(env, "Soroban Boards"),
            tagline: String::from_str(env, "Decentralized discussion forums on Stellar"),
            logo_url: String::from_str(env, ""),
            favicon_url: String::from_str(env, ""),
            footer_text: String::from_str(env, "Powered by Soroban Render on Stellar"),
            primary_color: String::from_str(env, "#7857e1"),
        };
        env.storage().instance().set(&ConfigKey::Branding, &branding);

        // Operational settings
        env.storage().instance().set(&ConfigKey::MaxReplyDepth, &10u32);
        env.storage().instance().set(&ConfigKey::ReplyChunkSize, &6u32);
        env.storage().instance().set(&ConfigKey::DefaultEditWindow, &86400u64);
        env.storage().instance().set(&ConfigKey::ThreadBodyMaxBytes, &16384u32);
        env.storage().instance().set(
            &ConfigKey::BoardNameLimits,
            &NameLimits {
                min_length: 3,
                max_length: 50,
            },
        );
        env.storage().instance().set(
            &ConfigKey::CommunityNameLimits,
            &NameLimits {
                min_length: 3,
                max_length: 30,
            },
        );

        // Voting defaults
        env.storage()
            .instance()
            .set(&ConfigKey::DefaultVotingConfig, &DefaultVotingConfig::default_config());
    }

    // =========================================================================
    // Admin Verification
    // =========================================================================

    /// Verify caller is a registry admin
    fn require_admin(env: &Env, caller: &Address) {
        caller.require_auth();

        let registry: Address = env
            .storage()
            .instance()
            .get(&ConfigKey::Registry)
            .expect("Not initialized");

        // Call registry.is_admin(caller)
        let args: Vec<Val> = Vec::from_array(env, [caller.clone().into_val(env)]);
        let is_admin: bool = env.invoke_contract(&registry, &Symbol::new(env, "is_admin"), args);

        if !is_admin {
            panic!("Caller is not a registry admin");
        }
    }

    // =========================================================================
    // Registry Getter
    // =========================================================================

    pub fn get_registry(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&ConfigKey::Registry)
            .expect("Not initialized")
    }

    // =========================================================================
    // Creation Thresholds
    // =========================================================================

    /// Get board creation thresholds
    pub fn get_board_thresholds(env: Env) -> CreationThresholds {
        env.storage()
            .instance()
            .get(&ConfigKey::BoardThresholds)
            .unwrap_or_else(|| CreationThresholds::permissive())
    }

    /// Set board creation thresholds (admin only)
    pub fn set_board_thresholds(env: Env, thresholds: CreationThresholds, caller: Address) {
        Self::require_admin(&env, &caller);
        env.storage()
            .instance()
            .set(&ConfigKey::BoardThresholds, &thresholds);
    }

    /// Get community creation thresholds
    pub fn get_community_thresholds(env: Env) -> CreationThresholds {
        env.storage()
            .instance()
            .get(&ConfigKey::CommunityThresholds)
            .unwrap_or_else(|| CreationThresholds::permissive())
    }

    /// Set community creation thresholds (admin only)
    pub fn set_community_thresholds(env: Env, thresholds: CreationThresholds, caller: Address) {
        Self::require_admin(&env, &caller);
        env.storage()
            .instance()
            .set(&ConfigKey::CommunityThresholds, &thresholds);
    }

    // =========================================================================
    // Branding
    // =========================================================================

    /// Get branding configuration
    pub fn get_branding(env: Env) -> Branding {
        env.storage().instance().get(&ConfigKey::Branding).unwrap_or_else(|| Branding {
            site_name: String::from_str(&env, "Soroban Boards"),
            tagline: String::from_str(&env, "Decentralized discussion forums on Stellar"),
            logo_url: String::from_str(&env, ""),
            favicon_url: String::from_str(&env, ""),
            footer_text: String::from_str(&env, "Powered by Soroban Render on Stellar"),
            primary_color: String::from_str(&env, "#7857e1"),
        })
    }

    /// Set branding configuration (admin only)
    pub fn set_branding(env: Env, branding: Branding, caller: Address) {
        Self::require_admin(&env, &caller);

        // Store footer_text in chonk for unlimited size rendering
        let footer_bytes = Self::string_to_bytes_chunked(&env, &branding.footer_text);
        let footer_chonk = Chonk::open(&env, Symbol::new(&env, "footer"));
        footer_chonk.clear();
        footer_chonk.write_chunked(footer_bytes, 1024);

        // Store tagline in chonk for unlimited size rendering
        let tagline_bytes = Self::string_to_bytes_chunked(&env, &branding.tagline);
        let tagline_chonk = Chonk::open(&env, Symbol::new(&env, "tagline"));
        tagline_chonk.clear();
        tagline_chonk.write_chunked(tagline_bytes, 1024);

        env.storage().instance().set(&ConfigKey::Branding, &branding);
    }

    /// Get custom CSS overrides
    pub fn get_custom_css(env: Env) -> Bytes {
        env.storage()
            .persistent()
            .get(&ConfigKey::CustomCss)
            .unwrap_or_else(|| Bytes::new(&env))
    }

    /// Set custom CSS overrides (admin only)
    pub fn set_custom_css(env: Env, css: Bytes, caller: Address) {
        Self::require_admin(&env, &caller);
        env.storage().persistent().set(&ConfigKey::CustomCss, &css);
    }

    // =========================================================================
    // Operational Settings
    // =========================================================================

    /// Get maximum reply depth
    pub fn get_max_reply_depth(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&ConfigKey::MaxReplyDepth)
            .unwrap_or(10)
    }

    /// Set maximum reply depth (admin only)
    pub fn set_max_reply_depth(env: Env, depth: u32, caller: Address) {
        Self::require_admin(&env, &caller);
        env.storage().instance().set(&ConfigKey::MaxReplyDepth, &depth);
    }

    /// Get reply chunk size for pagination
    pub fn get_reply_chunk_size(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&ConfigKey::ReplyChunkSize)
            .unwrap_or(6)
    }

    /// Set reply chunk size (admin only)
    pub fn set_reply_chunk_size(env: Env, size: u32, caller: Address) {
        Self::require_admin(&env, &caller);
        env.storage().instance().set(&ConfigKey::ReplyChunkSize, &size);
    }

    /// Get default edit window in seconds
    pub fn get_default_edit_window(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&ConfigKey::DefaultEditWindow)
            .unwrap_or(86400)
    }

    /// Set default edit window (admin only)
    pub fn set_default_edit_window(env: Env, seconds: u64, caller: Address) {
        Self::require_admin(&env, &caller);
        env.storage()
            .instance()
            .set(&ConfigKey::DefaultEditWindow, &seconds);
    }

    /// Get thread body max bytes
    pub fn get_thread_body_max_bytes(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&ConfigKey::ThreadBodyMaxBytes)
            .unwrap_or(16384)
    }

    /// Set thread body max bytes (admin only)
    pub fn set_thread_body_max_bytes(env: Env, max_bytes: u32, caller: Address) {
        Self::require_admin(&env, &caller);
        env.storage()
            .instance()
            .set(&ConfigKey::ThreadBodyMaxBytes, &max_bytes);
    }

    /// Get board name length limits
    pub fn get_board_name_limits(env: Env) -> NameLimits {
        env.storage()
            .instance()
            .get(&ConfigKey::BoardNameLimits)
            .unwrap_or(NameLimits {
                min_length: 3,
                max_length: 50,
            })
    }

    /// Set board name length limits (admin only)
    pub fn set_board_name_limits(env: Env, limits: NameLimits, caller: Address) {
        Self::require_admin(&env, &caller);
        env.storage()
            .instance()
            .set(&ConfigKey::BoardNameLimits, &limits);
    }

    /// Get community name length limits
    pub fn get_community_name_limits(env: Env) -> NameLimits {
        env.storage()
            .instance()
            .get(&ConfigKey::CommunityNameLimits)
            .unwrap_or(NameLimits {
                min_length: 3,
                max_length: 30,
            })
    }

    /// Set community name length limits (admin only)
    pub fn set_community_name_limits(env: Env, limits: NameLimits, caller: Address) {
        Self::require_admin(&env, &caller);
        env.storage()
            .instance()
            .set(&ConfigKey::CommunityNameLimits, &limits);
    }

    // =========================================================================
    // Voting Defaults
    // =========================================================================

    /// Get default voting config for new boards
    pub fn get_default_voting_config(env: Env) -> DefaultVotingConfig {
        env.storage()
            .instance()
            .get(&ConfigKey::DefaultVotingConfig)
            .unwrap_or_else(|| DefaultVotingConfig::default_config())
    }

    /// Set default voting config (admin only)
    pub fn set_default_voting_config(env: Env, config: DefaultVotingConfig, caller: Address) {
        Self::require_admin(&env, &caller);
        env.storage()
            .instance()
            .set(&ConfigKey::DefaultVotingConfig, &config);
    }

    // =========================================================================
    // Threshold Checking
    // =========================================================================

    /// Check if a user meets the creation thresholds
    ///
    /// Parameters:
    /// - `creation_type`: Whether checking for board or community creation
    /// - `user`: The address attempting to create
    /// - `user_creation_count`: How many boards/communities this user has already created
    /// - `user_karma`: The user's total karma (0 if karma not tracked)
    /// - `user_account_age_secs`: The user's account age in seconds (0 if not tracked)
    /// - `user_post_count`: The user's total post count (0 if not tracked)
    /// - `has_profile`: Whether the user has set up a profile
    ///
    /// Returns ThresholdResult with passed=true if all checks pass, or passed=false with reason
    pub fn check_thresholds(
        env: Env,
        creation_type: CreationType,
        _user: Address,
        user_creation_count: u32,
        user_karma: i64,
        user_account_age_secs: u64,
        user_post_count: u32,
        has_profile: bool,
    ) -> ThresholdResult {
        // Get appropriate thresholds
        let thresholds = if creation_type == CreationType::Board {
            Self::get_board_thresholds(env.clone())
        } else {
            Self::get_community_thresholds(env.clone())
        };

        // Check karma requirement
        if thresholds.min_karma > 0 && user_karma < thresholds.min_karma {
            return ThresholdResult {
                passed: false,
                reason: String::from_str(&env, "Insufficient karma"),
            };
        }

        // Check account age requirement
        if thresholds.min_account_age_secs > 0 && user_account_age_secs < thresholds.min_account_age_secs {
            return ThresholdResult {
                passed: false,
                reason: String::from_str(&env, "Account too new"),
            };
        }

        // Check post count requirement
        if thresholds.min_post_count > 0 && user_post_count < thresholds.min_post_count {
            return ThresholdResult {
                passed: false,
                reason: String::from_str(&env, "Insufficient post count"),
            };
        }

        // Check profile requirement
        if thresholds.require_profile && !has_profile {
            return ThresholdResult {
                passed: false,
                reason: String::from_str(&env, "Profile required"),
            };
        }

        // Check per-user limit
        if thresholds.per_user_limit > 0 && user_creation_count >= thresholds.per_user_limit {
            return ThresholdResult {
                passed: false,
                reason: String::from_str(&env, "Creation limit reached"),
            };
        }

        // Note: XLM lock checking is handled separately by the calling contract
        // since it involves token transfers

        // All checks passed
        ThresholdResult {
            passed: true,
            reason: String::from_str(&env, ""),
        }
    }

    /// Get the XLM lock requirement for a creation type (convenience method)
    pub fn get_xlm_lock_requirement(env: Env, creation_type: CreationType) -> i128 {
        let thresholds = if creation_type == CreationType::Board {
            Self::get_board_thresholds(env)
        } else {
            Self::get_community_thresholds(env)
        };
        thresholds.xlm_lock_stroops
    }

    // =========================================================================
    // Render Functions (for includes)
    // =========================================================================

    /// Render site name - for includes: {{include contract=CONFIG func="site_name"}}
    pub fn render_site_name(
        env: Env,
        _path: Option<String>,
        _viewer: Option<Address>,
    ) -> Bytes {
        let branding = Self::get_branding(env.clone());
        Self::string_to_bytes(&env, &branding.site_name)
    }

    /// Render tagline - for includes: {{include contract=CONFIG func="tagline"}}
    pub fn render_tagline(
        env: Env,
        _path: Option<String>,
        _viewer: Option<Address>,
    ) -> Bytes {
        let branding = Self::get_branding(env.clone());
        Self::string_to_bytes(&env, &branding.tagline)
    }

    /// Render logo HTML - for includes: {{include contract=CONFIG func="logo"}}
    pub fn render_logo(env: Env, _path: Option<String>, _viewer: Option<Address>) -> Bytes {
        let branding = Self::get_branding(env.clone());
        if branding.logo_url.len() == 0 {
            return Bytes::new(&env);
        }

        // Build: <img src="URL" alt="SITE_NAME" class="site-logo" />
        let mut result = Bytes::from_slice(&env, b"<img src=\"");
        result.append(&Self::string_to_bytes(&env, &branding.logo_url));
        result.append(&Bytes::from_slice(&env, b"\" alt=\""));
        result.append(&Self::string_to_bytes(&env, &branding.site_name));
        result.append(&Bytes::from_slice(&env, b"\" class=\"site-logo\" />"));
        result
    }

    /// Render footer text - for includes: {{include contract=CONFIG func="footer_text"}}
    /// Returns a continuation tag for progressive loading from chonk storage
    pub fn render_footer_text(
        env: Env,
        _path: Option<String>,
        _viewer: Option<Address>,
    ) -> Bytes {
        let footer_chonk = Chonk::open(&env, Symbol::new(&env, "footer"));
        let count = footer_chonk.count();

        if count > 0 {
            // Return continuation tag for progressive loading
            // {{continue collection="footer" from=0 total=N}}
            let mut result = Bytes::from_slice(&env, b"{{continue collection=\"footer\" from=0 total=");
            result.append(&Self::u32_to_bytes(&env, count));
            result.append(&Bytes::from_slice(&env, b"}}"));
            result
        } else {
            // Fallback to branding struct for backward compatibility
            // Use chunked conversion to handle any length
            let branding = Self::get_branding(env.clone());
            Self::string_to_bytes_chunked(&env, &branding.footer_text)
        }
    }

    /// Get a chunk from a collection (for progressive loading)
    /// Called by the viewer's progressive loader
    pub fn get_chunk(env: Env, collection: Symbol, index: u32) -> Option<Bytes> {
        let chonk = Chonk::open(&env, collection);
        chonk.get(index)
    }

    /// Get chunk metadata for a collection
    /// Returns { count, total_bytes, version }
    pub fn get_chunk_meta(env: Env, collection: Symbol) -> ChunkMeta {
        let chonk = Chonk::open(&env, collection);
        ChunkMeta {
            count: chonk.count(),
            total_bytes: chonk.total_bytes(),
            version: 1,
        }
    }

    /// Render header (site name as h1 + tagline) - for includes: {{include contract=CONFIG func="header"}}
    pub fn render_header(
        env: Env,
        _path: Option<String>,
        _viewer: Option<Address>,
    ) -> Bytes {
        let branding = Self::get_branding(env.clone());

        // Build: "# site_name\n\ntagline\n\n"
        MarkdownBuilder::new(&env)
            .raw_str("# ")
            .text_string(&branding.site_name)
            .newline()
            .newline()
            .text_string(&branding.tagline)
            .newline()
            .newline()
            .build()
    }

    /// Render custom CSS - for theme to include
    pub fn render_custom_css(
        env: Env,
        _path: Option<String>,
        _viewer: Option<Address>,
    ) -> Bytes {
        Self::get_custom_css(env)
    }

    /// Render primary color value - for CSS variable injection
    pub fn render_primary_color(
        env: Env,
        _path: Option<String>,
        _viewer: Option<Address>,
    ) -> Bytes {
        let branding = Self::get_branding(env.clone());
        Self::string_to_bytes(&env, &branding.primary_color)
    }

    /// Render meta tags for document head - for includes: {{include contract=CONFIG func="meta"}}
    ///
    /// Outputs meta tags that the viewer will extract and apply to the document:
    /// - `<meta name="favicon" content="URL">` - sets the page favicon
    /// - `<meta name="title" content="...">` - sets the page title
    /// - `<meta name="theme-color" content="...">` - sets the browser theme color
    pub fn render_meta(
        env: Env,
        _path: Option<String>,
        _viewer: Option<Address>,
    ) -> Bytes {
        let branding = Self::get_branding(env.clone());
        let mut md = MarkdownBuilder::new(&env);

        // Add favicon meta if set
        if branding.favicon_url.len() > 0 {
            md = md
                .raw_str("<meta name=\"favicon\" content=\"")
                .text_string(&branding.favicon_url)
                .raw_str("\" />\n");
        }

        // Add title meta (site name)
        if branding.site_name.len() > 0 {
            md = md
                .raw_str("<meta name=\"title\" content=\"")
                .text_string(&branding.site_name)
                .raw_str("\" />\n");
        }

        // Add theme-color meta (primary color)
        if branding.primary_color.len() > 0 {
            md = md
                .raw_str("<meta name=\"theme-color\" content=\"")
                .text_string(&branding.primary_color)
                .raw_str("\" />\n");
        }

        md.build()
    }

    // =========================================================================
    // Helpers
    // =========================================================================

    /// Convert u32 to Bytes (for building tags)
    fn u32_to_bytes(env: &Env, n: u32) -> Bytes {
        let mut buf = [0u8; 10]; // max digits for u32
        let mut i = 0;
        let mut num = n;

        if num == 0 {
            return Bytes::from_slice(env, b"0");
        }

        while num > 0 {
            buf[i] = b'0' + (num % 10) as u8;
            num /= 10;
            i += 1;
        }

        // Reverse the digits
        let mut result = [0u8; 10];
        for j in 0..i {
            result[j] = buf[i - 1 - j];
        }

        Bytes::from_slice(env, &result[..i])
    }

    /// Convert String to Bytes with a fixed buffer (for short strings)
    fn string_to_bytes(env: &Env, s: &String) -> Bytes {
        let len = s.len() as usize;
        if len == 0 {
            return Bytes::new(env);
        }
        // Use a buffer for shorter text content
        let mut buf = [0u8; 512];
        let copy_len = core::cmp::min(len, 512);
        s.copy_into_slice(&mut buf[..copy_len]);
        Bytes::from_slice(env, &buf[..copy_len])
    }

    /// Convert String to Bytes in chunks for unlimited size support
    /// Processes the string in 1024-byte chunks and assembles into a single Bytes
    fn string_to_bytes_chunked(env: &Env, s: &String) -> Bytes {
        let len = s.len() as usize;
        if len == 0 {
            return Bytes::new(env);
        }

        const CHUNK_SIZE: usize = 1024;
        let mut result = Bytes::new(env);
        let mut buf = [0u8; CHUNK_SIZE];
        let mut offset = 0;

        while offset < len {
            let remaining = len - offset;
            let chunk_len = core::cmp::min(remaining, CHUNK_SIZE);

            // Copy this chunk from string to buffer
            // Note: copy_into_slice copies the entire string, so we extract the slice we need
            if offset == 0 {
                // First chunk - copy and take what we need
                s.copy_into_slice(&mut buf[..chunk_len]);
            } else {
                // For subsequent chunks, we need to copy the full string and extract our portion
                // This is inefficient but necessary given Soroban's String API
                let mut full_buf = [0u8; 8192]; // Large enough for most strings
                let full_len = core::cmp::min(len, 8192);
                s.copy_into_slice(&mut full_buf[..full_len]);
                buf[..chunk_len].copy_from_slice(&full_buf[offset..offset + chunk_len]);
            }

            result.append(&Bytes::from_slice(env, &buf[..chunk_len]));
            offset += chunk_len;
        }

        result
    }

    // =========================================================================
    // Contract Upgrade
    // =========================================================================

    /// Upgrade this contract (called by registry only)
    pub fn upgrade(env: Env, new_wasm_hash: BytesN<32>) {
        // Verify caller is the registry (trusted upgrader)
        let registry: Address = env
            .storage()
            .instance()
            .get(&ConfigKey::Registry)
            .expect("Not initialized");
        registry.require_auth();
        env.deployer().update_current_contract_wasm(new_wasm_hash);
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::Env;

    fn setup_test() -> (Env, Address, BoardsConfigClient<'static>) {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsConfig, ());
        let client = BoardsConfigClient::new(&env, &contract_id);

        let registry = Address::generate(&env);

        (env, registry, client)
    }

    #[test]
    fn test_init_and_defaults() {
        let (env, registry, client) = setup_test();

        client.init(&registry);

        // Verify branding defaults
        let branding = client.get_branding();
        assert_eq!(branding.site_name, String::from_str(&env, "Soroban Boards"));
        assert_eq!(
            branding.tagline,
            String::from_str(&env, "Decentralized discussion forums on Stellar")
        );
        assert_eq!(branding.primary_color, String::from_str(&env, "#7857e1"));

        // Verify threshold defaults (permissive)
        let board_thresholds = client.get_board_thresholds();
        assert_eq!(board_thresholds.min_karma, 0);
        assert_eq!(board_thresholds.per_user_limit, 0);

        // Verify operational defaults
        assert_eq!(client.get_max_reply_depth(), 10);
        assert_eq!(client.get_reply_chunk_size(), 6);
        assert_eq!(client.get_default_edit_window(), 86400);
    }

    #[test]
    fn test_name_limits() {
        let (_, registry, client) = setup_test();

        client.init(&registry);

        let board_limits = client.get_board_name_limits();
        assert_eq!(board_limits.min_length, 3);
        assert_eq!(board_limits.max_length, 50);

        let community_limits = client.get_community_name_limits();
        assert_eq!(community_limits.min_length, 3);
        assert_eq!(community_limits.max_length, 30);
    }

    #[test]
    fn test_voting_defaults() {
        let (_, registry, client) = setup_test();

        client.init(&registry);

        let voting = client.get_default_voting_config();
        assert!(voting.enabled);
        assert!(voting.allow_downvotes);
        assert!(voting.karma_enabled);
        assert_eq!(voting.karma_multiplier, 1);
    }

    #[test]
    fn test_render_site_name() {
        let (_, registry, client) = setup_test();

        client.init(&registry);

        let result = client.render_site_name(&None, &None);
        let mut buf = [0u8; 32];
        let len = result.len() as usize;
        result.copy_into_slice(&mut buf[..len]);
        let text = core::str::from_utf8(&buf[..len]).unwrap();
        assert_eq!(text, "Soroban Boards");
    }

    #[test]
    fn test_render_header() {
        let (_, registry, client) = setup_test();

        client.init(&registry);

        let result = client.render_header(&None, &None);
        // Should contain h1 with site name and paragraph with tagline
        let mut buf = [0u8; 256];
        let len = core::cmp::min(result.len() as usize, 256);
        result.copy_into_slice(&mut buf[..len]);
        let text = core::str::from_utf8(&buf[..len]).unwrap();
        assert!(text.contains("Soroban Boards"));
    }

    #[test]
    fn test_check_thresholds_permissive() {
        let (env, registry, client) = setup_test();

        client.init(&registry);

        let user = Address::generate(&env);

        // With default permissive thresholds, all checks should pass
        let result = client.check_thresholds(
            &CreationType::Board,
            &user,
            &0,   // user_creation_count
            &0,   // user_karma
            &0,   // user_account_age_secs
            &0,   // user_post_count
            &false, // has_profile
        );
        assert!(result.passed);
    }

    #[test]
    fn test_check_thresholds_karma_check_logic() {
        // Test the threshold checking logic directly using storage
        // without going through admin-only setters
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsConfig, ());
        let client = BoardsConfigClient::new(&env, &contract_id);
        let registry = Address::generate(&env);
        client.init(&registry);

        let user = Address::generate(&env);

        // Set thresholds directly in storage (bypass admin check for testing)
        let thresholds = CreationThresholds {
            min_karma: 100,
            min_account_age_secs: 0,
            min_post_count: 0,
            require_profile: false,
            per_user_limit: 0,
            xlm_lock_stroops: 0,
        };
        env.as_contract(&contract_id, || {
            env.storage()
                .instance()
                .set(&ConfigKey::BoardThresholds, &thresholds);
        });

        // User with insufficient karma should fail
        let result = client.check_thresholds(
            &CreationType::Board,
            &user,
            &0,
            &50, // only 50 karma
            &0,
            &0,
            &false,
        );
        assert!(!result.passed);
        assert_eq!(result.reason, String::from_str(&env, "Insufficient karma"));

        // User with sufficient karma should pass
        let result = client.check_thresholds(
            &CreationType::Board,
            &user,
            &0,
            &100, // exactly 100 karma
            &0,
            &0,
            &false,
        );
        assert!(result.passed);
    }

    #[test]
    fn test_check_thresholds_per_user_limit_logic() {
        // Test the threshold checking logic directly using storage
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsConfig, ());
        let client = BoardsConfigClient::new(&env, &contract_id);
        let registry = Address::generate(&env);
        client.init(&registry);

        let user = Address::generate(&env);

        // Set thresholds directly in storage (bypass admin check for testing)
        let thresholds = CreationThresholds {
            min_karma: 0,
            min_account_age_secs: 0,
            min_post_count: 0,
            require_profile: false,
            per_user_limit: 3,
            xlm_lock_stroops: 0,
        };
        env.as_contract(&contract_id, || {
            env.storage()
                .instance()
                .set(&ConfigKey::CommunityThresholds, &thresholds);
        });

        // User with 2 communities should pass
        let result = client.check_thresholds(
            &CreationType::Community,
            &user,
            &2,
            &0,
            &0,
            &0,
            &false,
        );
        assert!(result.passed);

        // User with 3 communities should fail
        let result = client.check_thresholds(
            &CreationType::Community,
            &user,
            &3,
            &0,
            &0,
            &0,
            &false,
        );
        assert!(!result.passed);
        assert_eq!(result.reason, String::from_str(&env, "Creation limit reached"));
    }

    #[test]
    fn test_check_thresholds_profile_required() {
        // Test profile requirement
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(BoardsConfig, ());
        let client = BoardsConfigClient::new(&env, &contract_id);
        let registry = Address::generate(&env);
        client.init(&registry);

        let user = Address::generate(&env);

        // Set thresholds with profile required
        let thresholds = CreationThresholds {
            min_karma: 0,
            min_account_age_secs: 0,
            min_post_count: 0,
            require_profile: true,
            per_user_limit: 0,
            xlm_lock_stroops: 0,
        };
        env.as_contract(&contract_id, || {
            env.storage()
                .instance()
                .set(&ConfigKey::BoardThresholds, &thresholds);
        });

        // User without profile should fail
        let result = client.check_thresholds(
            &CreationType::Board,
            &user,
            &0,
            &0,
            &0,
            &0,
            &false, // no profile
        );
        assert!(!result.passed);
        assert_eq!(result.reason, String::from_str(&env, "Profile required"));

        // User with profile should pass
        let result = client.check_thresholds(
            &CreationType::Board,
            &user,
            &0,
            &0,
            &0,
            &0,
            &true, // has profile
        );
        assert!(result.passed);
    }
}
