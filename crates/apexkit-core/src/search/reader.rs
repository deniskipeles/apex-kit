use super::engine::{FUZZY_DISTANCE, FUZZY_MIN_LEN, SearchManager};
use crate::models::InstantResult;
use serde_json::{Map, Value as JsonValue};
use tantivy::TantivyDocument;
use tantivy::collector::TopDocs;
use tantivy::query::{BooleanQuery, FuzzyTermQuery, Occur, Query, QueryParser, TermQuery};
use tantivy::schema::{
    Field, FieldType as TantivyFieldType, IndexRecordOption, Schema, Term, Value as TantivyValue,
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

        let parser = QueryParser::for_index(&ci.index, text_fields.clone());
        let mut top_clauses: Vec<(Occur, Box<dyn Query>)> = Vec::new();

        for token in trimmed.split_whitespace() {
            let token_lower = token.to_lowercase();
            let mut per_token: Vec<(Occur, Box<dyn Query>)> = Vec::new();

            for field in &text_fields {
                let field_name = schema.get_field_name(*field);

                let distance = if token_lower.len() > FUZZY_MIN_LEN {
                    FUZZY_DISTANCE
                } else {
                    0
                };
                let term_val = Term::from_field_text(*field, &token_lower);
                let fuzzy_q = FuzzyTermQuery::new(term_val, distance, true);
                per_token.push((Occur::Should, Box::new(fuzzy_q)));

                let prefix_str = format!("{}:{}*", field_name, token_lower);
                if let Ok(prefix_q) = parser.parse_query(&prefix_str) {
                    per_token.push((Occur::Should, prefix_q));
                }
            }

            if let Ok(num_val) = token.parse::<f64>() {
                for field in &number_fields {
                    let term_val = Term::from_field_f64(*field, num_val);
                    let exact_q = TermQuery::new(term_val, IndexRecordOption::Basic);
                    per_token.push((Occur::Should, Box::new(exact_q)));
                }
            }

            if !per_token.is_empty() {
                top_clauses.push((Occur::Must, Box::new(BooleanQuery::new(per_token))));
            }
        }

        if top_clauses.is_empty() {
            return Ok(vec![]);
        }

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
