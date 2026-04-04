use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use mdict_web_config::{AppConfig, DictionaryBundleManifest};
use mdict_web_domain::{
    DictionaryDetail, DictionaryListResponse, DictionaryStatus, DictionarySummary, LookupResult,
    ReadinessResponse, ReloadResponse, RenderedEntryContent, ResourceContent, SearchLookupResponse,
    SearchSuggestResponse, SearchSuggestionItem, ServiceError, SuggestResponse, ThemeMode,
};
use mdict_web_engine::{DictionaryEngine, LoadedDictionary};
use thiserror::Error;
use tokio::{sync::RwLock, task::JoinSet};
use tracing::warn;

const DEFAULT_SUGGEST_LIMIT: usize = 20;
const MAX_SUGGEST_LIMIT: usize = 50;

#[derive(Clone)]
pub struct ReloadableDictionaryService {
    config_path: PathBuf,
    current: Arc<RwLock<Arc<DictionaryService>>>,
}

pub struct DictionaryService {
    config: AppConfig,
    engine: Arc<DictionaryEngine>,
    dictionaries: BTreeMap<String, DictionaryState>,
}

enum DictionaryState {
    Ready(Arc<LoadedDictionary>),
    Unavailable(UnavailableDictionary),
}

struct UnavailableDictionary {
    manifest: DictionaryBundleManifest,
    reason: String,
}

#[derive(Debug, Error)]
pub enum ServiceBuildError {
    #[error("{0}")]
    Config(#[from] mdict_web_config::ConfigError),
    #[error("failed to reload dictionary service")]
    Join,
}

impl ReloadableDictionaryService {
    pub async fn load_from_path(path: impl AsRef<Path>) -> Result<Self, ServiceBuildError> {
        let path = path.as_ref().to_path_buf();
        let service = Arc::new(load_service(path.clone()).await?);
        Ok(Self {
            config_path: path,
            current: Arc::new(RwLock::new(service)),
        })
    }

    pub async fn snapshot(&self) -> Arc<DictionaryService> {
        self.current.read().await.clone()
    }

    pub async fn reload(&self) -> Result<ReloadResponse, ServiceBuildError> {
        let next = Arc::new(load_service(self.config_path.clone()).await?);
        let dictionary_count = next.dictionary_count();
        *self.current.write().await = next;

        Ok(ReloadResponse {
            status: "reloaded".to_owned(),
            dictionary_count,
        })
    }
}

impl DictionaryService {
    pub fn build(config: AppConfig) -> Self {
        let engine = Arc::new(DictionaryEngine::new(&config));
        let mut dictionaries = BTreeMap::new();

        for manifest in config.bundles() {
            let state = match engine.open_dictionary(&config, manifest) {
                Ok(dictionary) => DictionaryState::Ready(Arc::new(dictionary)),
                Err(error) => {
                    warn!(
                        dictionary_id = %manifest.dictionary_id,
                        error = %error,
                        "dictionary bundle is unavailable"
                    );
                    DictionaryState::Unavailable(UnavailableDictionary {
                        manifest: manifest.clone(),
                        reason: "failed to open dictionary bundle".to_owned(),
                    })
                }
            };
            dictionaries.insert(manifest.dictionary_id.clone(), state);
        }

        Self {
            config,
            engine,
            dictionaries,
        }
    }

    pub fn config(&self) -> &AppConfig {
        &self.config
    }

    pub fn dictionary_count(&self) -> usize {
        self.dictionaries.len()
    }

    pub fn list_dictionaries(&self) -> DictionaryListResponse {
        let items = self
            .dictionaries
            .values()
            .map(|state| state.summary())
            .collect::<Vec<_>>();
        DictionaryListResponse { items }
    }

