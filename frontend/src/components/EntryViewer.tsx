import * as React from "react";
import { cn } from "@/lib/utils";
import type { ThemeMode } from "@/types/api";

const AUTO_THEME_STYLE_ID = "mdict-auto-theme-style";
const AUTO_THEME_MODE_ATTR = "data-mdict-theme";
const AUTO_THEME_INVERT_ATTR = "data-mdict-auto-invert";
const AUTO_THEME_PRESERVE_CLASS = "mdict-auto-theme-preserve";
const AUTO_THEME_QUERY = "(prefers-color-scheme: dark)";
const AUTO_THEME_PRESERVE_TAGS = new Set([
  "AUDIO",
  "CANVAS",
  "EMBED",
  "IFRAME",
  "IMG",
  "OBJECT",
  "PICTURE",
  "SVG",
  "VIDEO",
]);
const AUTO_THEME_STYLE = `
:root {
  color-scheme: light dark;
}

html[${AUTO_THEME_MODE_ATTR}="dark"] {
  color-scheme: dark;
}

html[${AUTO_THEME_MODE_ATTR}="light"] {
  color-scheme: light;
}

html[${AUTO_THEME_MODE_ATTR}="dark"][${AUTO_THEME_INVERT_ATTR}="true"] {
  filter: invert(1) hue-rotate(180deg);
}

html[${AUTO_THEME_MODE_ATTR}="dark"][${AUTO_THEME_INVERT_ATTR}="true"] :is(img, picture, video, canvas, svg, iframe, embed, object, audio),
html[${AUTO_THEME_MODE_ATTR}="dark"][${AUTO_THEME_INVERT_ATTR}="true"] .${AUTO_THEME_PRESERVE_CLASS} {
  filter: invert(1) hue-rotate(180deg);
}
`;

interface EntryViewerProps {
  contentUrl: string | null;
  themeMode?: ThemeMode;
  className?: string;
}

/**
 * Renders dictionary entry HTML in a sandboxed iframe.
 * Per security requirements: never injects HTML directly into the main DOM.
 */
