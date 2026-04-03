import * as React from "react";
import { searchSuggest } from "@/lib/api";
import { isApiError, type SearchSuggestionItem } from "@/types/api";
import { useDebounce } from "./useDebounce";

const EMPTY_DICTIONARY_IDS: string[] = [];

interface UseSearchSuggestOptions {
  query: string;
  limit?: number;
  debounceMs?: number;
  enabled?: boolean;
  dictionaryIds?: string[];
}

interface UseSearchSuggestResult {
  suggestions: SearchSuggestionItem[];
  isLoading: boolean;
  error: string | null;
}

export function useSearchSuggest({
  query,
  limit = 12,
  debounceMs = 180,
  enabled = true,
  dictionaryIds = EMPTY_DICTIONARY_IDS,
}: UseSearchSuggestOptions): UseSearchSuggestResult {
  const [suggestions, setSuggestions] = React.useState<SearchSuggestionItem[]>([]);
  const [isLoading, setIsLoading] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);

  const debouncedQuery = useDebounce(query, debounceMs);
  const dictionaryScope = dictionaryIds.join("\u001f");

  React.useEffect(() => {
    if (!enabled || !debouncedQuery.trim()) {
      setSuggestions([]);
      setIsLoading(false);
      return;
    }

    let cancelled = false;
    setIsLoading(true);
    setError(null);

    searchSuggest(debouncedQuery, limit, dictionaryIds)
      .then((res) => {
        if (!cancelled) {
          setSuggestions(res.items);
          setIsLoading(false);
        }
      })
      .catch((err: unknown) => {
        if (!cancelled) {
          setIsLoading(false);
          setSuggestions([]);
          if (isApiError(err)) {
            setError(err.body.message);
          } else {
            setError("Suggestion failed");
          }
        }
      });

    return () => {
      cancelled = true;
    };
  }, [debouncedQuery, dictionaryIds, dictionaryScope, enabled, limit]);

  return { suggestions, isLoading, error };
}
