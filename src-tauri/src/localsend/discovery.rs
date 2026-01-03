//! Multicast UDP device discovery for LocalSend
//!
//! Devices announce themselves via multicast UDP on port 53317 to address 224.0.0.167.
//! When a device receives an announcement with `announce: true`, it responds either:
//! - Via HTTP POST to /api/localsend/v2/register, or
//! - Via multicast with `announce: false`

use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::UdpSocket;
use tokio::sync::oneshot;
use tauri::{AppHandle, Emitter, Manager, State};

use super::crypto::get_local_ip_addresses;
use super::error::LocalSendError;
use super::protocol::DeviceAnnouncement;
use super::types::{now_millis, Device, DeviceType};
use super::{LocalSendState, DEFAULT_PORT, MULTICAST_ADDR, PROTOCOL_VERSION};
use crate::AppState;

/// How often to send discovery announcements (in seconds)
const ANNOUNCE_INTERVAL_SECS: u64 = 5;

/// How long until a device is considered "lost" (in seconds)
const DEVICE_TIMEOUT_SECS: u64 = 15;

/// Event names for discovery
pub const EVENT_DEVICE_DISCOVERED: &str = "localsend:device-discovered";
pub const EVENT_DEVICE_LOST: &str = "localsend:device-lost";

/// Start device discovery
pub async fn start_discovery(
    app_handle: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), LocalSendError> {
    let ls_state = state.localsend.read().await;

    // Check if already running
    if *ls_state.discovery_running.read().await {
        return Err(LocalSendError::DiscoveryAlreadyRunning);
    }

    // Create shutdown channel
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    *ls_state.discovery_shutdown.write().await = Some(shutdown_tx);
    *ls_state.discovery_running.write().await = true;

    // Get our device info for announcements
    let device_info = ls_state.device_info.read().await.clone();
    let our_fingerprint = device_info.fingerprint.clone();

    // Clone what we need for the async task
    let devices = ls_state.devices.clone();
    let app_handle_clone = app_handle.clone();

    // Spawn discovery task
    tokio::spawn(async move {
        if let Err(e) = run_discovery(
            app_handle_clone,
            devices,
            device_info,
            our_fingerprint,
            shutdown_rx,
        )
        .await
        {
            eprintln!("[LocalSend Discovery] Error: {}", e);
        }
    });

    Ok(())
}

/// Stop device discovery
pub async fn stop_discovery(state: State<'_, AppState>) -> Result<(), LocalSendError> {
    let ls_state = state.localsend.read().await;

    if !*ls_state.discovery_running.read().await {
        return Err(LocalSendError::DiscoveryNotRunning);
    }

    // Send shutdown signal
    if let Some(tx) = ls_state.discovery_shutdown.write().await.take() {
        let _ = tx.send(());
    }

    *ls_state.discovery_running.write().await = false;

    Ok(())
}

/// Get list of discovered devices
pub async fn get_devices(state: State<'_, AppState>) -> Result<Vec<Device>, LocalSendError> {
    let ls_state = state.localsend.read().await;
    let devices = ls_state.devices.read().await;
    Ok(devices.values().cloned().collect())
}

/// Run the discovery loop
async fn run_discovery(
    app_handle: AppHandle,
    devices: Arc<tokio::sync::RwLock<std::collections::HashMap<String, Device>>>,
    device_info: super::types::DeviceInfo,
    our_fingerprint: String,
    mut shutdown_rx: oneshot::Receiver<()>,
) -> Result<(), LocalSendError> {
    // Parse multicast address
    let multicast_addr: Ipv4Addr = MULTICAST_ADDR
        .parse()
        .map_err(|e| LocalSendError::NetworkError(format!("Invalid multicast address: {e}")))?;

    // Bind to any address on the discovery port
    let socket = UdpSocket::bind(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, DEFAULT_PORT))
        .await
        .map_err(|e| LocalSendError::NetworkError(format!("Failed to bind UDP socket: {e}")))?;

    // Join multicast group
    socket
        .join_multicast_v4(multicast_addr, Ipv4Addr::UNSPECIFIED)
        .map_err(|e| LocalSendError::NetworkError(format!("Failed to join multicast group: {e}")))?;

    // Enable broadcast
    socket
        .set_broadcast(true)
        .map_err(|e| LocalSendError::NetworkError(format!("Failed to set broadcast: {e}")))?;

    println!(
        "[LocalSend Discovery] Started on port {}, multicast group {}",
        DEFAULT_PORT, MULTICAST_ADDR
    );

    // Create our announcement message
    let announcement = DeviceAnnouncement {
        alias: device_info.alias.clone(),
        version: PROTOCOL_VERSION.to_string(),
        device_model: device_info.device_model.clone(),
        device_type: Some(device_info.device_type.clone()),
        fingerprint: device_info.fingerprint.clone(),
        port: device_info.port,
        protocol: device_info.protocol.clone(),
        download: device_info.download,
        announce: true,
    };

    let announcement_json = serde_json::to_vec(&announcement)?;
    let multicast_dest = SocketAddrV4::new(multicast_addr, DEFAULT_PORT);

    // Buffer for receiving messages
    let mut buf = vec![0u8; 65535];

    // Intervals
    let mut announce_interval = tokio::time::interval(Duration::from_secs(ANNOUNCE_INTERVAL_SECS));
    let mut cleanup_interval = tokio::time::interval(Duration::from_secs(DEVICE_TIMEOUT_SECS / 2));

    loop {
        tokio::select! {
            // Shutdown signal
            _ = &mut shutdown_rx => {
                println!("[LocalSend Discovery] Shutting down");
                // Leave multicast group
                let _ = socket.leave_multicast_v4(multicast_addr, Ipv4Addr::UNSPECIFIED);
                break;
            }

            // Send periodic announcements
            _ = announce_interval.tick() => {
                if let Err(e) = socket.send_to(&announcement_json, multicast_dest).await {
                    eprintln!("[LocalSend Discovery] Failed to send announcement: {}", e);
                }
            }

            // Cleanup stale devices
            _ = cleanup_interval.tick() => {
                cleanup_stale_devices(&app_handle, &devices, DEVICE_TIMEOUT_SECS).await;
            }

            // Receive messages
            result = socket.recv_from(&mut buf) => {
                match result {
                    Ok((len, src_addr)) => {
                        if let Err(e) = handle_message(
                            &app_handle,
                            &devices,
                            &our_fingerprint,
                            &buf[..len],
                            src_addr,
                            &socket,
                            &announcement,
                        ).await {
                            eprintln!("[LocalSend Discovery] Error handling message from {}: {}", src_addr, e);
                        }
                    }
                    Err(e) => {
                        eprintln!("[LocalSend Discovery] Receive error: {}", e);
                    }
                }
            }
        }
    }

    Ok(())
}

