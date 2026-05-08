#[cfg(test)]
mod tests {
    use super::{
        ConfigLoader, ConfigSource, EscalationPolicy, McpServerConfig, McpTransport,
        ResolvedPermissionMode, KLA_SETTINGS_SCHEMA_NAME,
    };
    use crate::json::JsonValue;
    use crate::sandbox::FilesystemIsolationMode;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir() -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time should be after epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("runtime-config-{nanos}"))
    }

    #[test]
    fn rejects_non_object_settings_files() {
        let root = temp_dir();
        let cwd = root.join("project");
        let home = root.join("home").join(".kla");
        fs::create_dir_all(&home).expect("home config dir");
        fs::create_dir_all(&cwd).expect("project dir");
        fs::write(home.join("settings.json"), "[]").expect("write bad settings");

        let error = ConfigLoader::new(&cwd, &home)
            .load()
            .expect_err("config should fail");
        assert!(error
            .to_string()
            .contains("top-level settings value must be a JSON object"));

        fs::remove_dir_all(root).expect("cleanup temp dir");
    }

    #[test]
    fn loads_and_merges_klako_config_files_by_precedence() {
        let root = temp_dir();
        let cwd = root.join("project");
        let home = root.join("home").join(".kla");
        fs::create_dir_all(cwd.join(".kla")).expect("project config dir");
        fs::create_dir_all(&home).expect("home config dir");

        fs::write(
            home.parent().expect("home parent").join(".kla.json"),
            r#"{"model":"haiku","env":{"A":"1"},"mcpServers":{"home":{"command":"uvx","args":["home"]}}}"#,
        )
        .expect("write user compat config");
        fs::write(
            home.join("settings.json"),
            r#"{"model":"sonnet","env":{"A2":"1"},"hooks":{"PreToolUse":["base"]},"permissions":{"defaultMode":"plan"}}"#,
        )
        .expect("write user settings");
        fs::write(
            cwd.join(".kla.json"),
            r#"{"model":"project-compat","env":{"B":"2"}}"#,
        )
        .expect("write project compat config");
        fs::write(
            cwd.join(".kla").join("settings.json"),
            r#"{"env":{"C":"3"},"hooks":{"PostToolUse":["project"]},"mcpServers":{"project":{"command":"uvx","args":["project"]}}}"#,
        )
        .expect("write project settings");
        fs::write(
            cwd.join(".kla").join("settings.local.json"),
            r#"{"model":"opus","permissionMode":"acceptEdits"}"#,
        )
        .expect("write local settings");

        let loaded = ConfigLoader::new(&cwd, &home)
            .load()
            .expect("config should load");

        assert_eq!(KLA_SETTINGS_SCHEMA_NAME, "SettingsSchema");
        assert_eq!(loaded.loaded_entries().len(), 5);
        assert_eq!(loaded.loaded_entries()[0].source, ConfigSource::User);
        assert_eq!(
            loaded.get("model"),
            Some(&JsonValue::String("opus".to_string()))
        );
        assert_eq!(loaded.model(), Some("opus"));
        assert_eq!(
            loaded.permission_mode(),
            Some(ResolvedPermissionMode::WorkspaceWrite)
        );
        assert_eq!(
            loaded
                .get("env")
                .and_then(JsonValue::as_object)
                .expect("env object")
                .len(),
            4
        );
        assert!(loaded
            .get("hooks")
            .and_then(JsonValue::as_object)
            .expect("hooks object")
            .contains_key("PreToolUse"));
        assert!(loaded
            .get("hooks")
            .and_then(JsonValue::as_object)
            .expect("hooks object")
            .contains_key("PostToolUse"));
        assert_eq!(loaded.hooks().pre_tool_use(), &["base".to_string()]);
        assert_eq!(loaded.hooks().post_tool_use(), &["project".to_string()]);
        assert!(loaded.mcp().get("home").is_some());
        assert!(loaded.mcp().get("project").is_some());

        fs::remove_dir_all(root).expect("cleanup temp dir");
    }

    #[test]
    fn parses_sandbox_config() {
        let root = temp_dir();
        let cwd = root.join("project");
        let home = root.join("home").join(".kla");
        fs::create_dir_all(cwd.join(".kla")).expect("project config dir");
        fs::create_dir_all(&home).expect("home config dir");

        fs::write(
            cwd.join(".kla").join("settings.local.json"),
            r#"{
              "sandbox": {
                "enabled": true,
                "namespaceRestrictions": false,
                "networkIsolation": true,
                "filesystemMode": "allow-list",
                "allowedMounts": ["logs", "tmp/cache"]
              }
            }"#,
        )
        .expect("write local settings");

        let loaded = ConfigLoader::new(&cwd, &home)
            .load()
            .expect("config should load");

        assert_eq!(loaded.sandbox().enabled, Some(true));
        assert_eq!(loaded.sandbox().namespace_restrictions, Some(false));
        assert_eq!(loaded.sandbox().network_isolation, Some(true));
        assert_eq!(
            loaded.sandbox().filesystem_mode,
            Some(FilesystemIsolationMode::AllowList)
        );
        assert_eq!(loaded.sandbox().allowed_mounts, vec!["logs", "tmp/cache"]);

        fs::remove_dir_all(root).expect("cleanup temp dir");
    }

    #[test]
    fn parses_typed_mcp_and_oauth_config() {
        let root = temp_dir();
        let cwd = root.join("project");
        let home = root.join("home").join(".kla");
        fs::create_dir_all(cwd.join(".kla")).expect("project config dir");
        fs::create_dir_all(&home).expect("home config dir");

        fs::write(
            home.join("settings.json"),
            r#"{
              "mcpServers": {
                "stdio-server": {
                  "command": "uvx",
                  "args": ["mcp-server"],
                  "env": {"TOKEN": "secret"}
                },
                "remote-server": {
                  "type": "http",
                  "url": "https://example.test/mcp",
                  "headers": {"Authorization": "Bearer token"},
                  "headersHelper": "helper.sh",
                  "oauth": {
                    "clientId": "mcp-client",
                    "callbackPort": 7777,
                    "authServerMetadataUrl": "https://issuer.test/.well-known/oauth-authorization-server",
                    "xaa": true
                  }
                }
              },
              "oauth": {
                "clientId": "runtime-client",
                "authorizeUrl": "https://console.test/oauth/authorize",
                "tokenUrl": "https://console.test/oauth/token",
                "callbackPort": 54545,
                "manualRedirectUrl": "https://console.test/oauth/callback",
                "scopes": ["org:read", "user:write"]
              }
            }"#,
        )
        .expect("write user settings");
        fs::write(
            cwd.join(".kla").join("settings.local.json"),
            r#"{
              "mcpServers": {
                "remote-server": {
                  "type": "ws",
                  "url": "wss://override.test/mcp",
                  "headers": {"X-Env": "local"}
                }
              }
            }"#,
        )
        .expect("write local settings");

        let loaded = ConfigLoader::new(&cwd, &home)
            .load()
            .expect("config should load");

        let stdio_server = loaded
            .mcp()
            .get("stdio-server")
            .expect("stdio server should exist");
        assert_eq!(stdio_server.scope, ConfigSource::User);
        assert_eq!(stdio_server.transport(), McpTransport::Stdio);

        let remote_server = loaded
            .mcp()
            .get("remote-server")
            .expect("remote server should exist");
        assert_eq!(remote_server.scope, ConfigSource::Local);
        assert_eq!(remote_server.transport(), McpTransport::Ws);
        match &remote_server.config {
            McpServerConfig::Ws(config) => {
                assert_eq!(config.url, "wss://override.test/mcp");
                assert_eq!(
                    config.headers.get("X-Env").map(String::as_str),
                    Some("local")
                );
            }
            other => panic!("expected ws config, got {other:?}"),
        }

        let oauth = loaded.oauth().expect("oauth config should exist");
        assert_eq!(oauth.client_id, "runtime-client");
        assert_eq!(oauth.callback_port, Some(54_545));
        assert_eq!(oauth.scopes, vec!["org:read", "user:write"]);

        fs::remove_dir_all(root).expect("cleanup temp dir");
    }

    #[test]
    fn parses_plugin_config_from_enabled_plugins() {
        let root = temp_dir();
        let cwd = root.join("project");
        let home = root.join("home").join(".kla");
        fs::create_dir_all(cwd.join(".kla")).expect("project config dir");
        fs::create_dir_all(&home).expect("home config dir");

        fs::write(
            home.join("settings.json"),
            r#"{
              "enabledPlugins": {
                "tool-guard@builtin": true,
                "sample-plugin@external": false
              }
            }"#,
        )
        .expect("write user settings");

        let loaded = ConfigLoader::new(&cwd, &home)
            .load()
            .expect("config should load");

        assert_eq!(
            loaded.plugins().enabled_plugins().get("tool-guard@builtin"),
            Some(&true)
        );
        assert_eq!(
            loaded
                .plugins()
                .enabled_plugins()
                .get("sample-plugin@external"),
            Some(&false)
        );

        fs::remove_dir_all(root).expect("cleanup temp dir");
    }

    #[test]
    fn parses_plugin_config() {
        let root = temp_dir();
        let cwd = root.join("project");
        let home = root.join("home").join(".kla");
        fs::create_dir_all(cwd.join(".kla")).expect("project config dir");
        fs::create_dir_all(&home).expect("home config dir");

        fs::write(
            home.join("settings.json"),
            r#"{
              "enabledPlugins": {
                "core-helpers@builtin": true
              },
              "plugins": {
                "externalDirectories": ["./external-plugins"],
                "installRoot": "plugin-cache/installed",
                "registryPath": "plugin-cache/installed.json",
                "bundledRoot": "./bundled-plugins"
              }
            }"#,
        )
        .expect("write plugin settings");

        let loaded = ConfigLoader::new(&cwd, &home)
            .load()
            .expect("config should load");

        assert_eq!(
            loaded
                .plugins()
                .enabled_plugins()
                .get("core-helpers@builtin"),
            Some(&true)
        );
        assert_eq!(
            loaded.plugins().external_directories(),
            &["./external-plugins".to_string()]
        );
        assert_eq!(
            loaded.plugins().install_root(),
            Some("plugin-cache/installed")
        );
        assert_eq!(
            loaded.plugins().registry_path(),
            Some("plugin-cache/installed.json")
        );
        assert_eq!(loaded.plugins().bundled_root(), Some("./bundled-plugins"));

        fs::remove_dir_all(root).expect("cleanup temp dir");
    }

    #[test]
    fn rejects_invalid_mcp_server_shapes() {
        let root = temp_dir();
        let cwd = root.join("project");
        let home = root.join("home").join(".kla");
        fs::create_dir_all(&home).expect("home config dir");
        fs::create_dir_all(&cwd).expect("project dir");
        fs::write(
            home.join("settings.json"),
            r#"{"mcpServers":{"broken":{"type":"http","url":123}}}"#,
        )
        .expect("write broken settings");

        let error = ConfigLoader::new(&cwd, &home)
            .load()
            .expect_err("config should fail");
        assert!(error
            .to_string()
            .contains("mcpServers.broken: missing string field url"));

        fs::remove_dir_all(root).expect("cleanup temp dir");
    }

    #[test]
    fn parses_agency_topology_config() {
        let root = temp_dir();
        let cwd = root.join("project");
        let home = root.join("home").join(".kla");
        fs::create_dir_all(cwd.join(".kla")).expect("project config dir");
        fs::create_dir_all(&home).expect("home config dir");

        fs::write(
            cwd.join(".kla").join("settings.json"),
            r#"{
              "agency_topology": {
                "default_tier": "L0",
                "escalation_policy": "sequential_chain",
                "max_parse_retries": 2,
                "providers": {
                  "L0_thinker": {
                    "engine": "llama_cpp",
                    "model": "gemma-4-E4B-it.gguf",
                    "endpoint": "http://localhost:8080/v1",
                    "capabilities": ["reasoning", "lean4", "chat"]
                  },
                  "L0_typist": {
                    "engine": "llama_cpp",
                    "model": "qwen2.5-coder-7b.gguf",
                    "endpoint": "http://localhost:8081/v1",
                    "capabilities": ["bash", "file_edit", "python"]
                  },
                  "L1_micro": {
                    "engine": "gemini",
                    "model": "gemini-3.1-flash-lite-preview",
                    "api_env_var": "GEMINI_API_KEY",
                    "fallback_for": ["L0_thinker", "L0_typist"]
                  },
                  "L2_standard": {
                    "engine": "gemini",
                    "model": "gemini-3-flash-preview",
                    "api_env_var": "GEMINI_API_KEY",
                    "fallback_for": ["L1_micro"]
                  },
                  "L3_complex": {
                    "engine": "gemini",
                    "model": "gemini-2.5-pro",
                    "api_env_var": "GEMINI_API_KEY",
                    "fallback_for": ["L2_standard"],
                    "skills": ["web_search", "data_analysis"]
                  },
                  "L4_frontier": {
                    "engine": "gemini",
                    "model": "gemini-3.1-pro-preview",
                    "api_env_var": "GEMINI_API_KEY",
                    "fallback_for": ["L3_complex"]
                  }
                }
              }
            }"#,
        )
        .expect("write topology config");

        let loaded = ConfigLoader::new(&cwd, &home)
            .load()
            .expect("config should load");

        let topology = loaded
            .agency_topology()
            .expect("agency_topology should be present");

        assert_eq!(topology.default_tier, "L0");
        assert_eq!(topology.escalation_policy, EscalationPolicy::SequentialChain);
        assert_eq!(topology.max_parse_retries, 2);
        assert_eq!(topology.providers.len(), 6);

        let thinker = topology
            .providers
            .get("L0_thinker")
            .expect("L0_thinker should exist");
        assert_eq!(thinker.engine, "llama_cpp");
        assert_eq!(thinker.model, "gemma-4-E4B-it.gguf");
        assert_eq!(
            thinker.endpoint.as_deref(),
            Some("http://localhost:8080/v1")
        );
        assert_eq!(
            thinker.capabilities,
            vec!["reasoning", "lean4", "chat"]
        );
        assert!(thinker.fallback_for.is_empty());

        let typist = topology
            .providers
            .get("L0_typist")
            .expect("L0_typist should exist");
        assert_eq!(typist.engine, "llama_cpp");
        assert_eq!(
            typist.capabilities,
            vec!["bash", "file_edit", "python"]
        );

        let l1 = topology
            .providers
            .get("L1_micro")
            .expect("L1_micro should exist");
        assert_eq!(l1.engine, "gemini");
        assert_eq!(l1.model, "gemini-3.1-flash-lite-preview");
        assert_eq!(l1.api_env_var.as_deref(), Some("GEMINI_API_KEY"));
        assert_eq!(
            l1.fallback_for,
            vec!["L0_thinker", "L0_typist"]
        );
        assert!(l1.endpoint.is_none());

        let l4 = topology
            .providers
            .get("L4_frontier")
            .expect("L4_frontier should exist");
        assert_eq!(l4.model, "gemini-3.1-pro-preview");
        assert_eq!(l4.fallback_for, vec!["L3_complex"]);

        fs::remove_dir_all(root).expect("cleanup temp dir");
    }
}
