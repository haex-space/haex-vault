//! The passkey methods of the bridge (spec 049, US4, FR-030 to FR-036, research R11,
//! `contracts/wire-protocol.md`): creating and using a passkey through the passkey service of 036,
//! with a presence confirmation per ceremony, and listing the passkeys of a site. Everything that
//! would fail anyway is refused before anyone is asked: the origin, an excluded credential, no
//! candidate, an unsupported algorithm. The client speaks standard Base64, the service Base64URL;
//! this module translates at the edge (FR-035).

use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use base64::Engine;
use serde::Deserialize;
use serde_json::{json, Value};

use super::items::{input, site_of_address, UNTITLED};
use super::presence::{self, Ceremony};
use super::{service_of, to_code, BoxedOutcome, Call, Outcome};
use crate::browser_bridge::prompts::{PresenceCeremony, PresenceChoice};
use crate::browser_bridge::protocol::ErrorCode;
use crate::passwords::model_passkeys::{
    NewPasskeyEntry, PasskeyConfirmRequest, PasskeyCreateRequest, PasskeyHeader,
    PasskeyListRequest, PasskeyTarget,
};
use crate::passwords::service::Headers;
use crate::passwords::site::site_of;
use crate::passwords::webauthn;

/// For the contract test: where these handlers live (FR-041).
pub const MODULE: &str = module_path!();

/// ES256 only for new passkeys (FR-034).
const ES256: i64 = -7;

