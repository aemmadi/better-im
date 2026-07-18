import { Fragment, type ReactNode } from "react";

// The FTS `snippet()` output wraps matched spans in `[` … `]` (see the core
// index's `snippet(messages_fts, 0, '[', ']', …)` call). Render those spans as
// <mark> and leave everything else as plain text.

export function renderSnippet(snippet: string): ReactNode {
  const nodes: ReactNode[] = [];
  let i = 0;
  let key = 0;
  while (i < snippet.length) {
    const open = snippet.indexOf("[", i);
    if (open === -1) {
      nodes.push(<Fragment key={key++}>{snippet.slice(i)}</Fragment>);
      break;
    }
    const close = snippet.indexOf("]", open + 1);
    if (close === -1) {
      nodes.push(<Fragment key={key++}>{snippet.slice(i)}</Fragment>);
      break;
    }
    if (open > i) {
      nodes.push(<Fragment key={key++}>{snippet.slice(i, open)}</Fragment>);
    }
    nodes.push(<mark key={key++}>{snippet.slice(open + 1, close)}</mark>);
    i = close + 1;
  }
  return nodes;
}
