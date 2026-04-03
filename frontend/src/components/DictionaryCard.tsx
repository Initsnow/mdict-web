import { BookOpen, Globe, Hash, ChevronRight } from "lucide-react";
import { Link } from "react-router-dom";
import { Card, CardContent } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import type { DictionarySummary } from "@/types/api";
import { cn } from "@/lib/utils";

interface DictionaryCardProps {
  dictionary: DictionarySummary;
  className?: string;
}

function statusClassName(status: DictionarySummary["status"]) {
  switch (status) {
    case "ready":
      return "bg-emerald-100 text-emerald-700 dark:bg-emerald-900/30 dark:text-emerald-400";
    case "loading":
      return "bg-amber-100 text-amber-700 dark:bg-amber-900/30 dark:text-amber-400";
    default:
      return undefined; // destructive variant handles it
  }
}

function statusLabel(status: DictionarySummary["status"]) {
  switch (status) {
    case "ready":      return "Ready";
    case "loading":    return "Loading";
    case "unavailable": return "Unavailable";
    case "error":      return "Error";
  }
}

export function DictionaryCard({ dictionary, className }: DictionaryCardProps) {
  const isReady = dictionary.status === "ready";
  const isError = dictionary.status === "unavailable" || dictionary.status === "error";

  return (
    <Link
      to={`/dictionaries/${encodeURIComponent(dictionary.dictionary_id)}`}
      className={cn(
        "group block rounded-xl outline-none focus-visible:ring-2 focus-visible:ring-ring",
        !isReady && "pointer-events-none opacity-60",
        className
      )}
      aria-disabled={!isReady}
    >
      <Card className="transition-all duration-200 group-hover:shadow-md group-hover:-translate-y-0.5">
        <CardContent className="flex flex-col gap-3">
          <div className="flex items-start justify-between gap-3">
            <div className="flex items-start gap-3 min-w-0">
              <div className="flex h-10 w-10 shrink-0 items-center justify-center rounded-lg bg-primary/10 text-primary">
                <BookOpen className="h-5 w-5" />
              </div>
              <div className="min-w-0 flex-1">
                <h3 className="font-semibold text-sm leading-tight truncate">
                  {dictionary.display_name}
                </h3>
                {dictionary.description && (
                  <p className="mt-0.5 text-xs text-muted-foreground line-clamp-2 leading-relaxed">
                    {dictionary.description}
                  </p>
                )}
              </div>
            </div>
            <ChevronRight className="h-4 w-4 shrink-0 text-muted-foreground transition-transform group-hover:translate-x-0.5 mt-0.5" />
          </div>

          <div className="flex flex-wrap items-center gap-2">
            <Badge
              variant={isError ? "destructive" : "secondary"}
              className={statusClassName(dictionary.status)}
            >
              {statusLabel(dictionary.status)}
            </Badge>

            {(dictionary.source_lang || dictionary.target_lang) && (
              <span className="inline-flex items-center gap-1 text-xs text-muted-foreground">
                <Globe className="h-3 w-3" />
                {dictionary.source_lang}
                {dictionary.target_lang !== dictionary.source_lang &&
                  ` → ${dictionary.target_lang}`}
              </span>
            )}

            <span className="inline-flex items-center gap-1 text-xs text-muted-foreground">
              <Hash className="h-3 w-3" />
              {dictionary.entry_count.toLocaleString()} entries
            </span>
          </div>

          {dictionary.tags.length > 0 && (
            <div className="flex flex-wrap gap-1.5">
              {dictionary.tags.map((tag) => (
                <Badge key={tag} variant="outline" className="text-xs">
                  {tag}
                </Badge>
              ))}
            </div>
          )}
        </CardContent>
      </Card>
    </Link>
  );
}
