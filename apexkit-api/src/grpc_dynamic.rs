use std::sync::Arc;
use axum::{
    extract::{State, Request},
    http::{StatusCode, HeaderMap, HeaderValue, header},
    response::{Response, IntoResponse},
    body::Body,
};
use bytes::{Buf, BufMut, Bytes, BytesMut};
use prost_reflect::{DescriptorPool, DynamicMessage};
use serde_json::Value;
use tokio::sync::RwLock;
use http_body_util::BodyExt; // Requires http-body-util crate

use apexkit_core::{Db, schema::FieldType, models::Collection};
use crate::AppState;

pub struct DynamicGrpcState {
    pub pool: RwLock<DescriptorPool>,
    pub proto_source: RwLock<String>,
}

impl DynamicGrpcState {
    pub fn new() -> Self {
        Self {
            pool: RwLock::new(DescriptorPool::new()),
            proto_source: RwLock::new(String::new()),
        }
    }

    /// Translates ApexKit Collections into a .proto string and compiles it in-memory
    pub async fn reload_schema(&self, db: Arc<dyn Db>) -> Result<(), String> {
        let collections = db.list_collections().await.map_err(|e| e.to_string())?;
        
        let mut s = String::new();
        s.push_str("syntax = \"proto3\";\n");
        s.push_str("package apex;\n\n");
        
        s.push_str("service ApexData {\n");
        
        for col in &collections {
            let name = capitalize(&col.name);
            s.push_str(&format!("  rpc Get{} (Get{}Req) returns ({});\n", name, name, name));
            s.push_str(&format!("  rpc Create{} (Create{}Req) returns ({});\n", name, name, name));
        }
        s.push_str("}\n\n");

        for col in &collections {
            let name = capitalize(&col.name);
            
            // Model Message
            s.push_str(&format!("message {} {{\n", name));
            s.push_str("  int64 id = 1;\n");
            
            let mut tag = 2;
            if let Some(schema) = &col.schema {
                for (fname, fdef) in &schema.fields {
                    let ptype = match fdef.r#type {
                        FieldType::Number => "double",
                        FieldType::Boolean => "bool",
                        FieldType::Blob => "bytes",
                        _ => "string",
                    };
                    s.push_str(&format!("  {} {} = {};\n", ptype, fname, tag));
                    tag += 1;
                }
            }
            s.push_str(&format!("  string created = {};\n", tag));
            s.push_str(&format!("  string updated = {};\n", tag + 1));
            s.push_str("}\n\n");

            // Request Messages
            s.push_str(&format!("message Get{}Req {{\n  int64 id = 1;\n}}\n\n", name));
            
            s.push_str(&format!("message Create{}Req {{\n", name));
            let mut req_tag = 1;
            if let Some(schema) = &col.schema {
                for (fname, fdef) in &schema.fields {
                    let ptype = match fdef.r#type {
                        FieldType::Number => "double",
                        FieldType::Boolean => "bool",
                        FieldType::Blob => "bytes",
                        _ => "string",
                    };
                    s.push_str(&format!("  {} {} = {};\n", ptype, fname, req_tag));
                    req_tag += 1;
                }
            }
            s.push_str("}\n\n");
        }

        // Compile dynamically using pure-rust protox
        let file_descriptor = protox::compile(&s, ["."]).map_err(|e| e.to_string())?;
        
        let mut pool = DescriptorPool::new();
        pool.add_file_descriptor_set(file_descriptor).map_err(|e| e.to_string())?;

        *self.proto_source.write().await = s;
        *self.pool.write().await = pool;
        
        Ok(())
    }
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
    }
}

// --- HTTP HANDLERS ---

/// Serves the raw `.proto` file to external JS clients
pub async fn serve_proto_file(State(state): State<AppState>) -> Response {
    // Note: You'll need to add dynamic_grpc to AppState
    // For this example, assume it's attached to extensions or State
    // let source = state.dynamic_grpc.proto_source.read().await.clone();
    
    // (Stubbed for integration, see router below)
    Response::builder()
        .header(header::CONTENT_TYPE, "text/plain")
        .body(Body::from("..."))
        .unwrap()
}

/// The Universal gRPC Router
/// Catches POST requests to /apex.ApexData/*
pub async fn handle_dynamic_grpc(
    State(state): State<AppState>,
    req: Request,
) -> Result<Response, StatusCode> {
    let path = req.uri().path().to_string(); // e.g., "/apex.ApexData/CreatePosts"
    let parts: Vec<&str> = path.split('/').collect();
    if parts.len() < 3 { return Err(StatusCode::NOT_FOUND); }
    
    let method_name = parts[2]; // e.g., "CreatePosts"

    // Extract raw body
    let bytes = req.into_body().collect().await.map_err(|_| StatusCode::BAD_REQUEST)?.to_bytes();
    
    // 1. Decode gRPC Framing (1 byte compress flag + 4 bytes length)
    if bytes.len() < 5 { return Err(StatusCode::BAD_REQUEST); }
    let mut buf = bytes;
    let _compressed = buf.get_u8();
    let length = buf.get_u32() as usize;
    let message_bytes = buf.split_to(length);

    // 2. Decode using Prost-Reflect
    // (Assuming pool is fetched from global state)
    // let pool = state.dynamic_grpc.pool.read().await;
    // let method_desc = pool.get_service_by_name("apex.ApexData").unwrap().get_method_by_name(method_name).unwrap();
    // let req_desc = method_desc.input();
    
    // let dynamic_msg = DynamicMessage::decode(req_desc, message_bytes).unwrap();
    
    // 3. Convert to JSON and route to DB
    // let mut json_payload = serde_json::to_value(&dynamic_msg).unwrap();
    
    // --> Call state.db.create_record(col_id, &json_payload)
    
    // 4. Convert Result back to DynamicMessage
    // let res_desc = method_desc.output();
    // let res_msg = DynamicMessage::deserialize(res_desc, json_result_from_db).unwrap();
    
    // 5. Encode gRPC Framing
    // let mut out_bytes = BytesMut::new();
    // res_msg.encode(&mut out_bytes).unwrap();
    
    // let mut final_frame = BytesMut::with_capacity(5 + out_bytes.len());
    // final_frame.put_u8(0); // uncompressed
    // final_frame.put_u32(out_bytes.len() as u32);
    // final_frame.put(out_bytes);

    // 6. Build gRPC Response (with grpc-status trailer/header)
    let response = Response::builder()
        .header(header::CONTENT_TYPE, "application/grpc")
        .header("grpc-status", "0") // 0 = OK
        .body(Body::from(vec![] /* final_frame */))
        .unwrap();

    Ok(response)
}