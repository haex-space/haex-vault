//! AWS-compatible IAM adapter (AWS + Wasabi).
//!
//! Speaks the classic Query API surface (`Action=…&Version=2010-05-08`,
//! POST form-urlencoded, XML response) that both AWS and Wasabi implement.
//! Signs each request with SigV4 via the [`aws_sigv4`] crate and dispatches
//! via the existing [`reqwest`] client.
//!
//! MinIO is intentionally NOT handled here — its admin API is JSON-shaped
//! and uses different endpoints (see the module doc on the parent module).
//! Attempts to construct an adapter for MinIO return an error rather than
//! silently misbehaving.

use std::time::SystemTime;

use aws_sigv4::http_request::{sign, SignableBody, SignableRequest, SigningSettings};
use reqwest::Client;
use serde_json::to_string as json_to_string;

use crate::remote_storage::iam_adapter::{IamAdapter, IamAdapterError, ScopedCred};
use crate::remote_storage::iam_policy::IamPolicy;

const IAM_API_VERSION: &str = "2010-05-08";
const IAM_SERVICE_NAME: &str = "iam";
const IAM_REGION_AWS_WASABI: &str = "us-east-1";
const AWS_IAM_ENDPOINT: &str = "https://iam.amazonaws.com";
const WASABI_IAM_ENDPOINT: &str = "https://iam.wasabisys.com";

/// Fixed name of the inline policy the adapter attaches to every scoped
/// user. Kept internal to the adapter (not caller-configurable) so that
/// `create_scoped_user` and `delete_scoped_user` stay symmetric — a
/// mismatched name here would leave an orphan policy on the user, which
/// then blocks `DeleteUser` at revoke time.
pub(crate) const HAEX_SHARE_POLICY_NAME: &str = "haex-share-policy";

/// Which vendor-specific IAM endpoint / region convention to use.
///
/// AWS and Wasabi both speak the classic XML Query API and share the
/// `us-east-1` signing region convention — only the endpoint hostname
/// differs.
///
/// `MinIO` is a placeholder variant so callers can construct the enum,
/// but [`AwsCompatIamAdapter::new`] refuses it with a clear error.
#[derive(Debug, Clone)]
pub enum ProviderFlavor {
    Aws,
    Wasabi,
    /// MinIO's admin API is JSON, not AWS-IAM-XML. Deferred to a follow-up
    /// task. The variant is kept so the enum stays a single canonical
    /// type across the sharing feature.
    MinIO {
        admin_endpoint: String,
    },
}

/// AWS-compatible IAM control-plane adapter.
///
/// Cheap to construct (no I/O); the inner `reqwest::Client` maintains a
/// connection pool across all IAM calls made on this instance.
pub struct AwsCompatIamAdapter {
    client: Client,
    access_key: String,
    secret_key: String,
    endpoint: &'static str,
    region: &'static str,
}

/// Custom `Debug` that never prints the admin credential. Keeps the adapter
/// usable with `Result::expect_err`, `dbg!`, and `tracing::debug!` without
/// leaking the secret.
impl std::fmt::Debug for AwsCompatIamAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AwsCompatIamAdapter")
            .field("access_key", &"<redacted>")
            .field("secret_key", &"<redacted>")
            .field("endpoint", &self.endpoint)
            .field("region", &self.region)
            .finish()
    }
}

impl AwsCompatIamAdapter {
    /// Construct a new adapter for the given provider flavor.
    ///
    /// Errors when `provider` is [`ProviderFlavor::MinIO`] — MinIO is
    /// intentionally not supported here (see module docs).
    pub fn new(
        access_key: &str,
        secret_key: &str,
        provider: ProviderFlavor,
    ) -> Result<Self, IamAdapterError> {
        let (endpoint, region) = match provider {
            ProviderFlavor::Aws => (AWS_IAM_ENDPOINT, IAM_REGION_AWS_WASABI),
            ProviderFlavor::Wasabi => (WASABI_IAM_ENDPOINT, IAM_REGION_AWS_WASABI),
            ProviderFlavor::MinIO { .. } => {
                return Err(IamAdapterError::Other(
                    "MinIO is not supported by AwsCompatIamAdapter; \
                     use a MinIO-specific adapter (separate D3 task)"
                        .to_string(),
                ));
            }
        };

        // Bound every IAM call: a stalled connection to the provider's IAM
        // endpoint would otherwise hang the share/revoke command forever.
        let client = Client::builder()
            .connect_timeout(std::time::Duration::from_secs(10))
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| IamAdapterError::Network(format!("reqwest client build failed: {e}")))?;

