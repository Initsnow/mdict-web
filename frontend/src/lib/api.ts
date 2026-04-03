import type {
  DictionaryListResponse,
  DictionaryDetail,
  SuggestionResponse,
  LookupResult,
  SearchLookupResponse,
  SearchSuggestResponse,
  ReadyzResponse,
  ApiErrorBody,
} from "@/types/api";
import { createApiError } from "@/types/api";

const BASE = "/api/v1";

async function request<T>(url: string, init?: RequestInit): Promise<T> {
  const res = await fetch(url, {
    headers: { Accept: "application/json" },
    ...init,
  });

  if (!res.ok) {
    let body: ApiErrorBody;
    try {
      const json = (await res.json()) as { error: ApiErrorBody };
      body = json.error;
    } catch {
      body = {
        code: "internal_error",
        message: `HTTP ${res.status}`,
        request_id: "",
      };
    }
    throw createApiError(res.status, body);
  }

  return res.json() as Promise<T>;
}

// ── Endpoints ─────────────────────────────────────────────────────────────────

export async function getDictionaries(): Promise<DictionaryListResponse> {
  return request<DictionaryListResponse>(`${BASE}/dictionaries`);
}

export async function getDictionary(id: string): Promise<DictionaryDetail> {
  return request<DictionaryDetail>(`${BASE}/dictionaries/${encodeURIComponent(id)}`);
}

export async function searchSuggest(
  q: string,
  limit = 12,
  dictionaryIds: string[] = []
): Promise<SearchSuggestResponse> {
  const params = new URLSearchParams({ q, limit: String(limit) });
  for (const dictionaryId of dictionaryIds) {
    params.append("dictionary_id", dictionaryId);
  }
  return request<SearchSuggestResponse>(`${BASE}/search/suggest?${params}`);
}

export async function suggest(
  dictionaryId: string,
  q: string,
  limit = 20
): Promise<SuggestionResponse> {
  const params = new URLSearchParams({ q, limit: String(limit) });
  return request<SuggestionResponse>(
    `${BASE}/dictionaries/${encodeURIComponent(dictionaryId)}/suggest?${params}`
  );
}

export async function lookup(
  dictionaryId: string,
  key: string
): Promise<LookupResult> {
  const params = new URLSearchParams({ key });
  return request<LookupResult>(
    `${BASE}/dictionaries/${encodeURIComponent(dictionaryId)}/entries/lookup?${params}`
  );
}

export async function searchLookup(
  key: string,
  dictionaryIds: string[] = []
): Promise<SearchLookupResponse> {
  const params = new URLSearchParams({ key });
  for (const dictionaryId of dictionaryIds) {
    params.append("dictionary_id", dictionaryId);
  }
  return request<SearchLookupResponse>(`${BASE}/search/lookup?${params}`);
}

export async function getReadyz(): Promise<ReadyzResponse> {
  return request<ReadyzResponse>("/readyz");
}
