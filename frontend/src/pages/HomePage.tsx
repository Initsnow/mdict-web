import * as React from "react";
import { AlertCircle, BookOpen, RefreshCw } from "lucide-react";
import { useSearchParams } from "react-router-dom";
import { EntryViewer } from "@/components/EntryViewer";
import { GlobalSearchBar } from "@/components/GlobalSearchBar";
import { DictionaryCard } from "@/components/DictionaryCard";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Skeleton } from "@/components/ui/skeleton";
import { useSearchSuggest } from "@/hooks/useSearchSuggest";
import { getDictionaries, searchLookup } from "@/lib/api";
import { cn } from "@/lib/utils";
import { isApiError, type DictionarySummary, type LookupResult } from "@/types/api";

function DictionaryListSkeleton() {
  return (
    <div className="space-y-1">
      {Array.from({ length: 4 }).map((_, i) => (
        <div key={i} className="flex items-center gap-3 px-3 py-3">
          <Skeleton className="h-4 w-4 shrink-0 rounded" />
          <Skeleton className="h-4 w-40" />
          <div className="ml-auto flex gap-4">
            <Skeleton className="h-3 w-16" />
            <Skeleton className="h-3 w-20" />
          </div>
        </div>
      ))}
    </div>
  );
}

function SearchResultsSkeleton() {
  return (
    <div className="grid gap-6 lg:grid-cols-[20rem_1fr]">
      <div className="space-y-1">
        <Skeleton className="mx-3 mb-2 h-4 w-32" />
        {Array.from({ length: 5 }).map((_, i) => (
          <div key={i} className="px-3 py-2.5">
            <Skeleton className="h-4 w-28" />
            <Skeleton className="mt-1.5 h-3 w-20" />
          </div>
        ))}
      </div>
      <Skeleton className="h-[40rem] rounded-lg" />
    </div>
  );
}

/* ── Helpers ─────────────────────────────────────────────────────────────────── */

function lookupResultKey(result: LookupResult) {
  return `${result.dictionary_id}:${result.resolved_key}`;
}

/* ── Search result list (left column) ────────────────────────────────────────── */

function SearchResultList({
  results,
  activeResultKey,
  dictionaryLabels,
  onSelect,
}: {
  results: LookupResult[];
  activeResultKey: string | null;
  dictionaryLabels: Record<string, string>;
  onSelect: (result: LookupResult) => void;
}) {
  return (
    <div className="min-w-0">
      <p className="mb-2 px-3 text-xs font-medium text-muted-foreground">
        {results.length} {results.length === 1 ? "result" : "results"}
      </p>
      <ScrollArea className="h-auto max-h-[15rem] lg:h-[40rem]">
        <nav className="space-y-0.5">
          {results.map((result) => {
            const key = lookupResultKey(result);
            const isActive = key === activeResultKey;
            const label =
              dictionaryLabels[result.dictionary_id] ?? result.dictionary_id;

            return (
              <button
                key={key}
                type="button"
                onClick={() => onSelect(result)}
                className={cn(
                  "flex w-full flex-col gap-0.5 rounded-lg px-3 py-2.5 text-left transition-colors",
                  isActive
                    ? "bg-accent text-accent-foreground"
                    : "hover:bg-accent/50"
                )}
              >
                <span className="text-sm font-medium leading-snug">
                  {result.resolved_key}
                </span>
                <span className="flex items-center gap-2 text-xs text-muted-foreground">
                  <span className="truncate">{label}</span>
                  {result.match_type !== "exact" && (
                    <Badge variant="outline" className="shrink-0 px-1.5 py-0 text-[10px] leading-4">
                      {result.match_type}
                    </Badge>
                  )}
                </span>
              </button>
            );
          })}
        </nav>
      </ScrollArea>
    </div>
  );
}

/* ── HomePage ────────────────────────────────────────────────────────────────── */

