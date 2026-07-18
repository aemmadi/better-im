import { useMemo } from "react";
import { useSearch } from "../queries";
import { renderSnippet } from "../lib/snippet";
import { formatFull } from "../lib/format";
import { useContactMap } from "../lib/contacts";
import type { SearchResultDto } from "../types";

interface Props {
  query: string;
  onOpenResult: (result: SearchResultDto) => void;
}

function chatLabel(r: SearchResultDto): string {
  return r.chatName ?? r.chatIdentifier ?? "Unknown chat";
}

export function SearchResults({ query, onOpenResult }: Props) {
  const { data, isLoading, isError, error, isFetching } = useSearch(query);
  const results = data ?? [];

  // Resolve the sender of each incoming result to a contact name.
  const senderHandles = useMemo(
    () => results.filter((r) => !r.isFromMe).map((r) => r.sender),
    [results],
  );
  const contacts = useContactMap(senderHandles);

  const senderLabel = (r: SearchResultDto): string =>
    r.isFromMe ? "You" : contacts.name(r.sender);

  return (
    <div className="search-results">
      <div className="search-results-header">
        <span className="thread-title">Search</span>
        <span className="muted">
          {isLoading
            ? "Searching…"
            : `${results.length} result${results.length === 1 ? "" : "s"} for “${query.trim()}”`}
          {isFetching && !isLoading ? " · updating…" : ""}
        </span>
      </div>

      {isError ? (
        <div className="placeholder">Search failed: {String(error)}</div>
      ) : !isLoading && results.length === 0 ? (
        <div className="placeholder">No messages match this search.</div>
      ) : (
        <ul className="result-list">
          {results.map((r) => (
            <li key={r.id}>
              <button className="result-row" onClick={() => onOpenResult(r)}>
                <div className="result-top">
                  <span className="result-sender">{senderLabel(r)}</span>
                  <span className="result-chat">in {chatLabel(r)}</span>
                  <span className="result-date">{formatFull(r.timestamp)}</span>
                </div>
                <div className="result-snippet">{renderSnippet(r.snippet)}</div>
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
