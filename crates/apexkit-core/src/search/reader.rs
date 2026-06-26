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

        let text_fields: Vec<Field> = schema
            .fields()
            .filter(|(_, e)| matches!(e.field_type(), TantivyFieldType::Str(_)))
            .map(|(f, _)| f)
            .collect();

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

        // We require EVERY word the user types to match *something* (Must)
        let mut top_clauses: Vec<(Occur, Box<dyn Query>)> = Vec::new();

        for token in trimmed.split_whitespace() {
            let token_lower = token.to_lowercase();
            // But within that word, it can match ANY field, as Exact, Prefix, or Fuzzy (Should)
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
                // Using Regex for prefix since it's highly optimized in Tantivy for typeahead
                let prefix_pattern = format!("{}.*", regex::escape(&token_lower));
                if let Ok(prefix_q) =
                    tantivy::query::RegexQuery::from_pattern(&prefix_pattern, *field)
                {
                    let boosted_prefix = BoostQuery::new(Box::new(prefix_q), 2.0);
                    field_clauses.push((Occur::Should, Box::new(boosted_prefix)));
                }

                // 3. FUZZY TYPO MATCH (Standard score: x1.0, only if word is long enough)
                // Prevents "in" from matching "it", "is", "if", which ruins results.
                if token_lower.len() >= 4 {
                    let fuzzy_q = FuzzyTermQuery::new(term.clone(), 1, true);
                    field_clauses.push((Occur::Should, Box::new(fuzzy_q)));
                }
            }

            // Numeric matching (Exact only)
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

    pub(crate) fn build_snippet(schema: &Schema, doc: &TantivyDocument) -> Map<String, JsonValue> {
        let mut map = Map::new();

        for (field, entry) in schema.fields() {
            let name = entry.name();
            if name == "record_id" {
                continue;
            }
            let Some(val) = doc.get_first(field) else {
                continue;
            };

            let json_val = if let Some(s) = val.as_str() {
                JsonValue::String(s.to_string())
            } else if let Some(f) = val.as_f64() {
                serde_json::Number::from_f64(f)
                    .map(JsonValue::Number)
                    .unwrap_or(JsonValue::Null)
            } else if let Some(u) = val.as_u64() {
                JsonValue::Number(serde_json::Number::from(u))
            } else if let Some(i) = val.as_i64() {
                JsonValue::Number(serde_json::Number::from(i))
            } else {
                continue;
            };

            map.insert(name.to_string(), json_val);
        }

        map
    }
}
