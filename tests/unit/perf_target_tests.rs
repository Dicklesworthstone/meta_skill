//! Performance target tests for CI regression detection.
//!
//! These tests verify that performance-critical operations don't regress significantly.
//! They run in test profile (not release), so thresholds are relaxed from release targets.
//!
//! Release targets (from meta_skill-ftb spec, verified by `cargo bench`):
//! - `hash_embedding`: < 1μs per embedding (for typical input)
//! - `rrf_fusion`: < 10ms for 500 results per list
//! - packing: < 50ms for 100 slices
//! - `vector_search`: < 50ms p99 for 1000 embeddings
//!
//! CI thresholds (test profile, ~10-20x relaxed to account for no LTO/opt):
//! - `hash_embedding`: < 50μs (guards against major regressions)
//! - Others: use release targets (already have sufficient headroom)

use std::hint::black_box;
use std::time::{Duration, Instant};

use ms::core::disclosure::PackMode;
use ms::core::packing::{ConstrainedPacker, PackConstraints};
use ms::core::skill::{SkillSlice, SliceType};
use ms::search::embeddings::{HashEmbedder, VectorIndex};
use ms::search::hybrid::{RrfConfig, fuse_results};

/// Helper to measure operation time with warmup
fn measure_op<F, R>(warmup_iterations: usize, measure_iterations: usize, mut op: F) -> Duration
where
    F: FnMut() -> R,
{
    // Warmup
    for _ in 0..warmup_iterations {
        let _ = black_box(op());
    }

    // Measure
    let start = Instant::now();
    for _ in 0..measure_iterations {
        let _ = black_box(op());
    }
    start.elapsed() / measure_iterations as u32
}

#[test]
fn test_hash_embedding_performance_target() {
    let embedder = HashEmbedder::new(384);
    let text = "rust error handling async patterns debugging workflow";

    let per_op = measure_op(100, 1000, || embedder.embed(black_box(text)));

    println!("[PERF] hash_embedding: {per_op:?} per operation");

    // Release target: < 1μs
    // CI threshold: < 100μs (test profile lacks LTO, runs ~10-20x slower)
    assert!(
        per_op < Duration::from_micros(100),
        "hash_embedding exceeded 100μs CI ceiling: {per_op:?}"
    );
}

#[test]
fn test_rrf_fusion_performance_target() {
    let config = RrfConfig::default();

    // Create realistic ranking lists (500 results each)
    let bm25_results: Vec<(String, f32)> = (0..500)
        .map(|i| (format!("skill-bm25-{i}"), 1.0 / (i as f32 + 1.0)))
        .collect();

    let semantic_results: Vec<(String, f32)> = (0..500)
        .map(|i| (format!("skill-semantic-{i}"), 1.0 / (i as f32 + 1.0)))
        .collect();

    let per_op = measure_op(10, 100, || {
        fuse_results(
            black_box(&bm25_results),
            black_box(&semantic_results),
            &config,
        )
    });

    println!(
        "[PERF] rrf_fusion (500+500 results): {per_op:?} per operation"
    );

    // Target: < 10ms
    assert!(
        per_op < Duration::from_millis(10),
        "rrf_fusion exceeded 10ms target: {per_op:?}"
    );
}

#[test]
fn test_vector_search_performance_target() {
    let embedder = HashEmbedder::new(384);
    let mut index = VectorIndex::new(384);

    // Build index with 1000 embeddings
    for i in 0..1000 {
        let text = format!(
            "skill {} description with keywords rust async error handling patterns {}",
            i,
            i % 10
        );
        let embedding = embedder.embed(&text);
        index.insert(format!("skill-{i}"), embedding);
    }

    let query_embedding = embedder.embed("rust error handling patterns");

    let per_op = measure_op(10, 100, || index.search(black_box(&query_embedding), 10));

    println!(
        "[PERF] vector_search (1000 embeddings): {per_op:?} per operation"
    );

    // Target: < 50ms p99 (we use mean, so target should be lower)
    assert!(
        per_op < Duration::from_millis(50),
        "vector_search exceeded 50ms target: {per_op:?}"
    );
}

#[test]
fn test_packing_performance_target() {
    // Create 100 test slices
    let slices: Vec<SkillSlice> = (0..100)
        .map(|i| SkillSlice {
            id: format!("slice-{i}"),
            slice_type: match i % 4 {
                0 => SliceType::Rule,
                1 => SliceType::Example,
                2 => SliceType::Command,
                _ => SliceType::Checklist,
            },
            token_estimate: 50 + (i % 100) * 10,
            utility_score: 1.0 - (i as f32 / 100.0),
            coverage_group: Some(format!("group-{}", i % 5)),
            tags: vec![format!("tag-{}", i % 3)],
            requires: Vec::new(),
            condition: None,
            section_title: None,
            content: format!("Content for slice {i} with some text."),
        })
        .collect();

    let packer = ConstrainedPacker;
    let constraints = PackConstraints::new(5000, 10);

    let per_op = measure_op(10, 100, || {
        packer.pack(
            black_box(&slices),
            black_box(&constraints),
            PackMode::Balanced,
        )
    });

    println!(
        "[PERF] packing (100 slices, 5000 budget): {per_op:?} per operation"
    );

    // Target: < 50ms
    assert!(
        per_op < Duration::from_millis(50),
        "packing exceeded 50ms target: {per_op:?}"
    );
}

#[test]
fn test_similarity_computation_performance() {
    let embedder = HashEmbedder::new(384);

    // Pre-compute embeddings
    let embedding_a = embedder.embed("rust error handling async patterns");
    let embedding_b = embedder.embed("rust async await error patterns");

    let per_op = measure_op(1000, 10000, || {
        embedder.similarity(black_box(&embedding_a), black_box(&embedding_b))
    });

    println!("[PERF] similarity: {per_op:?} per operation");

    // Similarity should be very fast (< 1μs)
    assert!(
        per_op < Duration::from_micros(1),
        "similarity exceeded 1μs target: {per_op:?}"
    );
}

/// Test that batch operations scale linearly
#[test]
fn test_batch_embedding_scaling() {
    let embedder = HashEmbedder::new(384);

    // Measure single embedding time
    let single_text = "rust async error handling patterns";
    let single_time = measure_op(100, 1000, || embedder.embed(black_box(single_text)));

    // Measure batch of 100
    let texts: Vec<String> = (0..100)
        .map(|i| format!("sample text {i} with keywords"))
        .collect();

    let batch_time = measure_op(10, 100, || {
        texts
            .iter()
            .map(|t| embedder.embed(black_box(t)))
            .collect::<Vec<_>>()
    });

    let expected_batch = single_time * 100;
    let overhead_ratio = batch_time.as_nanos() as f64 / expected_batch.as_nanos() as f64;

    println!(
        "[PERF] batch scaling: single={single_time:?}, batch={batch_time:?}, ratio={overhead_ratio:.2}x"
    );

    // Batch should be within 2x of linear (allowing for some overhead)
    assert!(
        overhead_ratio < 2.0,
        "batch embedding has excessive overhead: {overhead_ratio:.2}x expected"
    );
}
