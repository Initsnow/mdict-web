use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use mdict_rs::{OpenOptions, Passcode};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use walkdir::WalkDir;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub catalog: CatalogConfig,
    #[serde(default)]
    pub index: IndexConfig,
    #[serde(default)]
    pub cache: CacheConfig,
    #[serde(default)]
    pub observability: ObservabilityConfig,
    #[serde(default)]
    pub admin: AdminConfig,
    #[serde(skip)]
    pub config_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "default_bind")]
    pub bind: SocketAddr,
    #[serde(default = "default_blocking_concurrency")]
    pub blocking_concurrency: usize,
    #[serde(default = "default_query_length_limit")]
    pub query_length_limit: usize,
    #[serde(default = "default_request_body_limit")]
    pub request_body_limit_bytes: usize,
    #[serde(default = "default_rate_limit")]
    pub rate_limit_per_second: u64,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind: default_bind(),
            blocking_concurrency: default_blocking_concurrency(),
            query_length_limit: default_query_length_limit(),
            request_body_limit_bytes: default_request_body_limit(),
            rate_limit_per_second: default_rate_limit(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CatalogConfig {
    #[serde(default)]
    pub manifests_dir: Option<PathBuf>,
    #[serde(default)]
    pub bundles: Vec<DictionaryBundleManifest>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexConfig {
    #[serde(default = "default_index_dir")]
    pub dir: PathBuf,
    #[serde(default = "default_index_rebuild")]
    pub rebuild_on_startup: bool,
}

impl Default for IndexConfig {
    fn default() -> Self {
        Self {
            dir: default_index_dir(),
            rebuild_on_startup: default_index_rebuild(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CacheConfig {
    #[serde(default)]
    pub entry: EntryCacheConfig,
    #[serde(default)]
    pub resource: ResourceCacheConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntryCacheConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_entry_cache_capacity")]
    pub max_capacity: u64,
}

impl Default for EntryCacheConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            max_capacity: default_entry_cache_capacity(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceCacheConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_resource_cache_capacity")]
    pub max_capacity: u64,
    #[serde(default = "default_resource_cache_item_limit")]
    pub max_item_bytes: usize,
}

impl Default for ResourceCacheConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            max_capacity: default_resource_cache_capacity(),
            max_item_bytes: default_resource_cache_item_limit(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObservabilityConfig {
    #[serde(default = "default_metrics_enabled")]
    pub metrics_enabled: bool,
    #[serde(default = "default_metrics_path")]
    pub metrics_path: String,
}

impl Default for ObservabilityConfig {
    fn default() -> Self {
        Self {
            metrics_enabled: default_metrics_enabled(),
            metrics_path: default_metrics_path(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AdminConfig {
    #[serde(default)]
    pub reload_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DictionaryBundleManifest {
    pub dictionary_id: String,
    pub display_name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub source_lang: Option<String>,
    #[serde(default)]
    pub target_lang: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub mdx_path: PathBuf,
    #[serde(default)]
    pub mdd_paths: Vec<PathBuf>,
    #[serde(default)]
    pub entry_script_mode: EntryScriptMode,
    #[serde(default)]
    pub passcode: Option<PasscodeConfig>,
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryScriptMode {
    #[default]
    None,
    Original,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PasscodeConfig {
    pub reg_code_hex: String,
    pub user_id: String,
}

impl AppConfig {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let path = path.as_ref();
        let content = fs::read_to_string(path)?;
        let mut config: AppConfig = toml::from_str(&content)?;
        let base_dir = path.parent().unwrap_or_else(|| Path::new("."));
        config.config_path = path.to_path_buf();
        config.resolve_paths(base_dir)?;
        Ok(config)
    }

    pub fn bundles(&self) -> &[DictionaryBundleManifest] {
        &self.catalog.bundles
    }

    fn resolve_paths(&mut self, base_dir: &Path) -> Result<(), ConfigError> {
        self.index.dir = absolutize(base_dir, &self.index.dir);

        let mut manifests = Vec::new();
        manifests.extend(
            self.catalog
                .bundles
                .iter()
                .cloned()
                .map(|manifest| resolve_manifest(base_dir, manifest)),
        );

        if let Some(dir) = self.catalog.manifests_dir.clone() {
            let manifests_dir = absolutize(base_dir, &dir);
            if manifests_dir.exists() {
                for entry in WalkDir::new(&manifests_dir)
                    .min_depth(1)
                    .max_depth(4)
                    .into_iter()
                    .filter_map(Result::ok)
                    .filter(|entry| entry.file_type().is_file())
                    .filter(|entry| {
                        matches!(
                            entry.path().extension().and_then(|ext| ext.to_str()),
                            Some("toml")
                        )
                    })
                {
                    let content = fs::read_to_string(entry.path())?;
                    let manifest: DictionaryBundleManifest = toml::from_str(&content)?;
                    let manifest_dir = entry.path().parent().unwrap_or(&manifests_dir);
                    manifests.push(resolve_manifest(manifest_dir, manifest));
                }
            }
        }

        validate_manifests(&manifests)?;
        self.catalog.bundles = manifests;
        Ok(())
    }
}

impl DictionaryBundleManifest {
    pub fn has_resources(&self) -> bool {
        !self.mdd_paths.is_empty()
    }

    pub fn allows_dictionary_scripts(&self) -> bool {
        matches!(self.entry_script_mode, EntryScriptMode::Original)
    }

    pub fn open_options(&self) -> OpenOptions {
        OpenOptions {
            passcode: self.passcode.as_ref().map(|passcode| Passcode {
                reg_code_hex: passcode.reg_code_hex.clone(),
                user_id: passcode.user_id.clone(),
            }),
        }
    }
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("failed to read config file: {0}")]
    Io(#[from] std::io::Error),
    #[error("failed to parse TOML config: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("invalid config: {0}")]
    Invalid(String),
}

fn resolve_manifest(
    base_dir: &Path,
    mut manifest: DictionaryBundleManifest,
) -> DictionaryBundleManifest {
    manifest.mdx_path = absolutize(base_dir, &manifest.mdx_path);
    manifest.mdd_paths = manifest
        .mdd_paths
        .into_iter()
        .map(|path| absolutize(base_dir, &path))
        .collect();
    manifest.tags.sort();
    manifest.tags.dedup();
    manifest
}

fn validate_manifests(manifests: &[DictionaryBundleManifest]) -> Result<(), ConfigError> {
    if manifests.is_empty() {
        return Err(ConfigError::Invalid(
            "at least one dictionary bundle must be configured".to_owned(),
        ));
    }

    let mut ids = BTreeSet::new();
    for manifest in manifests {
        if manifest.dictionary_id.is_empty() {
            return Err(ConfigError::Invalid(
                "dictionary_id must not be empty".to_owned(),
            ));
        }
        if !manifest
            .dictionary_id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
        {
            return Err(ConfigError::Invalid(format!(
                "dictionary_id `{}` contains invalid characters",
                manifest.dictionary_id
            )));
        }
        if !ids.insert(manifest.dictionary_id.clone()) {
            return Err(ConfigError::Invalid(format!(
                "duplicate dictionary_id `{}`",
                manifest.dictionary_id
            )));
        }
        if manifest.display_name.trim().is_empty() {
            return Err(ConfigError::Invalid(format!(
                "display_name must not be empty for `{}`",
                manifest.dictionary_id
            )));
        }
    }

    for manifest in manifests {
        if !manifest.mdx_path.exists() {
            return Err(ConfigError::Invalid(format!(
                "mdx file does not exist for `{}`: {}",
                manifest.dictionary_id,
                manifest.mdx_path.display()
            )));
        }
        let mut seen_mdd_paths = BTreeSet::new();
        for path in &manifest.mdd_paths {
            if !path.exists() {
                return Err(ConfigError::Invalid(format!(
                    "mdd file does not exist for `{}`: {}",
                    manifest.dictionary_id,
                    path.display()
                )));
            }
            if !seen_mdd_paths.insert(path.clone()) {
                return Err(ConfigError::Invalid(format!(
                    "duplicate mdd path configured for `{}`: {}",
                    manifest.dictionary_id,
                    path.display()
                )));
            }
        }
    }
    Ok(())
}

fn absolutize(base_dir: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        base_dir.join(path)
    }
}

fn default_bind() -> SocketAddr {
    "127.0.0.1:8080"
        .parse()
        .expect("default socket address must parse")
}

fn default_blocking_concurrency() -> usize {
    32
}

fn default_query_length_limit() -> usize {
    512
}

fn default_request_body_limit() -> usize {
    32 * 1024
}

fn default_rate_limit() -> u64 {
    200
}

fn default_index_dir() -> PathBuf {
    PathBuf::from("index")
}

fn default_index_rebuild() -> bool {
    false
}

fn default_entry_cache_capacity() -> u64 {
    8 * 1024 * 1024
}

fn default_resource_cache_capacity() -> u64 {
    32 * 1024 * 1024
}

fn default_resource_cache_item_limit() -> usize {
    512 * 1024
}

fn default_metrics_enabled() -> bool {
    true
}

fn default_metrics_path() -> String {
    "/metrics".to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn rejects_duplicate_dictionary_ids() {
        let manifests = vec![
            DictionaryBundleManifest {
                dictionary_id: "demo".to_owned(),
                display_name: "Demo".to_owned(),
                description: None,
                source_lang: None,
                target_lang: None,
                tags: vec![],
                mdx_path: PathBuf::from("/tmp/demo.mdx"),
                mdd_paths: vec![],
                entry_script_mode: EntryScriptMode::None,
                passcode: None,
                metadata: BTreeMap::new(),
            },
            DictionaryBundleManifest {
                dictionary_id: "demo".to_owned(),
                display_name: "Demo 2".to_owned(),
                description: None,
                source_lang: None,
                target_lang: None,
                tags: vec![],
                mdx_path: PathBuf::from("/tmp/demo-2.mdx"),
                mdd_paths: vec![],
                entry_script_mode: EntryScriptMode::None,
                passcode: None,
                metadata: BTreeMap::new(),
            },
        ];

        let error = validate_manifests(&manifests).expect_err("duplicate ids must fail");
        assert!(error.to_string().contains("duplicate dictionary_id"));
    }

    #[test]
    fn rejects_legacy_mdd_path_field() {
        let error = toml::from_str::<DictionaryBundleManifest>(
            r#"
dictionary_id = "demo"
display_name = "Demo"
mdx_path = "demo.mdx"
mdd_path = "demo.mdd"
"#,
        )
        .expect_err("legacy mdd_path must be rejected");
        assert!(error.to_string().contains("unknown field `mdd_path`"));
    }

    #[test]
    fn app_config_resolves_relative_mdd_paths() {
        let dir = tempdir().expect("temp dir should exist");
        let dictionaries_dir = dir.path().join("dict");
        fs::create_dir_all(&dictionaries_dir).expect("dict dir should exist");
        fs::write(dictionaries_dir.join("demo.mdx"), b"mdx").expect("mdx should write");
        fs::write(dictionaries_dir.join("demo.1.mdd"), b"mdd1").expect("mdd1 should write");
        fs::write(dictionaries_dir.join("demo.2.mdd"), b"mdd2").expect("mdd2 should write");

        let config_path = dir.path().join("mdict-web.toml");
        fs::write(
            &config_path,
            r#"
[catalog]

[[catalog.bundles]]
dictionary_id = "demo"
display_name = "Demo"
mdx_path = "dict/demo.mdx"
mdd_paths = ["dict/demo.1.mdd", "dict/demo.2.mdd"]
"#,
        )
        .expect("config should write");

        let config = AppConfig::load(&config_path).expect("config should load");
        let manifest = &config.catalog.bundles[0];
        assert_eq!(
            manifest.mdd_paths,
            vec![
                dictionaries_dir.join("demo.1.mdd"),
                dictionaries_dir.join("demo.2.mdd"),
            ]
        );
    }

    #[test]
    fn validate_manifests_rejects_duplicate_mdd_paths() {
        let dir = tempdir().expect("temp dir should exist");
        let mdx_path = dir.path().join("demo.mdx");
        let mdd_path = dir.path().join("demo-a.mdd");
        fs::write(&mdx_path, b"mdx").expect("mdx should write");
        fs::write(&mdd_path, b"mdd").expect("mdd should write");
        let manifests = vec![DictionaryBundleManifest {
            dictionary_id: "demo".to_owned(),
            display_name: "Demo".to_owned(),
            description: None,
            source_lang: None,
            target_lang: None,
            tags: vec![],
            mdx_path,
            mdd_paths: vec![mdd_path.clone(), mdd_path],
            entry_script_mode: EntryScriptMode::None,
            passcode: None,
            metadata: BTreeMap::new(),
        }];

        let error = validate_manifests(&manifests).expect_err("duplicate mdd paths must fail");
        assert!(error.to_string().contains("duplicate mdd path configured"));
    }
}