export function HomePage() {
  const [searchParams, setSearchParams] = useSearchParams();
  const [dictionaries, setDictionaries] = React.useState<DictionarySummary[]>([]);
  const [loading, setLoading] = React.useState(true);
  const [error, setError] = React.useState<string | null>(null);

  const [query, setQuery] = React.useState(searchParams.get("q") ?? "");
  const [results, setResults] = React.useState<LookupResult[]>([]);
  const [lookupLoading, setLookupLoading] = React.useState(false);
  const [lookupError, setLookupError] = React.useState<string | null>(null);
  const [activeResultKey, setActiveResultKey] = React.useState<string | null>(null);

  const fetchDictionaries = React.useCallback(() => {
    setLoading(true);
    setError(null);
    getDictionaries()
      .then((response) => {
        setDictionaries(response.items);
        setLoading(false);
      })
      .catch((err: unknown) => {
        console.error("Failed to load dictionaries", err);
        setError(err instanceof Error ? err.message : "Failed to load dictionaries");
        setLoading(false);
      });
  }, []);

  React.useEffect(() => {
    fetchDictionaries();
  }, [fetchDictionaries]);

  const readyDictionaries = dictionaries.filter((d) => d.status === "ready");
  const dictionaryLabels = Object.fromEntries(
    dictionaries.map((d) => [d.dictionary_id, d.display_name])
  );
  const dictionaryThemeModes = Object.fromEntries(
    dictionaries.map((d) => [d.dictionary_id, d.theme_mode])
  );
  const totalEntries = readyDictionaries.reduce((s, d) => s + d.entry_count, 0);

  const { suggestions, isLoading: suggestLoading } = useSearchSuggest({
    query,
    enabled: !loading && readyDictionaries.length > 0,
  });

  const performLookup = React.useCallback(
    (key: string) => {
      const normalized = key.trim();
      if (!normalized) return;

      setLookupLoading(true);
      setLookupError(null);
      setResults([]);
      setActiveResultKey(null);
      setSearchParams({ q: normalized }, { replace: true });

      searchLookup(normalized)
        .then((response) => {
          setResults(response.items);
          setActiveResultKey(
            response.items.length > 0 ? lookupResultKey(response.items[0]) : null
          );
          setLookupLoading(false);
        })
        .catch((err: unknown) => {
          setLookupLoading(false);
          setResults([]);
          setActiveResultKey(null);
          if (isApiError(err) && err.body.code === "entry_not_found") {
            setLookupError(`No entry found for "${normalized}"`);
          } else {
            setLookupError(err instanceof Error ? err.message : "Lookup failed");
          }
        });
    },
    [setSearchParams]
  );

  React.useEffect(() => {
    if (loading) return;
    const initialQuery = searchParams.get("q");
    if (initialQuery) {
      setQuery(initialQuery);
      performLookup(initialQuery);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [loading]);

  React.useEffect(() => {
    if (!query.trim()) {
      setLookupError(null);
      setResults([]);
      setActiveResultKey(null);
      if (searchParams.has("q")) {
        setSearchParams({}, { replace: true });
      }
    }
  }, [query, searchParams, setSearchParams]);

  const activeResult =
    results.find((r) => lookupResultKey(r) === activeResultKey) ?? results[0] ?? null;

  const hasResults = results.length > 0 || lookupLoading || lookupError;
  const hasSearchQuery = query.trim().length > 0;

  return (
    <div className="min-h-screen">
      <div className="mx-auto max-w-6xl px-4 sm:px-6">
        {/* ── Header / Search ─────────────────────────────────────────── */}
        <header
          className={cn(
            "mx-auto flex flex-col items-center transition-all duration-300",
            hasResults ? "pb-4 pt-6 lg:pb-6 lg:pt-8" : "pb-8 pt-16 sm:pt-24"
          )}
        >
          <div
            className={cn(
              "flex items-center gap-3 transition-all duration-300",
              hasResults ? "mb-3 lg:mb-4" : "mb-6"
            )}
          >
            <img
              src="/apple-touch-icon.png"
              alt=""
              aria-hidden="true"
              className={cn(
                "shrink-0 rounded-[0.4rem] transition-all duration-300",
                hasResults ? "h-4 w-4 lg:h-5 lg:w-5" : "h-7 w-7"
              )}
            />
            <h1
              className={cn(
                "font-semibold tracking-tight transition-all duration-300",
                hasResults ? "text-base lg:text-lg" : "text-2xl sm:text-3xl"
              )}
            >
              MDict Web
            </h1>
          </div>

          {!hasResults && (
            <p className="mb-6 max-w-lg text-center text-sm text-muted-foreground">
              Search across {readyDictionaries.length > 0 ? readyDictionaries.length : "all"} dictionaries
              {totalEntries > 0 && <> &middot; {totalEntries.toLocaleString()} entries</>}
            </p>
          )}

          <div className={cn("w-full transition-all duration-300", hasResults ? "max-w-2xl" : "max-w-xl")}>
            <GlobalSearchBar
              value={query}
              onChange={setQuery}
              onSearch={performLookup}
              onSelect={(item) => {
                setQuery(item.key);
                performLookup(item.key);
              }}
              suggestions={suggestions}
              dictionaryLabels={dictionaryLabels}
              isLoading={suggestLoading}
              autoFocus
            />
          </div>

          {lookupError && (
            <div className="mt-4 flex items-center gap-2 text-sm text-destructive">
              <AlertCircle className="h-4 w-4 shrink-0" />
              <span>{lookupError}</span>
            </div>
          )}
        </header>

        {/* ── Search Results ──────────────────────────────────────────── */}
        {lookupLoading ? (
          <SearchResultsSkeleton />
        ) : activeResult ? (
          <section className="grid gap-6 lg:grid-cols-[20rem_1fr]">
            <SearchResultList
              results={results}
              activeResultKey={activeResultKey}
              dictionaryLabels={dictionaryLabels}
              onSelect={(r) => setActiveResultKey(lookupResultKey(r))}
            />

            <div className="min-w-0 space-y-3">
              <div className="px-1">
                <h2 className="truncate text-lg font-semibold">
                  {activeResult.resolved_key}
                  <span className="ml-2 text-sm font-normal text-muted-foreground">
                    {dictionaryLabels[activeResult.dictionary_id] ?? activeResult.dictionary_id}
                  </span>
                </h2>
              </div>
              <EntryViewer
                contentUrl={activeResult.content_url}
                themeMode={dictionaryThemeModes[activeResult.dictionary_id]}
                className="h-[30rem] lg:h-[40rem]"
              />
            </div>
          </section>
        ) : null}

        {!hasSearchQuery && (
          <section className="mt-2 pb-12">
            <div className="mb-4 flex items-center justify-between">
              <h2 className="text-sm font-medium text-muted-foreground">Dictionaries</h2>
              <Button
                variant="ghost"
                size="sm"
                onClick={fetchDictionaries}
                className="h-7 gap-1.5 px-2 text-xs text-muted-foreground"
              >
                <RefreshCw className="h-3 w-3" />
                Refresh
              </Button>
            </div>

            {loading ? (
              <DictionaryListSkeleton />
            ) : error ? (
              <div className="flex flex-col items-center gap-3 py-16 text-center">
                <AlertCircle className="h-6 w-6 text-destructive" />
                <p className="text-sm">{error}</p>
                <Button variant="outline" size="sm" onClick={fetchDictionaries}>
                  Try again
                </Button>
              </div>
            ) : dictionaries.length === 0 ? (
              <div className="flex flex-col items-center gap-2 py-16 text-center text-muted-foreground">
                <BookOpen className="h-6 w-6" />
                <p className="text-sm">No dictionaries found</p>
                <p className="text-xs">Add MDX or MDD files to your server configuration.</p>
              </div>
            ) : (
              <div className="divide-y divide-border rounded-lg border border-border">
                {dictionaries.map((dictionary) => (
                  <DictionaryCard
                    key={dictionary.dictionary_id}
                    dictionary={dictionary}
                  />
                ))}
              </div>
            )}
          </section>
        )}
      </div>
    </div>
  );
}
