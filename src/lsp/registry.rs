use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use tokio::sync::{mpsc, Mutex};

use crate::config::settings::LspServerConfig;
use crate::events::LspEvent;
use crate::lsp::client::LspClient;

/// Descriptor for a well-known language server.
pub struct KnownServer {
    /// The binary name on disk (e.g. `"rust-analyzer"`).
    pub binary: &'static str,
    /// File extensions this server handles.
    pub extensions: &'static [&'static str],
    /// Default CLI arguments (e.g. `&["--stdio"]`).
    pub args: &'static [&'static str],
}

/// Built-in list of well-known language servers.
pub const KNOWN_SERVERS: &[KnownServer] = &[
    KnownServer {
        binary: "rust-analyzer",
        extensions: &["rs"],
        args: &[],
    },
    KnownServer {
        binary: "typescript-language-server",
        extensions: &["ts", "tsx", "js", "jsx"],
        args: &["--stdio"],
    },
    KnownServer {
        binary: "pyright-langserver",
        extensions: &["py", "pyi"],
        args: &["--stdio"],
    },
    KnownServer {
        binary: "gopls",
        extensions: &["go"],
        args: &["serve"],
    },
    KnownServer {
        binary: "lua-language-server",
        extensions: &["lua"],
        args: &[],
    },
    KnownServer {
        binary: "clangd",
        extensions: &["c", "cpp", "h", "hpp"],
        args: &[],
    },
    KnownServer {
        binary: "zls",
        extensions: &["zig"],
        args: &[],
    },
    KnownServer {
        binary: "jdtls",
        extensions: &["java"],
        args: &[],
    },
];

/// Configuration for a language server that the registry can spawn.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub command: String,
    pub args: Vec<String>,
    pub extensions: Vec<String>,
}

/// Registry that manages language server discovery and lifecycle.
///
/// It maps file extensions to `ServerConfig`s and keeps track of running
/// `LspClient` instances so that each language only spawns one server.
pub struct LspRegistry {
    /// Extension -> server config mapping.
    servers: HashMap<String, ServerConfig>,
    /// Language key -> running client mapping.
    active_clients: HashMap<String, Arc<Mutex<LspClient>>>,
}

impl LspRegistry {
    /// Create a new registry, merging auto-discovered servers with user
    /// overrides from the configuration file.
    ///
    /// User-provided configs win over auto-discovered ones when both define
    /// a server for the same extension.
    pub fn new(user_config: &HashMap<String, LspServerConfig>) -> Self {
        let mut servers = Self::discover();

        // Merge user config, overriding auto-discovered entries.
        for (_name, cfg) in user_config {
            let server_config = ServerConfig {
                command: cfg.command.clone(),
                args: cfg.args.clone(),
                extensions: cfg.extensions.clone(),
            };
            for ext in &cfg.extensions {
                servers.insert(ext.clone(), server_config.clone());
            }
        }

        Self {
            servers,
            active_clients: HashMap::new(),
        }
    }

    /// Scan `PATH` for known language server binaries and return a map of
    /// extension -> `ServerConfig` for every server found.
    pub fn discover() -> HashMap<String, ServerConfig> {
        let mut result = HashMap::new();

        for server in KNOWN_SERVERS {
            if binary_exists(server.binary) {
                let config = ServerConfig {
                    command: server.binary.to_string(),
                    args: server.args.iter().map(|s| s.to_string()).collect(),
                    extensions: server.extensions.iter().map(|s| s.to_string()).collect(),
                };
                for ext in server.extensions {
                    result.insert(ext.to_string(), config.clone());
                }
            }
        }

        result
    }

    /// Return a running `LspClient` for the given file extension, spawning
    /// a new server if necessary.
    ///
    /// Returns `None` if no server is configured for the extension.
    pub fn client_for_extension(
        &mut self,
        ext: &str,
        root_uri: &str,
        event_tx: mpsc::UnboundedSender<LspEvent>,
    ) -> Option<Arc<Mutex<LspClient>>> {
        let config = self.servers.get(ext)?.clone();

        // Use the command name as the language key.
        let language_key = config.command.clone();

        if let Some(client) = self.active_clients.get(&language_key) {
            return Some(Arc::clone(client));
        }

        // Spawn a new client.
        match LspClient::new(&config.command, &config.args, root_uri, event_tx) {
            Ok(client) => {
                let client = Arc::new(Mutex::new(client));
                self.active_clients
                    .insert(language_key, Arc::clone(&client));
                Some(client)
            }
            Err(e) => {
                log::error!(
                    "failed to spawn language server '{}': {}",
                    config.command,
                    e
                );
                None
            }
        }
    }

