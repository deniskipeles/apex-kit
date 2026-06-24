use super::ApexKit;
use crate::database::traits::QueryEngineStore;
use crate::query::ApexQuery;
use async_trait::async_trait;

#[async_trait]
impl QueryEngineStore for ApexKit {
    async fn query_engine(
        &self,
        query: ApexQuery,
    ) -> std::result::Result<serde_json::Value, Box<dyn std::error::Error + Send + Sync>> {
        let (sql, params, pipeline) = crate::query::QueryBuilder::build(&query, self).await?;

        let conn = self.get_data_read().await;
        let mut stmt = conn.prepare(&sql)?;
        let column_names: Vec<String> = stmt
            .column_names()
            .into_iter()
            .map(|s| s.to_string())
            .collect();

        let mut rows = stmt.query(rusqlite::params_from_iter(params))?;
        let mut results = Vec::new();

        while let Some(row) = rows.next()? {
            let mut row_map = serde_json::Map::new();

            for (i, name) in column_names.iter().enumerate() {
                let val = row.get_ref(i)?;
                let json_val = match val {
                    rusqlite::types::ValueRef::Integer(val_i) => serde_json::json!(val_i),
                    rusqlite::types::ValueRef::Real(val_f) => serde_json::json!(val_f),
                    rusqlite::types::ValueRef::Text(val_s) => {
                        let text = std::str::from_utf8(val_s).unwrap_or("");
                        if (text.starts_with('{') && text.ends_with('}'))
                            || (text.starts_with('[') && text.ends_with(']'))
                        {
                            serde_json::from_str(text).unwrap_or(serde_json::json!(text))
                        } else {
                            serde_json::json!(text)
                        }
                    }
                    rusqlite::types::ValueRef::Blob(val_b) => {
                        serde_json::json!(crate::utils::to_hex(val_b))
                    }
                    rusqlite::types::ValueRef::Null => serde_json::Value::Null,
                };
                row_map.insert(name.clone(), json_val);
            }
            results.push(serde_json::Value::Object(row_map));
        }

        if !pipeline.is_empty() {
            results = crate::query::QueryProcessor::process(results, pipeline)?;
        }

        Ok(serde_json::Value::Array(results))
    }
}
