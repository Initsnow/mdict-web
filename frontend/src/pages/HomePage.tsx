import * as React from "react";
import { AlertCircle, BookOpen, ChevronRight, Layers3, RefreshCw, Sparkles } from "lucide-react";
import { Link, useSearchParams } from "react-router-dom";
import { EntryViewer } from "@/components/EntryViewer";
import { GlobalSearchBar } from "@/components/GlobalSearchBar";
import { DictionaryCard } from "@/components/DictionaryCard";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from "@/components/ui/card";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Separator } from "@/components/ui/separator";
import { Skeleton } from "@/components/ui/skeleton";
import { useSearchSuggest } from "@/hooks/useSearchSuggest";
import { getDictionaries, searchLookup } from "@/lib/api";
import { cn } from "@/lib/utils";
import { isApiError, type DictionarySummary, type LookupResult } from "@/types/api";

function DictionaryGridSkeleton() {
  return (
    <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
      {Array.from({ length: 6 }).map((_, index) => (
        <div key={index} className="rounded-xl border border-border p-5">
          <div className="flex items-start gap-3">
            <Skeleton className="h-10 w-10 shrink-0 rounded-lg" />
            <div className="flex-1 space-y-2">
              <Skeleton className="h-4 w-3/4" />
              <Skeleton className="h-3 w-full" />
              <Skeleton className="h-3 w-2/3" />
            </div>
          </div>
          <div className="mt-4 flex gap-2">
            <Skeleton className="h-5 w-14 rounded-full" />
            <Skeleton className="h-5 w-20 rounded-full" />
          </div>
        </div>
      ))}
    </div>
  );
}

function SearchResultsSkeleton() {
  return (
    <div className="grid gap-5 lg:grid-cols-[minmax(0,23rem)_minmax(0,1fr)]">
      <Card className="border-border/70">
        <CardHeader>
          <Skeleton className="h-5 w-32" />
          <Skeleton className="h-4 w-48" />
        </CardHeader>
        <CardContent className="space-y-3">
          {Array.from({ length: 4 }).map((_, index) => (
            <div key={index} className="rounded-xl border border-border/80 p-3">
              <Skeleton className="h-4 w-24" />
              <Skeleton className="mt-2 h-5 w-32" />
              <Skeleton className="mt-2 h-3 w-full" />
            </div>
          ))}
        </CardContent>
      </Card>
      <div className="rounded-3xl border border-border/70 bg-card/60 p-3">
        <Skeleton className="h-[38rem] w-full rounded-2xl" />
      </div>
    </div>
  );
}