export function EntryViewer({
  contentUrl,
  themeMode = "auto",
  className,
}: EntryViewerProps) {
  const [loading, setLoading] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);
  const cleanupRef = React.useRef<(() => void) | null>(null);

  React.useEffect(() => {
    cleanupRef.current?.();
    cleanupRef.current = null;
    if (!contentUrl) {
      setLoading(false);
      setError(null);
      return;
    }
    setLoading(true);
    setError(null);
  }, [contentUrl]);

  React.useEffect(() => {
    return () => {
      cleanupRef.current?.();
      cleanupRef.current = null;
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
        src={contentUrl}
        title="Dictionary entry"
        allow="autoplay"
        sandbox="allow-same-origin allow-scripts"
        className="h-full w-full border-0 bg-white dark:bg-neutral-900"
        onLoad={(event) => {
          cleanupRef.current?.();
          cleanupRef.current = installIframeAutoTheme(event.currentTarget, themeMode) ?? null;
          setLoading(false);
        }}
        onError={() => {
          cleanupRef.current?.();
          cleanupRef.current = null;
          setLoading(false);
          setError("Could not load the entry content.");
        }}
      />
    </div>
  );
}

function installIframeAutoTheme(
  iframe: HTMLIFrameElement,
  themeMode: ThemeMode
): (() => void) | undefined {
  const doc = iframe.contentDocument;
  if (!doc || !iframe.contentWindow) {
    return undefined;
  }

  ensureAutoThemeStyle(doc);

  const parentRoot = iframe.ownerDocument.documentElement;
  const media = window.matchMedia(AUTO_THEME_QUERY);
  let rafId: number | null = null;

  const syncTheme = () => {
    rafId = null;
    const mode = resolvePreferredTheme(parentRoot, media);
    applyAutoTheme(doc, mode, themeMode);
  };

  const scheduleThemeSync = () => {
    if (rafId !== null) {
      return;
    }
    rafId = window.requestAnimationFrame(syncTheme);
  };

  scheduleThemeSync();

  const frameObserver = new MutationObserver(() => {
    scheduleThemeSync();
  });
  frameObserver.observe(doc.documentElement, {
    childList: true,
    subtree: true,
    attributes: true,
    attributeFilter: ["class", "style", "src"],
  });

  const parentObserver = new MutationObserver(() => {
    scheduleThemeSync();
  });
  parentObserver.observe(parentRoot, {
    attributes: true,
    attributeFilter: ["class", "data-theme", "data-color-scheme"],
  });

  const onMediaChange = () => {
    scheduleThemeSync();
  };

  if (typeof media.addEventListener === "function") {
    media.addEventListener("change", onMediaChange);
  } else {
    media.addListener(onMediaChange);
  }

  return () => {
    if (rafId !== null) {
      window.cancelAnimationFrame(rafId);
    }
    frameObserver.disconnect();
    parentObserver.disconnect();
    if (typeof media.removeEventListener === "function") {
      media.removeEventListener("change", onMediaChange);
    } else {
      media.removeListener(onMediaChange);
    }
  };
}

function ensureAutoThemeStyle(doc: Document) {
  if (doc.getElementById(AUTO_THEME_STYLE_ID)) {
    return;
  }

  const style = doc.createElement("style");
  style.id = AUTO_THEME_STYLE_ID;
  style.textContent = AUTO_THEME_STYLE;

  if (doc.head) {
    doc.head.append(style);
    return;
  }

  doc.documentElement.prepend(style);
}

function resolvePreferredTheme(
  parentRoot: HTMLElement,
  media: MediaQueryList
): "dark" | "light" {
  const explicitTheme =
    parentRoot.getAttribute("data-theme") ?? parentRoot.getAttribute("data-color-scheme");
  if (explicitTheme === "dark" || parentRoot.classList.contains("dark")) {
    return "dark";
  }
  if (explicitTheme === "light" || parentRoot.classList.contains("light")) {
    return "light";
  }
  return media.matches ? "dark" : "light";
}

function applyAutoTheme(doc: Document, mode: "dark" | "light", themeMode: ThemeMode) {
  const root = doc.documentElement;
  root.setAttribute(AUTO_THEME_MODE_ATTR, mode);
  root.style.colorScheme = mode;

  const autoInvert = shouldAutoInvert(doc, mode, themeMode);
  root.setAttribute(AUTO_THEME_INVERT_ATTR, autoInvert ? "true" : "false");

  if (autoInvert) {
    markPreserveElements(doc);
  }
}

function shouldAutoInvert(
  doc: Document,
  mode: "dark" | "light",
  themeMode: ThemeMode
): boolean {
  if (mode !== "dark") {
    return false;
  }

  switch (themeMode) {
    case "dictionary":
      return false;
    case "force_auto_dark":
      return true;
    case "auto":
    default:
      return !documentLooksDark(doc);
  }
}

function markPreserveElements(doc: Document) {
  const view = doc.defaultView;
  if (!view) {
    return;
  }

  for (const element of doc.querySelectorAll("*")) {
    if (AUTO_THEME_PRESERVE_TAGS.has(element.tagName.toUpperCase())) {
      element.classList.add(AUTO_THEME_PRESERVE_CLASS);
      continue;
    }

    const style = view.getComputedStyle(element);
    const hasBackgroundImage = style.backgroundImage !== "none";
    const textContent = element.textContent?.trim() ?? "";
    if (hasBackgroundImage && element.childElementCount === 0 && textContent.length === 0) {
      element.classList.add(AUTO_THEME_PRESERVE_CLASS);
    }
  }
}

function documentLooksDark(doc: Document): boolean {
  const view = doc.defaultView;
  if (!view) {
    return false;
  }

  const background = firstOpaqueBackground(doc, view);
  const foreground = firstForegroundColor(doc, view);
  if (!background) {
    return !!foreground && luminance(foreground) > 0.65;
  }
  if (!foreground) {
    return false;
  }

  return luminance(background) < 0.35 && luminance(foreground) > 0.55;
}

function firstOpaqueBackground(doc: Document, view: Window): Rgba | null {
  for (const element of [doc.body, doc.documentElement]) {
    if (!element) {
      continue;
    }
    const color = parseCssColor(view.getComputedStyle(element).backgroundColor);
    if (color && color.a > 0.05) {
      return color;
    }
  }
  return null;
}

function firstForegroundColor(doc: Document, view: Window): Rgba | null {
  for (const element of [doc.body, doc.documentElement]) {
    if (!element) {
      continue;
    }
    const color = parseCssColor(view.getComputedStyle(element).color);
    if (color) {
      return color;
    }
  }
  return null;
}

interface Rgba {
  r: number;
  g: number;
  b: number;
  a: number;
}

function parseCssColor(value: string): Rgba | null {
  const match = value.trim().match(/^rgba?\((.+)\)$/i);
  if (!match) {
    return null;
  }

  const parts = match[1]
    .replaceAll("/", " ")
    .split(/[\s,]+/)
    .filter(Boolean);
  if (parts.length < 3) {
    return null;
  }

  const [r, g, b] = parts.slice(0, 3).map(parseColorChannel);
  const alpha = parts[3] ? parseAlpha(parts[3]) : 1;
  if ([r, g, b, alpha].some((item) => Number.isNaN(item))) {
    return null;
  }

  return {
    r: clamp(r, 0, 255),
    g: clamp(g, 0, 255),
    b: clamp(b, 0, 255),
    a: clamp(alpha, 0, 1),
  };
}

function parseColorChannel(value: string): number {
  if (value.endsWith("%")) {
    return (Number.parseFloat(value) / 100) * 255;
  }
  return Number.parseFloat(value);
}

function parseAlpha(value: string): number {
  if (value.endsWith("%")) {
    return Number.parseFloat(value) / 100;
  }
  return Number.parseFloat(value);
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}

function luminance(color: Rgba): number {
  const [r, g, b] = [color.r, color.g, color.b].map((channel) => {
    const normalized = channel / 255;
    if (normalized <= 0.04045) {
      return normalized / 12.92;
    }
    return ((normalized + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}
