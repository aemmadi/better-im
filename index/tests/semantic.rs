//! Phase 5 semantic-search tests. All offline and model-free: they use the
//! deterministic [`MockEmbedder`] over the same synthetic `chat.db` fixture as
//! the keyword tests. Covers vector upsert/roundtrip, cosine nearest-neighbour,
//! RRF fusion ordering, missing-only backfill, and operator filters in hybrid.

mod common;

use std::sync::Arc;

use better_im_index::embeddings::MockEmbedder;
use better_im_index::query::parse_query;
use better_im_index::{Embedder, Indexer, SearchOpts};

/// Mock embedding dimensionality shared by the index build and the query side.
const DIM: usize = 128;

/// Build a synthetic source db + fresh index with a MockEmbedder attached, run a
/// full reindex, and return `(indexer, embedder, tempdir)`.
fn indexed() -> (Indexer, MockEmbedder, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("chat.db");
    let index = dir.path().join("index.db");
    common::build_db(&source);

    let embedder = MockEmbedder::new(DIM);
    let indexer = Indexer::open_with_embedder(
        &source,
        &index,
        Some(Arc::new(embedder.clone()) as Arc<dyn Embedder>),
    )
    .expect("open indexer with embedder");
    indexer.full_reindex().expect("full reindex");
    (indexer, embedder, dir)
}

/// Embed a query string to a vector with the shared mock embedder.
fn qvec(embedder: &MockEmbedder, text: &str) -> Vec<f32> {
    embedder.embed_one(text)
}

#[test]
fn full_reindex_does_not_embed_until_opted_in() {
    // Embedding is opt-in: a fresh full reindex must not populate vectors.
    let (indexer, _e, _dir) = indexed();
    assert_eq!(indexer.db().message_count().unwrap(), 8);
    assert_eq!(indexer.db().vector_count().unwrap(), 0, "no vectors yet");
    assert!(indexer.has_embedder());
}

#[test]
fn vector_upsert_roundtrips_via_cosine() {
    let (indexer, embedder, _dir) = indexed();
    // Store a known vector for message 3, then query with the identical vector:
    // it must come back first with cosine ~= 1.0.
    let v = qvec(&embedder, "let's grab dinner tonight at the beach");
    indexer
        .db()
        .upsert_vectors(&[(3, v.clone())], "mock-test")
        .unwrap();
    assert_eq!(indexer.db().vector_count().unwrap(), 1);

    let hits = indexer
        .db()
        .semantic_search(&v, &Default::default(), 5)
        .unwrap();
    assert_eq!(hits[0].0, 3, "identical vector is the nearest match");
    assert!((hits[0].1 - 1.0).abs() < 1e-4, "cosine ~= 1: {}", hits[0].1);
}

#[test]
fn build_semantic_index_embeds_all_then_search_finds_nearest() {
    let (indexer, embedder, _dir) = indexed();

    // Backfill everything; progress is monotonic and ends at total.
    let mut last = (0usize, 0usize);
    let report = indexer
        .build_semantic_index(|p| {
            assert!(p.done >= last.0, "progress non-decreasing");
            last = (p.done, p.total);
        })
        .unwrap();
    assert_eq!(report.embedded, 8, "all 8 text messages embedded");
    assert_eq!(report.total_vectors, 8);
    assert_eq!(last, (8, 8), "final progress reaches total");

    // Nearest to "dinner tonight" are the two dinner messages (3 has both words,
    // 7 has one), ranked ahead of unrelated messages.
    let q = qvec(&embedder, "dinner tonight");
    let hits = indexer
        .db()
        .semantic_search(&q, &Default::default(), 3)
        .unwrap();
    assert_eq!(hits[0].0, 3, "closest is the dinner+tonight message");
    assert_eq!(hits[1].0, 7, "next is the other dinner message");
    assert!(hits[0].1 > hits[1].1, "3 is strictly nearer than 7");
}

#[test]
fn hybrid_rrf_fuses_both_arms() {
    let (indexer, embedder, _dir) = indexed();
    indexer.build_semantic_index(|_| {}).unwrap();

    // Pure keyword: "dinner beach" ANDs both terms, so ONLY message 3 matches.
    let kw = indexer.search("dinner beach", SearchOpts::default()).unwrap();
    assert_eq!(kw.len(), 1);
    assert_eq!(kw[0].message.id, 3);

    // Hybrid: message 3 tops (ranked #1 by both arms); message 7 also surfaces,
    // contributed by the vector arm alone (it lacks "beach" so FTS drops it).
    let parsed = parse_query("dinner beach");
    let q = qvec(&embedder, "dinner beach");
    let hits = indexer
        .db()
        .hybrid_search(Some(&q), &parsed, SearchOpts::default())
        .unwrap();
    let ids: Vec<i64> = hits.iter().map(|h| h.message.id).collect();
    assert_eq!(ids[0], 3, "message in both arms fuses to the top");
    assert!(ids.contains(&7), "vector-only hit still surfaces: {ids:?}");
    // Fused RRF scores are positive and ordered (higher is better).
    assert!(hits[0].score > 0.0);
    for w in hits.windows(2) {
        assert!(w[0].score >= w[1].score, "results are RRF-ordered");
    }
}

