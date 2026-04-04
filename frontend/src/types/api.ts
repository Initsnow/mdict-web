// ── Core domain types ────────────────────────────────────────────────────────

export type DictionaryStatus = "ready" | "loading" | "unavailable" | "error";
export type MatchType = "exact" | "normalized" | "prefix";
export type ThemeMode = "auto" | "dictionary" | "force_auto_dark";

export interface DictionarySummary {
  dictionary_id: string;
  display_name: string;
  description: string;
  source_lang: string;
  target_lang: string;
  entry_count: number;
  has_resources: boolean;
  theme_mode: ThemeMode;
  tags: string[];
  status: DictionaryStatus;
}

export interface DictionaryHeader {
  title: string;
  description: string;
  generated_by_engine_version: string;
  required_engine_version: string;
  encoding_label: string;
}

export interface DictionaryDetail extends DictionarySummary {
  header: DictionaryHeader;
}

export interface SuggestionItem {
  key: string;
  label: string;
  match_type: MatchType;
}

export interface SuggestionResponse {
  dictionary_id: string;
  query: string;
  items: SuggestionItem[];
}

export interface SearchSuggestionItem {
  dictionary_id: string;
  key: string;
  label: string;
  match_type: MatchType;
}

export interface SearchSuggestResponse {
  query: string;
  items: SearchSuggestionItem[];
}

export interface LookupResult {
  dictionary_id: string;
  query_key: string;
  resolved_key: string;
  redirected_from?: string;
  match_type: MatchType;
  has_resources: boolean;
  content_url: string;
  resource_url_template: string;
  etag: string;
}

export interface SearchLookupResponse {
  query_key: string;
  items: LookupResult[];
}

// ── API response wrappers ─────────────────────────────────────────────────────

export interface DictionaryListResponse {
  items: DictionarySummary[];
}

export interface HealthResponse {
  status: string;
}

export interface ReadyzResponse {
  status: "ready" | "degraded";
  ready_dictionaries: number;
  unavailable_dictionaries: string[];
}

// ── Error types ────────────────────────────────────────────────────────────────

export type ApiErrorCode =
  | "bad_request"
  | "dictionary_not_found"
  | "entry_not_found"
  | "resource_not_found"
  | "dictionary_unavailable"
  | "rate_limited"
  | "unauthorized"
  | "internal_error";

export interface ApiErrorBody {
  code: ApiErrorCode;
  message: string;
  request_id: string;
  details?: Record<string, unknown>;
}

export interface ApiError extends Error {
  readonly status: number;
  readonly body: ApiErrorBody;
}

export function createApiError(status: number, body: ApiErrorBody): ApiError {
  const err = new Error(body.message) as ApiError;
  err.name = "ApiError";
  Object.defineProperty(err, "status", { value: status, enumerable: true });
  Object.defineProperty(err, "body", { value: body, enumerable: true });
  return err;
}

export function isApiError(err: unknown): err is ApiError {
  return err instanceof Error && err.name === "ApiError";
}