        Ok(Self {
            client,
            access_key: access_key.to_string(),
            secret_key: secret_key.to_string(),
            endpoint,
            region,
        })
    }

    /// Adapter's IAM endpoint (test hook — used by unit tests to assert
    /// the routing is correct for the constructed provider flavor).
    #[cfg(test)]
    pub(crate) fn endpoint(&self) -> &str {
        self.endpoint
    }

    /// Adapter's IAM signing region (test hook).
    #[cfg(test)]
    pub(crate) fn region(&self) -> &str {
        self.region
    }

    /// Serialize the form parameters, SigV4-sign the request, dispatch,
    /// and return the raw response body + HTTP status. Response parsing
    /// is caller's problem — each IAM Action has a distinct XML shape.
    async fn signed_post(
        &self,
        params: &[(&str, &str)],
    ) -> Result<(reqwest::StatusCode, String), IamAdapterError> {
        let body = url_encode_form(params);

        // `aws-sigv4`'s `sign()` takes SigningParams pre-wrapped as
        // `aws_sigv4::http_request::SigningParams::V4(...)`. The v4 params
        // borrow an `Identity`, built from a static `Credentials`.
        // `Credentials::new(...)` (not `from_keys`, which is feature-gated)
        // is always available. `Identity` comes from `aws-smithy-runtime-api`,
        // pulled in as a peer dep alongside `aws-credential-types`.
        let identity: aws_smithy_runtime_api::client::identity::Identity =
            aws_credential_types::Credentials::new(
                &self.access_key,
                &self.secret_key,
                None, // session token
                None, // expires_after — static admin cred
                "haex-vault",
            )
            .into();
        let signing_settings = SigningSettings::default();
        let v4_params = aws_sigv4::sign::v4::SigningParams::builder()
            .identity(&identity)
            .region(self.region)
            .name(IAM_SERVICE_NAME)
            .time(SystemTime::now())
            .settings(signing_settings)
            .build()
            .map_err(|e| IamAdapterError::Other(format!("sigv4 params: {e}")))?;
        let signing_params: aws_sigv4::http_request::SigningParams<'_> = v4_params.into();

        let headers_in: Vec<(&str, &str)> = vec![
            ("host", host_of(self.endpoint)),
            ("content-type", "application/x-www-form-urlencoded"),
        ];

        let signable = SignableRequest::new(
            "POST",
            self.endpoint,
            headers_in.iter().copied(),
            SignableBody::Bytes(body.as_bytes()),
        )
        .map_err(|e| IamAdapterError::Other(format!("sigv4 signable: {e}")))?;

        let (instructions, _sig) = sign(signable, &signing_params)
            .map_err(|e| IamAdapterError::Other(format!("sigv4 sign: {e}")))?
            .into_parts();

        let mut req = self
            .client
            .post(self.endpoint)
            .header("content-type", "application/x-www-form-urlencoded")
            .body(body);

        for (name, value) in instructions.headers() {
            req = req.header(name, value);
        }

        let resp = req
            .send()
            .await
            .map_err(|e| IamAdapterError::Network(format!("send: {e}")))?;
        let status = resp.status();
        let text = resp
            .text()
            .await
            .map_err(|e| IamAdapterError::Network(format!("body read: {e}")))?;
        Ok((status, text))
    }

    /// Dispatch a signed request and map non-2xx into the right error
    /// variant using the `<Code>` element in the XML error envelope.
    ///
    /// Also scans 2xx bodies for an embedded `<ErrorResponse>` / `<Error>`
    /// envelope: Wasabi and some reverse-proxied IAM gateways occasionally
    /// return `200 OK` with an error payload. Real AWS/Wasabi success
    /// responses never contain those elements at any depth, so their mere
    /// presence is a strong signal that the call actually failed.
    async fn call(&self, params: &[(&str, &str)]) -> Result<String, IamAdapterError> {
        let (status, body) = self.signed_post(params).await?;
        if status.is_success() {
            if body_contains_error_envelope(&body) {
                return Err(classify_error(&body, status));
            }
            return Ok(body);
        }
        Err(classify_error(&body, status))
    }
}

