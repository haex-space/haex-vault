//! Soak (spec 072 T029, SC-006, quickstart A5): 1,000 messages between two vaults with two devices
//! each, devices going on and off at random; afterwards no message is missing or doubled on any
//! device. Ignored by default; run with `cargo test --lib chat::peer -- --ignored`.

use std::collections::BTreeSet;

use super::test_support::{mock_relay, two_vaults, ChatDevice};

const MESSAGES: usize = 1_000;
/// How long the devices may take to catch up after the last message.
const SETTLE: std::time::Duration = std::time::Duration::from_secs(300);

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "soak: minutes, not for every run"]
async fn a_thousand_messages_with_devices_coming_and_going_arrive_once_everywhere() {
    let (_relay, url) = mock_relay().await;
    let (anna, ben) = two_vaults(&[url]).await;
    let anna_phone = anna.link();
    let ben_phone = ben.link();
    anna.learn(&ben).await;
    ben.learn(&anna).await;
    let devices: [&ChatDevice; 4] = [&anna, &anna_phone, &ben, &ben_phone];
    for device in devices {
        device.start().await;
    }

    let mut texts = BTreeSet::new();
    for n in 0..MESSAGES {
        let draw = crate::sync::keys::random_bytes::<2>();
        let flip = devices[usize::from(draw[0]) % 4];
        if draw[1].is_multiple_of(7) {
            flip.stop();
        } else if draw[1] % 7 == 1 {
            flip.start().await;
        }
        let (from, to) = if n % 2 == 0 {
            (&anna, &ben)
        } else {
            (&ben, &anna)
        };
        let text = format!("Nachricht {n}");
        from.send(to, &text);
        texts.insert(text);
    }
    for device in devices {
        device.start().await;
    }
    let expected = texts.len();
    for (device, other) in [
        (&anna, &ben),
        (&anna_phone, &ben),
        (&ben, &anna),
        (&ben_phone, &anna),
    ] {
        let found = device
            .wait_within(SETTLE, other, "every message", |m| m.len() >= expected)
            .await;
        let seen: BTreeSet<_> = found.iter().filter_map(|m| m.text.clone()).collect();
        assert_eq!(found.len(), expected, "no duplicate");
        assert_eq!(seen, texts, "none lost");
    }
}
