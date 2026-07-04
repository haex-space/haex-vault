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

        let client = Client::builder()
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
    async fn call(&self, params: &[(&str, &str)]) -> Result<String, IamAdapterError> {
        let (status, body) = self.signed_post(params).await?;
        if status.is_success() {
            return Ok(body);
        }
        Err(classify_error(&body, status))
    }
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

#[async_trait::async_trait]
impl IamAdapter for AwsCompatIamAdapter {
    async fn create_scoped_user(
        &self,
        user_name: &str,
        policy_name: &str,
        policy: &IamPolicy,
    ) -> Result<ScopedCred, IamAdapterError> {
        // 1. CreateUser
        self.call(&[
            ("Action", "CreateUser"),
            ("UserName", user_name),
            ("Version", IAM_API_VERSION),
        ])
        .await?;

        // 2. PutUserPolicy — serialise the policy JSON via serde.
        let policy_doc = json_to_string(policy)
            .map_err(|e| IamAdapterError::Other(format!("policy serialize: {e}")))?;
        self.call(&[
            ("Action", "PutUserPolicy"),
            ("UserName", user_name),
            ("PolicyName", policy_name),
            ("PolicyDocument", &policy_doc),
            ("Version", IAM_API_VERSION),
        ])
        .await?;

        // 3. CreateAccessKey — parse the returned XML for the id + secret.
        let body = self
            .call(&[
                ("Action", "CreateAccessKey"),
                ("UserName", user_name),
                ("Version", IAM_API_VERSION),
            ])
            .await?;

        let access_key_id = extract_xml_tag(&body, "AccessKeyId")
            .ok_or_else(|| {
                IamAdapterError::Other("CreateAccessKey: missing AccessKeyId".to_string())
            })?
            .to_string();
        let secret_access_key = extract_xml_tag(&body, "SecretAccessKey")
            .ok_or_else(|| {
                IamAdapterError::Other("CreateAccessKey: missing SecretAccessKey".to_string())
            })?
            .to_string();

        Ok(ScopedCred {
            access_key_id,
            secret_access_key,
            iam_user_name: user_name.to_string(),
        })
    }

    async fn delete_scoped_user(
        &self,
        user_name: &str,
        access_key_id: &str,
    ) -> Result<(), IamAdapterError> {
        // Idempotent cleanup — NotFound on any of the three steps is
        // treated as "already gone" and swallowed.
        let steps: &[&[(&str, &str)]] = &[
            &[
                ("Action", "DeleteAccessKey"),
                ("UserName", user_name),
                ("AccessKeyId", access_key_id),
                ("Version", IAM_API_VERSION),
            ],
            &[
                ("Action", "DeleteUserPolicy"),
                ("UserName", user_name),
                ("PolicyName", "haex-share-policy"),
                ("Version", IAM_API_VERSION),
            ],
            &[
                ("Action", "DeleteUser"),
                ("UserName", user_name),
                ("Version", IAM_API_VERSION),
            ],
        ];

        for params in steps {
            match self.call(params).await {
                Ok(_) => {}
                Err(IamAdapterError::NotFound) => {}
                Err(other) => return Err(other),
            }
        }
        Ok(())
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