/// Returns true if a response body carries an IAM error envelope. Success
/// responses (`<CreateUserResponse>`, `<PutUserPolicyResponse>`, …) never
/// contain these tags, so their presence unambiguously signals failure —
/// even when the HTTP status is 2xx (Wasabi + some reverse proxies).
pub(crate) fn body_contains_error_envelope(body: &str) -> bool {
    body.contains("<ErrorResponse") || body.contains("<Error>")
}

fn host_of(endpoint: &'static str) -> &'static str {
    // strip leading "https://"; both const-endpoints we support start with it.
    endpoint.trim_start_matches("https://")
}

/// Build a `k=v&k=v` body with each key and value URL-encoded. We avoid
/// pulling in another dep by hand-rolling percent-encoding — the value set
/// we serialize is bounded (IAM action / user / policy name / policy doc).
fn url_encode_form(params: &[(&str, &str)]) -> String {
    let mut out = String::new();
    let mut first = true;
    for (k, v) in params {
        if !first {
            out.push('&');
        }
        first = false;
        out.push_str(&percent_encode(k));
        out.push('=');
        out.push_str(&percent_encode(v));
    }
    out
}

/// Percent-encode a string per RFC 3986 unreserved set. Matches the
/// canonicalisation SigV4 expects for form bodies.
fn percent_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for b in input.bytes() {
        let is_unreserved = b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~');
        if is_unreserved {
            out.push(b as char);
        } else {
            out.push('%');
            out.push_str(&format!("{:02X}", b));
        }
    }
    out
}

/// Companion to [`extract_xml_tag`] that collects EVERY occurrence of
/// `<tag>...</tag>`. Used by the rollback path to enumerate a user's access
/// keys from a `ListAccessKeys` response, where the tag legitimately repeats.
pub(crate) fn extract_all_xml_tags<'a>(xml: &'a str, tag: &str) -> Vec<&'a str> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(start) = rest.find(&open) {
        rest = &rest[start + open.len()..];
        let Some(end) = rest.find(&close) else { break };
        out.push(&rest[..end]);
        rest = &rest[end + close.len()..];
    }
    out
}

/// Very small XML scanner that extracts the text content of the first
/// occurrence of `<tag>...</tag>`. Used to pull `<AccessKeyId>` and
/// `<SecretAccessKey>` from `CreateAccessKey` responses and the `<Code>`
/// element from error envelopes.
///
/// Not a general XML parser — assumes the tag is present at most once and
/// contains no nested elements. Sufficient for the IAM response shapes
/// we consume.
pub(crate) fn extract_xml_tag<'a>(xml: &'a str, tag: &str) -> Option<&'a str> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = xml.find(&open)? + open.len();
    let rest = &xml[start..];
    let end = rest.find(&close)?;
    Some(&rest[..end])
}

/// Map an IAM error-envelope body + HTTP status to the right
/// [`IamAdapterError`] variant. AWS + Wasabi both wrap errors as
/// `<ErrorResponse><Error><Code>...</Code></Error></ErrorResponse>`.
pub(crate) fn classify_error(body: &str, status: reqwest::StatusCode) -> IamAdapterError {
    if let Some(code) = extract_xml_tag(body, "Code") {
        return match code {
            "NoSuchEntity" => IamAdapterError::NotFound,
            "AccessDenied"
            | "AccessDeniedException"
            | "UnauthorizedOperation"
            | "InvalidClientTokenId"
            | "SignatureDoesNotMatch" => IamAdapterError::AccessDenied(code.to_string()),
            other => IamAdapterError::Other(format!("iam error {other}")),
        };
    }
    IamAdapterError::Network(format!("http {status}: {body}"))
}

