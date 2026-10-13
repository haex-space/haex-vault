//! A WebDAV server in the test process (spec 050 research R14): `dav-server` with its files in
//! memory below `/dav/`, served over `hyper` on a free port of the loopback interface. It ends
//! with the test's runtime.

use std::convert::Infallible;

use dav_server::{memfs::MemFs, memls::MemLs, DavHandler};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use tokio::net::TcpListener;
use zeroize::Zeroizing;

use crate::remote_storage::{
    Access, Addressing, Credentials, EndpointScope, Location, PasswordCredentials, Protocol,
    ProviderKind,
};

/// Starts the server and returns the access to its folder `folder` (made here, empty).
pub(super) async fn start(folder: &str) -> Access {
    let dav = DavHandler::builder()
        .filesystem(MemFs::new())
        .locksystem(MemLs::new())
        .strip_prefix("/dav")
        .build_handler();
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let port = listener.local_addr().expect("address").port();
    tokio::spawn(serve(listener, dav));
    let access = Access {
        location: Location {
            protocol: Protocol::WebDav,
            provider_kind: ProviderKind::Other,
            endpoint: format!("http://127.0.0.1:{port}/dav/"),
            endpoint_scope: EndpointScope::Local,
            region: String::new(),
            addressing: Addressing::Path,
            bucket: String::new(),
            host_key: None,
        },
        credentials: Credentials::Password(PasswordCredentials {
            username: "anna".to_owned(),
            password: Zeroizing::new("placeholder-password".to_owned()),
        }),
    };
    if !folder.is_empty() {
        let mkcol = reqwest::Method::from_bytes(b"MKCOL").expect("method");
        let url = format!("{}{folder}/", access.location.endpoint);
        let made = reqwest::Client::new()
            .request(mkcol, url)
            .send()
            .await
            .expect("MKCOL");
        assert!(made.status().is_success(), "{}", made.status());
    }
    Access {
        location: Location {
            bucket: folder.to_owned(),
            ..access.location
        },
        ..access
    }
}

async fn serve(listener: TcpListener, dav: DavHandler) {
    while let Ok((stream, _)) = listener.accept().await {
        let dav = dav.clone();
        tokio::spawn(async move {
            let service = service_fn(move |request| {
                let dav = dav.clone();
                async move { Ok::<_, Infallible>(dav.handle(request).await) }
            });
            let _ = http1::Builder::new()
                .serve_connection(TokioIo::new(stream), service)
                .await;
        });
    }
}