    /// Shut down all active language server clients.
    pub async fn shutdown_all(&mut self) {
        let clients: Vec<_> = self.active_clients.drain().collect();
        for (name, client) in clients {
            let locked = client.lock().await;
            if let Err(e) = locked.shutdown().await {
                log::error!("error shutting down LSP server '{}': {}", name, e);
            }
        }
    }

    /// Return the number of configured servers (extensions with configs).
    pub fn server_count(&self) -> usize {
        self.servers.len()
    }

    /// Return the number of currently active (running) clients.
    pub fn active_client_count(&self) -> usize {
        self.active_clients.len()
    }

    /// Check if a server config exists for the given extension.
    pub fn has_server_for(&self, ext: &str) -> bool {
        self.servers.contains_key(ext)
    }
}

/// Check whether a binary exists on `PATH` using `which`.
fn binary_exists(name: &str) -> bool {
    std::process::Command::new("which")
        .arg(name)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_known_servers_not_empty() {
        assert!(!KNOWN_SERVERS.is_empty());
    }

    #[test]
    fn test_known_servers_rust_analyzer() {
        let ra = KNOWN_SERVERS
            .iter()
            .find(|s| s.binary == "rust-analyzer")
            .expect("rust-analyzer should be in KNOWN_SERVERS");
        assert!(ra.extensions.contains(&"rs"));
    }

    #[test]
    fn test_known_servers_typescript() {
        let ts = KNOWN_SERVERS
            .iter()
            .find(|s| s.binary == "typescript-language-server")
            .expect("typescript-language-server should be in KNOWN_SERVERS");
        assert!(ts.extensions.contains(&"ts"));
        assert!(ts.extensions.contains(&"tsx"));
        assert!(ts.extensions.contains(&"js"));
        assert!(ts.extensions.contains(&"jsx"));
        assert!(ts.args.contains(&"--stdio"));
    }

    #[test]
    fn test_known_servers_pyright() {
        let py = KNOWN_SERVERS
            .iter()
            .find(|s| s.binary == "pyright-langserver")
            .expect("pyright-langserver should be in KNOWN_SERVERS");
        assert!(py.extensions.contains(&"py"));
        assert!(py.extensions.contains(&"pyi"));
        assert!(py.args.contains(&"--stdio"));
    }

    #[test]
    fn test_known_servers_gopls() {
        let go = KNOWN_SERVERS
            .iter()
            .find(|s| s.binary == "gopls")
            .expect("gopls should be in KNOWN_SERVERS");
        assert!(go.extensions.contains(&"go"));
        assert!(go.args.contains(&"serve"));
    }

    #[test]
    fn test_known_servers_lua() {
        let lua = KNOWN_SERVERS
            .iter()
            .find(|s| s.binary == "lua-language-server")
            .expect("lua-language-server should be in KNOWN_SERVERS");
        assert!(lua.extensions.contains(&"lua"));
    }

    #[test]
    fn test_known_servers_clangd() {
        let clangd = KNOWN_SERVERS
            .iter()
            .find(|s| s.binary == "clangd")
            .expect("clangd should be in KNOWN_SERVERS");
        assert!(clangd.extensions.contains(&"c"));
        assert!(clangd.extensions.contains(&"cpp"));
        assert!(clangd.extensions.contains(&"h"));
        assert!(clangd.extensions.contains(&"hpp"));
    }

    #[test]
    fn test_known_servers_zls() {
        let zls = KNOWN_SERVERS
            .iter()
            .find(|s| s.binary == "zls")
            .expect("zls should be in KNOWN_SERVERS");
        assert!(zls.extensions.contains(&"zig"));
    }

    #[test]
    fn test_known_servers_jdtls() {
        let jdtls = KNOWN_SERVERS
            .iter()
            .find(|s| s.binary == "jdtls")
            .expect("jdtls should be in KNOWN_SERVERS");
        assert!(jdtls.extensions.contains(&"java"));
    }

    #[test]
    fn test_known_servers_count() {
        // We defined exactly 8 known servers.
        assert_eq!(KNOWN_SERVERS.len(), 8);
    }

    #[test]
    fn test_registry_new_empty_user_config() {
        let user_config = HashMap::new();
        let registry = LspRegistry::new(&user_config);
        // Active clients should be empty at start.
        assert_eq!(registry.active_client_count(), 0);
    }

    #[test]
    fn test_registry_new_with_user_config() {
        let mut user_config = HashMap::new();
        user_config.insert(
            "my-server".to_string(),
            LspServerConfig {
                command: "my-custom-lsp".to_string(),
                args: vec!["--stdio".to_string()],
                extensions: vec!["xyz".to_string()],
                root_markers: vec![],
            },
        );
        let registry = LspRegistry::new(&user_config);
        assert!(registry.has_server_for("xyz"));
    }

    #[test]
    fn test_registry_user_config_overrides_discovered() {
        // If the user provides a config for "rs", it should override rust-analyzer.
        let mut user_config = HashMap::new();
        user_config.insert(
            "custom-rs".to_string(),
            LspServerConfig {
                command: "my-rust-server".to_string(),
                args: vec![],
                extensions: vec!["rs".to_string()],
                root_markers: vec![],
            },
        );
        let registry = LspRegistry::new(&user_config);
        // The config for "rs" should now point to "my-rust-server".
        if let Some(cfg) = registry.servers.get("rs") {
            assert_eq!(cfg.command, "my-rust-server");
        }
        // (If "rs" wasn't discovered at all, the user config still inserted it.)
        assert!(registry.has_server_for("rs"));
    }

    #[test]
    fn test_registry_no_server_for_unknown_extension() {
        let registry = LspRegistry::new(&HashMap::new());
        assert!(!registry.has_server_for("abcxyz_not_real"));
    }

    #[test]
    fn test_server_config_clone() {
        let config = ServerConfig {
            command: "test-server".to_string(),
            args: vec!["--stdio".to_string()],
            extensions: vec!["test".to_string()],
        };
        let cloned = config.clone();
        assert_eq!(config.command, cloned.command);
        assert_eq!(config.args, cloned.args);
        assert_eq!(config.extensions, cloned.extensions);
    }

    #[test]
    fn test_server_config_debug() {
        let config = ServerConfig {
            command: "test".to_string(),
            args: vec![],
            extensions: vec!["t".to_string()],
        };
        let debug_str = format!("{:?}", config);
        assert!(debug_str.contains("test"));
    }

    #[test]
    fn test_discover_returns_hashmap() {
        // discover() should always return a HashMap, even if no servers are found.
        let discovered = LspRegistry::discover();
        // We can't assert specific contents because it depends on PATH,
        // but the return type is correct.
        assert!(discovered.len() <= 20); // sanity bound
    }

    #[test]
    fn test_binary_exists_known_binary() {
        // `ls` is virtually always on PATH.
        assert!(binary_exists("ls"));
    }

    #[test]
    fn test_binary_exists_unknown_binary() {
        assert!(!binary_exists("this_binary_does_not_exist_xyz_123"));
    }

    #[test]
    fn test_client_for_extension_returns_none_for_unknown() {
        let mut registry = LspRegistry::new(&HashMap::new());
        let (tx, _rx) = mpsc::unbounded_channel();
        let result = registry.client_for_extension("abcxyz_not_real", "file:///tmp", tx);
        assert!(result.is_none());
    }

    #[test]
    fn test_registry_multiple_extensions_same_server() {
        let mut user_config = HashMap::new();
        user_config.insert(
            "multi".to_string(),
            LspServerConfig {
                command: "multi-lsp".to_string(),
                args: vec![],
                extensions: vec!["aaa".to_string(), "bbb".to_string(), "ccc".to_string()],
                root_markers: vec![],
            },
        );
        let registry = LspRegistry::new(&user_config);
        assert!(registry.has_server_for("aaa"));
        assert!(registry.has_server_for("bbb"));
        assert!(registry.has_server_for("ccc"));
    }

    #[tokio::test]
    async fn test_shutdown_all_with_no_clients() {
        let mut registry = LspRegistry::new(&HashMap::new());
        // Should not panic even with no active clients.
        registry.shutdown_all().await;
        assert_eq!(registry.active_client_count(), 0);
    }
}
