// =========================== /teamspace/studios/this_studio/tinybase/tinybase/tinybase-core/src/embeddings.rs ===========================
use serde::{Deserialize, Serialize};
use reqwest::Client;
use serde_json::json;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum EmbedderProvider {
    Local,       // Logic handled by ApexVector (in main.rs), or dummy fallback here
    HuggingFace, // API (Free Tier)
    Gemini,      // API (Free Tier)
    OpenAI,      // API (Paid)
}

pub struct EmbedderService {
    http_client: Client,
}

impl EmbedderService {
    pub fn new() -> Self {
        Self {
            http_client: Client::new(),
        }
    }

    pub async fn generate(
        &self, 
        text: &str, 
        provider: EmbedderProvider, 
        api_key: Option<String>
    ) -> Result<Vec<f32>, String> {
        match provider {
            EmbedderProvider::Local => {
                // In the architecture with ApexVector, 'Local' calls from Scripts might 
                // route through here if not intercepted. We return a dummy or error.
                // The actual Local embedding happens in the API layer via VectorProvider trait.
                println!("WARNING: Local embedding requested via Core Service. This path is for external APIs.");
                Ok(vec![0.0; 384]) 
            },
            
            EmbedderProvider::HuggingFace => {
                let key = api_key.ok_or("HuggingFace API Key required")?;
                let url = "https://api-inference.huggingface.co/pipeline/feature-extraction/sentence-transformers/all-MiniLM-L6-v2";
                
                let res = self.http_client.post(url)
                    .header("Authorization", format!("Bearer {}", key))
                    .json(&json!({"inputs": text, "options": {"wait_for_model": true}}))
                    .send().await.map_err(|e| e.to_string())?;

                if !res.status().is_success() {
                    return Err(format!("HF Error: {}", res.text().await.unwrap_or_default()));
                }

                let json_val: serde_json::Value = res.json().await.map_err(|e| e.to_string())?;
                
                if let Some(arr) = json_val.as_array() {
                    if let Some(first) = arr.get(0) {
                        if first.is_array() {
                            let inner: Vec<f32> = serde_json::from_value(first.clone()).unwrap_or_default();
                            return Ok(inner);
                        } else if first.is_number() {
                            let flat: Vec<f32> = serde_json::from_value(json_val).unwrap_or_default();
                            return Ok(flat);
                        }
                    }
                }
                Err("Invalid response format from HF".into())
            },

            EmbedderProvider::Gemini => {
                let key = api_key.ok_or("Gemini API Key required")?;
                let url = format!("https://generativelanguage.googleapis.com/v1beta/models/text-embedding-004:embedContent?key={}", key);
                
                let body = json!({
                    "content": { "parts": [{ "text": text }] }
                });

                let res = self.http_client.post(url).json(&body).send().await.map_err(|e| e.to_string())?;
                
                if !res.status().is_success() {
                    return Err(format!("Gemini Error: {}", res.text().await.unwrap_or_default()));
                }

                let json_val: serde_json::Value = res.json().await.map_err(|e| e.to_string())?;
                
                let values = json_val["embedding"]["values"].as_array()
                    .ok_or("No embedding returned from Gemini")?;
                
                let floats: Vec<f32> = values.iter().filter_map(|v| v.as_f64().map(|f| f as f32)).collect();
                Ok(floats)
            },

            _ => Err("Provider not implemented".into())
        }
    }
}