    pub fn dictionary_detail(&self, dictionary_id: &str) -> Result<DictionaryDetail, ServiceError> {
        match self
            .dictionaries
            .get(dictionary_id)
            .ok_or_else(|| ServiceError::dictionary_not_found(dictionary_id))?
        {
            DictionaryState::Ready(dictionary) => Ok(DictionaryDetail {
                summary: state_summary_ready(dictionary),
                header: dictionary.header_info(),
                metadata: dictionary.manifest.metadata.clone(),
            }),
            DictionaryState::Unavailable(unavailable) => Err(ServiceError::dictionary_unavailable(
                dictionary_id,
                unavailable.reason.clone(),
            )),
        }
    }

    pub fn readiness(&self) -> ReadinessResponse {
        let unavailable_dictionaries = self
            .dictionaries
            .iter()
            .filter_map(|(dictionary_id, state)| match state {
                DictionaryState::Ready(_) => None,
                DictionaryState::Unavailable(_) => Some(dictionary_id.clone()),
            })
            .collect::<Vec<_>>();

        ReadinessResponse {
            status: if unavailable_dictionaries.is_empty() {
                "ready".to_owned()
            } else {
                "degraded".to_owned()
            },
            ready_dictionaries: self
                .dictionaries
                .values()
                .filter(|state| matches!(state, DictionaryState::Ready(_)))
                .count(),
            unavailable_dictionaries,
        }
    }

    pub async fn suggest(
        &self,
        dictionary_id: &str,
        query: String,
        limit: Option<usize>,
    ) -> Result<SuggestResponse, ServiceError> {
        validate_query("q", &query, self.config.server.query_length_limit)?;
        let limit = limit
            .unwrap_or(DEFAULT_SUGGEST_LIMIT)
            .min(MAX_SUGGEST_LIMIT);
        let dictionary = self.ready_dictionary(dictionary_id)?;
        let items = self.engine.suggest(dictionary, &query, limit).await?;
        Ok(SuggestResponse {
            dictionary_id: dictionary_id.to_owned(),
            query,
            items,
        })
    }

    pub async fn search_suggest(
        &self,
        query: String,
        limit: Option<usize>,
        dictionary_ids: Vec<String>,
    ) -> Result<SearchSuggestResponse, ServiceError> {
        validate_query("q", &query, self.config.server.query_length_limit)?;
        let limit = limit
            .unwrap_or(DEFAULT_SUGGEST_LIMIT)
            .min(MAX_SUGGEST_LIMIT);
        let dictionaries = self.selected_dictionaries(&dictionary_ids)?;
        let mut tasks = JoinSet::new();
        for (index, dictionary) in dictionaries.into_iter().enumerate() {
            let engine = self.engine.clone();
            let dictionary_id = dictionary.manifest.dictionary_id.clone();
            let query = query.clone();
            tasks.spawn(async move {
                let items = engine.suggest(dictionary, &query, limit).await?;
                let items = if items.is_empty() {
                    None
                } else {
                    Some(
                        items
                            .into_iter()
                            .map(|item| SearchSuggestionItem {
                                dictionary_id: dictionary_id.clone(),
                                key: item.key,
                                label: item.label,
                                match_type: item.match_type,
                            })
                            .collect::<VecDeque<_>>(),
                    )
                };
                Ok::<_, ServiceError>((index, items))
            });
        }

        let mut groups = (0..tasks.len()).map(|_| None).collect::<Vec<_>>();
        while let Some(joined) = tasks.join_next().await {
            let (index, items) = joined.map_err(|error| {
                ServiceError::internal(format!("search suggest task failed: {error}"))
            })??;
            groups[index] = items;
        }
        let mut groups = groups.into_iter().flatten().collect::<Vec<_>>();

        let mut items = Vec::with_capacity(limit);
        while items.len() < limit {
            let mut progressed = false;
            for group in &mut groups {
                if let Some(item) = group.pop_front() {
                    items.push(item);
                    progressed = true;
                    if items.len() >= limit {
                        break;
                    }
                }
            }
            if !progressed {
                break;
            }
        }

        Ok(SearchSuggestResponse { query, items })
    }

