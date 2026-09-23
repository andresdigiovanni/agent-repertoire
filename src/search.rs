use crate::models::SearchHit;
use crate::storage::Storage;
use crate::Result;

pub const DEFAULT_LIMIT: usize = 10;

pub fn build_match_query(query: &str) -> String {
    query
        .split_whitespace()
        .map(|t| t.replace('"', "\"\""))
        .map(|t| format!("\"{}\"*", t))
        .collect::<Vec<_>>()
        .join(" OR ")
}

pub fn normalize_score(bm25_value: f64) -> f64 {
    let magnitude = (-bm25_value).max(0.0);
    magnitude / (1.0 + magnitude)
}

pub fn like_pattern(query: &str) -> String {
    let escaped = query
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    format!("%{}%", escaped)
}

pub fn run_search(storage: &Storage, query: &str, limit: usize) -> Result<Vec<SearchHit>> {
    let match_query = build_match_query(query);
    if match_query.is_empty() {
        return Ok(Vec::new());
    }
    let mut hits: Vec<SearchHit> = storage
        .search(&match_query, limit)?
        .into_iter()
        .map(|(rec, rank)| SearchHit {
            name: rec.name,
            description: rec.description,
            score: normalize_score(rank),
        })
        .collect();
    if hits.is_empty() {
        hits = storage
            .like_search(&like_pattern(query), limit)?
            .into_iter()
            .map(|rec| SearchHit {
                name: rec.name,
                description: rec.description,
                score: 0.0,
            })
            .collect();
    }
    Ok(hits)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn match_query_building() {
        assert_eq!(build_match_query("gcp errors"), "\"gcp\"* OR \"errors\"*");
        assert_eq!(
            build_match_query("  spaced   out  "),
            "\"spaced\"* OR \"out\"*"
        );
        assert_eq!(build_match_query(""), "");
        assert_eq!(build_match_query("\"quoted\""), "\"\"\"quoted\"\"\"*");
    }

    #[test]
    fn score_normalization_is_monotonic() {
        assert!(normalize_score(-1.0) < normalize_score(-5.0));
        assert!((normalize_score(-1.0) - 0.5).abs() < 1e-9);
        assert!(normalize_score(0.0) >= 0.0);
        assert!(normalize_score(-5.0) < 1.0);
    }

    #[test]
    fn like_pattern_escapes() {
        assert_eq!(like_pattern("a%b"), "%a\\%b%");
        assert_eq!(like_pattern("under_score"), "%under\\_score%");
    }

    #[test]
    fn run_search_finds_and_falls_back() {
        let dir = tempfile::tempdir().unwrap();
        let storage = crate::storage::Storage::open(&dir.path().join("t.db")).unwrap();
        let mut rec = crate::models::ToolRecord {
            id: 0,
            name: "gcp_analyze_service_errors".into(),
            description: "Analyze recent errors from a GCP service".into(),
            language: crate::models::Language::Bash,
            entrypoint: "run.sh".into(),
            input_schema: serde_json::json!({}),
            capabilities: Default::default(),
            keywords: vec!["gcp".into(), "cloud run".into(), "logs".into()],
            commands: vec!["gcloud".into()],
            examples: vec![],
            timeout_seconds: None,
            version: 1,
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-01T00:00:00Z".into(),
            usage_count: 0,
            last_used_at: None,
        };
        storage.insert_tool(&rec).unwrap();
        rec.name = "k8s_diagnose_crashloop".into();
        rec.description = "Diagnose crashloop pods".into();
        rec.keywords = vec!["k8s".into()];
        rec.commands = vec!["kubectl".into()];
        storage.insert_tool(&rec).unwrap();

        let hits = run_search(&storage, "gcp logs", 10).unwrap();
        assert_eq!(hits[0].name, "gcp_analyze_service_errors");
        assert!(hits[0].score > 0.0 && hits[0].score <= 1.0);

        let fallback = run_search(&storage, "crashloop", 10).unwrap();
        assert!(fallback.iter().any(|h| h.name == "k8s_diagnose_crashloop"));

        assert!(run_search(&storage, "", 10).unwrap().is_empty());
        assert!(run_search(&storage, "nonexistent_zz", 10)
            .unwrap()
            .is_empty());
    }
}
