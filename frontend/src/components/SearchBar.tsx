import * as React from "react";
import { Search, X, Loader2 } from "lucide-react";
import { cn } from "@/lib/utils";
import { Input } from "@/components/ui/input";
import type { SuggestionItem } from "@/types/api";

interface SearchBarProps {
  value: string;
  onChange: (value: string) => void;
  onSelect: (key: string) => void;
  suggestions: SuggestionItem[];
  isLoading?: boolean;
  placeholder?: string;
  className?: string;
  autoFocus?: boolean;
}

export function SearchBar({
  value,
  onChange,
  onSelect,
  suggestions,
  isLoading = false,
  placeholder = "Search entries…",
  className,
  autoFocus,
}: SearchBarProps) {
  const [open, setOpen] = React.useState(false);
  const [activeIndex, setActiveIndex] = React.useState(-1);
  const inputRef = React.useRef<HTMLInputElement>(null);
  const listRef = React.useRef<HTMLUListElement>(null);

  const hasSuggestions = suggestions.length > 0;

  React.useEffect(() => {
    setActiveIndex(-1);
  }, [suggestions]);

  const handleKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (!open || !hasSuggestions) return;

    switch (e.key) {
      case "ArrowDown":
        e.preventDefault();
        setActiveIndex((i) => Math.min(i + 1, suggestions.length - 1));
        break;
      case "ArrowUp":
        e.preventDefault();
        setActiveIndex((i) => Math.max(i - 1, -1));
        break;
      case "Enter":
        e.preventDefault();
        if (activeIndex >= 0) {
          handleSelect(suggestions[activeIndex].key);
        } else if (value.trim()) {
          onSelect(value.trim());
          setOpen(false);
        }
        break;
      case "Escape":
        setOpen(false);
        inputRef.current?.blur();
        break;
    }
  };

  const handleSelect = (key: string) => {
    onSelect(key);
    setOpen(false);
    inputRef.current?.blur();
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
          className="pointer-events-none absolute left-3 h-4 w-4 text-muted-foreground"
          aria-hidden="true"
        />
        <Input
          ref={inputRef}
          value={value}
          autoFocus={autoFocus}
          onChange={(e) => {
            onChange(e.target.value);
            setOpen(true);
          }}
          onFocus={() => setOpen(true)}
          onBlur={() => setTimeout(() => setOpen(false), 150)}
          onKeyDown={handleKeyDown}
          placeholder={placeholder}
          className="pl-9 pr-9 h-11 text-base rounded-xl shadow-none border-border focus-visible:ring-primary"
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
              className="rounded-full p-0.5 text-muted-foreground hover:text-foreground transition-colors"
              aria-label="Clear search"
            >
              <X className="h-3.5 w-3.5" />
            </button>
          )}
        </span>
      </div>

      {open && hasSuggestions && (
        <ul
          ref={listRef}
          role="listbox"
          className={cn(
            "absolute z-50 mt-1.5 w-full overflow-hidden rounded-xl border border-border",
            "bg-popover text-popover-foreground shadow-lg",
            "max-h-72 overflow-y-auto py-1"
          )}
        >
          {suggestions.map((item, i) => (
            <li
              key={item.key}
              role="option"
              aria-selected={i === activeIndex}
              onMouseDown={() => handleSelect(item.key)}
              onMouseEnter={() => setActiveIndex(i)}
              className={cn(
                "flex items-center gap-2 px-3 py-2 text-sm cursor-pointer select-none transition-colors",
                i === activeIndex
                  ? "bg-accent text-accent-foreground"
                  : "hover:bg-accent"
              )}
            >
              <Search className="h-3 w-3 shrink-0 text-muted-foreground" />
              <span className="truncate">{item.label}</span>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