    pub async fn lookup(
        &self,
        dictionary_id: &str,
        key: String,
    ) -> Result<LookupResult, ServiceError> {
        validate_query("key", &key, self.config.server.query_length_limit)?;
        let dictionary = self.ready_dictionary(dictionary_id)?;
        let artifact = self.engine.lookup(dictionary.clone(), key.clone()).await?;

        Ok(LookupResult {
            dictionary_id: dictionary_id.to_owned(),
            query_key: key,
            resolved_key: artifact.resolved_key.clone(),
            redirected_from: artifact.redirected_from.clone(),
            match_type: artifact.match_type,
            has_resources: artifact.has_resources,
            content_url: entry_content_url(dictionary_id, &artifact.resolved_key),
            resource_url_template: resource_template_url(dictionary_id),
            etag: artifact.etag,
        })
    }

    pub async fn search_lookup(
        &self,
        key: String,
        dictionary_ids: Vec<String>,
    ) -> Result<SearchLookupResponse, ServiceError> {
        validate_query("key", &key, self.config.server.query_length_limit)?;
        let dictionaries = self.selected_dictionaries(&dictionary_ids)?;
        let mut tasks = JoinSet::new();
        for (index, dictionary) in dictionaries.into_iter().enumerate() {
            let engine = self.engine.clone();
            let dictionary_id = dictionary.manifest.dictionary_id.clone();
            let query_key = key.clone();
            tasks.spawn(async move {
                match engine.lookup(dictionary, query_key.clone()).await {
                    Ok(artifact) => Ok::<_, ServiceError>((
                        index,
                        Some(LookupResult {
                            dictionary_id: dictionary_id.clone(),
                            query_key: query_key.clone(),
                            resolved_key: artifact.resolved_key.clone(),
                            redirected_from: artifact.redirected_from.clone(),
                            match_type: artifact.match_type,
                            has_resources: artifact.has_resources,
                            content_url: entry_content_url(&dictionary_id, &artifact.resolved_key),
                            resource_url_template: resource_template_url(&dictionary_id),
                            etag: artifact.etag,
                        }),
                    )),
                    Err(error) if error.code == mdict_web_domain::ErrorCode::EntryNotFound => {
                        Ok((index, None))
                    }
                    Err(error) => Err(error),
                }
            });
        }

        let mut ordered = (0..tasks.len()).map(|_| None).collect::<Vec<_>>();
        while let Some(joined) = tasks.join_next().await {
            let (index, item) = joined.map_err(|error| {
                ServiceError::internal(format!("search lookup task failed: {error}"))
            })??;
            ordered[index] = item;
        }
        let items = ordered.into_iter().flatten().collect::<Vec<_>>();

        if items.is_empty() {
            return Err(ServiceError::entry_not_found_in_scope(&key));
        }

        Ok(SearchLookupResponse {
            query_key: key,
            items,
        })
    }

    pub async fn entry_content(
        &self,
        dictionary_id: &str,
        key: String,
    ) -> Result<RenderedEntryContent, ServiceError> {
        validate_query("key", &key, self.config.server.query_length_limit)?;
        let dictionary = self.ready_dictionary(dictionary_id)?;
        self.engine.render_entry(dictionary, key).await
    }

    pub async fn resource_content(
        &self,
        dictionary_id: &str,
        key: String,
    ) -> Result<ResourceContent, ServiceError> {
        validate_query("key", &key, self.config.server.query_length_limit)?;
        let dictionary = self.ready_dictionary(dictionary_id)?;
        self.engine.load_resource(dictionary, key).await
    }

    fn ready_dictionary(&self, dictionary_id: &str) -> Result<Arc<LoadedDictionary>, ServiceError> {
        match self
            .dictionaries
            .get(dictionary_id)
            .ok_or_else(|| ServiceError::dictionary_not_found(dictionary_id))?
        {
            DictionaryState::Ready(dictionary) => Ok(dictionary.clone()),
            DictionaryState::Unavailable(unavailable) => Err(ServiceError::dictionary_unavailable(
                dictionary_id,
                unavailable.reason.clone(),
            )),
        }
    }