/// Pure step-builder for `try_cleanup_user`. Returns the ordered list of
/// IAM Action forms needed to fully revoke a scoped user. When
/// `access_key_id` is `None` the `DeleteAccessKey` step is omitted — that
/// matches the rollback scenario where `CreateAccessKey` never succeeded.
///
/// Extracted from the async method so tests can pin the step count without
/// standing up a mock HTTP endpoint.
pub(crate) fn cleanup_user_steps<'a>(
    user_name: &'a str,
    access_key_id: Option<&'a str>,
) -> Vec<Vec<(&'a str, &'a str)>> {
    let mut steps: Vec<Vec<(&'a str, &'a str)>> = Vec::with_capacity(3);
    if let Some(key_id) = access_key_id {
        steps.push(vec![
            ("Action", "DeleteAccessKey"),
            ("UserName", user_name),
            ("AccessKeyId", key_id),
            ("Version", IAM_API_VERSION),
        ]);
    }
    steps.push(vec![
        ("Action", "DeleteUserPolicy"),
        ("UserName", user_name),
        ("PolicyName", HAEX_SHARE_POLICY_NAME),
        ("Version", IAM_API_VERSION),
    ]);
    steps.push(vec![
        ("Action", "DeleteUser"),
        ("UserName", user_name),
        ("Version", IAM_API_VERSION),
    ]);
    steps
}

impl AwsCompatIamAdapter {
    /// Internal cleanup helper used by both the public `delete_scoped_user`
    /// and the rollback path in `create_scoped_user`. Skips
    /// `DeleteAccessKey` when the caller has no access-key id (which happens
    /// during rollback if `CreateAccessKey` never succeeded). Idempotent —
    /// `NoSuchEntity` on any step is swallowed.
    async fn try_cleanup_user(
        &self,
        user_name: &str,
        access_key_id: Option<&str>,
    ) -> Result<(), IamAdapterError> {
        // `None` covers two rollback shapes: CreateAccessKey never ran, OR
        // it succeeded but returned a body we couldn't parse an id out of.
        // In the latter case a live key exists that we don't know — discover
        // it via ListAccessKeys, otherwise the DeleteUser step below fails
        // (AWS refuses to delete users with keys) and the credential is
        // orphaned with no record of how to revoke it.
        if access_key_id.is_none() {
            for key_id in self.list_access_key_ids(user_name).await? {
                match self
                    .call(&[
                        ("Action", "DeleteAccessKey"),
                        ("UserName", user_name),
                        ("AccessKeyId", &key_id),
                        ("Version", IAM_API_VERSION),
                    ])
                    .await
                {
                    Ok(_) | Err(IamAdapterError::NotFound) => {}
                    Err(other) => return Err(other),
                }
            }
        }

        for step in cleanup_user_steps(user_name, access_key_id) {
            // Re-borrow the owned step into the shape `call` expects.
            let params: Vec<(&str, &str)> = step.iter().map(|(k, v)| (*k, *v)).collect();
            match self.call(&params).await {
                Ok(_) => {}
                Err(IamAdapterError::NotFound) => {}
                Err(other) => return Err(other),
            }
        }
        Ok(())
    }

    /// All access-key ids currently attached to `user_name`. A missing user
    /// yields an empty list — that matches the idempotent cleanup contract.
    async fn list_access_key_ids(&self, user_name: &str) -> Result<Vec<String>, IamAdapterError> {
        match self
            .call(&[
                ("Action", "ListAccessKeys"),
                ("UserName", user_name),
                ("Version", IAM_API_VERSION),
            ])
            .await
        {
            Ok(body) => Ok(extract_all_xml_tags(&body, "AccessKeyId")
                .into_iter()
                .map(str::to_string)
                .collect()),
            Err(IamAdapterError::NotFound) => Ok(Vec::new()),
            Err(other) => Err(other),
        }
    }
}

