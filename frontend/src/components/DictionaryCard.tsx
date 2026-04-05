import { BookOpen } from "lucide-react";
import type { DictionarySummary } from "@/types/api";
import { cn } from "@/lib/utils";

interface DictionaryCardProps {
  dictionary: DictionarySummary;
  className?: string;
}

const statusDot: Record<DictionarySummary["status"], string> = {
  ready: "bg-emerald-500",
  loading: "bg-amber-400 animate-pulse",
  unavailable: "bg-muted-foreground/40",
  error: "bg-destructive",
};

const statusText: Record<DictionarySummary["status"], string> = {
  ready: "Ready",
  loading: "Loading",
  unavailable: "Unavailable",
  error: "Error",
};

export function DictionaryCard({ dictionary, className }: DictionaryCardProps) {
  const langLabel =
    dictionary.source_lang && dictionary.target_lang && dictionary.target_lang !== dictionary.source_lang
      ? `${dictionary.source_lang} → ${dictionary.target_lang}`
      : dictionary.source_lang || dictionary.target_lang || null;

  return (
    <div
      className={cn(
        "flex items-center gap-3 px-4 py-3",
        dictionary.status !== "ready" && "opacity-50",
        className
      )}
    >
      <div className="flex h-8 w-8 shrink-0 items-center justify-center rounded-md bg-muted text-muted-foreground">
        <BookOpen className="h-4 w-4" />
      </div>

      <div className="min-w-0 flex-1">
        <p className="truncate text-sm font-medium">{dictionary.display_name}</p>
        {dictionary.description && (
          <p className="truncate text-xs text-muted-foreground">
            {dictionary.description}
          </p>
        )}
      </div>

      <div className="hidden items-center gap-4 text-xs text-muted-foreground sm:flex">
        {langLabel && <span>{langLabel}</span>}
        <span className="tabular-nums">{dictionary.entry_count.toLocaleString()} entries</span>
        <span className="flex items-center gap-1.5">
          <span className={cn("inline-block h-1.5 w-1.5 rounded-full", statusDot[dictionary.status])} />
          {statusText[dictionary.status]}
        </span>
      </div>
    </div>
  );
}
