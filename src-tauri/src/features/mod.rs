//! Phase 4 read-only feature endpoints: media galleries, links hub, insights,
//! and the global unified timeline.
//!
//! Each submodule owns its command(s) + DTO(s). The command **signatures** and
//! DTO **shapes** here are the frozen contract mirrored by `frontend/src/types.ts`
//! and `frontend/src/api.ts`; the Phase 4 feature agents fill in the command
//! bodies (and may add private helpers / `AppState` access) without changing the
//! JS-facing shape.

pub mod insights;
pub mod links;
pub mod media;
pub mod timeline;
