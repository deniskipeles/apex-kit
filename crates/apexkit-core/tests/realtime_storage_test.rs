//! Unit tests for realtime event types, storage backends, and the local
//! embedder service.

use apexkit_core::realtime::types::get_node_id;
use apexkit_core::realtime::{DbEvent, EventScope};
use apexkit_core::storage::{LocalStorage, StorageBackend};
use serde_json::json;

// ---------------------------------------------------------------------------
// Realtime event scopes
// ---------------------------------------------------------------------------

#[test]
fn event_scope_defaults_to_root() {
    assert_eq!(EventScope::default(), EventScope::Root);
}

#[test]
fn event_scope_variants_are_distinct() {
    assert_ne!(EventScope::Root, EventScope::Tenant("a".into()));
    assert_ne!(
        EventScope::Tenant("a".into()),
        EventScope::Tenant("b".into())
    );
    assert_ne!(
        EventScope::Sandbox("s".into()),
        EventScope::Channel("c".into())
    );
}

#[test]
fn event_scope_serializes_with_type_tag() {
    let scope = EventScope::Tenant("t1".into());
    let v = serde_json::to_value(&scope).unwrap();
    assert_eq!(v, json!({"Tenant": "t1"}));
}

#[test]
fn db_event_node_id_accessor() {
    let evt = DbEvent::Insert {
        collection_id: 1,
        record_id: 2,
        data: json!({}),
        node_id: Some("node-a".into()),
        scope: EventScope::Root,
    };
    assert_eq!(evt.node_id(), Some("node-a"));

    let evt_none = DbEvent::Delete {
        collection_id: 1,
        record_id: 2,
        node_id: None,
        scope: EventScope::Root,
    };
    assert_eq!(evt_none.node_id(), None);
}

#[test]
fn db_event_is_from_current_node() {
    let me = get_node_id().to_string();
    let mine = DbEvent::Update {
        collection_id: 1,
        record_id: 1,
        data: json!({}),
        node_id: Some(me.clone()),
        scope: EventScope::Root,
    };
    assert!(mine.is_from_current_node());

    let theirs = DbEvent::Update {
        collection_id: 1,
        record_id: 1,
        data: json!({}),
        node_id: Some("some-other-node".into()),
        scope: EventScope::Root,
    };
    assert!(!theirs.is_from_current_node());
}

#[test]
fn db_event_round_trips_through_json() {
    let evt = DbEvent::Custom {
        event: "ping".into(),
        data: json!({"x": 1}),
        node_id: Some("n1".into()),
        is_local: true,
        scope: EventScope::Channel("room".into()),
    };
    let s = serde_json::to_string(&evt).unwrap();
    let back: DbEvent = serde_json::from_str(&s).unwrap();
    assert_eq!(back.node_id(), Some("n1"));
}

// ---------------------------------------------------------------------------
// Local storage backend
// ---------------------------------------------------------------------------

#[tokio::test]
async fn local_storage_save_get_delete() {
    let id = uuid::Uuid::new_v4();
    let dir = std::env::temp_dir().join(format!("apexkit_storage_{id}"));
    let storage = LocalStorage::new(dir.to_str().unwrap(), "/files/").await;

    let name = "hello.txt";
    storage.save(name, b"hello world", "text/plain").await.unwrap();
    assert_eq!(storage.get(name).await.unwrap(), b"hello world");

    storage.delete(name).await.unwrap();
    assert!(storage.get(name).await.is_err(), "file must be gone");

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn local_storage_delete_missing_is_ok() {
    let id = uuid::Uuid::new_v4();
    let dir = std::env::temp_dir().join(format!("apexkit_storage_{id}"));
    let storage = LocalStorage::new(dir.to_str().unwrap(), "/files/").await;
    // Deleting a non-existent file must not error.
    assert!(storage.delete("nope.txt").await.is_ok());
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn local_storage_list_prefix_finds_files() {
    let id = uuid::Uuid::new_v4();
    let dir = std::env::temp_dir().join(format!("apexkit_storage_{id}"));
    let storage = LocalStorage::new(dir.to_str().unwrap(), "/files/").await;

    storage.save("a/one.txt", b"1", "text/plain").await.unwrap();
    storage.save("a/two.txt", b"22", "text/plain").await.unwrap();

    let listed = storage.list_prefix("a").await.unwrap();
    let names: Vec<String> = listed.iter().map(|(n, _, _)| n.clone()).collect();
    assert!(names.iter().any(|n| n.ends_with("one.txt")), "{names:?}");
    assert!(names.iter().any(|n| n.ends_with("two.txt")), "{names:?}");

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn local_storage_public_url_and_signed_url() {
    let id = uuid::Uuid::new_v4();
    let dir = std::env::temp_dir().join(format!("apexkit_storage_{id}"));
    let storage = LocalStorage::new(dir.to_str().unwrap(), "https://cdn.test/").await;

    assert_eq!(storage.get_public_url_base(), "https://cdn.test/");
    assert_eq!(
        storage.get_signed_url("x.png", 60).await.unwrap(),
        "https://cdn.test/x.png"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------
// Embedder service (local provider path)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn local_embedder_returns_384_dim_vector() {
    use apexkit_core::embeddings::{EmbedderProvider, EmbedderService};
    let svc = EmbedderService::new();
    let v = svc
        .generate("hello", EmbedderProvider::Local, None)
        .await
        .unwrap();
    assert_eq!(v.len(), 384);
}

#[tokio::test]
async fn remote_embedder_without_key_errors() {
    use apexkit_core::embeddings::{EmbedderProvider, EmbedderService};
    let svc = EmbedderService::new();
    let res = svc.generate("hi", EmbedderProvider::HuggingFace, None).await;
    assert!(res.is_err(), "missing API key must be an error");
}
