//! A Blossom-Server for the end-to-end tests (spec 072, T105): the in-memory server the unit tests
//! use (`BlossomTestServer`), run as a process so two application processes can send each other
//! files through it without a network. Prints its address (`http://127.0.0.1:<port>/`) as the only
//! line on stdout and runs until it is killed. Built only with `--features e2e`.
//!
//! The port is chosen by the system unless `HAEX_E2E_BLOSSOM_PORT` names one: a harness that picks
//! the port itself can make the address reachable from a phone before the server starts. The server
//! keeps its blobs in memory only.

use haex_lib::chat::peer::files::test_server::BlossomTestServer;

const PORT_VARIABLE: &str = "HAEX_E2E_BLOSSOM_PORT";

#[tokio::main]
async fn main() {
    let port = match std::env::var(PORT_VARIABLE) {
        Ok(value) => match value.parse::<u16>() {
            Ok(port) => port,
            Err(error) => {
                eprintln!("{PORT_VARIABLE} is not a port ({value:?}): {error}");
                std::process::exit(2);
            }
        },
        Err(_) => 0,
    };
    let server = match BlossomTestServer::start_on(port).await {
        Ok(server) => server,
        Err(error) => {
            eprintln!("the Blossom test server does not start: {error}");
            std::process::exit(1);
        }
    };
    println!("{}", server.url);
    // The server lives as long as this value does.
    std::future::pending::<()>().await;
    drop(server);
}