    fn selected_dictionaries(
        &self,
        dictionary_ids: &[String],
    ) -> Result<Vec<Arc<LoadedDictionary>>, ServiceError> {
        if dictionary_ids.is_empty() {
            return Ok(self
                .dictionaries
                .values()
                .filter_map(|state| match state {
                    DictionaryState::Ready(dictionary) => Some(dictionary.clone()),
                    DictionaryState::Unavailable(_) => None,
                })
                .collect());
        }

        let mut seen = BTreeSet::new();
        let mut dictionaries = Vec::new();
        for dictionary_id in dictionary_ids {
            if !seen.insert(dictionary_id.clone()) {
                continue;
            }
            dictionaries.push(self.ready_dictionary(dictionary_id)?);
        }
        Ok(dictionaries)
    }
}

impl DictionaryState {
    fn summary(&self) -> DictionarySummary {
        match self {
            DictionaryState::Ready(dictionary) => state_summary_ready(dictionary),
            DictionaryState::Unavailable(unavailable) => DictionarySummary {
                dictionary_id: unavailable.manifest.dictionary_id.clone(),
                display_name: unavailable.manifest.display_name.clone(),
                description: unavailable.manifest.description.clone(),
                source_lang: unavailable.manifest.source_lang.clone(),
                target_lang: unavailable.manifest.target_lang.clone(),
                entry_count: 0,
                has_resources: unavailable.manifest.has_resources(),
                theme_mode: map_theme_mode(unavailable.manifest.theme_mode),
                status: DictionaryStatus::Unavailable,
            },
        }
    }
}

fn state_summary_ready(dictionary: &LoadedDictionary) -> DictionarySummary {
    DictionarySummary {
        dictionary_id: dictionary.manifest.dictionary_id.clone(),
        display_name: dictionary.manifest.display_name.clone(),
        description: dictionary.manifest.description.clone(),
        source_lang: dictionary.manifest.source_lang.clone(),
        target_lang: dictionary.manifest.target_lang.clone(),
        entry_count: dictionary.entry_count,
        has_resources: dictionary.manifest.has_resources(),
        theme_mode: map_theme_mode(dictionary.manifest.theme_mode),
        status: DictionaryStatus::Ready,
    }
}

fn map_theme_mode(theme_mode: mdict_web_config::ThemeMode) -> ThemeMode {
    match theme_mode {
        mdict_web_config::ThemeMode::Auto => ThemeMode::Auto,
        mdict_web_config::ThemeMode::Dictionary => ThemeMode::Dictionary,
        mdict_web_config::ThemeMode::ForceAutoDark => ThemeMode::ForceAutoDark,
    }
}

async fn load_service(path: PathBuf) -> Result<DictionaryService, ServiceBuildError> {
    tokio::task::spawn_blocking(move || {
        let config = AppConfig::load(&path)?;
        Ok(DictionaryService::build(config))
    })
    .await
    .map_err(|_| ServiceBuildError::Join)?
}

fn validate_query(name: &str, value: &str, max_len: usize) -> Result<(), ServiceError> {
    if value.trim().is_empty() {
        return Err(ServiceError::bad_request(format!(
            "query parameter `{name}` must not be empty"
        )));
    }
    if value.len() > max_len {
        return Err(ServiceError::bad_request(format!(
            "query parameter `{name}` exceeds the maximum length of {max_len}"
        )));
    }
    Ok(())
}

fn entry_content_url(dictionary_id: &str, key: &str) -> String {
    format!(
        "/api/v1/dictionaries/{dictionary_id}/entries/content?key={}",
        percent_encode(key)
    )
}

fn resource_template_url(dictionary_id: &str) -> String {
    format!("/api/v1/dictionaries/{dictionary_id}/resources/content?key={{resource_key}}")
}

fn percent_encode(value: &str) -> String {
    use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};

    utf8_percent_encode(value, NON_ALPHANUMERIC).to_string()
}
