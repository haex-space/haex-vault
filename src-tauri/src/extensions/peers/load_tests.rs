//! The load of a four-player game (spec 057 T024, SC-003, SC-004 baseline): a host with three
//! channels, 35 fast messages of 1,024 bytes per second in each direction for 10 seconds, on
//! loopback. At least 99 % arrive; the delay holzi adds is printed as the baseline for SC-004.

use std::time::{Duration, Instant};

use base64::engine::general_purpose::STANDARD;
use base64::Engine;

use super::channel::{Channel, MESSAGE};
use super::limits::FAST_BYTES;
use super::test_support::{open_test_channel, Admit, Vault, Window};

const PER_SECOND: u32 = 35;
const SECONDS: u32 = 10;

/// A message: the channel's number, the message's number, then filler up to the limit.
fn message(lane: u8, number: u32) -> Vec<u8> {
    let mut bytes = vec![lane];
    bytes.extend_from_slice(&number.to_be_bytes());
    bytes.resize(FAST_BYTES, 0x5a);
    bytes
}

/// When each message of `channel` arrived, by `(lane, number)`.
fn arrivals(window: &Window, channel: &Channel) -> Vec<((u8, u32), Instant)> {
    window
        .frame_events(MESSAGE)
        .into_iter()
        .filter(|(_, _, data)| data["channelId"] == channel.id.to_string())
        .filter_map(|(at, _, data)| {
            let bytes = STANDARD.decode(data["data"].as_str()?).ok()?;
            let number = u32::from_be_bytes(bytes.get(1..5)?.try_into().ok()?);
            Some(((bytes[0], number), at))
        })
        .collect()
}

fn percentile(sorted: &[Duration], share: f64) -> Duration {
    let index = ((sorted.len() as f64 - 1.0) * share).round() as usize;
    sorted.get(index).copied().unwrap_or_default()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn three_channels_carry_35_fast_messages_a_second_each_way() {
    let host = Vault::start(Admit::AtOnce).await;
    let mut players = Vec::new();
    for _ in 0..3 {
        players.push(Vault::start(Admit::AtOnce).await);
    }
    let mut lanes = Vec::new();
    for player in &players {
        let (on_player, on_host) = open_test_channel(player, &host).await;
        lanes.push((player, on_player, on_host));
    }

    let mut sent_at = std::collections::HashMap::new();
    let mut tick = tokio::time::interval(Duration::from_secs(1) / PER_SECOND);
    let started = Instant::now();
    for number in 0..PER_SECOND * SECONDS {
        tick.tick().await;
        for (lane, (_, on_player, on_host)) in lanes.iter().enumerate() {
            let lane = u8::try_from(lane).expect("three lanes");
            let now = Instant::now();
            on_player.send_fast(&message(lane, number)).expect("send");
            on_host
                .send_fast(&message(lane + 100, number))
                .expect("send");
            sent_at.insert((lane, number), now);
            sent_at.insert((lane + 100, number), now);
        }
    }
    let measured = started.elapsed();
    let expected = (PER_SECOND * SECONDS) as usize;
    let enough = expected * 99 / 100;
    let all_arrived = |w: &Window, channel: &Channel| {
        let id = channel.id.to_string();
        let count = w
            .frame_events(MESSAGE)
            .iter()
            .filter(|(_, _, data)| data["channelId"] == id.as_str())
            .count();
        count >= expected
    };
    for (player, on_player, on_host) in &lanes {
        // Waits for every message, at most the window's limit; a lost one ends the wait there.
        let _ = tokio::time::timeout(Duration::from_secs(5), async {
            host.window.wait_until(|w| all_arrived(w, on_host)).await;
            player
                .window
                .wait_until(|w| all_arrived(w, on_player))
                .await;
        })
        .await;
    }

    let mut delays = Vec::new();
    for (player, on_player, on_host) in &lanes {
        for (window, channel) in [(&host.window, on_host), (&player.window, on_player)] {
            let got = arrivals(window, channel);
            assert!(
                got.len() >= enough,
                "{} of {expected} arrived on one direction of a channel",
                got.len()
            );
            delays.extend(
                got.iter()
                    .filter_map(|(key, at)| Some(at.duration_since(*sent_at.get(key)?))),
            );
        }
    }
    delays.sort();
    eprintln!(
        "peers load: {} messages over {measured:?}; delay median {:?}, p95 {:?}",
        delays.len(),
        percentile(&delays, 0.5),
        percentile(&delays, 0.95),
    );
    for (player, _, _) in &lanes {
        player.shutdown().await;
    }
    host.shutdown().await;
}
