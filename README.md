# Better iMessage

A fast, native macOS reader and search client for your local iMessage history.

Better iMessage opens the Messages database that already lives on your Mac
(`~/Library/Messages/chat.db`), builds its own search index, and gives you a
clean three-pane app for **reading and searching** everything — conversations,
media, links, and stats. All processing happens on-device; **nothing ever leaves
your machine**, and there are no accounts, servers, or telemetry.

It is **read-only by design.** See [Read-only, by design](#read-only-by-design).

> **Platform:** macOS 12 (Monterey) or later. Requires Full Disk Access (a
> one-time system permission) to read the Messages database.

---

## Features

- **Read your conversations.** Threads render as native-feeling chat bubbles with
  proper sender grouping, timestamps, and attachment placeholders. Text is
  extracted correctly even for messages whose body lives in the `attributedBody`
  typedstream blob (the field is `NULL` in the database) — the #1 correctness
  problem this app solves before anything else.
- **Ranked keyword search.** Full-text search over every message via SQLite FTS5
  (BM25 ranking), with highlighted snippets and an operator mini-language:
  - `from:<name/number>` · `in:<chat>` · `before:<date>` · `after:<date>`
  - `has:photo` · `has:link` · `has:attachment`
  - `is:sent` · `is:received`
  - e.g. `from:alice dinner after:2023-01-01 has:photo`
- **Semantic ("Smart") search.** Optional on-device embeddings let you search by
  *meaning*, not just keywords. Results fuse semantic similarity with keyword
  ranking via Reciprocal Rank Fusion, and degrade gracefully to keyword ranking
  until the semantic index is built. The production build uses the
  `BAAI/bge-small-en-v1.5` model running locally (see below).
- **Galleries, Links, Insights, Timeline.**
  - **Media** — a scrollable grid of every photo/video/attachment.
  - **Links** — every shared URL, grouped and copyable.
  - **Insights** — per-conversation stats and charts (volume by day/hour,
    sent/received split, top contacts).
  - **Timeline** — a unified, newest-first feed across all conversations.
- **Contacts integration.** Message handles (phone numbers / emails) are resolved
  to names and photos from your macOS Contacts. Fully optional and best-effort:
  deny the permission and everything still works with raw identifiers.
- **Live updates.** An FSEvents watcher keeps the index in sync as new messages
  arrive — the open thread, sidebar, and feeds refresh automatically.

---

## Read-only, by design

Better iMessage reads and searches; it does **not** send. This is deliberate.
Programmatic sending on modern macOS requires a materially lower security posture
(disabling System Integrity Protection to link Apple's private `IMCore`
framework, or granting Automation control of Messages.app). That belongs behind a
separate, explicit opt-in — not in the default app.

The codebase leaves a clean **seam** for it:

- **Backend** — `core::models::MessageActionProvider` is the provider trait, and
  `ReadOnlyProvider` (empty capability set) is the shipping implementation. The
  `capabilities` Tauri command exposes a provider's advertised actions to the UI.
- **Frontend** — every thread shows a `ThreadComposer` that reads `capabilities()`
  and enables sending **only** when the backend advertises `"SendText"`. Today it
  renders a polished, disabled composer with a short note.

Dropping in a future `IMCoreProvider` / `AppleScriptProvider` that returns
`SendText` is all it takes for the composer to light up — no other UI change. See
`docs`-grade comments at `core/src/models.rs` (`ReadOnlyProvider`) and
`src-tauri/src/commands.rs` (`capabilities`).

---

## Architecture

Three Rust crates plus a React webview, wired together by Tauri v2:

```
┌─────────────────────────────────────────────────────────────┐
│  frontend/  — React + TanStack Query + react-virtual         │
│  3-pane UI · virtualized threads · charts · search           │
└───────────────▲─────────────────────────────────────────────┘
                │  Tauri IPC (invoke / events)
┌───────────────┴─────────────────────────────────────────────┐
│  src-tauri/ (better-im-app) — the Tauri v2 app shell         │
│  IPC commands · DTOs · Contacts (CNContactStore) · FDA        │
│  onboarding · feature endpoints (media/links/insights/time)  │
└───────┬───────────────────────────────────┬─────────────────┘
        │                                     │
┌───────▼──────────────────┐   ┌──────────────▼─────────────────┐
│  index/ (better-im-index)│   │  core/ (better-im-core)         │
│  SQLite + FTS5 keyword    │   │  read-only chat.db reader,      │
│  search · on-device       │◀──│  attributedBody text decoding,  │
│  embeddings + hybrid RRF  │   │  domain models, provider seam   │
│  · URL extraction ·       │   │  (wraps `imessage-database`)    │
│  FSEvents watcher         │   └──────────────┬─────────────────┘
└───────────────────────────┘                  │
                                    ┌───────────▼─────────────┐
                                    │  ~/Library/Messages/    │
                                    │  chat.db  (read-only)   │
                                    └─────────────────────────┘
```

- **`core` (better-im-core)** — a headless, **read-only** reader. It wraps the
  [`imessage-database`](https://crates.io/crates/imessage-database) crate to open
  `chat.db` and, crucially, decode message text out of the `attributedBody`
  typedstream. It maps upstream rows into source-agnostic domain models so
  nothing else depends on the on-disk schema.
- **`index` (better-im-index)** — our own search engine. It builds a denormalized,
  full-text-searchable **SQLite** database (FTS5, BM25) so search never
  re-decodes messages, adds on-device semantic embeddings stored alongside as
  `f32` BLOBs (hybrid ranked via RRF), extracts shared URLs, and runs an FSEvents
  watcher for incremental syncs.
- **`src-tauri` (better-im-app)** — the Tauri v2 desktop shell. It exposes the
  reader and index to the webview over typed IPC commands, resolves Contacts via
  `CNContactStore`, drives Full Disk Access onboarding, and serves the Phase 4
  feature endpoints (media / links / insights / timeline).
- **`frontend`** — a React + TypeScript webview: three-pane layout, virtualized
  message lists, Recharts insights, and the search UI.

### Why GPL-3.0

Better iMessage is licensed **GPL-3.0-or-later**. This is not a preference — it
is required. The `core` crate depends on
[`imessage-database`](https://crates.io/crates/imessage-database), which is
licensed under the GPL-3.0. Linking it makes the combined work a derivative, so
the whole project inherits the GPL-3.0. We embrace it: this is on-device, no-cloud
software, and copyleft fits. See [`LICENSE`](./LICENSE).

---

## Running it (development)

**Prerequisites**

- macOS 12+ with the **Xcode Command Line Tools** (`xcode-select --install`).
- **Rust** (stable) — <https://rustup.rs>.
- **Node.js 18+** and npm.

**Steps**

```bash
npm install          # installs the Tauri CLI + the frontend workspace deps
npm run tauri dev    # builds the Rust app + starts the Vite dev server
```

On first launch the app asks for **Full Disk Access** — this is what lets it read
`~/Library/Messages/chat.db`. Grant it in **System Settings › Privacy & Security ›
Full Disk Access**, then click *Re-check* (the app self-heals without a relaunch).
Optionally grant **Contacts** access to see names and photos instead of raw
numbers.

Dev builds use a lightweight, deterministic mock embedder for Smart search so no
model download or ONNX toolchain is needed. The real embedding model is enabled
only in release builds — see below.

**Building a distributable app**

```bash
npm run build:release   # == tauri build --features fastembed  (real semantic model)
```

Producing a signed, notarized `.dmg` is documented step by step in
**[BUILD.md](./BUILD.md)**.

---

## Repository layout

```
core/         better-im-core   — read-only chat.db reader + domain models + seam
index/        better-im-index  — FTS5 keyword + semantic search index & watcher
src-tauri/    better-im-app    — Tauri v2 shell, IPC commands, Contacts, features
frontend/     React + TS webview (three-pane UI)
BUILD.md      release / signing / notarization guide
```

## License

[GNU GPL-3.0-or-later](./LICENSE). © 2026 Anirudh Emmadi.
