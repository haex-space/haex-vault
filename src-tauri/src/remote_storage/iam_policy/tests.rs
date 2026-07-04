use super::*;
use crate::remote_storage::share_access_flags::*;

#[test]
fn read_only_whole_bucket_generates_get_and_list_statements() {
    let policy = build_policy("mybucket", None, READ_ONLY);
    let json = serde_json::to_value(&policy).unwrap();
    let statements = json["Statement"].as_array().unwrap();
    assert_eq!(statements.len(), 2);
    // First statement: GetObject on all objects
    assert!(statements.iter().any(|s| s["Action"]
        .as_array()
        .unwrap()
        .iter()
        .any(|a| a == "s3:GetObject")
        && s["Resource"] == "arn:aws:s3:::mybucket/*"));
    // Second statement: ListBucket on the bucket
    assert!(statements.iter().any(|s| s["Action"]
        .as_array()
        .unwrap()
        .iter()
        .any(|a| a == "s3:ListBucket")
        && s["Resource"] == "arn:aws:s3:::mybucket"));
}

#[test]
fn prefix_scoped_policy_adds_s3_prefix_condition_on_listbucket() {
    let policy = build_policy("mybucket", Some("media/photos"), READ_ONLY);
    let json = serde_json::to_value(&policy).unwrap();
    let list_stmt = json["Statement"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| {
            s["Action"]
                .as_array()
                .unwrap()
                .iter()
                .any(|a| a == "s3:ListBucket")
        })
        .unwrap();
    let cond = &list_stmt["Condition"]["StringLike"]["s3:prefix"];
    let prefixes: Vec<&str> = cond
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert!(prefixes.contains(&"media/photos/*"));
    assert!(prefixes.contains(&"media/photos/"));
    assert!(prefixes.contains(&"media/photos"));
}

#[test]
fn read_write_adds_put_and_delete_actions() {
    let policy = build_policy("mybucket", None, READ_WRITE);
    let json = serde_json::to_value(&policy).unwrap();
    let object_stmt = json["Statement"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["Resource"] == "arn:aws:s3:::mybucket/*")
        .unwrap();
    let actions: Vec<&str> = object_stmt["Action"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert!(actions.contains(&"s3:GetObject"));
    assert!(actions.contains(&"s3:PutObject"));
    assert!(actions.contains(&"s3:DeleteObject"));
    assert!(actions.contains(&"s3:AbortMultipartUpload"));
}

#[test]
fn object_scoped_share_omits_listbucket() {
    let policy = build_object_policy("mybucket", "media/track.mp3", GET);
    let json = serde_json::to_value(&policy).unwrap();
    let statements = json["Statement"].as_array().unwrap();
    assert_eq!(statements.len(), 1);
    assert!(!statements.iter().any(|s| s["Action"]
        .as_array()
        .unwrap()
        .iter()
        .any(|a| a == "s3:ListBucket")));
    assert_eq!(
        statements[0]["Resource"],
        "arn:aws:s3:::mybucket/media/track.mp3"
    );
}

#[test]
fn list_only_flag_generates_only_listbucket_statement() {
    let policy = build_policy("mybucket", None, LIST);
    let json = serde_json::to_value(&policy).unwrap();
    let statements = json["Statement"].as_array().unwrap();
    assert_eq!(statements.len(), 1);
    assert!(statements[0]["Action"]
        .as_array()
        .unwrap()
        .iter()
        .any(|a| a == "s3:ListBucket"));
}

// flags=0 is a legal input; result is a policy with an empty Statement list.
// AWS PutUserPolicy will reject it — enforcement lives at the D2 adapter
// boundary. These regression tests pin the current behaviour so a well-meant
// refactor cannot silently emit synthetic empty statements or panic.
#[test]
fn build_policy_with_zero_flags_produces_empty_statement_list() {
    let policy = build_policy("mybucket", None, 0);
    assert!(policy.statement.is_empty());
}

#[test]
fn build_object_policy_with_zero_flags_produces_empty_statement_list() {
    let policy = build_object_policy("mybucket", "media/track.mp3", 0);
    assert!(policy.statement.is_empty());
}