function lookupResultKey(result: LookupResult) {
  return `${result.dictionary_id}:${result.resolved_key}`;
}

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
    <Card className="border-border/70 bg-card/95 shadow-[0_20px_60px_-35px_rgba(15,23,42,0.4)]">
      <CardHeader className="gap-2">
        <CardTitle>Matched Dictionaries</CardTitle>
        <CardDescription>
          Exact and normalized hits from all ready dictionaries.
        </CardDescription>
      </CardHeader>
      <CardContent className="px-0">
        <ScrollArea className="h-[38rem] px-4">
          <div className="space-y-2 pb-4">
            {results.map((result) => {
              const key = lookupResultKey(result);
              const isActive = key === activeResultKey;
              const dictionaryLabel =
                dictionaryLabels[result.dictionary_id] ?? result.dictionary_id;

              return (
                <div
                  key={key}
                  onClick={() => onSelect(result)}
                  onKeyDown={(event) => {
                    if (event.key === "Enter" || event.key === " ") {
                      event.preventDefault();
                      onSelect(result);
                    }
                  }}
                  role="button"
                  tabIndex={0}
                  className={cn(
                    "w-full rounded-2xl border p-3 text-left transition-all",
                    isActive
                      ? "border-primary/40 bg-primary/5 shadow-[0_18px_40px_-30px_rgba(37,99,235,0.5)]"
                      : "border-border/80 bg-background/70 hover:border-border hover:bg-accent/40"
                  )}
                >
                  <div className="flex items-start justify-between gap-3">
                    <div className="min-w-0">
                      <p className="truncate text-xs font-semibold uppercase tracking-[0.18em] text-muted-foreground">
                        {dictionaryLabel}
                      </p>
                      <p className="mt-1 truncate text-base font-semibold">
                        {result.resolved_key}
                      </p>
                      {result.query_key !== result.resolved_key && (
                        <p className="mt-1 text-xs text-muted-foreground">
                          searched as {result.query_key}
                        </p>
                      )}
                    </div>
                    <Badge variant="secondary" className="shrink-0">
                      {result.match_type}
                    </Badge>
                  </div>

                  <div className="mt-3 flex items-center justify-between gap-3">
                    <p className="text-xs text-muted-foreground">
                      {result.has_resources ? "MDX + MDD resources available" : "MDX entry only"}
                    </p>
                    <Link
                      to={`/dictionaries/${encodeURIComponent(result.dictionary_id)}?q=${encodeURIComponent(result.resolved_key)}`}
                      className="inline-flex items-center gap-1 text-xs font-medium text-primary"
                      onClick={(event) => event.stopPropagation()}
                    >
                      Open
                      <ChevronRight className="h-3.5 w-3.5" />
                    </Link>
                  </div>
                </div>
              );
            })}
          </div>
        </ScrollArea>
      </CardContent>
    </Card>
  );
}

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
        setError(err instanceof Error ? err.message : "Failed to load dictionaries");
        setLoading(false);
      });
  }, []);

  React.useEffect(() => {
    fetchDictionaries();
  }, [fetchDictionaries]);

  const readyDictionaries = dictionaries.filter((dictionary) => dictionary.status === "ready");
  const dictionaryLabels = Object.fromEntries(
    dictionaries.map((dictionary) => [dictionary.dictionary_id, dictionary.display_name])
  );
  const totalEntries = readyDictionaries.reduce(
    (sum, dictionary) => sum + dictionary.entry_count,
    0
  );

  const { suggestions, isLoading: suggestLoading } = useSearchSuggest({
    query,
    enabled: !loading && readyDictionaries.length > 0,
  });

  const performLookup = React.useCallback(
    (key: string) => {
      const normalized = key.trim();
      if (!normalized) {
        return;
      }

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
    if (loading) {
      return;
    }

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
    results.find((result) => lookupResultKey(result) === activeResultKey) ?? results[0] ?? null;

  return (
    <div className="min-h-screen bg-[radial-gradient(circle_at_top,rgba(15,23,42,0.08),transparent_42%),linear-gradient(180deg,rgba(255,255,255,0.96),rgba(248,250,252,0.92))]">
      <div className="mx-auto max-w-7xl px-4 py-8 sm:px-6 lg:px-8">
        <div className="grid gap-8">
          <section className="overflow-hidden rounded-[2rem] border border-border/70 bg-background/85 p-6 shadow-[0_28px_90px_-50px_rgba(15,23,42,0.45)] backdrop-blur">
            <div className="grid gap-6 lg:grid-cols-[minmax(0,1.15fr)_minmax(18rem,0.85fr)] lg:items-end">
              <div className="space-y-5">
                <div className="inline-flex items-center gap-2 rounded-full border border-border/70 bg-muted/60 px-3 py-1 text-xs font-medium text-muted-foreground">
                  <Sparkles className="h-3.5 w-3.5" />
                  Search-first dictionary workspace
                </div>

                <div className="space-y-3">
                  <div className="flex items-center gap-3">
                    <div className="flex h-12 w-12 items-center justify-center rounded-2xl bg-primary text-primary-foreground shadow-sm">
                      <BookOpen className="h-6 w-6" />
                    </div>
                    <div>
                      <h1 className="text-3xl font-semibold tracking-tight sm:text-4xl">
                        Search every ready dictionary from one box
                      </h1>
                    </div>
                  </div>
                  <p className="max-w-3xl text-sm leading-6 text-muted-foreground sm:text-base">
                    Multi-dictionary lookup lands here first. Pick a result on the left,
                    preview the rewritten entry on the right, then jump into the dedicated
                    dictionary page if you need focused browsing.
                  </p>
                </div>

                <div className="space-y-3">
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
                  <div className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
                    <Badge variant="secondary" className="rounded-full px-2.5 py-1">
                      {readyDictionaries.length} ready dictionaries
                    </Badge>
                    <Badge variant="outline" className="rounded-full px-2.5 py-1">
                      {totalEntries.toLocaleString()} indexed entries
                    </Badge>
                    <span>Press Enter to search all dictionaries.</span>
                  </div>
                </div>

                {lookupError && (
                  <div className="flex items-start gap-3 rounded-2xl border border-destructive/25 bg-destructive/8 px-4 py-3 text-sm text-destructive">
                    <AlertCircle className="mt-0.5 h-4 w-4 shrink-0" />
                    <div>
                      <p className="font-medium">{lookupError}</p>
                      <p className="mt-1 text-destructive/80">
                        Try a different spelling or pick a suggestion from another dictionary.
                      </p>
                    </div>
                  </div>
                )}
              </div>

              <Card className="border-border/70 bg-card/90">
                <CardHeader>
                  <CardTitle className="flex items-center gap-2">
                    <Layers3 className="h-4 w-4 text-muted-foreground" />
                    Library Snapshot
                  </CardTitle>
                  <CardDescription>
                    The homepage now treats your catalog as a shared search scope.
                  </CardDescription>
                </CardHeader>
                <CardContent className="space-y-4">
                  <div className="grid gap-3 sm:grid-cols-2">
                    <div className="rounded-2xl border border-border/70 bg-muted/40 p-4">
                      <p className="text-xs font-semibold uppercase tracking-[0.2em] text-muted-foreground">
                        Catalog
                      </p>
                      <p className="mt-2 text-2xl font-semibold">
                        {dictionaries.length.toLocaleString()}
                      </p>
                      <p className="mt-1 text-xs text-muted-foreground">
                        loaded dictionaries
                      </p>
                    </div>
                    <div className="rounded-2xl border border-border/70 bg-muted/40 p-4">
                      <p className="text-xs font-semibold uppercase tracking-[0.2em] text-muted-foreground">
                        Search Scope
                      </p>
                      <p className="mt-2 text-2xl font-semibold">
                        {readyDictionaries.length.toLocaleString()}
                      </p>
                      <p className="mt-1 text-xs text-muted-foreground">
                        ready for global lookup
                      </p>
                    </div>
                  </div>

                  <Separator />

                  <div className="flex items-center justify-between gap-3">
                    <div>
                      <p className="text-sm font-medium">Dictionary cards stay available</p>
                      <p className="text-xs text-muted-foreground">
                        Use them when you want a single-dictionary workflow.
                      </p>
                    </div>
                    <Button
                      variant="ghost"
                      size="sm"
                      onClick={fetchDictionaries}
                      className="shrink-0"
                    >
                      <RefreshCw className="h-3.5 w-3.5" />
                      Refresh
                    </Button>
                  </div>
                </CardContent>
              </Card>
            </div>
          </section>

          {lookupLoading ? (
            <SearchResultsSkeleton />
          ) : activeResult ? (
            <section className="grid gap-5 lg:grid-cols-[minmax(0,23rem)_minmax(0,1fr)]">
              <SearchResultList
                results={results}
                activeResultKey={activeResultKey}
                dictionaryLabels={dictionaryLabels}
                onSelect={(result) => setActiveResultKey(lookupResultKey(result))}
              />

              <div className="space-y-3">
                <div className="flex flex-wrap items-center justify-between gap-3 px-1">
                  <div>
                    <p className="text-xs font-semibold uppercase tracking-[0.2em] text-muted-foreground">
                      Preview
                    </p>
                    <h2 className="text-xl font-semibold">
                      {dictionaryLabels[activeResult.dictionary_id] ?? activeResult.dictionary_id}
                    </h2>
                  </div>
                  <Link
                    to={`/dictionaries/${encodeURIComponent(activeResult.dictionary_id)}?q=${encodeURIComponent(activeResult.resolved_key)}`}
                    className="inline-flex items-center gap-1 text-sm font-medium text-primary"
                  >
                    Open dedicated page
                    <ChevronRight className="h-4 w-4" />
                  </Link>
                </div>
                <EntryViewer contentUrl={activeResult.content_url} className="h-[42rem]" />
              </div>
            </section>
          ) : null}

          <section className="space-y-4">
            <div className="flex items-center justify-between gap-3">
              <div>
                <h2 className="text-sm font-semibold uppercase tracking-[0.2em] text-muted-foreground">
                  Dictionaries
                </h2>
                <p className="mt-1 text-sm text-muted-foreground">
                  Browse the catalog directly when you want a single dictionary at a time.
                </p>
              </div>
            </div>

            {loading ? (
              <DictionaryGridSkeleton />
            ) : error ? (
              <div className="flex flex-col items-center justify-center gap-4 rounded-xl border border-dashed border-border bg-muted/20 py-16 text-center">
                <AlertCircle className="h-8 w-8 text-destructive" />
                <div>
                  <p className="text-sm font-medium">Failed to load dictionaries</p>
                  <p className="mt-1 text-xs text-muted-foreground">{error}</p>
                </div>
                <Button variant="outline" size="sm" onClick={fetchDictionaries}>
                  Try again
                </Button>
              </div>
            ) : dictionaries.length === 0 ? (
              <div className="flex flex-col items-center justify-center gap-3 rounded-xl border border-dashed border-border bg-muted/20 py-16 text-center">
                <BookOpen className="h-8 w-8 text-muted-foreground" />
                <div>
                  <p className="text-sm font-medium">No dictionaries found</p>
                  <p className="mt-1 text-xs text-muted-foreground">
                    Add MDX or MDD files to your server configuration.
                  </p>
                </div>
              </div>
            ) : (
              <div className="grid gap-4 sm:grid-cols-2 xl:grid-cols-3">
                {dictionaries.map((dictionary) => (
                  <DictionaryCard
                    key={dictionary.dictionary_id}
                    dictionary={dictionary}
                    className="h-full"
                  />
                ))}
              </div>
            )}
          </section>
        </div>
      </div>
    </div>
  );
}
