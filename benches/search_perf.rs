//! Criterion benchmarks for performance-critical paths.
//!
//! Performance targets (from meta_skill-ftb spec):
//! - hash_embedding: < 1μs per embedding
//! - rrf_fusion: < 10ms for combining rankings
//! - packing: < 50ms for constrained optimization
//! - vector_search: < 50ms p99 for 1000 embeddings

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};

use ms::core::disclosure::PackMode;
use ms::core::packing::{ConstrainedPacker, PackConstraints};
use ms::core::skill::{SkillSlice, SliceType};
use ms::search::embeddings::{HashEmbedder, VectorIndex};
use ms::search::hybrid::{fuse_results, RrfConfig};

// =============================================================================
// Hash Embedding Benchmarks
// =============================================================================

fn hash_embedding_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("hash_embedding");

    let embedder = HashEmbedder::new(384);

    // Benchmark different input sizes
    for size in [10, 100, 500, 1000].iter() {
        let input: String = "word ".repeat(*size);

        group.throughput(Throughput::Bytes(input.len() as u64));
        group.bench_with_input(
            BenchmarkId::new("text_size", size),
            &input,
            |b, input| b.iter(|| embedder.embed(black_box(input))),
        );
    }

    group.finish();

    // Batch embedding benchmark
    let mut batch_group = c.benchmark_group("hash_embedding_batch");
    let inputs: Vec<String> = (0..100)
        .map(|i| format!("sample text {} with various keywords rust async error", i))
        .collect();

    batch_group.throughput(Throughput::Elements(100));
    batch_group.bench_function("batch_100", |b| {
        b.iter(|| {
            inputs
                .iter()
                .map(|s| embedder.embed(black_box(s)))
                .collect::<Vec<_>>()
        })
    });

    batch_group.finish();
}

// =============================================================================
// RRF Fusion Benchmarks
// =============================================================================

fn rrf_fusion_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("rrf_fusion");

    let config = RrfConfig::default();

    // Benchmark different ranking sizes
    for size in [10, 50, 100, 500].iter() {
        let bm25_results: Vec<(String, f32)> = (0..*size)
            .map(|i| (format!("skill-bm25-{}", i), 1.0 / (i as f32 + 1.0)))
            .collect();

        let semantic_results: Vec<(String, f32)> = (0..*size)
            .map(|i| {
                (
                    format!("skill-semantic-{}", i),
                    1.0 / (i as f32 + 1.0),
                )
            })
            .collect();

        group.throughput(Throughput::Elements(*size as u64 * 2)); // Both lists
        group.bench_with_input(
            BenchmarkId::new("ranking_size", size),
            &(&bm25_results, &semantic_results),
            |b, (bm25, semantic)| {
                b.iter(|| fuse_results(black_box(bm25), black_box(semantic), &config))
            },
        );
    }

    group.finish();

    // Benchmark with overlapping results (common case)
    let mut overlap_group = c.benchmark_group("rrf_fusion_overlap");

    for overlap_pct in [25, 50, 75].iter() {
        let size = 100;
        let overlap = size * overlap_pct / 100;

        // Create overlapping results
        let bm25_results: Vec<(String, f32)> = (0..size)
            .map(|i| (format!("skill-{}", i), 1.0 / (i as f32 + 1.0)))
            .collect();

        let mut semantic_results: Vec<(String, f32)> = Vec::new();
        // First add overlapping skills
        for i in 0..overlap {
            semantic_results.push((format!("skill-{}", i), 0.9 - (i as f32 * 0.01)));
        }
        // Then add unique skills
        for i in 0..(size - overlap) {
            semantic_results.push((
                format!("skill-unique-{}", i),
                0.5 - (i as f32 * 0.005),
            ));
        }

        overlap_group.bench_with_input(
            BenchmarkId::new("overlap_pct", overlap_pct),
            &(&bm25_results, &semantic_results),
            |b, (bm25, semantic)| {
                b.iter(|| fuse_results(black_box(bm25), black_box(semantic), &config))
            },
        );
    }

    overlap_group.finish();
}

// =============================================================================
// Vector Index Search Benchmarks
// =============================================================================

