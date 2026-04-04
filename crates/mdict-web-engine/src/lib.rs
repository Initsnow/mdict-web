use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::UNIX_EPOCH;

use blake3::Hasher;
use bytes::Bytes;
use lol_html::{RewriteStrSettings, element, rewrite_str};
use mdict_rs::{Error as MdictError, Header, MddFile, MddResourceSpan, MdxFile, MdxRecord};
use mdict_web_config::{AppConfig, DictionaryBundleManifest, EntryScriptMode};
use mdict_web_domain::{
    DictionaryHeaderInfo, LookupMatchType, RenderedEntryContent, ResourceBody, ResourceContent,
    ServiceError, SuggestionItem, SuggestionMatchType,
};
use mdict_web_index::DictionarySuggestIndex;
use metrics::counter;
use mime::Mime;
use moka::sync::Cache;
use percent_encoding::{NON_ALPHANUMERIC, percent_decode_str, utf8_percent_encode};
use regex::Regex;
use tokio::sync::{Semaphore, mpsc};
use tokio_stream::wrappers::ReceiverStream;
use tracing::{info, warn};
use url::Url;

const ENTRY_CACHE_CONTROL: &str = "public, max-age=60";
const RESOURCE_CACHE_CONTROL: &str = "public, max-age=86400";
const ENTRY_RENDER_VERSION_NONE: &str = "entry-html-v8:none";
const ENTRY_RENDER_VERSION_ORIGINAL: &str = "entry-html-v8:original";
const RESOURCE_RENDER_VERSION: &str = "resource-v2";
const ENTRY_LINK_PREFIX: &str = "@@@LINK=";
const MAX_ENTRY_REDIRECT_DEPTH: usize = 8;
const ENTRY_AUDIO_RUNTIME_SCRIPT: &str = r#"<script>
(() => {
  const audio = new Audio();
  audio.preload = "none";
  document.addEventListener("click", (event) => {
    const target = event.target;
    const element =
      target instanceof Element
        ? target
        : target instanceof Node
          ? target.parentElement
          : null;
    if (!element) {
      return;
    }
    const link = element.closest("a[data-audio-href]");
    if (!(link instanceof HTMLAnchorElement)) {
      return;
    }
    const href = link.getAttribute("data-audio-href");
    if (!href) {
      return;
    }
    event.preventDefault();
    event.stopPropagation();
    if (audio.src === href) {
      audio.currentTime = 0;
    } else {
      audio.src = href;
    }
    void audio.play().catch(() => {});
  }, true);
})();
</script>"#;

#[derive(Debug)]
pub struct LoadedDictionary {
    pub manifest: DictionaryBundleManifest,
    pub header: Header,
    pub entry_count: u64,
    pub mdx: Arc<MdxFile>,
    pub mdds: Vec<Arc<MddFile>>,
    pub index: DictionarySuggestIndex,
    pub version_tag: String,
}

#[derive(Clone)]
pub struct DictionaryEngine {
    io_gate: Arc<Semaphore>,
    entry_cache: Option<Cache<String, RenderedEntryContent>>,
    resource_cache: Option<Cache<String, CachedResource>>,
    resource_cache_item_limit: usize,
}

