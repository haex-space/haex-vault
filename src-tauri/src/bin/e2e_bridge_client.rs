//! A stand-in for haex-pass-browser in the end-to-end scenario `browser-bridge` (spec 049, T035):
//! connects to the External Bridge like the extension (origin `chrome-extension://e2e`, X25519 key
//! from a seed, protocol v2 with a password declaration), waits for the approval and prints what it
//! hears as one line per event, `BRIDGE {json}`. Built only with `--features e2e`.
//!
//! Usage: `e2e_bridge_client <port> <seed 0-255> [--hold] [--request <action> <json payload>]`.
//! Without `--hold` it ends once it is approved or turned away (after the request, if one is given);
//! with it, it stays until the bridge closes the connection.

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use curve25519_dalek::montgomery::MontgomeryPoint;
use futures::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message;

use holzi_lib::browser_bridge::envelope::{client_id_of, decode_key, seal_with, ServerKeys};

struct Args {
    port: u16,
    seed: u8,
    hold: bool,
    request: Option<(String, Value)>,
}

fn parse_args() -> Result<Args, String> {
    let mut args = std::env::args().skip(1);
    let port = args
        .next()
        .and_then(|p| p.parse().ok())
        .ok_or("the first argument is the port")?;
    let seed = args
        .next()
        .and_then(|s| s.parse().ok())
        .ok_or("the second argument is a seed 0-255")?;
    let mut hold = false;
    let mut request = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--hold" => hold = true,
            "--request" => {
                let action = args.next().ok_or("--request needs an action")?;
                let payload = args.next().ok_or("--request needs a JSON payload")?;
                let payload: Value =
                    serde_json::from_str(&payload).map_err(|e| format!("payload: {e}"))?;
                request = Some((action, payload));
            }
            other => return Err(format!("unknown argument {other}")),
        }
    }
    Ok(Args {
        port,
        seed,
        hold,
        request,
    })
}

fn say(event: Value) {
    println!("BRIDGE {event}");
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let args = match parse_args() {
        Ok(args) => args,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    };
    if let Err(error) = run(args).await {
        say(json!({ "event": "failed", "reason": error }));
        std::process::exit(1);
    }
}

async fn run(args: Args) -> Result<(), String> {
    let secret = [args.seed; 32];
    let public = MontgomeryPoint::mul_base_clamped(secret).to_bytes();
    let client_id = client_id_of(&public).map_err(|e| e.to_string())?;
    let mut request = format!("ws://127.0.0.1:{}/", args.port)
        .into_client_request()
        .map_err(|e| e.to_string())?;
    request.headers_mut().insert(
        "Origin",
        "chrome-extension://e2e"
            .parse()
            .map_err(|_| "origin header")?,
    );
    let stream = TcpStream::connect(("127.0.0.1", args.port))
        .await
        .map_err(|e| e.to_string())?;
    let (mut socket, _) = tokio_tungstenite::client_async(request, stream)
        .await
        .map_err(|e| e.to_string())?;
    let handshake = json!({
        "type": "handshake",
        "version": 2,
        "client": {
            "clientId": client_id,
            "clientName": "haex-pass Browser Extension",
            "publicKey": STANDARD.encode(public),
            "requestedExtensions": [{ "name": "core", "extensionPublicKey": "__core__" }],
            "permissions": { "core": { "passwords": [{ "target": "*", "operation": "readWrite" }] } }
        }
    });
    socket
        .send(Message::text(handshake.to_string()))
        .await
        .map_err(|e| e.to_string())?;

    let mut server_key = None;
    let mut asked = false;
    while let Some(frame) = socket.next().await {
        let Ok(Message::Text(text)) = frame else {
            continue;
        };
        let frame: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
        let authorized = match frame["type"].as_str() {
            Some("handshakeResponse") => {
                server_key = frame["serverPublicKey"].as_str().map(str::to_owned);
                say(json!({
                    "event": "handshake",
                    "clientId": client_id,
                    "authorized": frame["authorized"],
                    "pending": frame["pendingApproval"],
                }));
                frame["authorized"] == true
            }
            Some("authorizationUpdate") => {
                say(json!({ "event": "authorization", "authorized": frame["authorized"] }));
                frame["authorized"] == true
            }
            Some("response") => {
                let plain = ServerKeys::from_secret(secret)
                    .open_request(
                        frame["publicKey"].as_str().unwrap_or_default(),
                        frame["iv"].as_str().unwrap_or_default(),
                        frame["message"].as_str().unwrap_or_default(),
                    )
                    .map_err(|e| e.to_string())?;
                let reply: Value = serde_json::from_slice(&plain).map_err(|e| e.to_string())?;
                say(json!({ "event": "reply", "reply": reply }));
                if !args.hold {
                    break;
                }
                false
            }
            _ => {
                say(json!({ "event": "frame", "frame": frame }));
                false
            }
        };
        if authorized && !asked {
            match (&args.request, &server_key) {
                (Some((action, payload)), Some(key)) => {
                    asked = true;
                    let sealed = seal_request(key, payload)?;
                    let envelope = json!({
                        "type": "request", "action": action,
                        "message": sealed.0, "iv": sealed.1, "clientId": client_id,
                        "publicKey": sealed.2,
                        "extensionPublicKey": "__core__", "extensionName": "core"
                    });
                    socket
                        .send(Message::text(envelope.to_string()))
                        .await
                        .map_err(|e| e.to_string())?;
                }
                _ if !args.hold => break,
                _ => {}
            }
        }
    }
    say(json!({ "event": "closed" }));
    Ok(())
}

/// Seals a request with a fresh key and a random IV, as `background/crypto.ts` does.
fn seal_request(server_key: &str, payload: &Value) -> Result<(String, String, String), String> {
    let server = decode_key(server_key).map_err(|e| e.to_string())?;
    let mut ephemeral = [0u8; 32];
    let mut iv = [0u8; 12];
    getrandom::fill(&mut ephemeral).map_err(|e| e.to_string())?;
    getrandom::fill(&mut iv).map_err(|e| e.to_string())?;
    let mut payload = payload.clone();
    payload["requestId"] = json!(format!("e2e-{}", u32::from(iv[0])));
    let sealed = seal_with(ephemeral, iv, &server, payload.to_string().as_bytes())
        .map_err(|e| e.to_string())?;
    Ok((sealed.message, sealed.iv, sealed.public_key))
}
