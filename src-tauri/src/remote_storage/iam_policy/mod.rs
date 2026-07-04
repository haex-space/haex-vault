//! IAM-Policy generator for S3-compatible bucket sharing.
//!
//! Emits AWS IAM policy JSON scoped to a bucket, an optional key-prefix, or a
//! single object. Used by the IAM adapter (Task D2) to call
//! `iam:PutUserPolicy` on the scoped share-user.
//!
//! See `docs/plans/2026-07-04-s3-bucket-sharing-via-spaces-design.md` §5 for
//! the exact JSON shapes and the action-mapping table.
//!
//! Security note: prefix-scoped policies MUST include the `s3:prefix`
//! Condition on `s3:ListBucket` — otherwise members can enumerate the whole
//! bucket root. See design §9.

use serde::Serialize;

use crate::remote_storage::share_access_flags::{has_flag, DELETE, GET, LIST, PUT};

const POLICY_VERSION: &str = "2012-10-17";
const EFFECT_ALLOW: &str = "Allow";

/// AWS IAM Policy document.
#[derive(Debug, Clone, Serialize)]
pub struct IamPolicy {
    #[serde(rename = "Version")]
    pub version: &'static str,
    #[serde(rename = "Statement")]
    pub statement: Vec<Statement>,
}

/// A single IAM policy statement.
#[derive(Debug, Clone, Serialize)]
pub struct Statement {
    #[serde(rename = "Effect")]
    pub effect: &'static str,
    #[serde(rename = "Action")]
    pub action: Vec<String>,
    #[serde(rename = "Resource")]
    pub resource: String,
    #[serde(rename = "Condition", skip_serializing_if = "Option::is_none")]
    pub condition: Option<Condition>,
}

/// Condition block on a Statement. Currently only carries `StringLike`.
#[derive(Debug, Clone, Serialize)]
pub struct Condition {
    #[serde(rename = "StringLike")]
    pub string_like: StringLike,
}

/// The `StringLike` operator body; only `s3:prefix` is used today.
#[derive(Debug, Clone, Serialize)]
pub struct StringLike {
    #[serde(rename = "s3:prefix")]
    pub s3_prefix: Vec<String>,
}

/// Build a bucket-or-prefix scoped IAM policy.
///
/// - `bucket`: bucket name (no `arn:` prefix)
/// - `prefix`: optional key-prefix (e.g. `"media/photos"`) — no trailing slash
/// - `flags`: bitmap from `share_access_flags` (LIST/GET/PUT/DELETE)
///
/// Emits at most two statements: object-actions on `arn:aws:s3:::<bucket>/...`
/// and — when LIST is set — an `s3:ListBucket` statement on the bucket ARN.
/// If `prefix` is present, the ListBucket statement carries an `s3:prefix`
/// Condition so the member cannot enumerate the bucket root.
pub fn build_policy(bucket: &str, prefix: Option<&str>, flags: i64) -> IamPolicy {
    debug_assert!(
        prefix.is_none_or(|p| !p.ends_with('/')),
        "prefix must not end with '/' — caller should trim before passing",
    );

    let mut statements = Vec::new();

    let object_actions = collect_object_actions(flags);
    if !object_actions.is_empty() {
        let object_resource = match prefix {
            Some(p) => format!("arn:aws:s3:::{bucket}/{p}/*"),
            None => format!("arn:aws:s3:::{bucket}/*"),
        };
        statements.push(Statement {
            effect: EFFECT_ALLOW,
            action: object_actions,
            resource: object_resource,
            condition: None,
        });
    }

    if has_flag(flags, LIST) {
        statements.push(Statement {
            effect: EFFECT_ALLOW,
            action: vec!["s3:ListBucket".to_string()],
            resource: format!("arn:aws:s3:::{bucket}"),
            condition: prefix.map(prefix_condition),
        });
    }

    IamPolicy {
        version: POLICY_VERSION,
        statement: statements,
    }
}

/// Build an object-scoped IAM policy (single object key, no ListBucket).
///
/// The member already knows the full object key from the share config, so
/// enumeration is neither useful nor granted.
pub fn build_object_policy(bucket: &str, object_key: &str, flags: i64) -> IamPolicy {
    debug_assert!(
        !object_key.starts_with('/'),
        "object_key must not start with '/' — pass the raw S3 key",
    );

    let object_actions = collect_object_actions(flags);
    let mut statements = Vec::new();
    if !object_actions.is_empty() {
        statements.push(Statement {
            effect: EFFECT_ALLOW,
            action: object_actions,
            resource: format!("arn:aws:s3:::{bucket}/{object_key}"),
            condition: None,
        });
    }
    IamPolicy {
        version: POLICY_VERSION,
        statement: statements,
    }
}

/// Map the GET/PUT/DELETE bits to their S3 IAM actions.
/// LIST is handled separately because it lives on a different Resource.
fn collect_object_actions(flags: i64) -> Vec<String> {
    let mut actions = Vec::new();
    if has_flag(flags, GET) {
        actions.push("s3:GetObject".to_string());
    }
    if has_flag(flags, PUT) {
        actions.push("s3:PutObject".to_string());
        actions.push("s3:AbortMultipartUpload".to_string());
    }
    if has_flag(flags, DELETE) {
        actions.push("s3:DeleteObject".to_string());
    }
    actions
}

fn prefix_condition(prefix: &str) -> Condition {
    Condition {
        string_like: StringLike {
            s3_prefix: vec![
                format!("{prefix}/*"),
                format!("{prefix}/"),
                prefix.to_string(),
            ],
        },
    }
}

#[cfg(test)]
mod tests;
