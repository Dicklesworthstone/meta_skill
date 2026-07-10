//! E2E Scenario: Hybrid Search Workflow
//!
//! Covers BM25, semantic, and hybrid search modes plus filters and caching.

use super::fixture::E2EFixture;
use ms::error::Result;
use ms::search::embeddings::HashEmbedder;
use ms::storage::Database;
use ms::storage::sqlite::EmbeddingRecord;

const SKILL_ALPHA: &str = r#"---
name: Alpha Search
description: Alpha skill for search tests
tags: [alpha, search, commonterm]
---

# Alpha Search

Alpha content with unique token zebradrive and commonterm once.

## Notes

zebradrive is unique to this skill.
"#;

const SKILL_BETA: &str = r#"---
name: Beta Search
description: Beta skill for search tests
tags: [beta, search, commonterm]
---

# Beta Search

Beta content with unique token bananacore and commonterm commonterm commonterm.

## Notes

bananacore is unique to this skill.
"#;

const SKILL_GAMMA: &str = r#"---
name: Gamma Search
description: Gamma skill for search tests
tags: [gamma, search, commonterm]
---

# Gamma Search

Gamma content with unique token citruscore and commonterm once.
"#;

fn setup_search_fixture(scenario: &str) -> Result<E2EFixture> {
    let mut fixture = E2EFixture::new(scenario);

    fixture.log_step("Initialize ms");
    let output = fixture.init();
    fixture.assert_success(&output, "init");

    fixture.log_step("Configure skill paths for global/local layers");
    let output = fixture.run_ms(&[
        "--robot",
        "config",
        "skill_paths.global",
        r#"[\"./global_skills\"]"#,
    ]);
    fixture.assert_success(&output, "config skill_paths.global");
    let output = fixture.run_ms(&[
        "--robot",
        "config",
        "skill_paths.local",
        r#"[\"./local_skills\"]"#,
    ]);
    fixture.assert_success(&output, "config skill_paths.local");

    fixture.log_step("Create skills in multiple layers");
    fixture.create_skill_in_layer("alpha-search", SKILL_ALPHA, "project")?;
    fixture.create_skill_in_layer("beta-search", SKILL_BETA, "global")?;
    fixture.create_skill_in_layer("gamma-search", SKILL_GAMMA, "local")?;

    fixture.log_step("Index skills");
    let output = fixture.run_ms(&["--robot", "index"]);
    fixture.assert_success(&output, "index");

    fixture.log_step("Seed embeddings for semantic search");
    seed_embeddings(&fixture)?;

    Ok(fixture)
}

fn seed_embeddings(fixture: &E2EFixture) -> Result<()> {
    let db_path = fixture.ms_root.join("ms.db");
    let db = Database::open(&db_path)?;
    let embedder = HashEmbedder::new(384);

    let skills = db.list_skills(50, 0)?;
    assert!(
        !skills.is_empty(),
        "Expected skills to be indexed before embeddings"
    );

    for skill in skills {
        let text = format!("{}\n{}\n{}", skill.name, skill.description, skill.body);
        let embedding = embedder.embed(&text);
        let record = EmbeddingRecord {
            skill_id: skill.id.clone(),
            embedding,
            dims: embedder.dims(),
            embedder_type: "hash".to_string(),
            content_hash: Some(skill.content_hash.clone()),
            computed_at: String::new(),
        };
        db.upsert_embedding(&record)?;
    }

    Ok(())
}

#[test]
fn test_search_bm25_only() -> Result<()> {
    let mut fixture = setup_search_fixture("search_bm25_only")?;

    fixture.log_step("BM25 search");
    let output = fixture.run_ms(&["--robot", "search", "zebradrive", "--search-type", "bm25"]);
    fixture.assert_success(&output, "search bm25");

    let json = output.json();
    let results = json["results"].as_array().expect("results array");
    assert!(!results.is_empty(), "BM25 search should return results");
    let top_id = results[0]["id"].as_str().unwrap_or_default();
    assert_eq!(top_id, "alpha-search", "BM25 should rank alpha first");

    Ok(())
}

#[test]
fn test_search_semantic_only() -> Result<()> {
    let mut fixture = setup_search_fixture("search_semantic_only")?;

    fixture.log_step("Semantic search");
    let output = fixture.run_ms(&[
        "--robot",
        "search",
        "bananacore",
        "--search-type",
        "semantic",
    ]);
    fixture.assert_success(&output, "search semantic");

    let json = output.json();
    let results = json["results"].as_array().expect("results array");
    assert!(!results.is_empty(), "Semantic search should return results");
    let top_id = results[0]["id"].as_str().unwrap_or_default();
    assert_eq!(
        top_id, "beta-search",
        "Semantic search should rank beta first"
    );

    Ok(())
}