/// The fields `passkey-create` reads; serde passes over every other, also an `itemId` a page may
/// slip in (FR-035).
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateInput {
    relying_party_id: String,
    #[serde(default)]
    relying_party_name: Option<String>,
    user_handle: String,
    user_name: String,
    #[serde(default)]
    user_display_name: Option<String>,
    challenge: String,
    #[serde(default)]
    exclude_credentials: Vec<String>,
    #[serde(default)]
    user_verification: Option<String>,
    #[serde(default)]
    origin: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GetInput {
    relying_party_id: String,
    challenge: String,
    #[serde(default)]
    allow_credentials: Option<Vec<AllowedCredential>>,
    #[serde(default)]
    user_verification: Option<String>,
    #[serde(default)]
    origin: Option<String>,
}

#[derive(Deserialize)]
struct AllowedCredential {
    id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListInput {
    relying_party_id: String,
}

/// Standard Base64 from the client to bytes.
fn bytes_of(text: &str) -> Result<Vec<u8>, ErrorCode> {
    STANDARD
        .decode(text.trim())
        .map_err(|_| ErrorCode::InvalidRequest)
}

/// Standard Base64 from the client to the service's Base64URL.
fn to_url(text: &str) -> Result<String, ErrorCode> {
    Ok(URL_SAFE_NO_PAD.encode(bytes_of(text)?))
}

/// The service's Base64URL to standard Base64 for the client.
fn to_std(text: &str) -> Result<String, ErrorCode> {
    URL_SAFE_NO_PAD
        .decode(text)
        .map(|bytes| STANDARD.encode(bytes))
        .map_err(|_| ErrorCode::ServerError)
}

/// The page's origin for the relying party; missing or foreign, nobody is asked (FR-032).
fn checked_origin(origin: Option<&str>, rp_id: &str) -> Result<(String, String), ErrorCode> {
    origin
        .and_then(|origin| webauthn::checked_origin(origin, rp_id))
        .ok_or(ErrorCode::OriginMismatch)
}

fn requires_uv(user_verification: Option<&str>) -> bool {
    user_verification == Some("required")
}

/// The passkeys of a relying party the caller reaches (FR-036).
async fn reachable(call: &Call<'_>, rp_id: &str) -> Result<Vec<PasskeyHeader>, ErrorCode> {
    service_of(call.core)
        .passkey_list(
            &call.caller,
            &call.approval.grants(),
            PasskeyListRequest {
                rp_id: Some(rp_id.to_owned()),
                ..PasskeyListRequest::default()
            },
        )
        .await
        .map_err(to_code)
}

/// The entries the caller reaches, as (id, title, url).
async fn entries(call: &Call<'_>) -> Result<Vec<(String, String, Option<String>)>, ErrorCode> {
    let headers = service_of(call.core)
        .list_headers(&call.caller, &call.approval.grants())
        .await
        .map_err(to_code)?;
    let Headers::Items(headers) = headers else {
        return Ok(Vec::new());
    };
    Ok(headers
        .into_iter()
        .map(|h| (h.id, h.title.unwrap_or_else(|| UNTITLED.to_owned()), h.url))
        .collect())
}

pub fn passkey_create(call: Call<'_>) -> BoxedOutcome<'_> {
    Box::pin(passkey_create_async(call))
}

async fn passkey_create_async(mut call: Call<'_>) -> Outcome {
    let request: CreateInput = input(std::mem::take(&mut call.payload))?;
    let (origin, rp_id) = checked_origin(request.origin.as_deref(), &request.relying_party_id)?;
    let challenge = bytes_of(&request.challenge)?;
    let user_handle = to_url(&request.user_handle)?;
    let excluded = request
        .exclude_credentials
        .iter()
        .map(|id| to_url(id))
        .collect::<Result<Vec<_>, _>>()?;
    // An excluded credential the caller reaches ends it without a question (Edge Case); the
    // service checks again in its transaction.
    if reachable(&call, &rp_id)
        .await?
        .iter()
        .any(|known| excluded.contains(&known.credential_id))
    {
        return Err(ErrorCode::PasskeyExcluded);
    }
    let site = site_of(&rp_id);
    let mut choices = vec![PresenceChoice::NewEntry];
    for (item_id, title, url) in entries(&call).await? {
        let same_site = url.as_deref().and_then(site_of_address);
        if site.is_some() && same_site == site {
            choices.push(PresenceChoice::Entry {
                item_id,
                title,
                url,
            });
        }
    }
    let rp_name = request
        .relying_party_name
        .clone()
        .filter(|name| !name.trim().is_empty());
    let confirmed = presence::confirm(
        &call,
        Ceremony {
            kind: PresenceCeremony::Create,
            rp_id: rp_id.clone(),
            rp_name: rp_name.clone(),
            origin: origin.clone(),
            user_name: Some(request.user_name.clone()),
            require_uv: requires_uv(request.user_verification.as_deref()),
            challenge: challenge.clone(),
            choices,
        },
    )
    .await?;
    // Nothing after the end of the connection (FR-031).
    if call.cancel.is_cancelled() {
        return Err(ErrorCode::UserCancelled);
    }
    let target = match confirmed.choice {
        PresenceChoice::NewEntry => PasskeyTarget::New(NewPasskeyEntry {
            title: rp_name.clone().unwrap_or_else(|| rp_id.clone()),
            url: Some(origin.clone()),
            username: Some(request.user_name.clone()),
            tags: call.approval.default_tag.iter().cloned().collect(),
        }),
        PresenceChoice::Entry { item_id, .. } => PasskeyTarget::Existing(item_id),
        PresenceChoice::Passkey { .. } => return Err(ErrorCode::UserCancelled),
    };
    let created = service_of(call.core)
        .passkey_create(
            &call.caller,
            &call.approval.grants(),
            PasskeyCreateRequest {
                target,
                rp_id: rp_id.clone(),
                rp_name: rp_name.unwrap_or(rp_id),
                origin,
                user_handle,
                user_name: request.user_name,
                user_display_name: request.user_display_name,
                challenge: URL_SAFE_NO_PAD.encode(&challenge),
                exclude_credentials: excluded,
                pub_key_cred_params: vec![ES256],
                discoverable: true,
                presence: confirmed.presence,
            },
        )
        .await
        .map_err(to_code)?;
    Ok(json!({
        "credentialId": to_std(&created.credential_id)?,
        "publicKey": to_std(&created.public_key_spki)?,
        "attestationObject": to_std(&created.attestation_object)?,
        "clientDataJson": to_std(&created.client_data_json)?,
        "transports": ["internal"],
        "passkeyId": created.passkey_id,
    }))
}

pub fn passkey_get(call: Call<'_>) -> BoxedOutcome<'_> {
    Box::pin(passkey_get_async(call))
}

async fn passkey_get_async(mut call: Call<'_>) -> Outcome {
    let request: GetInput = input(std::mem::take(&mut call.payload))?;
    let (origin, rp_id) = checked_origin(request.origin.as_deref(), &request.relying_party_id)?;
    let challenge = bytes_of(&request.challenge)?;
    let allowed = request
        .allow_credentials
        .unwrap_or_default()
        .iter()
        .map(|credential| to_url(&credential.id))
        .collect::<Result<Vec<_>, _>>()?;
    let candidates: Vec<PasskeyHeader> = reachable(&call, &rp_id)
        .await?
        .into_iter()
        .filter(|known| {
            if allowed.is_empty() {
                known.is_discoverable
            } else {
                allowed.contains(&known.credential_id)
            }
        })
        .collect();
    if candidates.is_empty() {
        return Err(ErrorCode::NotFound);
    }
    let usable: Vec<PasskeyHeader> = candidates
        .into_iter()
        .filter(|known| webauthn::choose_algorithm(&[known.algorithm]).is_some())
        .collect();
    if usable.is_empty() {
        return Err(ErrorCode::UnsupportedAlgorithm);
    }
    let titles = entries(&call).await?;
    let title_of = |item_id: &str| {
        titles
            .iter()
            .find(|(id, _, _)| id == item_id)
            .map(|(_, title, _)| title.clone())
    };
    let choices = usable
        .iter()
        .map(|known| PresenceChoice::Passkey {
            passkey_id: known.id.clone(),
            user_name: known.user_name.clone(),
            item_title: title_of(&known.item_id),
        })
        .collect();
    let confirmed = presence::confirm(
        &call,
        Ceremony {
            kind: PresenceCeremony::Get,
            rp_id: rp_id.clone(),
            rp_name: usable[0].rp_name.clone(),
            origin: origin.clone(),
            user_name: (usable.len() == 1)
                .then(|| usable[0].user_name.clone())
                .flatten(),
            require_uv: requires_uv(request.user_verification.as_deref()),
            challenge: challenge.clone(),
            choices,
        },
    )
    .await?;
    if call.cancel.is_cancelled() {
        return Err(ErrorCode::UserCancelled);
    }
    let PresenceChoice::Passkey { passkey_id, .. } = confirmed.choice else {
        return Err(ErrorCode::UserCancelled);
    };
    let chosen = usable
        .iter()
        .find(|known| known.id == passkey_id)
        .ok_or(ErrorCode::UserCancelled)?;
    let assertion = service_of(call.core)
        .passkey_confirm(
            &call.caller,
            &call.approval.grants(),
            PasskeyConfirmRequest {
                rp_id,
                origin,
                challenge: URL_SAFE_NO_PAD.encode(&challenge),
                allow_credentials: vec![chosen.credential_id.clone()],
                item_id: None,
                presence: confirmed.presence,
            },
        )
        .await
        .map_err(to_code)?;
    let mut reply = json!({
        "credentialId": to_std(&assertion.credential_id)?,
        "authenticatorData": to_std(&assertion.authenticator_data)?,
        "signature": to_std(&assertion.signature)?,
        "clientDataJson": to_std(&assertion.client_data_json)?,
        "passkeyId": chosen.id,
    });
    if let Some(handle) = assertion.user_handle.filter(|_| chosen.is_discoverable) {
        reply["userHandle"] = Value::String(to_std(&handle)?);
    }
    Ok(reply)
}

pub fn passkey_list(call: Call<'_>) -> BoxedOutcome<'_> {
    Box::pin(passkey_list_async(call))
}

/// The headers of the passkeys in scope, never a key (FR-036).
async fn passkey_list_async(mut call: Call<'_>) -> Outcome {
    let request: ListInput = input(std::mem::take(&mut call.payload))?;
    let passkeys = reachable(&call, &request.relying_party_id)
        .await?
        .into_iter()
        .map(|known| {
            Ok(json!({
                "id": known.id,
                "credentialId": to_std(&known.credential_id)?,
                "relyingPartyId": known.rp_id,
                "relyingPartyName": known.rp_name,
                "userName": known.user_name,
                "userDisplayName": known.user_display_name,
                "nickname": known.nickname,
                "createdAt": known.created_at,
                "lastUsedAt": known.last_used_at,
                "isDiscoverable": known.is_discoverable,
                "itemId": known.item_id,
            }))
        })
        .collect::<Result<Vec<_>, ErrorCode>>()?;
    Ok(json!({ "passkeys": passkeys }))
}
