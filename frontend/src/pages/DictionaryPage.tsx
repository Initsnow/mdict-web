import * as React from "react";
import { useParams, useNavigate, useSearchParams, Link } from "react-router-dom";
import {
  ArrowLeft,
  BookOpen,
  Globe,
  Hash,
  Info,
  AlertCircle,
  ChevronDown,
  ChevronUp,
} from "lucide-react";
import { getDictionary, lookup } from "@/lib/api";
import { isApiError } from "@/types/api";
import type { DictionaryDetail, LookupResult } from "@/types/api";
import { SearchBar } from "@/components/SearchBar";
import { EntryViewer } from "@/components/EntryViewer";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { Separator } from "@/components/ui/separator";
import { useSuggest } from "@/hooks/useSuggest";

// ── Sidebar dictionary metadata ────────────────────────────────────────────────

function DictionaryMeta({
  dictionary,
  collapsed,
  onToggle,
}: {
  dictionary: DictionaryDetail;
  collapsed: boolean;
  onToggle: () => void;
}) {
  return (
    <div className="rounded-xl border border-border bg-card overflow-hidden">
      <button
        type="button"
        onClick={onToggle}
        className="flex w-full items-center justify-between px-4 py-3 text-sm font-medium hover:bg-accent transition-colors"
      >
        <span className="flex items-center gap-2">
          <Info className="h-3.5 w-3.5 text-muted-foreground" />
          Dictionary Info
        </span>
        {collapsed ? (
          <ChevronDown className="h-3.5 w-3.5 text-muted-foreground" />
        ) : (
          <ChevronUp className="h-3.5 w-3.5 text-muted-foreground" />
        )}
      </button>

      {!collapsed && (
        <>
          <Separator />
          <div className="p-4 space-y-3">
            <div>
              <p className="text-xs font-medium text-muted-foreground uppercase tracking-wide mb-1">
                Name
              </p>
              <p className="text-sm font-medium">{dictionary.display_name}</p>
            </div>

            {dictionary.description && (
              <div>
                <p className="text-xs font-medium text-muted-foreground uppercase tracking-wide mb-1">
                  Description
                </p>
                <p className="text-xs text-muted-foreground leading-relaxed">
                  {dictionary.description}
                </p>
              </div>
            )}

            <div className="flex items-center gap-4">
              <div>
                <p className="text-xs font-medium text-muted-foreground uppercase tracking-wide mb-1">
                  Language
                </p>
                <span className="inline-flex items-center gap-1 text-xs">
                  <Globe className="h-3 w-3" />
                  {dictionary.source_lang}
                  {dictionary.target_lang !== dictionary.source_lang &&
                    ` → ${dictionary.target_lang}`}
                </span>
              </div>
              <div>
                <p className="text-xs font-medium text-muted-foreground uppercase tracking-wide mb-1">
                  Entries
                </p>
                <span className="inline-flex items-center gap-1 text-xs">
                  <Hash className="h-3 w-3" />
                  {dictionary.entry_count.toLocaleString()}
                </span>
              </div>
            </div>

            {dictionary.tags.length > 0 && (
              <div>
                <p className="text-xs font-medium text-muted-foreground uppercase tracking-wide mb-1.5">
                  Tags
                </p>
                <div className="flex flex-wrap gap-1">
                  {dictionary.tags.map((tag) => (
                    <Badge key={tag} variant="outline" className="text-xs">
                      {tag}
                    </Badge>
                  ))}
                </div>
              </div>
            )}
          </div>
        </>
      )}
    </div>
  );
}

// ── Entry result header ─────────────────────────────────────────────────────

function EntryHeader({ result }: { result: LookupResult }) {
  return (
    <div className="flex items-center gap-3 px-1">
      <div className="min-w-0 flex-1">
        <h2 className="text-xl font-bold tracking-tight truncate">
          {result.resolved_key}
        </h2>
        {result.query_key !== result.resolved_key && (
          <p className="text-xs text-muted-foreground mt-0.5">
            Showing result for &ldquo;{result.resolved_key}&rdquo; (searched: &ldquo;{result.query_key}&rdquo;)
          </p>
        )}
      </div>
      {result.match_type === "normalized" && (
        <Badge variant="secondary" className="shrink-0 bg-amber-100 text-amber-700 dark:bg-amber-900/30 dark:text-amber-400">normalized</Badge>
      )}
    </div>
  );
}

// ── Main page ───────────────────────────────────────────────────────────────

