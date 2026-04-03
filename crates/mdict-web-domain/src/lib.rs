use std::collections::BTreeMap;
use std::pin::Pin;

use bytes::Bytes;
use futures_core::Stream;
use http::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DictionaryStatus {
    Ready,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LookupMatchType {
    Exact,
    Normalized,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SuggestionMatchType {
    Prefix,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    BadRequest,
    DictionaryNotFound,
    EntryNotFound,
    ResourceNotFound,
    DictionaryUnavailable,
    RateLimited,
    Unauthorized,
    InternalError,
}

impl ErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::BadRequest => "bad_request",
            Self::DictionaryNotFound => "dictionary_not_found",
            Self::EntryNotFound => "entry_not_found",
            Self::ResourceNotFound => "resource_not_found",
            Self::DictionaryUnavailable => "dictionary_unavailable",
            Self::RateLimited => "rate_limited",
            Self::Unauthorized => "unauthorized",
            Self::InternalError => "internal_error",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorEnvelope {
    pub error: ErrorPayload,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorPayload {
    pub code: String,
    pub message: String,
    pub request_id: String,
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    pub details: Map<String, Value>,
}

#[derive(Debug, Clone)]
pub struct ServiceError {
    pub status: StatusCode,
    pub code: ErrorCode,
    pub message: String,
    pub details: Map<String, Value>,
}

impl ServiceError {
    pub fn new(status: StatusCode, code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            status,
            code,
            message: message.into(),
            details: Map::new(),
        }
    }

    pub fn with_detail(mut self, key: impl Into<String>, value: impl Serialize) -> Self {
        if let Ok(value) = serde_json::to_value(value) {
            self.details.insert(key.into(), value);
        }
        self
    }

    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, ErrorCode::BadRequest, message)
    }

    pub fn dictionary_not_found(dictionary_id: &str) -> Self {
        Self::new(
            StatusCode::NOT_FOUND,
            ErrorCode::DictionaryNotFound,
            format!("dictionary `{dictionary_id}` was not found"),
        )
    }

    pub fn entry_not_found(dictionary_id: &str, key: &str) -> Self {
        Self::new(
            StatusCode::NOT_FOUND,
            ErrorCode::EntryNotFound,
            format!("entry `{key}` was not found in dictionary `{dictionary_id}`"),
        )
    }

    pub fn entry_not_found_in_scope(key: &str) -> Self {
        Self::new(
            StatusCode::NOT_FOUND,
            ErrorCode::EntryNotFound,
            format!("entry `{key}` was not found in the selected dictionaries"),
        )
    }

    pub fn resource_not_found(dictionary_id: &str, key: &str) -> Self {
        Self::new(
            StatusCode::NOT_FOUND,
            ErrorCode::ResourceNotFound,
            format!("resource `{key}` was not found in dictionary `{dictionary_id}`"),
        )
    }

    pub fn dictionary_unavailable(dictionary_id: &str, message: impl Into<String>) -> Self {
        Self::new(
            StatusCode::SERVICE_UNAVAILABLE,
            ErrorCode::DictionaryUnavailable,
            format!(
                "dictionary `{dictionary_id}` is unavailable: {}",
                message.into()
            ),
        )
    }

    pub fn rate_limited(message: impl Into<String>) -> Self {
        Self::new(
            StatusCode::TOO_MANY_REQUESTS,
            ErrorCode::RateLimited,
            message,
        )
    }

    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self::new(StatusCode::UNAUTHORIZED, ErrorCode::Unauthorized, message)
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            ErrorCode::InternalError,
            message,
        )
    }

    pub fn into_envelope(self, request_id: impl Into<String>) -> ErrorEnvelope {
        ErrorEnvelope {
            error: ErrorPayload {
                code: self.code.as_str().to_owned(),
                message: self.message,
                request_id: request_id.into(),
                details: self.details,
            },
        }
    }
}

impl std::fmt::Display for ServiceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.message, self.code.as_str())
    }
}

impl std::error::Error for ServiceError {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DictionaryHeaderInfo {
    pub title: Option<String>,
    pub description: Option<String>,
    pub generated_by_engine_version: String,
    pub required_engine_version: String,
    pub encoding_label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DictionarySummary {
    pub dictionary_id: String,
    pub display_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_lang: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_lang: Option<String>,
    pub entry_count: u64,
    pub has_resources: bool,
    pub tags: Vec<String>,
    pub status: DictionaryStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DictionaryDetail {
    #[serde(flatten)]
    pub summary: DictionarySummary,
    pub header: DictionaryHeaderInfo,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DictionaryListResponse {
    pub items: Vec<DictionarySummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuggestionItem {
    pub key: String,
    pub label: String,
    pub match_type: SuggestionMatchType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuggestResponse {
    pub dictionary_id: String,
    pub query: String,
    pub items: Vec<SuggestionItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchSuggestionItem {
    pub dictionary_id: String,
    pub key: String,
    pub label: String,
    pub match_type: SuggestionMatchType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchSuggestResponse {
    pub query: String,
    pub items: Vec<SearchSuggestionItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LookupResult {
    pub dictionary_id: String,
    pub query_key: String,
    pub resolved_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub redirected_from: Option<String>,
    pub match_type: LookupMatchType,
    pub has_resources: bool,
    pub content_url: String,
    pub resource_url_template: String,
    pub etag: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchLookupResponse {
    pub query_key: String,
    pub items: Vec<LookupResult>,
}

#[derive(Debug, Clone)]
pub struct RenderedEntryContent {
    pub dictionary_id: String,
    pub resolved_key: String,
    pub html: String,
    pub etag: String,
    pub cache_control: String,
}

pub type ByteStream = Pin<Box<dyn Stream<Item = Result<Bytes, std::io::Error>> + Send>>;

pub enum ResourceBody {
    Bytes(Bytes),
    Stream(ByteStream),
}

pub struct ResourceContent {
    pub dictionary_id: String,
    pub resolved_key: String,
    pub content_type: String,
    pub body: ResourceBody,
    pub content_length: Option<u64>,
    pub etag: String,
    pub cache_control: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadinessResponse {
    pub status: String,
    pub ready_dictionaries: usize,
    pub unavailable_dictionaries: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReloadResponse {
    pub status: String,
    pub dictionary_count: usize,
}

#[derive(Debug, Error)]
pub enum DomainError {
    #[error("{0}")]
    Message(String),
}