#[derive(Debug, Clone)]
pub struct LookupArtifact {
    pub resolved_key: String,
    pub redirected_from: Option<String>,
    pub match_type: LookupMatchType,
    pub etag: String,
    pub has_resources: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum EngineInitError {
    #[error("mdict open error: {0}")]
    Mdict(#[from] mdict_rs::Error),
    #[error("sidecar index error: {0}")]
    Index(#[from] mdict_web_index::IndexError),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Clone)]
struct CachedResource {
    resolved_key: String,
    content_type: String,
    bytes: Bytes,
    etag: String,
    cache_control: String,
}

#[derive(Debug, Clone)]
struct ResolvedMdxRecord {
    matched_key: String,
    redirected_from: Option<String>,
    record: MdxRecord,
}

#[derive(Debug, Clone)]
struct ResolvedSidecarResource {
    resolved_key: String,
    path: PathBuf,
}

#[derive(Debug, Clone)]
struct ResolvedMddResourceSpan {
    mdd: Arc<MddFile>,
    span: MddResourceSpan,
}

impl LoadedDictionary {
    pub fn header_info(&self) -> DictionaryHeaderInfo {
        DictionaryHeaderInfo {
            title: self.header.title.clone(),
            description: self.header.description.clone(),
            generated_by_engine_version: self.header.generated_by_engine_version.clone(),
            required_engine_version: self.header.required_engine_version.clone(),
            encoding_label: self.header.encoding_label.clone(),
        }
    }
}

impl DictionaryEngine {
    pub fn new(config: &AppConfig) -> Self {
        let entry_cache = config.cache.entry.enabled.then(|| {
            Cache::builder()
                .max_capacity(config.cache.entry.max_capacity)
                .weigher(|_key: &String, value: &RenderedEntryContent| {
                    value.html.len().min(u32::MAX as usize) as u32
                })
                .build()
        });
        let resource_cache = config.cache.resource.enabled.then(|| {
            Cache::builder()
                .max_capacity(config.cache.resource.max_capacity)
                .weigher(|_key: &String, value: &CachedResource| {
                    value.bytes.len().min(u32::MAX as usize) as u32
                })
                .build()
        });

        Self {
            io_gate: Arc::new(Semaphore::new(config.server.blocking_concurrency.max(1))),
            entry_cache,
            resource_cache,
            resource_cache_item_limit: config.cache.resource.max_item_bytes,
        }
    }

    pub fn open_dictionary(
        &self,
        config: &AppConfig,
        manifest: &DictionaryBundleManifest,
    ) -> Result<LoadedDictionary, EngineInitError> {
        let mdx = MdxFile::open_with_options(&manifest.mdx_path, manifest.open_options())?;
        let header = mdx.header().clone();
        let entry_count = mdx.len();
        let index = DictionarySuggestIndex::load_or_build(
            &manifest.dictionary_id,
            &manifest.mdx_path,
            &header,
            entry_count,
            &config.index.dir,
            config.index.rebuild_on_startup,
            &mdx,
        )?;
        let mdds = manifest
            .mdd_paths
            .iter()
            .map(|path| MddFile::open_with_options(path, manifest.open_options()).map(Arc::new))
            .collect::<Result<Vec<_>, _>>()?;
        let version_tag = dictionary_version_tag(&manifest.mdx_path, &manifest.mdd_paths)?;

        info!(
            dictionary_id = %manifest.dictionary_id,
            entry_count,
            mdd_count = mdds.len(),
            has_resources = manifest.has_resources(),
            "loaded dictionary bundle"
        );

        Ok(LoadedDictionary {
            manifest: manifest.clone(),
            header,
            entry_count,
            mdx: Arc::new(mdx),
            mdds,
            index,
            version_tag,
        })
    }

    pub fn suggest(
        &self,
        dictionary: &LoadedDictionary,
        query: &str,
        limit: usize,
    ) -> Vec<SuggestionItem> {
        dictionary
            .index
            .suggest(
                query,
                dictionary.header.key_case_sensitive,
                dictionary.header.strip_key,
                limit,
            )
            .into_iter()
            .map(|key| SuggestionItem {
                label: key.clone(),
                key,
                match_type: SuggestionMatchType::Prefix,
            })
            .collect()
    }

    pub async fn lookup(
        &self,
        dictionary: Arc<LoadedDictionary>,
        query_key: String,
    ) -> Result<LookupArtifact, ServiceError> {
        let dictionary_id = dictionary.manifest.dictionary_id.clone();
        let record = self
            .lookup_record(dictionary.clone(), query_key.clone())
            .await?;
        let match_type = if record.matched_key == query_key {
            LookupMatchType::Exact
        } else {
            LookupMatchType::Normalized
        };

        Ok(LookupArtifact {
            resolved_key: record.record.key.clone(),
            redirected_from: record.redirected_from.clone(),
            match_type,
            etag: entry_etag(
                &dictionary_id,
                &dictionary.version_tag,
                &record.record.key,
                entry_render_version(dictionary.manifest.entry_script_mode),
            ),
            has_resources: !dictionary.mdds.is_empty(),
        })
    }

    pub async fn render_entry(
        &self,
        dictionary: Arc<LoadedDictionary>,
        query_key: String,
    ) -> Result<RenderedEntryContent, ServiceError> {
        let dictionary_id = dictionary.manifest.dictionary_id.clone();
        let record = self.lookup_record(dictionary.clone(), query_key).await?;
        let cache_key = format!("{dictionary_id}:{}", record.record.key);

        if let Some(cache) = &self.entry_cache {
            if let Some(cached) = cache.get(&cache_key) {
                counter!("mdict_web_entry_cache_hits_total").increment(1);
                return Ok(cached);
            }
            counter!("mdict_web_entry_cache_misses_total").increment(1);
        }

        let (html, allow_entry_runtime) = rewrite_entry_html(
            &dictionary_id,
            &record.record.text,
            dictionary.manifest.entry_script_mode,
        )?;
        let content = RenderedEntryContent {
            dictionary_id: dictionary_id.clone(),
            resolved_key: record.record.key.clone(),
            html,
            allow_dictionary_scripts: dictionary.manifest.allows_dictionary_scripts(),
            allow_entry_runtime,
            etag: entry_etag(
                &dictionary_id,
                &dictionary.version_tag,
                &record.record.key,
                entry_render_version(dictionary.manifest.entry_script_mode),
            ),
            cache_control: ENTRY_CACHE_CONTROL.to_owned(),
        };

        if let Some(cache) = &self.entry_cache {
            cache.insert(cache_key, content.clone());
        }

        Ok(content)
    }

    pub async fn load_resource(
        &self,
        dictionary: Arc<LoadedDictionary>,
        query_key: String,
    ) -> Result<ResourceContent, ServiceError> {
        let dictionary_id = dictionary.manifest.dictionary_id.clone();
        if let Some(sidecar) = resolve_sidecar_resource(&dictionary.manifest.mdx_path, &query_key) {
            return self
                .serve_sidecar_resource(&dictionary.manifest.dictionary_id, sidecar)
                .await;
        }

        if !dictionary.mdds.is_empty() {
            match self
                .lookup_resource_span(dictionary.mdds.clone(), &dictionary_id, query_key.clone())
                .await
            {
                Ok(resolved) => {
                    let cache_key = format!(
                        "{}:{}",
                        dictionary.manifest.dictionary_id, resolved.span.key
                    );
                    if let Some(cache) = &self.resource_cache {
                        if let Some(cached) = cache.get(&cache_key) {
                            counter!("mdict_web_resource_cache_hits_total").increment(1);
                            return Ok(cached.into_content(dictionary_id));
                        }
                        counter!("mdict_web_resource_cache_misses_total").increment(1);
                    }

                    let content_type = guess_content_type(&resolved.span.key);
                    if should_materialize_resource(
                        &content_type,
                        resolved.span.len(),
                        self.resource_cache_item_limit,
                    ) {
                        let payload = self
                            .load_buffered_resource(
                                resolved.mdd,
                                &dictionary.manifest.dictionary_id,
                                &dictionary.version_tag,
                                resolved.span,
                                content_type,
                            )
                            .await?;
                        if let Some(cache) = &self.resource_cache {
                            if let ResourceBody::Bytes(bytes) = &payload.body {
                                cache.insert(
                                    cache_key,
                                    CachedResource {
                                        resolved_key: payload.resolved_key.clone(),
                                        content_type: payload.content_type.clone(),
                                        bytes: bytes.clone(),
                                        etag: payload.etag.clone(),
                                        cache_control: payload.cache_control.clone(),
                                    },
                                );
                            }
                        }
                        return Ok(payload);
                    }

                    return self
                        .stream_resource(
                            resolved.mdd,
                            &dictionary.manifest.dictionary_id,
                            &dictionary.version_tag,
                            resolved.span,
                            content_type,
                        )
                        .await;
                }
                Err(error) if error.code == mdict_web_domain::ErrorCode::ResourceNotFound => {}
                Err(error) => return Err(error),
            }
        }

        Err(ServiceError::resource_not_found(&dictionary_id, &query_key))
    }

    async fn serve_sidecar_resource(
        &self,
        dictionary_id: &str,
        sidecar: ResolvedSidecarResource,
    ) -> Result<ResourceContent, ServiceError> {
        let cache_key = format!("{}:{}", dictionary_id, sidecar.resolved_key);
        if let Some(cache) = &self.resource_cache {
            if let Some(cached) = cache.get(&cache_key) {
                counter!("mdict_web_resource_cache_hits_total").increment(1);
                return Ok(cached.into_content(dictionary_id.to_owned()));
            }
            counter!("mdict_web_resource_cache_misses_total").increment(1);
        }

        let payload = self
            .load_buffered_sidecar_resource(dictionary_id, sidecar)
            .await?;
        if let Some(cache) = &self.resource_cache {
            if let ResourceBody::Bytes(bytes) = &payload.body {
                cache.insert(
                    cache_key,
                    CachedResource {
                        resolved_key: payload.resolved_key.clone(),
                        content_type: payload.content_type.clone(),
                        bytes: bytes.clone(),
                        etag: payload.etag.clone(),
                        cache_control: payload.cache_control.clone(),
                    },
                );
            }
        }
        Ok(payload)
    }

    async fn load_buffered_sidecar_resource(
        &self,
        dictionary_id: &str,
        sidecar: ResolvedSidecarResource,
    ) -> Result<ResourceContent, ServiceError> {
        let dictionary_id = dictionary_id.to_owned();
        let resolved_key = sidecar.resolved_key.clone();
        let path_for_read = sidecar.path.clone();
        let dictionary_id_for_read = dictionary_id.clone();
        let raw_bytes = self
            .run_blocking("sidecar_read", move || {
                fs::read(&path_for_read).map_err(|error| {
                    ServiceError::dictionary_unavailable(&dictionary_id_for_read, error.to_string())
                })
            })
            .await?;
        let metadata = fs::metadata(&sidecar.path).map_err(|error| {
            ServiceError::dictionary_unavailable(&dictionary_id, error.to_string())
        })?;
        let modified = metadata
            .modified()
            .unwrap_or(UNIX_EPOCH)
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let version_tag = format!("sidecar:{}:{modified}", metadata.len());
        let content_type = guess_content_type(&resolved_key);
        let bytes = if content_type.essence_str() == "text/css" {
            let css = String::from_utf8_lossy(&raw_bytes);
            Bytes::from(rewrite_css_urls(&dictionary_id, &css))
        } else {
            Bytes::from(raw_bytes)
        };

        Ok(ResourceContent {
            dictionary_id: dictionary_id.clone(),
            resolved_key: resolved_key.clone(),
            content_type: content_type.to_string(),
            body: ResourceBody::Bytes(bytes.clone()),
            content_length: Some(bytes.len() as u64),
            etag: resource_etag(
                &dictionary_id,
                &version_tag,
                &resolved_key,
                RESOURCE_RENDER_VERSION,
            ),
            cache_control: RESOURCE_CACHE_CONTROL.to_owned(),
        })
    }

    async fn lookup_record(
        &self,
        dictionary: Arc<LoadedDictionary>,
        query_key: String,
    ) -> Result<ResolvedMdxRecord, ServiceError> {
        let dictionary_id = dictionary.manifest.dictionary_id.clone();
        self.run_blocking("entry_lookup", move || {
            resolve_mdx_record(dictionary, &dictionary_id, &query_key)
        })
        .await
    }

    async fn lookup_resource_span(
        &self,
        mdds: Vec<Arc<MddFile>>,
        dictionary_id: &str,
        query_key: String,
    ) -> Result<ResolvedMddResourceSpan, ServiceError> {
        let dictionary_id = dictionary_id.to_owned();
        self.run_blocking("resource_lookup", move || {
            let candidates = resource_key_candidates(&query_key);
            for mdd in mdds {
                for candidate in &candidates {
                    match mdd.lookup_span(candidate) {
                        Ok(Some(span)) => {
                            return Ok(ResolvedMddResourceSpan {
                                mdd: mdd.clone(),
                                span,
                            });
                        }
                        Ok(None) => continue,
                        Err(error) => {
                            return Err(ServiceError::dictionary_unavailable(
                                &dictionary_id,
                                error.to_string(),
                            ));
                        }
                    }
                }
            }
            Err(ServiceError::resource_not_found(&dictionary_id, &query_key))
        })
        .await
    }

    async fn load_buffered_resource(
        &self,
        mdd: Arc<MddFile>,
        dictionary_id: &str,
        version_tag: &str,
        span: MddResourceSpan,
        content_type: Mime,
    ) -> Result<ResourceContent, ServiceError> {
        let dictionary_id = dictionary_id.to_owned();
        let version_tag = version_tag.to_owned();
        let span_for_read = span.clone();
        let dictionary_id_for_read = dictionary_id.clone();
        let raw_bytes = self
            .run_blocking("resource_read", move || {
                let mut data = Vec::with_capacity(span_for_read.len() as usize);
                mdd.read_record_span_with(&span_for_read, |chunk| {
                    data.extend_from_slice(chunk);
                    Ok(())
                })
                .map_err(|error| {
                    ServiceError::dictionary_unavailable(&dictionary_id_for_read, error.to_string())
                })?;
                Ok(data)
            })
            .await?;

        let bytes = if content_type.essence_str() == "text/css" {
            let css = String::from_utf8_lossy(&raw_bytes);
            Bytes::from(rewrite_css_urls(&dictionary_id, &css))
        } else {
            Bytes::from(raw_bytes)
        };

        Ok(ResourceContent {
            dictionary_id: dictionary_id.clone(),
            resolved_key: span.key.clone(),
            content_type: content_type.to_string(),
            body: ResourceBody::Bytes(bytes.clone()),
            content_length: Some(bytes.len() as u64),
            etag: resource_etag(
                &dictionary_id,
                &version_tag,
                &span.key,
                RESOURCE_RENDER_VERSION,
            ),
            cache_control: RESOURCE_CACHE_CONTROL.to_owned(),
        })
    }

    async fn stream_resource(
        &self,
        mdd: Arc<MddFile>,
        dictionary_id: &str,
        version_tag: &str,
        span: MddResourceSpan,
        content_type: Mime,
    ) -> Result<ResourceContent, ServiceError> {
        let permit = self
            .io_gate
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| ServiceError::internal("blocking gate is closed"))?;
        let (tx, rx) = mpsc::channel::<Result<Bytes, io::Error>>(8);
        let span_for_task = span.clone();
        let dictionary_id_for_task = dictionary_id.to_owned();

        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            let result = mdd.read_record_span_with(&span_for_task, |chunk| {
                tx.blocking_send(Ok(Bytes::copy_from_slice(chunk)))
                    .map_err(|_| {
                        MdictError::InvalidData("resource stream receiver dropped".to_owned())
                    })
            });
            if let Err(error) = result {
                let _ = tx.blocking_send(Err(io::Error::other(error.to_string())));
                warn!(
                    dictionary_id = %dictionary_id_for_task,
                    error = %error,
                    "resource streaming failed"
                );
            }
        });

        Ok(ResourceContent {
            dictionary_id: dictionary_id.to_owned(),
            resolved_key: span.key.clone(),
            content_type: content_type.to_string(),
            body: ResourceBody::Stream(Box::pin(ReceiverStream::new(rx))),
            content_length: Some(span.len()),
            etag: resource_etag(
                dictionary_id,
                version_tag,
                &span.key,
                RESOURCE_RENDER_VERSION,
            ),
            cache_control: RESOURCE_CACHE_CONTROL.to_owned(),
        })
    }

    async fn run_blocking<T, F>(&self, operation: &'static str, task: F) -> Result<T, ServiceError>
    where
        T: Send + 'static,
        F: FnOnce() -> Result<T, ServiceError> + Send + 'static,
    {
        let permit = self
            .io_gate
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| ServiceError::internal("blocking gate is closed"))?;

        let join = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            task()
        });

