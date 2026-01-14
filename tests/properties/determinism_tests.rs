use proptest::prelude::*;

use ms::search::HashEmbedder;

proptest! {
    #[test]
    fn test_hash_embedding_deterministic(text in ".*") {
        let embedder = HashEmbedder::new(64);
        let first = embedder.embed(&text);
        let second = embedder.embed(&text);
        prop_assert_eq!(first, second);
    }

    #[test]
    fn test_hash_embedding_length(text in ".*", dim in 1usize..256usize) {
        let embedder = HashEmbedder::new(dim);
        let embedding = embedder.embed(&text);
        prop_assert_eq!(embedding.len(), dim);
    }
}
