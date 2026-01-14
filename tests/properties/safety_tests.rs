use proptest::prelude::*;

use ms::core::spec_lens::parse_markdown;
use ms::search::SearchFilters;

proptest! {
    #[test]
    fn test_parse_markdown_never_panics(input in ".*") {
        let _ = parse_markdown(&input);
    }

    #[test]
    fn test_search_tags_never_panics(input in ".*") {
        let _ = SearchFilters::parse_tags(&input);
    }
}