        match join.await {
            Ok(result) => result,
            Err(error) => {
                warn!(%operation, %error, "blocking task failed");
                Err(ServiceError::internal(format!(
                    "blocking operation `{operation}` failed"
                )))
            }
        }
    }
}

fn guess_content_type(key: &str) -> Mime {
    mime_guess::from_path(key).first_or_octet_stream()
}

fn should_materialize_resource(
    content_type: &Mime,
    content_len: u64,
    cache_item_limit: usize,
) -> bool {
    content_type.essence_str() == "text/css" || content_len <= cache_item_limit as u64
}

fn dictionary_version_tag(
    mdx_path: &Path,
    mdd_paths: &[PathBuf],
) -> Result<String, std::io::Error> {
    let mdx_meta = fs::metadata(mdx_path)?;
    let mdx_modified = mdx_meta
        .modified()
        .unwrap_or(UNIX_EPOCH)
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();

    let mut version = format!("mdx:{}:{mdx_modified}", mdx_meta.len());

    for (index, path) in mdd_paths.iter().enumerate() {
        let metadata = fs::metadata(path)?;
        let modified = metadata
            .modified()
            .unwrap_or(UNIX_EPOCH)
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        version.push_str(&format!(":mdd{index}:{}:{modified}", metadata.len()));
    }

    Ok(version)
}

