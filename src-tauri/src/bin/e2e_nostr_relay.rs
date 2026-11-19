//! A Nostr relay for the end-to-end tests (spec 024, T078): the same
//! in-process relay the integration tests use (`MockRelay`), run as a process
//! so two application processes can find each other through it without a
//! network. Prints its address (`ws://127.0.0.1:<port>`) as the only line on
//! stdout and runs until it is killed. Built only with `--features e2e`.
//!
//! The port is chosen by the system unless `HAEX_E2E_RELAY_PORT` names one
//! (spec 033, R3): a harness that picks the port itself can stop the relay and
//! start it again on the same address. The relay keeps nothing across starts.

use nostr_sdk::local_relay::LocalRelayBuilder;

const PORT_VARIABLE: &str = "HAEX_E2E_RELAY_PORT";

#[tokio::main]
async fn main() {
    // The same defaults as `MockRelay::run()`, with an optional port.
    let mut builder = LocalRelayBuilder::default();
    if let Ok(value) = std::env::var(PORT_VARIABLE) {
        match value.parse::<u16>() {
            Ok(port) => builder = builder.port(port),
            Err(error) => {
                eprintln!("{PORT_VARIABLE} is not a port ({value:?}): {error}");
                std::process::exit(2);
            }
        }
    }
    let relay = builder.build();
    if let Err(error) = relay.run().await {
        eprintln!("the relay does not start: {error}");
        std::process::exit(1);
    }
    println!("{}", relay.url().await);
    // The relay lives as long as this value does.
    std::future::pending::<()>().await;
    drop(relay);
}
