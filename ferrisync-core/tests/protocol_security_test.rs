//! Security regression suite — protocol deserialization bounds (Phase 1).
//!
//! A remote peer must not be able to force unbounded allocation or CPU by
//! sending a small or malformed message. The framing layer caps the encoded
//! frame size; these tests pin the *decoding* budget as well, so a
//! length-prefix inside a small frame can never drive a long traversal.
//!
//! See `protocol::deserialize_message` for the single bounded entry point.

use ferrisync_core::protocol::{
    deserialize_message, frame_message, parse_chunk_frame, parse_frame, FileChunk, Index,
    IndexEntry, SyncMessage, MAX_CHUNK_FRAME, MAX_CONTROL_FRAME,
};
use std::time::{Duration, Instant};

fn valid_index() -> Index {
    Index {
        folder_id: "1".into(),
        device_id: "remote".into(),
        remote_path: None,
        entries: vec![IndexEntry {
            path: "f.txt".into(),
            local_version: 1,
            remote_version: 0,
            mtime: 1,
            size: 3,
            hash: vec![0u8; 32],
        }],
    }
}

/// A valid message round-trips through the bounded decoder unchanged.
#[test]
fn valid_message_is_accepted() {
    let framed = frame_message(&SyncMessage::Index(valid_index())).unwrap();
    let (msg, consumed) = parse_frame(&framed).unwrap();
    assert_eq!(consumed, framed.len());
    match msg {
        SyncMessage::Index(idx) => assert_eq!(idx.entries.len(), 1),
        other => panic!("expected Index, got {other:?}"),
    }
}

/// A frame whose declared length is one byte over the control cap is rejected
/// at the framing layer, before any deserialization work.
#[test]
fn frame_one_byte_over_the_control_limit_is_rejected() {
    let mut framed = vec![0u8; 4];
    let len = (MAX_CONTROL_FRAME + 1) as u32;
    framed[..4].copy_from_slice(&len.to_be_bytes());
    framed.extend_from_slice(&vec![0u8; MAX_CONTROL_FRAME + 1]);
    assert!(parse_frame(&framed).is_err());
}

/// A frame declaring a length larger than the bytes actually present is a clean
/// parse error, never a panic or an out-of-bounds read.
#[test]
fn truncated_frame_is_a_clean_error() {
    let framed = frame_message(&SyncMessage::Index(valid_index())).unwrap();
    assert!(parse_frame(&framed[..framed.len() - 1]).is_err());
    assert!(parse_frame(&[0u8, 0, 0]).is_err(), "shorter than a header");
}

/// A tiny payload that declares an enormous `Vec` length must be rejected
/// without a huge allocation or a long traversal. The 8-byte payload below
/// encodes `entries.len() == u64::MAX`; the decoder must stop on its byte
/// budget almost immediately rather than iterating toward that count.
#[test]
fn enormous_declared_length_is_rejected_quickly() {
    // Index.entries is a Vec<IndexEntry>: bincode reads a u64 length prefix.
    // We hand the decoder only the 8-byte prefix, which on its own has no
    // "size limit" under bincode's default options — exactly the amplification
    // this test guards against.
    let mut payload = Vec::new();
    payload.extend_from_slice(&u64::MAX.to_le_bytes());

    let start = Instant::now();
    let result = deserialize_message(&payload);
    let elapsed = start.elapsed();

    assert!(result.is_err(), "a u64::MAX length prefix must be rejected");
    // The budget makes this terminate well under a second; a regression that
    // removed the limit would spin for many seconds (or exhaust memory).
    assert!(
        elapsed < Duration::from_secs(2),
        "decoding an over-limit length prefix took {elapsed:?}; the byte budget is not being enforced"
    );
}

/// A small frame carrying many nested length prefixes must also be bounded by
/// the decoder budget rather than traversing an attacker-chosen count.
#[test]
fn nested_length_prefixes_are_bounded() {
    // 1 MiB of 8-byte prefixes, each declaring a huge inner length. Without a
    // budget the decoder would attempt to visit each one in turn.
    let mut payload = Vec::new();
    payload.extend_from_slice(&(MAX_CONTROL_FRAME as u64).to_le_bytes());
    for _ in 0..(MAX_CONTROL_FRAME / 8) {
        payload.extend_from_slice(&u64::MAX.to_le_bytes());
    }

    let start = Instant::now();
    let result = deserialize_message(&payload);
    let elapsed = start.elapsed();
    assert!(result.is_err());
    assert!(
        elapsed < Duration::from_secs(5),
        "nested length prefixes took {elapsed:?}; the byte budget is not being enforced"
    );
}

/// A chunk frame exceeding the tighter chunk limit is rejected at the framing
/// layer even though it is within the control-frame limit.
#[test]
fn oversized_chunk_frame_is_rejected() {
    let chunk = FileChunk {
        folder_id: "1".into(),
        path: "big.bin".into(),
        offset: 0,
        data: vec![0u8; MAX_CHUNK_FRAME + 1],
        total_size: (MAX_CHUNK_FRAME + 1) as u64,
    };
    let framed = frame_message(&SyncMessage::FileChunk(chunk)).unwrap();
    assert!(framed.len() > MAX_CHUNK_FRAME);
    assert!(parse_chunk_frame(&framed).is_err());
}

/// Random/zero bytes of a plausible length must never panic the decoder.
#[test]
fn garbage_payloads_never_panic() {
    for seed in 0u8..64 {
        let mut payload = vec![seed; 256];
        // A varied length prefix so different code paths are exercised.
        payload[..4].copy_from_slice(&(seed as u32 * 7).to_be_bytes());
        let _ = deserialize_message(&payload);
    }
}
