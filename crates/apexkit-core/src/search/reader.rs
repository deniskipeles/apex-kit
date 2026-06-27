use super::engine::SearchManager;
use crate::models::InstantResult;
use serde_json::{Map, Value as JsonValue};
use tantivy::TantivyDocument;
use tantivy::collector::TopDocs;
use tantivy::query::{BooleanQuery, BoostQuery, FuzzyTermQuery, Occur, Query, TermQuery};
use tantivy::schema::{
    Field, FieldType as TantivyFieldType, IndexRecordOption, Schema, Term, Value,
};

impl SearchManager {
    /// Convenience wrapper around `instant_search` that returns just the
    /// matching record IDs (discarding scores/snippets). Useful for callers
    /// that only need the ID list (e.g. to fetch full records from the DB).
    pub fn search(
        &self,
        collection_id: i64,
        query_str: &str,
        limit: usize,
    ) -> Result<Vec<i64>, String> {
        Ok(self
            .instant_search(collection_id, query_str, limit)?
            .into_iter()
            .map(|r| r.id)
            .collect())
    }

    /// Core search implementation. Builds a multi-field, multi-strategy
    /// boolean query per whitespace-separated token in `query_str`, runs it
    /// against the collection's index, and returns scored results with
    /// inline field snippets (so callers don't necessarily need a DB
    /// round-trip just to render result previews).
    pub fn instant_search(
        &self,
        collection_id: i64,
        query_str: &str,
        limit: usize,
    ) -> Result<Vec<InstantResult>, String> {
        let lock = self.collections.read().unwrap();
        let ci = lock.get(&collection_id).ok_or("Index not loaded")?;

        let searcher = ci.reader.searcher();
        let schema = ci.index.schema();

        // Collect all text fields up front — every token will be matched
        // against each of these.
        let text_fields: Vec<Field> = schema
            .fields()
            .filter(|(_, e)| matches!(e.field_type(), TantivyFieldType::Str(_)))
            .map(|(f, _)| f)
            .collect();

        // Collect numeric (F64) fields too, but explicitly exclude the
        // synthetic "_lat"/"_lng" sub-fields generated for GeoPoint types —
        // those aren't meaningful targets for plain numeric term matching.
        let number_fields: Vec<Field> = schema
            .fields()
            .filter(|(_, e)| {
                let name = e.name();
                matches!(e.field_type(), TantivyFieldType::F64(_))
                    && !name.ends_with("_lat")
                    && !name.ends_with("_lng")
            })
            .map(|(f, _)| f)
            .collect();

        let trimmed = query_str.trim();
        if trimmed.is_empty() {
            return Ok(vec![]);
        }

        // top_clauses holds one sub-query per input token. Each sub-query
        // itself is an OR (Should) across all the ways that token could
        // match across all fields (see below).
        let mut top_clauses: Vec<(Occur, Box<dyn Query>)> = Vec::new();

        for token in trimmed.split_whitespace() {
            let token_lower = token.to_lowercase();

            // field_clauses: for THIS token, all the (field, strategy)
            // combinations that could match it. These are combined with
            // Occur::Should, i.e. "match any of these, score additively".
            let mut field_clauses: Vec<(Occur, Box<dyn Query>)> = Vec::new();

            for field in &text_fields {
                let term = Term::from_field_text(*field, &token_lower);

                // 1. EXACT MATCH (Massive Boost: x10.0)
                // Ensures that if they type "app", the word "app" wins against "apple" or "ape"
                let exact_q =
                    TermQuery::new(term.clone(), IndexRecordOption::WithFreqsAndPositions);
                let boosted_exact = BoostQuery::new(Box::new(exact_q), 10.0);
                field_clauses.push((Occur::Should, Box::new(boosted_exact)));

                // 2. PREFIX MATCH (Moderate Boost: x2.0)
                // Using Regex for prefix since it's highly optimized in Tantivy for typeahead.
                // regex::escape ensures any regex metacharacters in the user's
                // input are treated literally rather than as regex syntax.
                let prefix_pattern = format!("{}.*", regex::escape(&token_lower));
                if let Ok(prefix_q) =
                    tantivy::query::RegexQuery::from_pattern(&prefix_pattern, *field)
                {
                    let boosted_prefix = BoostQuery::new(Box::new(prefix_q), 2.0);
                    field_clauses.push((Occur::Should, Box::new(boosted_prefix)));
                }

                // 3. FUZZY TYPO MATCH (Standard score: x1.0, only if word is long enough)
                // Prevents "in" from matching "it", "is", "if", which ruins results.
                // Edit distance of 1, with `true` enabling "transposition" cost
                // handling (so e.g. swapped adjacent letters count as distance 1).
                if token_lower.len() >= 4 {
                    let fuzzy_q = FuzzyTermQuery::new(term.clone(), 1, true);
                    field_clauses.push((Occur::Should, Box::new(fuzzy_q)));
                }
            }

            // Numeric matching (Exact only) — if the token parses as a float,
            // also try matching it exactly against every numeric field.
            if let Ok(num_val) = token.parse::<f64>() {
                for field in &number_fields {
                    let term_val = Term::from_field_f64(*field, num_val);
                    let exact_q = TermQuery::new(term_val, IndexRecordOption::Basic);
                    field_clauses.push((Occur::Should, Box::new(exact_q)));
                }
            }

            // [THE FIX IS HERE]:
            // Change Occur::Must to Occur::Should so it doesn't require EVERY word to match.
            // Tantivy will sum the scores, so matching more words still ranks higher!
            //
            // i.e. this token's combined OR-query becomes one Should clause
            // in the outer query, rather than a Must — so documents matching
            // only some of the query's words can still appear (ranked lower)
            // instead of being excluded entirely.
            if !field_clauses.is_empty() {
                top_clauses.push((Occur::Should, Box::new(BooleanQuery::new(field_clauses))));
            }
        }

        if top_clauses.is_empty() {
            return Ok(vec![]);
        }

        // Because we changed the inner items to Should, this BooleanQuery now acts as a massive OR statement,
        // prioritizing documents that trigger the most Should clauses.
        let query = BooleanQuery::new(top_clauses);
        let top_docs = searcher
            .search(&query, &TopDocs::with_limit(limit))
            .map_err(|e| e.to_string())?;

        let id_field = schema
            .get_field("record_id")
            .map_err(|_| "record_id missing")?;
        let mut results = Vec::with_capacity(top_docs.len());

        // For each scored hit, fetch the full stored document, pull out the
        // record_id, and build a JSON "snippet" of all other stored fields
        // (so the caller gets a preview without needing a separate DB fetch).
        for (score, doc_addr) in top_docs {
            let doc: TantivyDocument = searcher.doc(doc_addr).map_err(|e| e.to_string())?;

            let doc_id = doc
                .get_first(id_field)
                .and_then(|v| v.as_i64())
                .unwrap_or(0);

            let snippet = Self::build_snippet(&schema, &doc);

            results.push(InstantResult {
                id: doc_id,
                score,
                snippet: JsonValue::Object(snippet),
            });
        }

        Ok(results)
    }

