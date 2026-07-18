import type { SearchMode } from "../queries";

interface Props {
  value: string;
  onChange: (value: string) => void;
  onClear: () => void;
  mode: SearchMode;
  onModeChange: (mode: SearchMode) => void;
}

const MODES: { id: SearchMode; label: string; hint: string }[] = [
  { id: "keyword", label: "Keyword", hint: "Exact word / operator search" },
  { id: "smart", label: "Smart", hint: "Search by meaning (semantic + keyword)" },
];

export function SearchBar({ value, onChange, onClear, mode, onModeChange }: Props) {
  return (
    <div className="searchbar">
      <span className="search-icon" aria-hidden>
        🔍
      </span>
      <input
        className="search-input"
        type="search"
        placeholder={
          mode === "smart"
            ? "Search by meaning…  (e.g. plans for the weekend)"
            : "Search messages…  (try from:alice, has:photo, after:2023-01-01)"
        }
        value={value}
        spellCheck={false}
        autoCorrect="off"
        onChange={(e) => onChange(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Escape") onClear();
        }}
      />
      {value.length > 0 && (
        <button className="search-clear" onClick={onClear} aria-label="Clear search">
          ✕
        </button>
      )}
      <div className="search-mode-toggle" role="radiogroup" aria-label="Search mode">
        {MODES.map((m) => (
          <button
            key={m.id}
            type="button"
            role="radio"
            aria-checked={mode === m.id}
            title={m.hint}
            className={`search-mode-btn${mode === m.id ? " active" : ""}`}
            onClick={() => onModeChange(m.id)}
          >
            {m.label}
          </button>
        ))}
      </div>
    </div>
  );
}