/// Handle an incoming discovery message
async fn handle_message(
    app_handle: &AppHandle,
    devices: &Arc<tokio::sync::RwLock<std::collections::HashMap<String, Device>>>,
    our_fingerprint: &str,
    data: &[u8],
    src_addr: SocketAddr,
    socket: &UdpSocket,
    our_announcement: &DeviceAnnouncement,
) -> Result<(), LocalSendError> {
    // Parse the announcement
    let announcement: DeviceAnnouncement = serde_json::from_slice(data)?;

    // Ignore our own announcements
    if announcement.fingerprint == our_fingerprint {
        return Ok(());
    }

    // Create device from announcement
    let device = Device {
        alias: announcement.alias.clone(),
        version: announcement.version.clone(),
        device_model: announcement.device_model.clone(),
        device_type: announcement.device_type.clone().unwrap_or(DeviceType::Desktop),
        fingerprint: announcement.fingerprint.clone(),
        address: src_addr.ip().to_string(),
        port: announcement.port,
        protocol: announcement.protocol.clone(),
        download: announcement.download,
        last_seen: now_millis(),
    };

    // Check if this is a new device
    let is_new = {
        let devices_read = devices.read().await;
        !devices_read.contains_key(&device.fingerprint)
    };

    // Update device in our list
    {
        let mut devices_write = devices.write().await;
        devices_write.insert(device.fingerprint.clone(), device.clone());
    }

    // Emit event for new devices
    if is_new {
        println!(
            "[LocalSend Discovery] New device: {} ({})",
            device.alias, device.address
        );
        let _ = app_handle.emit(EVENT_DEVICE_DISCOVERED, &device);
    }

    // If this was an announcement (not a response), send our info back
    if announcement.announce {
        let response = DeviceAnnouncement {
            announce: false,
            ..our_announcement.clone()
        };
        let response_json = serde_json::to_vec(&response)?;

        // Send response directly to the sender
        let dest = SocketAddrV4::new(
            match src_addr.ip() {
                std::net::IpAddr::V4(v4) => v4,
                _ => return Ok(()), // Skip IPv6 for now
            },
            DEFAULT_PORT,
        );

        socket.send_to(&response_json, dest).await?;
    }

    Ok(())
}

/// Remove devices that haven't been seen recently
async fn cleanup_stale_devices(
    app_handle: &AppHandle,
    devices: &Arc<tokio::sync::RwLock<std::collections::HashMap<String, Device>>>,
    timeout_secs: u64,
) {
    let now = now_millis();
    let timeout_millis = timeout_secs * 1000;

    let stale_fingerprints: Vec<String> = {
        let devices_read = devices.read().await;
        devices_read
            .iter()
            .filter(|(_, d)| now - d.last_seen > timeout_millis)
            .map(|(fp, _)| fp.clone())
            .collect()
    };

    if !stale_fingerprints.is_empty() {
        let mut devices_write = devices.write().await;
        for fingerprint in stale_fingerprints {
            if let Some(device) = devices_write.remove(&fingerprint) {
                println!(
                    "[LocalSend Discovery] Device lost: {} ({})",
                    device.alias, device.address
                );
                let _ = app_handle.emit(EVENT_DEVICE_LOST, &fingerprint);
            }
        }
    }
}

/// Manually register a device (for HTTP fallback discovery)
pub async fn register_device(
    app_handle: &AppHandle,
    devices: &Arc<tokio::sync::RwLock<std::collections::HashMap<String, Device>>>,
    announcement: DeviceAnnouncement,
    address: String,
) -> Result<(), LocalSendError> {
    let device = Device {
        alias: announcement.alias,
        version: announcement.version,
        device_model: announcement.device_model,
        device_type: announcement.device_type.unwrap_or(DeviceType::Desktop),
        fingerprint: announcement.fingerprint.clone(),
        address,
        port: announcement.port,
        protocol: announcement.protocol,
        download: announcement.download,
        last_seen: now_millis(),
    };

    let is_new = {
        let devices_read = devices.read().await;
        !devices_read.contains_key(&device.fingerprint)
    };

    {
        let mut devices_write = devices.write().await;
        devices_write.insert(device.fingerprint.clone(), device.clone());
    }

    if is_new {
        let _ = app_handle.emit(EVENT_DEVICE_DISCOVERED, &device);
    }

    Ok(())
}
