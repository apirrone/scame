use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// AI completion configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiConfig {
    /// Enable/disable AI completions
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    /// AI provider to use: "copilot", "openai", "claude", "local"
    #[serde(default = "default_provider")]
    pub provider: String,

    /// Debounce time in milliseconds before triggering completion
    #[serde(default = "default_debounce_ms")]
    pub debounce_ms: u64,

    /// Copilot-specific configuration
    #[serde(default)]
    pub copilot: CopilotConfig,

    /// OpenAI-specific configuration
    #[serde(default)]
    pub openai: OpenAiConfig,

    /// Claude-specific configuration
    #[serde(default)]
    pub claude: ClaudeConfig,

    /// Local LLM configuration
    #[serde(default)]
    pub local: LocalLlmConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CopilotConfig {
    /// GitHub Copilot API token
    pub api_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiConfig {
    /// OpenAI API key
    pub api_key: Option<String>,

    /// Model to use (e.g., "gpt-4", "gpt-3.5-turbo")
    #[serde(default = "default_openai_model")]
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeConfig {
    /// Anthropic API key
    pub api_key: Option<String>,

    /// Model to use (e.g., "claude-3-5-sonnet-20241022")
    #[serde(default = "default_claude_model")]
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalLlmConfig {
    /// HTTP endpoint for local LLM (e.g., "http://localhost:11434/api/generate")
    #[serde(default = "default_local_endpoint")]
    pub endpoint: String,
}

// Default values
fn default_enabled() -> bool {
    true
}

fn default_provider() -> String {
    "copilot".to_string()
}

fn default_debounce_ms() -> u64 {
    150
}

fn default_openai_model() -> String {
    "gpt-4".to_string()
}

fn default_claude_model() -> String {
    "claude-3-5-sonnet-20241022".to_string()
}

fn default_local_endpoint() -> String {
    "http://localhost:11434/api/generate".to_string()
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            enabled: default_enabled(),
            provider: default_provider(),
            debounce_ms: default_debounce_ms(),
            copilot: CopilotConfig::default(),
            openai: OpenAiConfig::default(),
            claude: ClaudeConfig::default(),
            local: LocalLlmConfig::default(),
        }
    }
}

impl Default for CopilotConfig {
    fn default() -> Self {
        Self { api_token: None }
    }
}

impl Default for OpenAiConfig {
    fn default() -> Self {
        Self {
            api_key: None,
            model: default_openai_model(),
        }
    }
}

impl Default for ClaudeConfig {
    fn default() -> Self {
        Self {
            api_key: None,
            model: default_claude_model(),
        }
    }
}

impl Default for LocalLlmConfig {
    fn default() -> Self {
        Self {
            endpoint: default_local_endpoint(),
        }
    }
}

/// Main configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub ai: AiConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            ai: AiConfig::default(),
        }
    }
}

impl Config {
    /// Load configuration from file or create default
    pub fn load() -> Result<Self> {
        let config_path = Self::config_path()?;

        if config_path.exists() {
            let content = std::fs::read_to_string(&config_path)?;
            let mut config: Config = toml::from_str(&content)?;

            // Override with environment variables if present
            config.apply_env_overrides();

            Ok(config)
        } else {
            // Create default config
            let mut config = Config::default();
            config.apply_env_overrides();
            Ok(config)
        }
    }

    /// Apply environment variable overrides
    fn apply_env_overrides(&mut self) {
        // Copilot token from GITHUB_TOKEN
        if let Ok(token) = std::env::var("GITHUB_TOKEN") {
            self.ai.copilot.api_token = Some(token);
        }

        // OpenAI API key from OPENAI_API_KEY
        if let Ok(key) = std::env::var("OPENAI_API_KEY") {
            self.ai.openai.api_key = Some(key);
        }

        // Claude API key from ANTHROPIC_API_KEY
        if let Ok(key) = std::env::var("ANTHROPIC_API_KEY") {
            self.ai.claude.api_key = Some(key);
        }

        // AI provider from SCAME_AI_PROVIDER
        if let Ok(provider) = std::env::var("SCAME_AI_PROVIDER") {
            self.ai.provider = provider;
        }
    }

    /// Get path to configuration file
    fn config_path() -> Result<PathBuf> {
        let home = std::env::var("HOME")?;
        let config_dir = PathBuf::from(home).join(".scame");
        std::fs::create_dir_all(&config_dir)?;
        Ok(config_dir.join("config.toml"))
    }

    /// Save configuration to file
    pub fn save(&self) -> Result<()> {
        let config_path = Self::config_path()?;
        let content = toml::to_string_pretty(self)?;
        std::fs::write(config_path, content)?;
        Ok(())
    }
}
