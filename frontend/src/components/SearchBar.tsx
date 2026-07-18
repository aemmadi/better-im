interface Props {
  value: string;
  onChange: (value: string) => void;
  onClear: () => void;
}

export function SearchBar({ value, onChange, onClear }: Props) {
  return (
    <div className="searchbar">
      <span className="search-icon" aria-hidden>
        🔍
      </span>
      <input
        className="search-input"
        type="search"
        placeholder="Search messages…  (try from:alice, has:photo, after:2023-01-01)"
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
    </div>
  );
}
