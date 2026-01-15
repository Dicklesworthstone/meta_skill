use ms::search::HashEmbedder;
use ms::test_utils::{TestCase, run_table_tests};

#[test]
fn hash_embedding_dimensions_table() -> Result<(), String> {
    let cases = vec![
        TestCase {
            name: "dims_32",
            input: (32usize, "git commit workflow"),
            expected: 32usize,
            should_panic: false,
        },
        TestCase {
            name: "dims_64",
            input: (64usize, "skill search"),
            expected: 64usize,
            should_panic: false,
        },
    ];

    run_table_tests(cases, |(dim, text)| {
        let embedder = HashEmbedder::new(dim);
        let embedding = embedder.embed(text);
        embedding.len()
    })?;
    Ok(())
}
