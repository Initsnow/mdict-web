import * as React from "react";
import { suggest } from "@/lib/api";
import { useDebounce } from "./useDebounce";
import type { SuggestionItem } from "@/types/api";
import { isApiError } from "@/types/api";

interface UseSuggestOptions {
  dictionaryId: string;
  query: string;
  limit?: number;
  debounceMs?: number;
  enabled?: boolean;
}

interface UseSuggestResult {
  suggestions: SuggestionItem[];
  isLoading: boolean;
  error: string | null;
}

export function useSuggest({
  dictionaryId,
  query,
  limit = 20,
  debounceMs = 200,
  enabled = true,
}: UseSuggestOptions): UseSuggestResult {
  const [suggestions, setSuggestions] = React.useState<SuggestionItem[]>([]);
  const [isLoading, setIsLoading] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);

  const debouncedQuery = useDebounce(query, debounceMs);

  React.useEffect(() => {
    if (!enabled || !debouncedQuery.trim() || !dictionaryId) {
      setSuggestions([]);
      setIsLoading(false);
      return;
    }

    let cancelled = false;
    setIsLoading(true);
    setError(null);

    suggest(dictionaryId, debouncedQuery, limit)
      .then((res) => {
        if (!cancelled) {
          setSuggestions(res.items);
          setIsLoading(false);
        }
      })
      .catch((err: unknown) => {
        if (!cancelled) {
          setIsLoading(false);
          if (isApiError(err)) {
            setError(err.body.message);
          } else {
            setError("Suggestion failed");
          }
          setSuggestions([]);
        }
      });

    return () => {
      cancelled = true;
    };
  }, [dictionaryId, debouncedQuery, limit, enabled]);

  return { suggestions, isLoading, error };
}
