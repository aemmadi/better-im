import { useMemo } from "react";
import {
  useBuildSemanticIndex,
  useSearch,
  useSemanticStatus,
  type SearchMode,
} from "../queries";
import { renderSnippet } from "../lib/snippet";
import { formatFull } from "../lib/format";
import { useContactMap } from "../lib/contacts";
import type { SearchResultDto } from "../types";

interface Props {
  query: string;
  mode: SearchMode;
  onOpenResult: (result: SearchResultDto) => void;
}

function chatLabel(r: SearchResultDto): string {
  return r.chatName ?? r.chatIdentifier ?? "Unknown chat";
}

/**
 * Smart-mode affordance: when no embeddings exist yet, offer to build the
 * semantic index; while building, show live progress. Smart search still works
 * without it (it degrades to keyword ranking), so this is a non-blocking prompt.
 */
function SemanticIndexBanner() {
  const status = useSemanticStatus(true);
  const { build, building, progress } = useBuildSemanticIndex();

  if (building) {
    const pct =
      progress && progress.total > 0
        ? Math.round((progress.done / progress.total) * 100)
        : 0;
    return (
      <div className="smart-banner">
        <div className="smart-banner-text">
          Building semantic index…{" "}
          {progress && progress.total > 0
            ? `${progress.done.toLocaleString()} / ${progress.total.toLocaleString()}`
            : "starting"}
        </div>
        <div className="smart-progress" aria-hidden>
          <div className="smart-progress-fill" style={{ width: `${pct}%` }} />
        </div>
      </div>
    );
  }

  // Only prompt once we know the index is empty (and this build can embed).
  if (!status.data || !status.data.available || status.data.vectorCount > 0) {
    return null;
  }

  return (
    <div className="smart-banner">
      <div className="smart-banner-text">
        Smart search ranks by meaning. Build the on-device semantic index to
        enable it
        {status.data.embeddableCount > 0
          ? ` (${status.data.embeddableCount.toLocaleString()} messages to embed)`
          : ""}
        . Until then, results fall back to keyword ranking.
      </div>
      <button type="button" className="primary-button" onClick={() => build()}>
        Build semantic index
      </button>
    </div>
  );
}

export function SearchResults({ query, mode, onOpenResult }: Props) {
  const { data, isLoading, isError, error, isFetching } = useSearch(query, mode);
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
        <span className="thread-title">
          {mode === "smart" ? "Smart search" : "Search"}
        </span>
        <span className="muted">
          {isLoading
            ? "Searching…"
            : `${results.length} result${results.length === 1 ? "" : "s"} for “${query.trim()}”`}
          {isFetching && !isLoading ? " · updating…" : ""}
        </span>
      </div>

      {mode === "smart" && <SemanticIndexBanner />}

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
