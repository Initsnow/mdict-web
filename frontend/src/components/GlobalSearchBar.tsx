import * as React from "react";
import { Layers3, Loader2, Search, X } from "lucide-react";
import { Input } from "@/components/ui/input";
import { cn } from "@/lib/utils";
import type { SearchSuggestionItem } from "@/types/api";

interface GlobalSearchBarProps {
  value: string;
  onChange: (value: string) => void;
  onSearch: (query: string) => void;
  onSelect: (item: SearchSuggestionItem) => void;
  suggestions: SearchSuggestionItem[];
  dictionaryLabels: Record<string, string>;
  isLoading?: boolean;
  placeholder?: string;
  className?: string;
  autoFocus?: boolean;
}

export function GlobalSearchBar({
  value,
  onChange,
  onSearch,
  onSelect,
  suggestions,
  dictionaryLabels,
  isLoading = false,
  placeholder = "Search across dictionaries…",
  className,
  autoFocus,
}: GlobalSearchBarProps) {
  const [open, setOpen] = React.useState(false);
  const [activeIndex, setActiveIndex] = React.useState(-1);
  const inputRef = React.useRef<HTMLInputElement>(null);

  React.useEffect(() => {
    setActiveIndex(-1);
  }, [suggestions]);

  const hasSuggestions = suggestions.length > 0;

  const handleSelect = (item: SearchSuggestionItem) => {
    onSelect(item);
    setOpen(false);
    inputRef.current?.blur();
  };

  const handleKeyDown = (event: React.KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "Escape") {
      setOpen(false);
      inputRef.current?.blur();
      return;
    }

    if (!open || !hasSuggestions) {
      if (event.key === "Enter" && value.trim()) {
        event.preventDefault();
        onSearch(value.trim());
        setOpen(false);
      }
      return;
    }

    switch (event.key) {
      case "ArrowDown":
        event.preventDefault();
        setActiveIndex((index) => Math.min(index + 1, suggestions.length - 1));
        break;
      case "ArrowUp":
        event.preventDefault();
        setActiveIndex((index) => Math.max(index - 1, -1));
        break;
      case "Enter":
        event.preventDefault();
        if (activeIndex >= 0) {
          handleSelect(suggestions[activeIndex]);
        } else if (value.trim()) {
          onSearch(value.trim());
          setOpen(false);
        }
        break;
      default:
        break;
    }
  };

  const handleClear = () => {
    onChange("");
    setOpen(false);
    inputRef.current?.focus();
  };

  return (
    <div className={cn("relative w-full", className)}>
      <div className="relative flex items-center">
        <Search
          className="pointer-events-none absolute left-4 h-4 w-4 text-muted-foreground"
          aria-hidden="true"
        />
        <Input
          ref={inputRef}
          value={value}
          autoFocus={autoFocus}
          onChange={(event) => {
            onChange(event.target.value);
            setActiveIndex(-1);
            setOpen(true);
          }}
          onFocus={() => setOpen(true)}
          onBlur={() => setTimeout(() => setOpen(false), 120)}
          onKeyDown={handleKeyDown}
          placeholder={placeholder}
          className="h-12 rounded-xl border-border bg-background pl-11 pr-10 text-base shadow-sm focus-visible:ring-primary/40"
          aria-autocomplete="list"
          aria-expanded={open && hasSuggestions}
          role="combobox"
          autoComplete="off"
          spellCheck={false}
        />
        <span className="absolute right-3 flex items-center gap-1">
          {isLoading && (
            <Loader2 className="h-4 w-4 animate-spin text-muted-foreground" />
          )}
          {!isLoading && value && (
            <button
              type="button"
              onClick={handleClear}
              className="rounded-full p-1 text-muted-foreground transition-colors hover:text-foreground"
              aria-label="Clear search"
            >
              <X className="h-3.5 w-3.5" />
            </button>
          )}
        </span>
      </div>

      {open && hasSuggestions && (
        <ul
          role="listbox"
          className={cn(
            "absolute z-50 mt-1.5 w-full overflow-hidden rounded-xl border border-border bg-popover text-popover-foreground shadow-lg",
            "max-h-72 overflow-y-auto py-1"
          )}
        >
          {suggestions.map((item, index) => {
            const dictionaryLabel = dictionaryLabels[item.dictionary_id] ?? item.dictionary_id;
            return (
              <li
                key={`${item.dictionary_id}:${item.key}:${index}`}
                role="option"
                aria-selected={index === activeIndex}
                onMouseDown={() => handleSelect(item)}
                onMouseEnter={() => setActiveIndex(index)}
                className={cn(
                  "flex cursor-pointer items-start gap-3 px-3 py-2.5 transition-colors",
                  index === activeIndex
                    ? "bg-accent text-accent-foreground"
                    : "hover:bg-accent"
                )}
              >
                <Search className="mt-0.5 h-3.5 w-3.5 shrink-0 text-muted-foreground" />
                <div className="min-w-0 flex-1">
                  <p className="truncate text-sm font-medium">{item.label}</p>
                  <p className="mt-0.5 flex items-center gap-1 text-xs text-muted-foreground">
                    <Layers3 className="h-3 w-3 shrink-0" />
                    <span className="truncate">{dictionaryLabel}</span>
                  </p>
                </div>
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}
