//! The fixture site of the agent browser tests for the manual checks of spec 065
//! (specs/065-browser-tools/quickstart.md): `pnpm agent-browser:fixtures [port]` serves
//! `tests/fixtures/agent_browser/site/` over HTTPS with the test certificate, as
//! `https://127.0.0.1:<port>` and, as a second website, `https://localhost:<port>`, and prints
//! every request with its host and method. The same server the integration tests start.

#[path = "../tests/common/agent_browser_fixture.rs"]
#[allow(dead_code)]
mod agent_browser_fixture;

use std::time::Duration;

#[tokio::main]
async fn main() {
    let port = std::env::args()
        .nth(1)
        .map(|port| port.parse().expect("a port number"))
        .unwrap_or(8443);
    let site = agent_browser_fixture::start_tls_on(port).await;
    println!("agent browser fixtures (test certificate, Ctrl+C ends):");
    println!("  website  {}", site.url("/"));
    println!("  partner  {}", site.partner("/partner.html"));
    let mut shown = 0;
    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => break,
            _ = tokio::time::sleep(Duration::from_millis(300)) => {
                let seen = site.seen();
                for request in &seen[shown..] {
                    println!("{} {} {}", request.method, request.host, request.path);
                }
                shown = seen.len();
            }
        }
    }
}