fn entry_render_version(entry_script_mode: EntryScriptMode) -> &'static str {
    match entry_script_mode {
        EntryScriptMode::None => ENTRY_RENDER_VERSION_NONE,
        EntryScriptMode::Original => ENTRY_RENDER_VERSION_ORIGINAL,
    }
}

fn entry_etag(dictionary_id: &str, version_tag: &str, key: &str, render_version: &str) -> String {
    strong_etag(&format!(
        "entry:{dictionary_id}:{version_tag}:{key}:{render_version}"
    ))
}

fn resource_etag(
    dictionary_id: &str,
    version_tag: &str,
    key: &str,
    render_version: &str,
) -> String {
    strong_etag(&format!(
        "resource:{dictionary_id}:{version_tag}:{key}:{render_version}"
    ))
}

fn strong_etag(input: &str) -> String {
    let mut hasher = Hasher::new();
    hasher.update(input.as_bytes());
    format!("\"{}\"", hasher.finalize().to_hex())
}

impl CachedResource {
    fn into_content(self, dictionary_id: String) -> ResourceContent {
        ResourceContent {
            dictionary_id,
            resolved_key: self.resolved_key,
            content_type: self.content_type,
            content_length: Some(self.bytes.len() as u64),
            body: ResourceBody::Bytes(self.bytes),
            etag: self.etag,
            cache_control: self.cache_control,
        }
    }
}