fn vector_search_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("vector_search");

    let embedder = HashEmbedder::new(384);

    // Benchmark different index sizes
    for index_size in [100, 500, 1000, 2000].iter() {
        let mut index = VectorIndex::new(384);

        // Populate index
        for i in 0..*index_size {
            let text = format!(
                "skill {} description with keywords rust async error handling patterns {}",
                i,
                i % 10
            );
            let embedding = embedder.embed(&text);
            index.insert(format!("skill-{}", i), embedding);
        }

        let query_embedding = embedder.embed("rust error handling patterns");

        group.throughput(Throughput::Elements(*index_size as u64));
        group.bench_with_input(
            BenchmarkId::new("index_size", index_size),
            &(&index, &query_embedding),
            |b, (idx, query)| b.iter(|| idx.search(black_box(query), 10)),
        );
    }

    group.finish();

    // Benchmark different result limits
    let mut limit_group = c.benchmark_group("vector_search_limit");

    let mut index = VectorIndex::new(384);
    for i in 0..1000 {
        let text = format!("skill {} rust async patterns", i);
        let embedding = embedder.embed(&text);
        index.insert(format!("skill-{}", i), embedding);
    }

    let query_embedding = embedder.embed("rust patterns");

    for limit in [5, 10, 25, 50, 100].iter() {
        limit_group.bench_with_input(
            BenchmarkId::new("limit", limit),
            limit,
            |b, &limit| b.iter(|| index.search(black_box(&query_embedding), limit)),
        );
    }

    limit_group.finish();
}

// =============================================================================
// Packing Benchmarks
// =============================================================================

fn packing_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("packing");

    // Create test slices
    fn create_test_slices(count: usize) -> Vec<SkillSlice> {
        (0..count)
            .map(|i| SkillSlice {
                id: format!("slice-{}", i),
                slice_type: match i % 4 {
                    0 => SliceType::Rule,
                    1 => SliceType::Example,
                    2 => SliceType::Command,
                    _ => SliceType::Checklist,
                },
                token_estimate: 50 + (i % 100) * 10,
                utility_score: 1.0 - (i as f32 / count as f32),
                coverage_group: Some(format!("group-{}", i % 5)),
                tags: vec![format!("tag-{}", i % 3)],
                requires: Vec::new(),
                condition: None,
                content: format!("Content for slice {} with some text.", i),
            })
            .collect()
    }

    let packer = ConstrainedPacker;

    // Benchmark different slice counts
    for slice_count in [10, 50, 100, 200].iter() {
        let slices = create_test_slices(*slice_count);
        let constraints = PackConstraints::new(5000, 10);

        group.throughput(Throughput::Elements(*slice_count as u64));
        group.bench_with_input(
            BenchmarkId::new("slice_count", slice_count),
            &(&slices, &constraints),
            |b, (slices, constraints)| {
                b.iter(|| {
                    packer.pack(black_box(slices), black_box(constraints), PackMode::Balanced)
                })
            },
        );
    }

    group.finish();

    // Benchmark different budget constraints
    let mut budget_group = c.benchmark_group("packing_budget");

    let slices = create_test_slices(100);

    for budget in [1000, 5000, 10000, 20000].iter() {
        let constraints = PackConstraints::new(*budget, 10);

        budget_group.bench_with_input(
            BenchmarkId::new("budget", budget),
            &constraints,
            |b, constraints| {
                b.iter(|| packer.pack(black_box(&slices), black_box(constraints), PackMode::Balanced))
            },
        );
    }

    budget_group.finish();
}

// =============================================================================
// Similarity Computation Benchmarks
// =============================================================================

fn similarity_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("similarity");

    let embedder = HashEmbedder::new(384);

    // Pre-compute embeddings
    let embedding_a = embedder.embed("rust error handling async patterns");
    let embedding_b = embedder.embed("rust async await error patterns");

    group.bench_function("cosine_similarity", |b| {
        b.iter(|| embedder.similarity(black_box(&embedding_a), black_box(&embedding_b)))
    });

    // Batch similarity computation
    let embeddings: Vec<Vec<f32>> = (0..100)
        .map(|i| embedder.embed(&format!("sample text {} keywords", i)))
        .collect();

    let query = embedder.embed("sample text keywords");

    group.throughput(Throughput::Elements(100));
    group.bench_function("batch_similarity_100", |b| {
        b.iter(|| {
            embeddings
                .iter()
                .map(|emb| embedder.similarity(black_box(&query), black_box(emb)))
                .collect::<Vec<_>>()
        })
    });

    group.finish();
}

criterion_group!(
    benches,
    hash_embedding_benchmarks,
    rrf_fusion_benchmarks,
    vector_search_benchmarks,
    packing_benchmarks,
    similarity_benchmarks,
);

criterion_main!(benches);