#[async_trait::async_trait]
impl IamAdapter for AwsCompatIamAdapter {
    async fn create_scoped_user(
        &self,
        user_name: &str,
        policy: &IamPolicy,
    ) -> Result<ScopedCred, IamAdapterError> {
        // 1. CreateUser — the "commit point" for cleanup: after this
        //    succeeds any further error must trigger rollback so we don't
        //    orphan an IAM user in the customer's AWS account.
        self.call(&[
            ("Action", "CreateUser"),
            ("UserName", user_name),
            ("Version", IAM_API_VERSION),
        ])
        .await?;

        // Steps 2 + 3. On ANY failure past this point we must roll back the
        // CreateUser above, otherwise we orphan the user in the customer's
        // AWS account. Written imperatively so the tracked `created_key_id`
        // stays a plain `Option` — nesting this in an `async` block plus a
        // `Cell` breaks the `Send` requirement on the async-trait future.
        let mut created_key_id: Option<String> = None;
        let inner: Result<ScopedCred, IamAdapterError> = 'inner: {
            // 2. PutUserPolicy — serialise the policy JSON via serde.
            let policy_doc = match json_to_string(policy) {
                Ok(s) => s,
                Err(e) => {
                    break 'inner Err(IamAdapterError::Other(format!("policy serialize: {e}")))
                }
            };
            if let Err(e) = self
                .call(&[
                    ("Action", "PutUserPolicy"),
                    ("UserName", user_name),
                    ("PolicyName", HAEX_SHARE_POLICY_NAME),
                    ("PolicyDocument", &policy_doc),
                    ("Version", IAM_API_VERSION),
                ])
                .await
            {
                break 'inner Err(e);
            }

            // 3. CreateAccessKey — parse the returned XML for the id + secret.
            let body = match self
                .call(&[
                    ("Action", "CreateAccessKey"),
                    ("UserName", user_name),
                    ("Version", IAM_API_VERSION),
                ])
                .await
            {
                Ok(b) => b,
                Err(e) => break 'inner Err(e),
            };

            let access_key_id = match extract_xml_tag(&body, "AccessKeyId") {
                Some(s) => s.to_string(),
                None => {
                    break 'inner Err(IamAdapterError::Other(
                        "CreateAccessKey: missing AccessKeyId".to_string(),
                    ))
                }
            };
            // Track it before the SecretAccessKey extraction so that if the
            // response is malformed we still know a key exists in AWS and
            // can revoke it during rollback.
            created_key_id = Some(access_key_id.clone());
            let secret_access_key = match extract_xml_tag(&body, "SecretAccessKey") {
                Some(s) => s.to_string(),
                None => {
                    break 'inner Err(IamAdapterError::Other(
                        "CreateAccessKey: missing SecretAccessKey".to_string(),
                    ))
                }
            };

            Ok(ScopedCred {
                access_key_id,
                secret_access_key,
                iam_user_name: user_name.to_string(),
            })
        };

        match inner {
            Ok(cred) => Ok(cred),
            Err(err) => {
                // Best-effort rollback. Surface the ORIGINAL error to the
                // caller regardless of rollback outcome — losing the primary
                // failure reason to a secondary cleanup error would obscure
                // the real bug.
                let key_id = created_key_id.as_deref();
                if let Err(cleanup_err) = self.try_cleanup_user(user_name, key_id).await {
                    tracing::warn!(
                        user_name = %user_name,
                        primary_error = %err,
                        cleanup_error = %cleanup_err,
                        "create_scoped_user rollback failed; IAM user may be orphaned"
                    );
                }
                Err(err)
            }
        }
    }

    async fn delete_scoped_user(
        &self,
        user_name: &str,
        access_key_id: &str,
    ) -> Result<(), IamAdapterError> {
        self.try_cleanup_user(user_name, Some(access_key_id)).await
    }

    async fn probe_iam_capability(&self) -> Result<bool, IamAdapterError> {
        // ListAccessKeys against a user name that (almost certainly)
        // does not exist. Two accepted responses:
        //   - 200 with `<ListAccessKeysResult>` — user exists (shouldn't
        //     but harmless) → we have permission.
        //   - 404/`NoSuchEntity` (→ `IamAdapterError::NotFound`) — user
        //     doesn't exist, but the *request* was authorised → true.
        // AccessDenied → false. Anything else propagates as Network/Other.
        let probe_name = "haex-iam-capability-probe-does-not-exist";
        match self
            .call(&[
                ("Action", "ListAccessKeys"),
                ("UserName", probe_name),
                ("Version", IAM_API_VERSION),
            ])
            .await
        {
            Ok(_) => Ok(true),
            Err(IamAdapterError::NotFound) => Ok(true),
            Err(IamAdapterError::AccessDenied(_)) => Ok(false),
            Err(other) => Err(other),
        }
    }
}