fn rewrite_entry_html(
    dictionary_id: &str,
    html: &str,
    entry_script_mode: EntryScriptMode,
) -> Result<(String, bool), ServiceError> {
    let allow_dictionary_scripts = matches!(entry_script_mode, EntryScriptMode::Original);
    let style_blocks_rewritten = rewrite_style_blocks(dictionary_id, html)?;

    let sanitized = rewrite_str(
        &style_blocks_rewritten,
        RewriteStrSettings {
            element_content_handlers: vec![
                element!("script", |element| {
                    if !allow_dictionary_scripts {
                        element.remove();
                    }
                    Ok(())
                }),
                element!("iframe", |element| {
                    element.remove();
                    Ok(())
                }),
                element!("object", |element| {
                    element.remove();
                    Ok(())
                }),
                element!("embed", |element| {
                    element.remove();
                    Ok(())
                }),
                element!("base", |element| {
                    element.remove();
                    Ok(())
                }),
                element!("meta[http-equiv]", |element| {
                    element.remove();
                    Ok(())
                }),
                element!("form", |element| {
                    element.remove();
                    Ok(())
                }),
                element!("*[src]", |element| {
                    Ok(rewrite_attr(
                        dictionary_id,
                        element,
                        "src",
                        entry_script_mode,
                    )?)
                }),
                element!("*[href]", |element| {
                    Ok(rewrite_attr(
                        dictionary_id,
                        element,
                        "href",
                        entry_script_mode,
                    )?)
                }),
                element!("*[poster]", |element| {
                    Ok(rewrite_attr(
                        dictionary_id,
                        element,
                        "poster",
                        entry_script_mode,
                    )?)
                }),
                element!("*[style]", |element| {
                    if let Some(style) = element.get_attribute("style") {
                        let rewritten = rewrite_css_urls(dictionary_id, &style);
                        element
                            .set_attribute("style", &rewritten)
                            .map_err(box_error)?;
                    }
                    Ok(())
                }),
                element!("*[srcset]", |element| {
                    if let Some(srcset) = element.get_attribute("srcset") {
                        let rewritten = rewrite_srcset(dictionary_id, &srcset, entry_script_mode);
                        element
                            .set_attribute("srcset", &rewritten)
                            .map_err(box_error)?;
                    }
                    Ok(())
                }),
            ],
            ..RewriteStrSettings::default()
        },
    )
    .map_err(|error| ServiceError::internal(format!("failed to rewrite entry HTML: {error}")))?;

    if allow_dictionary_scripts {
        wrap_html_document(&sanitized)
    } else {
        let without_event_handlers = strip_event_handlers(&sanitized)?;
        wrap_html_document(&without_event_handlers)
    }
}

fn rewrite_style_blocks(dictionary_id: &str, html: &str) -> Result<String, ServiceError> {
    let pattern = Regex::new(r"(?is)<style(?P<attrs>[^>]*)>(?P<css>.*?)</style>")
        .map_err(|error| ServiceError::internal(format!("invalid style regex: {error}")))?;
    Ok(pattern
        .replace_all(html, |captures: &regex::Captures<'_>| {
            let attrs = captures
                .name("attrs")
                .map(|match_| match_.as_str())
                .unwrap_or("");
            let css = captures
                .name("css")
                .map(|match_| match_.as_str())
                .unwrap_or("");
            format!(
                "<style{attrs}>{}</style>",
                rewrite_css_urls(dictionary_id, css)
            )
        })
        .into_owned())
}