export function DictionaryPage() {
  const { dictionaryId } = useParams<{ dictionaryId: string }>();
  const navigate = useNavigate();
  const [searchParams, setSearchParams] = useSearchParams();

  const [dictionary, setDictionary] = React.useState<DictionaryDetail | null>(null);
  const [dictLoading, setDictLoading] = React.useState(true);
  const [dictError, setDictError] = React.useState<string | null>(null);

  const [query, setQuery] = React.useState(searchParams.get("q") ?? "");
  const [lookupResult, setLookupResult] = React.useState<LookupResult | null>(null);
  const [lookupLoading, setLookupLoading] = React.useState(false);
  const [lookupError, setLookupError] = React.useState<string | null>(null);

  const [metaCollapsed, setMetaCollapsed] = React.useState(false);

  const id = dictionaryId ?? "";

  // Load dictionary metadata
  React.useEffect(() => {
    if (!id) return;
    setDictLoading(true);
    setDictError(null);
    getDictionary(id)
      .then((d) => {
        setDictionary(d);
        setDictLoading(false);
      })
      .catch((err: unknown) => {
        setDictError(err instanceof Error ? err.message : "Failed to load dictionary");
        setDictLoading(false);
      });
  }, [id]);

  // Restore lookup from URL on initial load
  React.useEffect(() => {
    const key = searchParams.get("q");
    if (key && id) {
      performLookup(key);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [id]);

  const { suggestions, isLoading: suggestLoading } = useSuggest({
    dictionaryId: id,
    query,
    enabled: !!id && !!dictionary,
  });

  const performLookup = React.useCallback(
    (key: string) => {
      if (!key.trim() || !id) return;
      setLookupLoading(true);
      setLookupError(null);
      setLookupResult(null);

      setSearchParams({ q: key }, { replace: true });

      lookup(id, key)
        .then((result) => {
          setLookupResult(result);
          setLookupLoading(false);
        })
        .catch((err: unknown) => {
          setLookupLoading(false);
          if (isApiError(err) && err.body.code === "entry_not_found") {
            setLookupError(`No entry found for "${key}"`);
          } else {
            setLookupError(err instanceof Error ? err.message : "Lookup failed");
          }
        });
    },
    [id, setSearchParams]
  );

  const handleSelect = (key: string) => {
    setQuery(key);
    performLookup(key);
  };

  if (dictError) {
    return (
      <div className="flex min-h-screen flex-col items-center justify-center gap-4 px-4 text-center">
        <AlertCircle className="h-10 w-10 text-destructive" />
        <div>
          <p className="font-semibold">Failed to load dictionary</p>
          <p className="text-sm text-muted-foreground mt-1">{dictError}</p>
        </div>
        <Button variant="outline" size="sm" onClick={() => navigate("/")}>
          <ArrowLeft className="h-3.5 w-3.5 mr-1.5" />
          Back to home
        </Button>
      </div>
    );
  }

  return (
    <div className="flex min-h-screen flex-col">
      {/* Top navigation bar */}
      <header className="sticky top-0 z-40 border-b border-border bg-background/80 backdrop-blur-sm">
        <div className="mx-auto flex max-w-7xl items-center gap-3 px-4 py-3 sm:px-6">
          <Link
            to="/"
            className="flex items-center gap-1.5 text-sm text-muted-foreground hover:text-foreground transition-colors shrink-0"
          >
            <ArrowLeft className="h-4 w-4" />
            <span className="hidden sm:inline">Dictionaries</span>
          </Link>

          <Separator orientation="vertical" className="h-4 hidden sm:block" />

          <div className="flex items-center gap-2 min-w-0 flex-1">
            <div className="flex h-7 w-7 shrink-0 items-center justify-center rounded-md bg-primary/10 text-primary">
              <BookOpen className="h-3.5 w-3.5" />
            </div>
            {dictLoading ? (
              <Skeleton className="h-4 w-40" />
            ) : (
              <span className="text-sm font-medium truncate">
                {dictionary?.display_name ?? id}
              </span>
            )}
          </div>
        </div>
      </header>

      {/* Main content */}
      <div className="mx-auto flex w-full max-w-7xl flex-1 gap-6 px-4 py-6 sm:px-6 lg:flex-row lg:items-start">
        {/* Sidebar */}
        <aside className="hidden lg:block w-72 shrink-0 space-y-4">
          {dictLoading ? (
            <div className="rounded-xl border border-border p-4 space-y-3">
              <Skeleton className="h-4 w-32" />
              <Skeleton className="h-3 w-full" />
              <Skeleton className="h-3 w-4/5" />
              <Skeleton className="h-3 w-2/3" />
            </div>
          ) : dictionary ? (
            <DictionaryMeta
              dictionary={dictionary}
              collapsed={metaCollapsed}
              onToggle={() => setMetaCollapsed((v) => !v)}
            />
          ) : null}
        </aside>

        {/* Main column */}
        <main className="flex-1 min-w-0 space-y-5">
          {/* Search bar */}
          <div className="space-y-2">
            <SearchBar
              value={query}
              onChange={setQuery}
              onSelect={handleSelect}
              suggestions={suggestions}
              isLoading={suggestLoading}
              placeholder="Search entries…"
              autoFocus
            />
          </div>

          {/* Lookup result / entry viewer */}
          {lookupLoading ? (
            <div className="space-y-3 px-1">
              <Skeleton className="h-7 w-48" />
              <div className="rounded-xl border border-border h-[520px] overflow-hidden">
                <div className="flex h-full items-center justify-center">
                  <div className="flex flex-col items-center gap-2">
                    <div className="h-5 w-5 animate-spin rounded-full border-2 border-primary border-t-transparent" />
                    <span className="text-xs text-muted-foreground">
                      Looking up…
                    </span>
                  </div>
                </div>
              </div>
            </div>
          ) : lookupError ? (
            <div className="flex flex-col items-center justify-center gap-3 rounded-xl border border-dashed border-border py-14 text-center">
              <AlertCircle className="h-7 w-7 text-muted-foreground" />
              <div>
                <p className="text-sm font-medium">{lookupError}</p>
                <p className="text-xs text-muted-foreground mt-1">
                  Try a different spelling or search term.
                </p>
              </div>
            </div>
          ) : lookupResult ? (
            <div className="space-y-3">
              <EntryHeader result={lookupResult} />
              <EntryViewer
                contentUrl={lookupResult.content_url}
                className="h-[520px]"
              />
            </div>
          ) : (
            <EntryViewer contentUrl={null} className="h-[520px]" />
          )}
        </main>
      </div>
    </div>
  );
}
