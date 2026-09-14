//! Testes do source model.

use nexa_source::{SourceId, SourceLocation, SourceManager, SourceSpan};

fn source(text: &str) -> (SourceManager, SourceId) {
    let mut mgr = SourceManager::new();
    let id = mgr.load_text("test.nexa".into(), text.to_owned());
    (mgr, id)
}

#[test]
fn source_id_is_stable_sequential() {
    let mut mgr = SourceManager::new();
    let a = mgr.load_text("a.nexa".into(), "a".into());
    let b = mgr.load_text("b.nexa".into(), "b".into());
    assert_eq!(a, SourceId(0));
    assert_eq!(b, SourceId(1));
    assert!(mgr.source(a).is_some());
    assert!(mgr.source(SourceId(99)).is_none());
}

#[test]
fn span_basics() {
    let (_, id) = source("let x = 10");
    let span = SourceSpan::new(id, 0, 3);
    assert_eq!(span.start, 0);
    assert_eq!(span.end, 3);
    assert!(!span.is_zero_width());
    assert!(SourceSpan::point(id, 5).is_zero_width());
}

#[test]
fn span_cover_same_source() {
    let (_, id) = source("abcdef");
    let a = SourceSpan::new(id, 0, 2);
    let b = SourceSpan::new(id, 4, 6);
    assert_eq!(SourceSpan::cover(a, b), None); // não adjacentes
    let c = SourceSpan::new(id, 2, 4);
    assert_eq!(SourceSpan::cover(a, c), Some(SourceSpan::new(id, 0, 4)));
    let d = SourceSpan::new(SourceId(7), 0, 2);
    assert_eq!(SourceSpan::cover(a, d), None); // sources diferentes
}

#[test]
fn span_text_and_validity() {
    let (mgr, id) = source("let x = 10");
    let f = mgr.source(id).unwrap();
    let span = SourceSpan::new(id, 0, 3);
    assert_eq!(f.span_text(span), Some("let"));
    assert!(f.is_valid_span(span));
    assert!(!f.is_valid_span(SourceSpan::new(id, 0, 100))); // além do fim
    assert!(!f.is_valid_span(SourceSpan {
        source: id,
        start: 10,
        end: 5
    })); // start > end
    assert!(!f.is_valid_span(SourceSpan::new(SourceId(1), 0, 3))); // outro file
}

#[test]
fn location_unicode_scalar_columns() {
    // "a😀b": a=0..1, 😀=1..5, b=5..6
    let (mgr, id) = source("a😀b");
    let f = mgr.source(id).unwrap();
    assert_eq!(f.location(0), SourceLocation::new(0, 0));
    assert_eq!(f.location(1), SourceLocation::new(0, 1));
    assert_eq!(f.location(5), SourceLocation::new(0, 2));
    assert_eq!(f.location(6), SourceLocation::new(0, 3));
}

#[test]
fn location_multiple_lines() {
    let (mgr, id) = source("ab\ncd\nef");
    let f = mgr.source(id).unwrap();
    assert_eq!(f.location(0), SourceLocation::new(0, 0));
    assert_eq!(f.location(1), SourceLocation::new(0, 1));
    assert_eq!(f.location(3), SourceLocation::new(1, 0));
    assert_eq!(f.location(4), SourceLocation::new(1, 1));
    assert_eq!(f.location(7), SourceLocation::new(2, 1));
}

#[test]
fn line_text_strips_newline() {
    let (mgr, id) = source("ab\r\ncd\n");
    let f = mgr.source(id).unwrap();
    assert_eq!(f.line_text(0), Some("ab"));
    assert_eq!(f.line_text(1), Some("cd"));
    assert_eq!(f.line_text(2), Some(""));
    assert_eq!(f.line_text(3), None);
}

#[test]
fn span_contains_line_break() {
    let (mgr, id) = source("a\nb");
    let f = mgr.source(id).unwrap();
    assert!(f.span_contains_line_break(SourceSpan::new(id, 0, 3)));
    assert!(!f.span_contains_line_break(SourceSpan::new(id, 0, 1)));
}

#[test]
fn load_bytes_valid_and_invalid_utf8() {
    let mut mgr = SourceManager::new();
    let ok = mgr.load_bytes("ok.nexa".into(), b"hello".to_vec());
    assert!(ok.is_ok());
    // 0xFF 0xFE são inválidos em UTF-8.
    let bad = mgr.load_bytes("bad.nexa".into(), vec![0xFF, 0xFE, 0x41]);
    match bad {
        Err(nexa_source::SourceLoadError::InvalidUtf8 { byte_offset, .. }) => {
            assert_eq!(byte_offset, 0);
        }
        other => panic!("expected InvalidUtf8, got {:?}", other.is_ok()),
    }
}

#[test]
fn empty_source_line_index() {
    let (mgr, id) = source("");
    let f = mgr.source(id).unwrap();
    assert_eq!(f.line_count(), 1);
    assert_eq!(f.location(0), SourceLocation::new(0, 0));
    assert_eq!(f.line_text(0), Some(""));
}
