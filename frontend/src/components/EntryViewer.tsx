import * as React from "react";
import { cn } from "@/lib/utils";

interface EntryViewerProps {
  contentUrl: string | null;
  className?: string;
}

/**
 * Renders dictionary entry HTML in a sandboxed iframe.
 * Per security requirements: never injects HTML directly into the main DOM.
 */
export function EntryViewer({ contentUrl, className }: EntryViewerProps) {
  const [loading, setLoading] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);
  const iframeRef = React.useRef<HTMLIFrameElement>(null);

  React.useEffect(() => {
    if (!contentUrl) return;
    setLoading(true);
    setError(null);
  }, [contentUrl]);

  if (!contentUrl) {
    return (
      <div
        className={cn(
          "flex flex-col items-center justify-center rounded-xl border border-dashed border-border bg-muted/30 text-muted-foreground select-none",
          className
        )}
      >
        <div className="flex flex-col items-center gap-3 p-12 text-center">
          <span className="text-4xl">📖</span>
          <p className="text-sm font-medium">Search for a word to get started</p>
          <p className="text-xs opacity-60">Type in the search box above</p>
        </div>
      </div>
    );
  }

  return (
    <div className={cn("relative overflow-hidden rounded-xl border border-border", className)}>
      {loading && (
        <div className="absolute inset-0 z-10 flex items-center justify-center bg-background/80 backdrop-blur-sm">
          <div className="flex flex-col items-center gap-2">
            <div className="h-5 w-5 animate-spin rounded-full border-2 border-primary border-t-transparent" />
            <span className="text-xs text-muted-foreground">Loading entry…</span>
          </div>
        </div>
      )}
      {error && (
        <div className="absolute inset-0 z-10 flex items-center justify-center bg-background">
          <div className="text-center text-sm text-destructive">
            <p className="font-medium">Failed to load entry</p>
            <p className="text-xs opacity-70 mt-1">{error}</p>
          </div>
        </div>
      )}
      <iframe
        ref={iframeRef}
        src={contentUrl}
        title="Dictionary entry"
        sandbox="allow-same-origin"
        className="h-full w-full border-0 bg-white dark:bg-neutral-900"
        onLoad={() => setLoading(false)}
        onError={() => {
          setLoading(false);
          setError("Could not load the entry content.");
        }}
      />
    </div>
  );
}