#[test]
fn hybrid_falls_back_to_keyword_without_free_text() {
    let (indexer, _embedder, _dir) = indexed();
    indexer.build_semantic_index(|_| {}).unwrap();

    // Filters-only query has no free text to embed: hybrid == keyword/filter path.
    let parsed = parse_query("is:from-me");
    let hits = indexer
        .db()
        .hybrid_search(None, &parsed, SearchOpts::default())
        .unwrap();
    assert_eq!(hits.len(), 2, "messages 2 and 4 are from me");
    assert!(hits.iter().all(|h| h.message.is_from_me));
}

#[test]
fn backfill_covers_only_missing_rows() {
    let (indexer, _embedder, _dir) = indexed();
    // First build embeds all 8.
    assert_eq!(indexer.build_semantic_index(|_| {}).unwrap().embedded, 8);
    // Re-running is a no-op: nothing is missing.
    assert_eq!(indexer.build_semantic_index(|_| {}).unwrap().embedded, 0);

    // Drop one vector; only that row is now missing.
    indexer
        .db()
        .connection()
        .execute("DELETE FROM message_vectors WHERE id = 3", [])
        .unwrap();
    let missing = indexer.db().messages_missing_vectors(100).unwrap();
    assert_eq!(missing.len(), 1);
    assert_eq!(missing[0].0, 3);

    let report = indexer.build_semantic_index(|_| {}).unwrap();
    assert_eq!(report.embedded, 1, "only the missing row is re-embedded");
    assert_eq!(report.total_vectors, 8);
}

#[test]
fn incremental_sync_embeds_new_messages_when_enabled() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("chat.db");
    let index = dir.path().join("index.db");
    common::build_db(&source);

    let embedder = MockEmbedder::new(DIM);
    let indexer = Indexer::open_with_embedder(
        &source,
        &index,
        Some(Arc::new(embedder.clone()) as Arc<dyn Embedder>),
    )
    .unwrap();
    indexer.full_reindex().unwrap();

    // Before opting in, a sync must not embed (semantic index empty).
    common::append_message(&source, 9, "g9", "brand new lunch plans", 1);
    indexer.incremental_sync().unwrap();
    assert_eq!(indexer.db().vector_count().unwrap(), 0, "still opted out");

    // Opt in, then a further new message is embedded automatically by sync.
    indexer.build_semantic_index(|_| {}).unwrap();
    let before = indexer.db().vector_count().unwrap();
    common::append_message(&source, 10, "g10", "another fresh message", 1);
    indexer.incremental_sync().unwrap();
    assert_eq!(
        indexer.db().vector_count().unwrap(),
        before + 1,
        "the new message got a vector"
    );
    assert_eq!(indexer.db().missing_vector_count().unwrap(), 0);
}

#[test]
fn operator_filters_apply_to_both_hybrid_arms() {
    let (indexer, embedder, _dir) = indexed();
    indexer.build_semantic_index(|_| {}).unwrap();

    // "dinner" restricted to from-me: the semantically-best matches (alice's
    // dinner messages 3 & 7) are received, so the filter must exclude them from
    // BOTH arms. Only from-me messages {2, 4} can appear.
    let parsed = parse_query("is:from-me dinner");
    let q = qvec(&embedder, "dinner");
    let hits = indexer
        .db()
        .hybrid_search(Some(&q), &parsed, SearchOpts::default())
        .unwrap();
    assert!(!hits.is_empty(), "vector arm still returns filtered candidates");
    assert!(
        hits.iter().all(|h| h.message.is_from_me),
        "every hit respects is:from-me"
    );
    let ids: Vec<i64> = hits.iter().map(|h| h.message.id).collect();
    assert!(
        !ids.contains(&3) && !ids.contains(&7),
        "filtered-out dinner messages never appear: {ids:?}"
    );

    // A from:bob filter likewise constrains the vector arm to bob's messages.
    let parsed_bob = parse_query("from:bob dinner");
    let hits_bob = indexer
        .db()
        .hybrid_search(Some(&q), &parsed_bob, SearchOpts::default())
        .unwrap();
    assert!(
        hits_bob
            .iter()
            .all(|h| h.message.sender.as_deref().unwrap_or("").contains("bob")),
        "every hit is from bob"
    );
}
