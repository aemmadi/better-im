//! Proves that we can extract clean plain text from a real captured
//! `attributedBody` typedstream blob — the case where the `message.text`
//! column is `NULL` and the content lives only in the blob.
//!
//! The fixtures under `tests/fixtures/` are real typedstream captures vendored
//! from the `imessage-database` crate's own test data.

use std::fs;
use std::path::PathBuf;

use imessage_database::util::streamtyped;

fn fixture(name: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name);
    fs::read(&path).unwrap_or_else(|e| panic!("reading fixture {}: {e}", path.display()))
}

#[test]
fn decodes_text_from_real_attributed_body_blob() {
    // This blob has no accompanying plain-text column; its only text is inside
    // the typedstream. The legacy `streamtyped` parser recovers it.
    let bytes = fixture("AttributedBodyTextOnly");
    let text = streamtyped::parse(bytes).expect("should decode typedstream text");
    assert_eq!(text, "Noter test");
}

#[test]
fn decodes_second_real_attributed_body_blob() {
    let bytes = fixture("AttributedBodyTextOnly2");
    let text = streamtyped::parse(bytes).expect("should decode typedstream text");
    assert_eq!(text, "Test 3");
}