    /// Build a JSON object of all stored fields on `doc` (excluding
    /// `record_id`, which is surfaced separately as `InstantResult.id`).
    /// Used to give search callers a lightweight preview of matched records
    /// without a round-trip to the primary datastore.
    pub(crate) fn build_snippet(schema: &Schema, doc: &TantivyDocument) -> Map<String, JsonValue> {
        let mut map = Map::new();

        for (field, entry) in schema.fields() {
            let name = entry.name();
            // record_id is reported separately on InstantResult, so skip it here.
            if name == "record_id" {
                continue;
            }
            let Some(val) = doc.get_first(field) else {
                continue;
            };

            // Convert whatever Tantivy value type this field holds into the
            // corresponding serde_json::Value variant. Order matters here:
            // as_str/as_f64/as_u64/as_i64 are tried in sequence until one matches.
            let json_val = if let Some(s) = val.as_str() {
                JsonValue::String(s.to_string())
            } else if let Some(f) = val.as_f64() {
                // NaN/infinite floats have no valid JSON Number representation,
                // so fall back to Null rather than panicking.
                serde_json::Number::from_f64(f)
                    .map(JsonValue::Number)
                    .unwrap_or(JsonValue::Null)
            } else if let Some(u) = val.as_u64() {
                JsonValue::Number(serde_json::Number::from(u))
            } else if let Some(i) = val.as_i64() {
                JsonValue::Number(serde_json::Number::from(i))
            } else {
                // Unsupported/unknown stored value type — skip it rather
                // than guessing.
                continue;
            };

            map.insert(name.to_string(), json_val);
        }

        map
    }
}