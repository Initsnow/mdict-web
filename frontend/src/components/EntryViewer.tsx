import * as React from "react";
import { cn } from "@/lib/utils";

interface EntryViewerProps {
  contentUrl: string | null;
  className?: string;
}

const AUDIO_EXTENSIONS = [".mp3", ".wav", ".ogg", ".oga", ".m4a", ".aac", ".flac"];

/**
 * Renders dictionary entry HTML in a sandboxed iframe.
 * Per security requirements: never injects HTML directly into the main DOM.
 */
export function EntryViewer({ contentUrl, className }: EntryViewerProps) {
  const [loading, setLoading] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);
  const iframeRef = React.useRef<HTMLIFrameElement>(null);
  const audioRef = React.useRef<HTMLAudioElement>(null);
  const iframeCleanupRef = React.useRef<null | (() => void)>(null);

  React.useEffect(() => {
    iframeCleanupRef.current?.();
    iframeCleanupRef.current = null;
    if (audioRef.current) {
      audioRef.current.pause();
      audioRef.current.removeAttribute("src");
      audioRef.current.load();
    }
    if (!contentUrl) return;
    setLoading(true);
    setError(null);
  }, [contentUrl]);

  React.useEffect(() => {
    return () => {
      iframeCleanupRef.current?.();
      if (audioRef.current) {
        audioRef.current.pause();
        audioRef.current.removeAttribute("src");
        audioRef.current.load();
      }
    };
  }, []);

  if (!contentUrl) {
    return (
      <div
        className={cn(
          "flex items-center justify-center rounded-lg border border-dashed border-border text-muted-foreground select-none",
          className
        )}
      >
        <div className="text-center">
          <p className="text-sm">Search for a word to get started</p>
        </div>
      </div>
    );
  }

  return (
    <div className={cn("relative overflow-hidden rounded-lg border border-border", className)}>
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
        onLoad={() => {
          setLoading(false);

          iframeCleanupRef.current?.();
          iframeCleanupRef.current = null;

          const doc = iframeRef.current?.contentDocument;
          if (!doc) return;

          const handleClick = (event: MouseEvent) => {
            const target = event.target;
            if (!(target instanceof Element)) return;

            const link = target.closest("a[href]");
            if (!(link instanceof HTMLAnchorElement) || !isAudioResourceHref(link.href)) {
              return;
            }

            event.preventDefault();

            const audio = audioRef.current;
            if (!audio) return;

            if (audio.src === link.href) {
              audio.currentTime = 0;
            } else {
              audio.src = link.href;
            }

            void audio.play().catch(() => {});
          };

          doc.addEventListener("click", handleClick);
          iframeCleanupRef.current = () => {
            doc.removeEventListener("click", handleClick);
          };
        }}
        onError={() => {
          setLoading(false);
          setError("Could not load the entry content.");
        }}
      />
      <audio ref={audioRef} preload="none" className="hidden" />
    </div>
  );
}

function isAudioResourceHref(rawHref: string): boolean {
  try {
    const url = new URL(rawHref, window.location.href);
    if (hasAudioExtension(url.pathname)) {
      return true;
    }

    const resourceKey = url.searchParams.get("key");
    return resourceKey != null && (resourceKey.startsWith("sound://") || hasAudioExtension(resourceKey));
  } catch {
    return false;
  }
}

function hasAudioExtension(value: string): boolean {
  const lower = value.toLowerCase();
  return AUDIO_EXTENSIONS.some((extension) => lower.endsWith(extension));
}