#[test]
fn test_search_hybrid() -> Result<()> {
    let mut fixture = setup_search_fixture("search_hybrid")?;

    fixture.log_step("Hybrid search");
    let output = fixture.run_ms(&["--robot", "search", "zebradrive", "--search-type", "hybrid"]);
    fixture.assert_success(&output, "search hybrid");

    let json = output.json();
    let results = json["results"].as_array().expect("results array");
    assert!(!results.is_empty(), "Hybrid search should return results");
    let top_id = results[0]["id"].as_str().unwrap_or_default();
    assert_eq!(
        top_id, "alpha-search",
        "Hybrid search should rank alpha first"
    );

    Ok(())
}

#[test]
fn test_search_filters_tags() -> Result<()> {
    let mut fixture = setup_search_fixture("search_filters_tags")?;

    fixture.log_step("Search with tag filter");
    let output = fixture.run_ms(&[
        "--robot",
        "search",
        "commonterm",
        "--search-type",
        "bm25",
        "--tags",
        "beta",
    ]);
    fixture.assert_success(&output, "search tag filter");

    let json = output.json();
    let results = json["results"].as_array().expect("results array");
    assert_eq!(results.len(), 1, "Tag filter should return one result");
    let top_id = results[0]["id"].as_str().unwrap_or_default();
    assert_eq!(top_id, "beta-search", "Tag filter should keep beta only");

    Ok(())
}

#[test]
fn test_search_filters_layers() -> Result<()> {
    let mut fixture = setup_search_fixture("search_filters_layers")?;

    fixture.log_step("Search with layer filter");
    let output = fixture.run_ms(&[
        "--robot",
        "search",
        "commonterm",
        "--search-type",
        "bm25",
        "--layer",
        "global",
    ]);
    fixture.assert_success(&output, "search layer filter");

    let json = output.json();
    let results = json["results"].as_array().expect("results array");
    assert_eq!(results.len(), 1, "Layer filter should return one result");
    let top_id = results[0]["id"].as_str().unwrap_or_default();
    let top_layer = results[0]["layer"].as_str().unwrap_or_default();
    assert_eq!(top_id, "beta-search", "Layer filter should keep beta only");
    assert_eq!(top_layer, "org", "Global skills should map to org layer");

    Ok(())
}

#[test]
fn test_search_ranking() -> Result<()> {
    let mut fixture = setup_search_fixture("search_ranking")?;

    fixture.log_step("Search ranking by term frequency");
    let output = fixture.run_ms(&["--robot", "search", "commonterm", "--search-type", "bm25"]);
    fixture.assert_success(&output, "search ranking");

    let json = output.json();
    let results = json["results"].as_array().expect("results array");
    assert!(!results.is_empty(), "Ranking search should return results");
    let top_id = results[0]["id"].as_str().unwrap_or_default();
    assert_eq!(
        top_id, "beta-search",
        "Repeated term should rank beta first"
    );

    Ok(())
}

#[test]
fn test_search_caching() -> Result<()> {
    let mut fixture = setup_search_fixture("search_caching")?;

    fixture.log_step("Search caching - first run");
    let output1 = fixture.run_ms(&["--robot", "search", "commonterm", "--search-type", "bm25"]);
    fixture.assert_success(&output1, "search caching run 1");

    fixture.log_step("Search caching - second run");
    let output2 = fixture.run_ms(&["--robot", "search", "commonterm", "--search-type", "bm25"]);
    fixture.assert_success(&output2, "search caching run 2");

    let results1 = output1.json()["results"].clone();
    let results2 = output2.json()["results"].clone();
    assert_eq!(results1, results2, "Repeated searches should be consistent");

    let max_allowed = output1.elapsed.mul_f32(5.0);
    assert!(
        output2.elapsed <= max_allowed,
        "Second search should not be drastically slower ({}ms vs {}ms)",
        output2.elapsed.as_millis(),
        output1.elapsed.as_millis()
    );

    Ok(())
}

// ==================== Relevance-ranking scenarios (issue #144) ====================
//
// These skills reproduce the corpus shape from issue #144: several skills that
// merely *mention* a term (and sort alphabetically before the on-topic skill)
// versus one skill that is actually *about* the term. The pre-#144 substring
// scan ordered results by `quality_score DESC, id ASC`, so the alphabetical
// decoys always won; real BM25 must rank the on-topic skill first.

const SKILL_DECOY_A: &str = r#"---
name: Aardvark CI Migration
description: Migrating CI runners between clouds
tags: [ci, migration]
---

# Aardvark CI Migration

Guide for migrating CI runner fleets. One step deploys the runner chart with
helm before switching traffic over to the new cloud.
"#;

const SKILL_DECOY_B: &str = r#"---
name: Abacus Production Readiness
description: Production readiness review checklist
tags: [production, checklist]
---

