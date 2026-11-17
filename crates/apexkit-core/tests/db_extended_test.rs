//! Functional integration tests for the SQLite-backed `Db` trait covering
//! collections, records, relations, users, tenants, sandboxes, scripts,
//! templates, AI actions, files, configs and audit logs.

mod common;

use apexkit_core::database::traits::{
    AiActionStore, AuditStore, CollectionStore, ConfigStore, Db, FileStore, RecordStore,
    RelationStore, SandboxStore, ScriptStore, TemplateStore, TenantStore, UserStore,
};
use apexkit_core::models::ai::CreateActionReq;
use apexkit_core::models::script::CreateScriptReq;
use common::{fresh_db, parse_schema, simple_schema};
use serde_json::json;

#[tokio::test]
async fn collection_and_record_full_lifecycle() {
    let (dir, db) = fresh_db("coll_records").await;
    let schema = parse_schema(simple_schema());

    let col_id = db
        .create_collection("posts", &Some(schema.clone()), None)
        .await
        .unwrap();
    assert!(col_id > 0);

    let rec_id = db
        .create_record(col_id, &json!({"title": "Hello", "views": 1}))
        .await
        .unwrap();
    assert!(rec_id > 0);

    let got = db.get_record(col_id, rec_id, None).await.unwrap().unwrap();
    assert_eq!(got.data["title"], "Hello");

    let updated = db
        .update_record(col_id, rec_id, &json!({"title": "Updated", "views": 2}))
        .await
        .unwrap();
    assert_eq!(updated.data["title"], "Updated");

    let listed = db
        .list_records(col_id, Default::default())
        .await
        .unwrap();
    assert_eq!(listed.total, 1);

    db.delete_record(col_id, rec_id).await.unwrap();
    assert!(db.get_record(col_id, rec_id, None).await.unwrap().is_none());

    db.delete_collection(col_id).await.unwrap();
    assert!(db.get_collection(col_id).await.unwrap().is_none());

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn list_records_pagination_and_total() {
    let (dir, db) = fresh_db("pagination").await;
    let col_id = db
        .create_collection("items", &Some(parse_schema(simple_schema())), None)
        .await
        .unwrap();
    for i in 0..5 {
        db.create_record(col_id, &json!({"title": format!("r{i}"), "views": i}))
            .await
            .unwrap();
    }

    let mut opts = apexkit_core::query::QueryOptions::default();
    opts.limit = Some(2);
    opts.offset = Some(0);
    let page = db.list_records(col_id, opts).await.unwrap();
    assert_eq!(page.total, 5, "total must reflect all records");
    assert_eq!(page.items.len(), 2, "page must be limited");

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn relation_creation_and_lookup() {
    let (dir, db) = fresh_db("relations").await;
    let authors = db
        .create_collection("authors", &Some(parse_schema(simple_schema())), None)
        .await
        .unwrap();
    let books = db
        .create_collection("books", &Some(parse_schema(simple_schema())), None)
        .await
        .unwrap();

    let author_id = db.create_record(authors, &json!({"title": "Jane"})).await.unwrap();
    let book_id = db.create_record(books, &json!({"title": "Book"})).await.unwrap();

    db.create_relation(books, book_id, authors, author_id, "author")
        .await
        .unwrap();

    let related = db.get_related_ids(books, book_id, "author").await.unwrap();
    assert_eq!(related, vec![(authors, author_id)]);

    let records = db.get_records_by_ids(authors, &[author_id]).await.unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].data["title"], "Jane");

    db.delete_relation(books, book_id, authors, author_id, "author")
        .await
        .unwrap();
    assert!(db.get_related_ids(books, book_id, "author").await.unwrap().is_empty());

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn user_lifecycle() {
    let (dir, db) = fresh_db("users").await;

    let u = db
        .create_user("a@b.com", "hash", "user", Some(json!({"k": 1})))
        .await
        .unwrap();
    assert_eq!(u.email, "a@b.com");
    assert_eq!(db.count_users(None).await.unwrap(), 1);

    let fetched = db.get_user_by_email("a@b.com").await.unwrap().unwrap();
    assert_eq!(fetched.id, u.id);

    db.update_user(u.id, None, Some("admin".into()), None, None)
        .await
        .unwrap();
    assert_eq!(db.get_user_by_email("a@b.com").await.unwrap().unwrap().role, "admin");

    let batch = db.get_users_by_ids(&[u.id]).await.unwrap();
    assert_eq!(batch.len(), 1);

    db.delete_user(u.id).await.unwrap();
    assert!(db.get_user_by_email("a@b.com").await.unwrap().is_none());

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn tenant_lifecycle() {
    let (dir, db) = fresh_db("tenants").await;

    db.register_tenant("t1", None, Some("Tenant One".into()), Some("pro".into()))
        .await
        .unwrap();

    let tenants = db.list_tenants().await.unwrap();
    assert_eq!(tenants.len(), 1);
    assert_eq!(tenants[0].id, "t1");

    assert_eq!(db.get_tenant_status("t1").await.unwrap(), "active");
    db.update_tenant_status("t1", "suspended").await.unwrap();
    assert_eq!(db.get_tenant_status("t1").await.unwrap(), "suspended");

    db.update_tenant_full("t1", Some("Renamed".into()), None, None, Some(50), None, None, None, None)
        .await
        .unwrap();
    let t = &db.list_tenants().await.unwrap()[0];
    assert_eq!(t.name.as_deref(), Some("Renamed"));
    assert_eq!(t.stats.max_vectors, 50);

    db.delete_tenant_metadata("t1").await.unwrap();
    assert!(db.list_tenants().await.unwrap().is_empty());

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn sandbox_lifecycle() {
    let (dir, db) = fresh_db("sandboxes").await;

    db.register_sandbox(
        "s1",
        None,
        Some("Sandbox One".into()),
        None,
        "root",
        None,
        100,
        1000,
        10,
    )
    .await
    .unwrap();

    let list = db.list_sandboxes(None).await.unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].id, "s1");

    db.update_sandbox_full("s1", Some("Renamed".into()), Some("closed".into()), None)
        .await
        .unwrap();
    let list = db.list_sandboxes(None).await.unwrap();
    assert_eq!(list[0].name.as_deref(), Some("Renamed"));
    assert_eq!(list[0].status, "closed");

    db.delete_sandbox_metadata("s1").await.unwrap();
    assert!(db.list_sandboxes(None).await.unwrap().is_empty());

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn script_lifecycle() {
    let (dir, db) = fresh_db("scripts").await;

    let id = db
        .create_script(CreateScriptReq {
            name: "hook1".into(),
            trigger_type: "before_create".into(),
            target_collection: Some("posts".into()),
            code: "export default async function(){}".into(),
            active: true,
            visibility: "private".into(),
            metadata: None,
        })
        .await
        .unwrap();
    assert!(id > 0);

    let by_name = db.get_script_by_name("hook1").await.unwrap().unwrap();
    assert_eq!(by_name.trigger_type, "before_create");

    let by_trigger = db.get_scripts_by_trigger("before_create").await.unwrap();
    assert_eq!(by_trigger.len(), 1);

    db.delete_script(id).await.unwrap();
    assert!(db.get_script_by_name("hook1").await.unwrap().is_none());

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn template_lifecycle() {
    let (dir, db) = fresh_db("templates").await;
    use apexkit_core::models::CreateTemplateReq;

    let id = db
        .create_template(CreateTemplateReq {
            slug: "page".into(),
            content: "<h1>{{ title }}</h1>".into(),
            script_id: None,
        })
        .await
        .unwrap();
    assert!(id > 0);

    let t = db.get_template_by_slug("page").await.unwrap().unwrap();
    assert_eq!(t.slug, "page");

    db.update_template(id, "<p>updated</p>".into(), None)
        .await
        .unwrap();
    let t = db.get_template_by_slug("page").await.unwrap().unwrap();
    assert_eq!(t.content, "<p>updated</p>");

    assert_eq!(db.list_templates().await.unwrap().len(), 1);

    db.delete_template(id).await.unwrap();
    assert!(db.get_template_by_slug("page").await.unwrap().is_none());

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn ai_action_lifecycle() {
    let (dir, db) = fresh_db("ai_actions").await;

    let id = db
        .create_ai_action(CreateActionReq {
            slug: "summarize".into(),
            name: "Summarize".into(),
            model: "gemini-flash".into(),
            system_prompt: Some("be brief".into()),
            template: "{{prompt}}".into(),
            config: None,
        })
        .await
        .unwrap();
    assert!(id > 0);

    let action = db.get_ai_action("summarize").await.unwrap().unwrap();
    assert_eq!(action.name, "Summarize");

    assert_eq!(db.list_ai_actions().await.unwrap().len(), 1);

    db.delete_ai_action(id).await.unwrap();
    assert!(db.get_ai_action("summarize").await.unwrap().is_none());

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn file_metadata_lifecycle() {
    let (dir, db) = fresh_db("files").await;

    let id = db
        .create_file_metadata("stored.txt", "orig.txt", "text/plain", 11, None)
        .await
        .unwrap();
    assert_eq!(db.count_files().await.unwrap(), 1);

    let meta = db.get_file_metadata(id).await.unwrap().unwrap();
    assert_eq!(meta.original_name, "orig.txt");

    let by_name = db.get_file_by_filename("stored.txt").await.unwrap().unwrap();
    assert_eq!(by_name.id, id);

    assert_eq!(db.list_files(10, 0).await.unwrap().len(), 1);

    db.delete_file_metadata(id).await.unwrap();
    assert_eq!(db.count_files().await.unwrap(), 0);

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn config_encrypted_round_trip() {
    let (dir, db) = fresh_db("configs").await;

    db.set_config("plain", &json!("value"), false).await.unwrap();
    db.set_config("secret", &json!("shh"), true).await.unwrap();

    assert_eq!(db.get_config("plain").await.unwrap().unwrap(), json!("value"));
    assert_eq!(db.get_config("secret").await.unwrap().unwrap(), json!("shh"));

    let items = db.list_configs().await.unwrap();
    assert_eq!(items.len(), 2);
    let secret_item = items.iter().find(|c| c.key == "secret").unwrap();
    assert!(secret_item.encrypted);

    db.delete_config("plain").await.unwrap();
    assert!(db.get_config("plain").await.unwrap().is_none());

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn audit_and_paginated_logs() {
    let (dir, db) = fresh_db("audit").await;

    db.log_audit_event("info", "created", "test", Some(json!({"x": 1})))
        .await
        .unwrap();
    db.log_system_event("warn", "engine", "slow query").await.unwrap();

    let logs = db.list_audit_logs().await.unwrap();
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0]["message"], "created");

    let (items, total) = db
        .list_paginated_logs("audit", 1, 10, None, None, None)
        .await
        .unwrap();
    assert_eq!(total, 1);
    assert_eq!(items.len(), 1);

    let _ = std::fs::remove_dir_all(&dir);
}
