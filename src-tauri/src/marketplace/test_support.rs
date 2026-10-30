//! A scripted marketplace for the tests (T013): a local `wiremock` server that speaks the protocol
//! of contracts/marketplace-api.md, optionally behind a path prefix, with every request recorded.
//! The fixtures have the shape of the live answers of 2026-10-28; ids and checksums are made up.
//! No test talks to the internet.

use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Value};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

use crate::passwords::test_support::open_test_vault;
use crate::vault_gate::{VaultDb, VaultGate};

/// A vault with the tables of the marketplace and the password manager; keep the directory alive.
pub fn vault() -> (tempfile::TempDir, VaultDb) {
    let (dir, db) = open_test_vault();
    let vault = VaultGate::new().vault_db(Arc::new(db)).unwrap();
    (dir, vault)
}

/// A marketplace served on `127.0.0.1` under `prefix` (empty or `/market`).
pub struct TestMarketplace {
    pub server: MockServer,
    prefix: String,
}

impl TestMarketplace {
    pub async fn start() -> Self {
        Self::with_prefix("").await
    }

    pub async fn with_prefix(prefix: &str) -> Self {
        Self {
            server: MockServer::start().await,
            prefix: prefix.trim_end_matches('/').to_owned(),
        }
    }

    /// The address a marketplace row would hold.
    pub fn base_url(&self) -> String {
        format!("{}{}", self.server.uri(), self.prefix)
    }

    /// The address of this server without the prefix.
    pub fn origin(&self) -> String {
        self.server.uri()
    }

    fn full_path(&self, route: &str) -> String {
        format!("{}{}", self.prefix, route)
    }

    /// Answers `GET route` with `status` and a JSON body.
    pub async fn json(&self, route: &str, status: u16, body: Value) {
        self.reply(route, ResponseTemplate::new(status).set_body_json(body))
            .await;
    }

    /// Answers `GET route` with a raw body (a broken answer).
    pub async fn raw(&self, route: &str, status: u16, body: &str) {
        self.reply(
            route,
            ResponseTemplate::new(status)
                .set_body_raw(body.as_bytes().to_vec(), "application/json"),
        )
        .await;
    }

    /// Answers `GET route` with the given template after `delay`.
    pub async fn delayed(&self, route: &str, body: Value, delay: Duration) {
        self.reply(
            route,
            ResponseTemplate::new(200)
                .set_body_json(body)
                .set_delay(delay),
        )
        .await;
    }

    /// Answers `GET route` with a redirect to `location`.
    pub async fn redirect(&self, route: &str, location: &str) {
        self.reply(
            route,
            ResponseTemplate::new(302).insert_header("Location", location),
        )
        .await;
    }

    /// Answers `GET route` with a template of the caller's own.
    pub async fn reply(&self, route: &str, response: ResponseTemplate) {
        Mock::given(method("GET"))
            .and(path(self.full_path(route)))
            .respond_with(response)
            .mount(&self.server)
            .await;
    }

    /// `GET /health`: `{ "status": "ok" }`.
    pub async fn healthy(&self) {
        self.json("/health", 200, json!({ "status": "ok" })).await;
    }

    /// Every request received so far, in order.
    pub async fn requests(&self) -> Vec<Request> {
        self.server.received_requests().await.unwrap_or_default()
    }

    /// How many requests reached `route`.
    pub async fn hits(&self, route: &str) -> usize {
        let wanted = self.full_path(route);
        self.requests()
            .await
            .iter()
            .filter(|request| request.url.path() == wanted)
            .count()
    }
}

/// A version of a list entry: size and checksum made up from `seed`.
pub fn version(number: &str, seed: u8) -> Value {
    json!({
        "id": format!("v-{number}"),
        "version": number,
        "changelog": null,
        "bundleSize": 1000 + u64::from(seed),
        "bundleHash": format!("{seed:02x}").repeat(32),
        "permissions": null,
        "minAppVersion": null,
        "maxAppVersion": null,
        "downloads": 0,
        "publishedAt": "2026-10-27T09:19:04.962Z"
    })
}

/// An entry of `GET /extensions`, newest version first as the live service sends them.
pub fn entry(slug: &str, downloads: u64, versions: Vec<Value>) -> Value {
    json!({
        "id": format!("id-{slug}"),
        "extensionId": format!("haex/{slug}"),
        "name": slug,
        "slug": slug,
        "shortDescription": format!("The extension {slug}"),
        "iconUrl": null,
        "verified": false,
        "totalDownloads": downloads,
        "averageRating": null,
        "reviewCount": 0,
        "tags": [],
        "publishedAt": null,
        "publisher": { "displayName": "haex", "slug": "haex", "verified": false },
        "category": null,
        "versions": versions
    })
}

/// A page of `GET /extensions`.
pub fn page(entries: Vec<Value>, page: u32, total_pages: u32, total: u64) -> Value {
    json!({
        "extensions": entries,
        "pagination": { "page": page, "limit": 20, "total": total, "totalPages": total_pages }
    })
}