fn strip_event_handlers(html: &str) -> Result<String, ServiceError> {
    let pattern = Regex::new(r#"(?is)\s+on[a-z0-9_-]+\s*=\s*(?:"[^"]*"|'[^']*'|[^\s>]+)"#)
        .map_err(|error| ServiceError::internal(format!("invalid event handler regex: {error}")))?;
    Ok(pattern.replace_all(html, "").into_owned())
}

fn wrap_html_document(html: &str) -> Result<(String, bool), ServiceError> {
    let lower = html.to_ascii_lowercase();
    let document = if lower.contains("<html") {
        html.to_owned()
    } else {
        format!(
            "<!doctype html><html><head><meta charset=\"utf-8\"></head><body>{html}</body></html>"
        )
    };
    let allow_entry_runtime = document.contains("data-audio-href=");
    if allow_entry_runtime {
        Ok((inject_entry_runtime(&document), true))
    } else {
        Ok((document, false))
    }
}

fn inject_entry_runtime(html: &str) -> String {
    let lower = html.to_ascii_lowercase();
    if let Some(index) = lower.rfind("</body>") {
        let mut out = String::with_capacity(html.len() + ENTRY_AUDIO_RUNTIME_SCRIPT.len());
        out.push_str(&html[..index]);
        out.push_str(ENTRY_AUDIO_RUNTIME_SCRIPT);
        out.push_str(&html[index..]);
        return out;
    }
    if let Some(index) = lower.rfind("</html>") {
        let mut out = String::with_capacity(html.len() + ENTRY_AUDIO_RUNTIME_SCRIPT.len());
        out.push_str(&html[..index]);
        out.push_str(ENTRY_AUDIO_RUNTIME_SCRIPT);
        out.push_str(&html[index..]);
        return out;
    }
    let mut out = String::with_capacity(html.len() + ENTRY_AUDIO_RUNTIME_SCRIPT.len());
    out.push_str(html);
    out.push_str(ENTRY_AUDIO_RUNTIME_SCRIPT);
    out
}

fn rewrite_srcset(dictionary_id: &str, srcset: &str, entry_script_mode: EntryScriptMode) -> String {
    srcset
        .split(',')
        .filter_map(|item| {
            let trimmed = item.trim();
            if trimmed.is_empty() {
                return None;
            }
            let mut parts = trimmed.split_whitespace();
            let url = parts.next()?;
            let descriptor = parts.collect::<Vec<_>>().join(" ");
            let rewritten = rewrite_url_value(dictionary_id, "srcset", url, entry_script_mode)?;
            if descriptor.is_empty() {
                Some(rewritten)
            } else {
                Some(format!("{rewritten} {descriptor}"))
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn rewrite_attr(
    dictionary_id: &str,
    element: &mut lol_html::html_content::Element<'_, '_>,
    attribute: &str,
    entry_script_mode: EntryScriptMode,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if let Some(value) = element.get_attribute(attribute) {
        let mark_audio = attribute == "href" && resource_looks_like_audio(&value);
        match rewrite_url_value(dictionary_id, attribute, &value, entry_script_mode) {
            Some(rewritten) => {
                if mark_audio {
                    element
                        .set_attribute("data-audio-href", &rewritten)
                        .map_err(box_error)?;
                    element.remove_attribute(attribute);
                } else {
                    element
                        .set_attribute(attribute, &rewritten)
                        .map_err(box_error)?;
                }
            }
            None => element.remove_attribute(attribute),
        }
    }
    Ok(())
}

fn box_error<E>(error: E) -> Box<dyn std::error::Error + Send + Sync>
where
    E: std::error::Error + Send + Sync + 'static,
{
    Box::new(error)
}

fn rewrite_url_value(
    dictionary_id: &str,
    attribute: &str,
    raw: &str,
    entry_script_mode: EntryScriptMode,
) -> Option<String> {
    let value = raw.trim();
    if value.is_empty() {
        return None;
    }

    if value.starts_with('#') {
        return Some(value.to_owned());
    }

    if attribute == "href" && value.starts_with("entry://") {
        return entry_content_href(dictionary_id, value);
    }

    if attribute == "href" && resource_looks_like_audio(value) {
        return Some(match entry_script_mode {
            EntryScriptMode::None => resource_content_url(dictionary_id, value),
            EntryScriptMode::Original => resource_content_url_opaque(dictionary_id, value),
        });
    }

    if attribute == "href"
        && matches!(entry_script_mode, EntryScriptMode::Original)
        && value.to_ascii_lowercase().starts_with("javascript:")
    {
        return Some(value.to_owned());
    }

    if is_dangerous_scheme(value) {
        return None;
    }

    if has_explicit_safe_scheme(value) {
        return Some(value.to_owned());
    }

    if attribute == "href" && value.starts_with("mailto:") {
        return Some(value.to_owned());
    }

    Some(resource_content_url(dictionary_id, value))
}

fn rewrite_css_urls(dictionary_id: &str, css: &str) -> String {
    let pattern = Regex::new(r#"url\(\s*['"]?(?P<url>[^"'()]+)['"]?\s*\)"#)
        .expect("css url regex should compile");

    pattern
        .replace_all(css, |captures: &regex::Captures<'_>| {
            let raw_url = captures
                .name("url")
                .map(|match_| match_.as_str())
                .unwrap_or("");
            match rewrite_url_value(dictionary_id, "style", raw_url, EntryScriptMode::None) {
                Some(rewritten) => format!("url(\"{rewritten}\")"),
                None => "url(\"\")".to_owned(),
            }
        })
        .into_owned()
}

fn has_explicit_safe_scheme(value: &str) -> bool {
    Url::parse(value)
        .map(|url| {
            matches!(
                url.scheme(),
                "http" | "https" | "data" | "blob" | "mailto" | "tel"
            )
        })
        .unwrap_or(false)
}

fn is_dangerous_scheme(value: &str) -> bool {
    Url::parse(value)
        .map(|url| matches!(url.scheme(), "javascript" | "vbscript" | "file"))
        .unwrap_or(false)
}

fn resource_content_url(dictionary_id: &str, resource_key: &str) -> String {
    format!(
        "/api/v1/dictionaries/{dictionary_id}/resources/content?key={}",
        encode_resource_query_value(resource_key)
    )
}

fn resource_content_url_opaque(dictionary_id: &str, resource_key: &str) -> String {
    format!(
        "/api/v1/dictionaries/{dictionary_id}/resources/content?key={}",
        opaque_percent_encode(resource_key)
    )
}

fn opaque_percent_encode(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len() * 3);
    for byte in raw.as_bytes() {
        push_percent_encoded_byte(&mut out, *byte);
    }
    out
}

fn encode_resource_query_value(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for byte in raw.as_bytes() {
        if is_resource_query_byte_safe(*byte) {
            out.push(char::from(*byte));
        } else {
            push_percent_encoded_byte(&mut out, *byte);
        }
    }
    out
}

fn is_resource_query_byte_safe(byte: u8) -> bool {
    matches!(
        byte,
        b'A'..=b'Z'
            | b'a'..=b'z'
            | b'0'..=b'9'
            | b'-'
            | b'_'
            | b'.'
            | b'~'
            | b'/'
            | b':'
    )
}

fn push_percent_encoded_byte(out: &mut String, byte: u8) {
    out.push('%');
    out.push(char::from(b"0123456789ABCDEF"[(byte >> 4) as usize]));
    out.push(char::from(b"0123456789ABCDEF"[(byte & 0x0F) as usize]));
}

fn entry_content_href(dictionary_id: &str, raw: &str) -> Option<String> {
    let target = raw.strip_prefix("entry://")?;
    let (entry_key_raw, fragment) = match target.split_once('#') {
        Some((entry_key, fragment)) => (entry_key, Some(fragment)),
        None => (target, None),
    };
    if entry_key_raw.is_empty() {
        return None;
    }
    let entry_key = percent_decode_str(entry_key_raw).decode_utf8_lossy();
    let mut href = format!(
        "/api/v1/dictionaries/{dictionary_id}/entries/content?key={}",
        utf8_percent_encode(entry_key.as_ref(), NON_ALPHANUMERIC)
    );
    if let Some(fragment) = fragment.filter(|fragment| !fragment.is_empty()) {
        href.push('#');
        href.push_str(fragment);
    }
    Some(href)
}

fn resource_key_candidates(raw: &str) -> Vec<String> {
    let trimmed = raw.trim_matches(char::is_whitespace).trim_matches('\0');
    let mut seen = HashSet::new();
    let mut out = Vec::new();

    let mut bases = vec![trimmed.to_owned()];
    if let Some(normalized) = resource_lookup_base_from_sound_url(trimmed) {
        bases.push(normalized);
    }

    for base in bases {
        let normalized = base.trim_start_matches(['/', '\\']);
        let variants = [
            base.clone(),
            normalized.to_owned(),
            normalized.replace('/', "\\"),
            normalized.replace('\\', "/"),
        ];

        for variant in variants {
            if variant.is_empty() {
                continue;
            }

            for candidate in [
                variant.clone(),
                format!("\\{variant}"),
                format!("/{variant}"),
            ] {
                if seen.insert(candidate.clone()) {
                    out.push(candidate);
                }
            }
        }
    }

    out
}

fn resolve_sidecar_resource(mdx_path: &Path, raw: &str) -> Option<ResolvedSidecarResource> {
    let root = mdx_path.parent()?;
    let candidates = resource_key_candidates(raw);
    for candidate in candidates {
        let normalized = normalize_sidecar_candidate(&candidate)?;
        let path = root.join(&normalized);
        if path.is_file() {
            return Some(ResolvedSidecarResource {
                resolved_key: normalized,
                path,
            });
        }
    }
    None
}

fn normalize_sidecar_candidate(candidate: &str) -> Option<String> {
    let trimmed = candidate.trim_start_matches(['/', '\\']);
    if trimmed.is_empty() {
        return None;
    }

    let normalized = trimmed.replace('\\', "/");
    let path = Path::new(&normalized);
    let mut safe = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => safe.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    if safe.as_os_str().is_empty() {
        return None;
    }

    let extension = safe
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())?;
    if !matches!(extension.as_str(), "css" | "js") {
        return None;
    }

    Some(safe.to_string_lossy().replace('\\', "/"))
}

fn resource_lookup_base_from_sound_url(raw: &str) -> Option<String> {
    let url = Url::parse(raw).ok()?;
    if url.scheme() != "sound" {
        return None;
    }

    let host = url.host_str()?;
    let path = url.path().trim_start_matches('/');
    Some(if path.is_empty() {
        host.to_owned()
    } else {
        format!("{host}/{path}")
    })
}

fn resource_looks_like_audio(value: &str) -> bool {
    let trimmed = value.trim();
    if trimmed.starts_with("sound://") || has_audio_extension(trimmed) {
        return true;
    }

    parse_url_like(trimmed).is_some_and(|url| {
        has_audio_extension(url.path())
            || url.query_pairs().any(|(key, value)| {
                key == "key"
                    && (value.starts_with("sound://") || has_audio_extension(value.as_ref()))
            })
    })
}

fn has_audio_extension(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    [".mp3", ".wav", ".ogg", ".oga", ".m4a", ".aac", ".flac"]
        .iter()
        .any(|extension| lower.ends_with(extension))
}

fn parse_url_like(raw: &str) -> Option<Url> {
    Url::parse(raw).ok().or_else(|| {
        let base = Url::parse("http://localhost/").ok()?;
        base.join(raw).ok()
    })
}

fn resolve_mdx_record(
    dictionary: Arc<LoadedDictionary>,
    dictionary_id: &str,
    query_key: &str,
) -> Result<ResolvedMdxRecord, ServiceError> {
    let mut current_query = query_key.to_owned();
    let mut matched_key = None;
    let mut redirected_from = None;
    let mut visited = HashSet::new();

    for depth in 0..=MAX_ENTRY_REDIRECT_DEPTH {
        let maybe_record = dictionary.mdx.lookup(&current_query).map_err(|error| {
            ServiceError::dictionary_unavailable(dictionary_id, error.to_string())
        })?;
        let Some(record) = maybe_record else {
            return if redirected_from.is_some() {
                Err(ServiceError::internal(format!(
                    "entry redirect target `{current_query}` was not found in dictionary `{dictionary_id}` for query `{query_key}`"
                )))
            } else {
                Err(ServiceError::entry_not_found(dictionary_id, &current_query))
            };
        };

        if matched_key.is_none() {
            matched_key = Some(record.key.clone());
        }

        let Some(target) = entry_link_target(&record.text) else {
            return Ok(ResolvedMdxRecord {
                matched_key: matched_key.unwrap_or_else(|| record.key.clone()),
                redirected_from,
                record,
            });
        };

        if redirected_from.is_none() {
            redirected_from = Some(record.key.clone());
        }

        if depth == MAX_ENTRY_REDIRECT_DEPTH {
            return Err(ServiceError::internal(format!(
                "entry redirect chain exceeded {MAX_ENTRY_REDIRECT_DEPTH} hops in dictionary `{dictionary_id}` for query `{query_key}`"
            )));
        }

        if !visited.insert(record.key.clone()) {
            return Err(ServiceError::internal(format!(
                "entry redirect loop detected in dictionary `{dictionary_id}` for query `{query_key}`"
            )));
        }

        current_query = target;
    }

    Err(ServiceError::internal(format!(
        "entry redirect resolution failed in dictionary `{dictionary_id}` for query `{query_key}`"
    )))
}

fn entry_link_target(text: &str) -> Option<String> {
    let trimmed = text.trim_matches(|ch: char| ch.is_whitespace() || ch == '\0');
    trimmed
        .strip_prefix(ENTRY_LINK_PREFIX)
        .map(str::trim)
        .filter(|target| !target.is_empty())
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn rewrite_css_urls_maps_relative_paths() {
        let rewritten = rewrite_css_urls("demo", "body { background: url(images/a.png); }");
        assert!(
            rewritten.contains("/api/v1/dictionaries/demo/resources/content?key=images/a.png"),
            "{rewritten}"
        );
    }

    #[test]
    fn resource_content_url_preserves_filename_punctuation() {
        let rewritten = resource_content_url("demo", "\\spkr_demo-icon.v1.png");
        assert_eq!(
            rewritten,
            "/api/v1/dictionaries/demo/resources/content?key=%5Cspkr_demo-icon.v1.png"
        );
    }

    #[test]
    fn resource_key_candidates_include_path_variants() {
        let variants = resource_key_candidates("images/a.png");
        assert!(variants.contains(&"images/a.png".to_owned()));
        assert!(variants.contains(&"images\\a.png".to_owned()));
        assert!(variants.contains(&"\\images/a.png".to_owned()));
        assert!(variants.contains(&"\\images\\a.png".to_owned()));
    }

    #[test]
    fn resource_key_candidates_expand_sound_urls() {
        let variants = resource_key_candidates("sound://media/english/ameProns/laadbuild-up.mp3");
        assert!(variants.contains(&"media/english/ameProns/laadbuild-up.mp3".to_owned()));
        assert!(variants.contains(&"\\media\\english\\ameProns\\laadbuild-up.mp3".to_owned()));
    }

    #[test]
    fn resolve_sidecar_resource_uses_mdx_sibling_css_js_only() {
        let dir = tempdir().expect("temp dir should exist");
        let mdx_path = dir.path().join("demo.mdx");
        fs::write(&mdx_path, b"mdx").expect("mdx placeholder should write");
        fs::write(dir.path().join("lm6.css"), b"body{}").expect("css should write");
        fs::write(dir.path().join("lm6.js"), b"console.log(1);").expect("js should write");
        fs::write(dir.path().join("image.png"), b"png").expect("png should write");

        let css =
            resolve_sidecar_resource(&mdx_path, "lm6.css").expect("css sidecar should resolve");
        assert_eq!(css.resolved_key, "lm6.css");

        let js =
            resolve_sidecar_resource(&mdx_path, "\\lm6.js").expect("js sidecar should resolve");
        assert_eq!(js.resolved_key, "lm6.js");

        assert!(resolve_sidecar_resource(&mdx_path, "image.png").is_none());
        assert!(resolve_sidecar_resource(&mdx_path, "../lm6.css").is_none());
    }

    #[test]
    fn dictionary_version_tag_depends_on_ordered_mdd_paths() {
        let dir = tempdir().expect("temp dir should exist");
        let mdx_path = dir.path().join("demo.mdx");
        let first_mdd = dir.path().join("demo.1.mdd");
        let second_mdd = dir.path().join("demo.2.mdd");
        fs::write(&mdx_path, b"mdx").expect("mdx should write");
        fs::write(&first_mdd, b"first").expect("first mdd should write");
        fs::write(&second_mdd, b"second second").expect("second mdd should write");

        let ordered = dictionary_version_tag(&mdx_path, &[first_mdd.clone(), second_mdd.clone()])
            .expect("version tag should build");
        let reversed = dictionary_version_tag(&mdx_path, &[second_mdd, first_mdd])
            .expect("version tag should build");

        assert_ne!(ordered, reversed);
    }

    #[test]
    fn rewrite_entry_html_rewrites_audio_links_without_runtime() {
        let (rewritten, allow_entry_runtime) = rewrite_entry_html(
            "demo",
            r#"<a class="speaker" href="sound://media/english/ameProns/apple1.mp3"> </a>"#,
            EntryScriptMode::None,
        )
        .expect("entry html should rewrite");
        assert!(
            rewritten.contains(
                r#"class="speaker" data-audio-href="/api/v1/dictionaries/demo/resources/content?key=sound://media/english/ameProns/apple1.mp3""#
            ),
            "{rewritten}"
        );
        assert!(allow_entry_runtime);
        assert!(
            rewritten.contains(ENTRY_AUDIO_RUNTIME_SCRIPT),
            "{rewritten}"
        );
        assert!(
            !rewritten.contains(
                r#"class="speaker" href="/api/v1/dictionaries/demo/resources/content?key=sound://media/english/ameProns/apple1.mp3""#
            ),
            "{rewritten}"
        );
    }

    #[test]
    fn rewrite_entry_html_strips_dictionary_scripts_in_none_mode() {
        let (rewritten, allow_entry_runtime) = rewrite_entry_html(
            "demo",
            r#"<a onclick="play()">play</a><script>play()</script>"#,
            EntryScriptMode::None,
        )
        .expect("entry html should rewrite");
        assert!(!rewritten.contains("<script>"), "{rewritten}");
        assert!(!rewritten.contains("onclick="), "{rewritten}");
        assert!(!allow_entry_runtime);
    }

    #[test]
    fn rewrite_entry_html_preserves_dictionary_scripts_in_original_mode() {
        let (rewritten, allow_entry_runtime) = rewrite_entry_html(
            "demo",
            r#"<a href="javascript:play()" onclick="play()">play</a><script src="js/app.js"></script>"#,
            EntryScriptMode::Original,
        )
        .expect("entry html should rewrite");
        assert!(
            rewritten.contains(r#"href="javascript:play()""#),
            "{rewritten}"
        );
        assert!(rewritten.contains(r#"onclick="play()""#), "{rewritten}");
        assert!(
            rewritten
                .contains(r#"src="/api/v1/dictionaries/demo/resources/content?key=js/app.js""#),
            "{rewritten}"
        );
        assert!(!allow_entry_runtime);
    }

    #[test]
    fn rewrite_entry_html_uses_opaque_audio_urls_in_original_mode() {
        let (rewritten, allow_entry_runtime) = rewrite_entry_html(
            "demo",
            r#"<a class="speaker" href="sound://media/english/ameProns/rust1.mp3">play</a>"#,
            EntryScriptMode::Original,
        )
        .expect("entry html should rewrite");
        assert!(
            rewritten.contains(
                r#"data-audio-href="/api/v1/dictionaries/demo/resources/content?key=%73%6F%75%6E%64%3A%2F%2F%6D%65%64%69%61"#
            ),
            "{rewritten}"
        );
        assert!(allow_entry_runtime);
        assert!(
            rewritten.contains(ENTRY_AUDIO_RUNTIME_SCRIPT),
            "{rewritten}"
        );
        assert!(!rewritten.contains("?key=sound%3A"), "{rewritten}");
    }

    #[test]
    fn rewrite_entry_html_maps_entry_links_to_entry_content() {
        let (rewritten, allow_entry_runtime) = rewrite_entry_html(
            "demo",
            r#"<a class="crossRef" href="entry://building%20block#building-block__1__a">building block</a>"#,
            EntryScriptMode::None,
        )
        .expect("entry html should rewrite");
        assert!(
            rewritten.contains(
                r#"href="/api/v1/dictionaries/demo/entries/content?key=building%20block#building-block__1__a""#
            ),
            "{rewritten}"
        );
        assert!(
            !rewritten.contains(
                r#"href="/api/v1/dictionaries/demo/resources/content?key=entry%3A%2F%2Fbuilding%2520block"#
            ),
            "{rewritten}"
        );
        assert!(!allow_entry_runtime);
    }

    #[test]
    fn entry_link_target_parses_redirect_records() {
        assert_eq!(
            entry_link_target("@@@LINK=build up\r\n"),
            Some("build up".to_owned())
        );
    }

    #[test]
    fn entry_link_target_ignores_normal_html() {
        assert_eq!(
            entry_link_target("<link href=\"LM5style.css\" rel=\"stylesheet\" />"),
            None
        );
    }
}