# Abacus Production Readiness

Checklist for production readiness reviews. Confirm the service chart installs
cleanly with helm as part of the final sign-off pass.
"#;

const SKILL_HELM: &str = r#"---
name: Helm Validation
description: Validate helm charts before deploying to Kubernetes
tags: [helm, kubernetes, charts]
---

# Helm Validation

How to validate helm charts. Run helm lint on every chart, then helm template
to render manifests, and helm upgrade --dry-run against the target cluster.

## Notes

Helm chart validation catches schema errors before deploy.
"#;

const SKILL_SECRETS: &str = r#"---
name: Secrets Rotation
description: Rotate secrets stored in vault on a schedule
tags: [secrets, vault, security]
---

# Secrets Rotation

How to rotate secrets kept in vault. Rotate database credentials monthly and
API tokens weekly; vault leases enforce the rotation window.
"#;

fn setup_relevance_fixture(scenario: &str) -> Result<E2EFixture> {
    let mut fixture = E2EFixture::new(scenario);

    fixture.log_step("Initialize ms");
    let output = fixture.init();
    fixture.assert_success(&output, "init");

    fixture.log_step("Create relevance corpus (alphabetical decoys + on-topic skills)");
    fixture.create_skill_in_layer("aardvark-ci-migration", SKILL_DECOY_A, "project")?;
    fixture.create_skill_in_layer("abacus-production-readiness", SKILL_DECOY_B, "project")?;
    fixture.create_skill_in_layer("helm-validation", SKILL_HELM, "project")?;
    fixture.create_skill_in_layer("secrets-rotation", SKILL_SECRETS, "project")?;

    fixture.log_step("Index skills");
    let output = fixture.run_ms(&["--robot", "index"]);
    fixture.assert_success(&output, "index");

    fixture.log_step("Seed embeddings for hybrid search");
    seed_embeddings(&fixture)?;

    Ok(fixture)
}

/// Issue #144: a query term that several skills merely mention must rank the
/// skill that is actually *about* the term first — not whichever id sorts
/// alphabetically earliest.
#[test]
fn test_search_relevance_beats_alphabetical_order() -> Result<()> {
    let mut fixture = setup_relevance_fixture("search_relevance_alpha")?;

    for search_type in ["bm25", "hybrid"] {
        fixture.log_step(&format!("Relevance search ({search_type})"));
        let output = fixture.run_ms(&["--robot", "search", "helm", "--search-type", search_type]);
        fixture.assert_success(&output, "search helm");

        let json = output.json();
        let results = json["results"].as_array().expect("results array");
        assert!(
            !results.is_empty(),
            "{search_type}: 'helm' should return results"
        );
        let top_id = results[0]["id"].as_str().unwrap_or_default();
        assert_eq!(
            top_id,
            "helm-validation",
            "{search_type}: the skill about helm must outrank alphabetical decoys \
             that merely mention it (issue #144); got order {:?}",
            results
                .iter()
                .map(|r| r["id"].as_str().unwrap_or_default())
                .collect::<Vec<_>>()
        );
    }

    Ok(())
}

/// Issue #144: multi-word queries must not silently AND away to zero results.
/// "rotate secrets infisical vault" matched nothing under the substring scan
/// because no skill contains the literal token "infisical"; BM25 treats terms
/// as a disjunction and still surfaces the secrets/vault skill.
#[test]
fn test_search_multiword_query_partial_match() -> Result<()> {
    let mut fixture = setup_relevance_fixture("search_relevance_multiword")?;

    fixture.log_step("Multi-word query with one unmatched token");
    let output = fixture.run_ms(&[
        "--robot",
        "search",
        "rotate secrets infisical vault",
        "--search-type",
        "bm25",
    ]);
    fixture.assert_success(&output, "search multiword");

    let json = output.json();
    let results = json["results"].as_array().expect("results array");
    assert!(
        !results.is_empty(),
        "multi-word query must not AND away to zero results (issue #144)"
    );
    let top_id = results[0]["id"].as_str().unwrap_or_default();
    assert_eq!(
        top_id, "secrets-rotation",
        "the secrets/vault skill should top a rotate-secrets-vault query"
    );

    Ok(())
}

#[test]
fn test_search_no_results() -> Result<()> {
    let mut fixture = setup_search_fixture("search_no_results")?;

    fixture.log_step("Search with no results");
    let output = fixture.run_ms(&[
        "--robot",
        "search",
        "nonexistenttoken",
        "--search-type",
        "bm25",
    ]);
    fixture.assert_success(&output, "search no results");

    let json = output.json();
    let results = json["results"].as_array().expect("results array");
    assert!(
        results.is_empty(),
        "Search with no results should return empty list"
    );

    Ok(())
}
