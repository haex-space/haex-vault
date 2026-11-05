//! Connections to mail servers: TCP, then TLS from the start, STARTTLS, or (to this device
//! only) nothing. TLS is rustls with ring and the system's certificate check.

use std::io;
use std::pin::Pin;
use std::sync::{Arc, OnceLock};
use std::task::{Context, Poll};

use rustls::pki_types::ServerName;
use rustls::ClientConfig;
use rustls_platform_verifier::BuilderVerifierExt;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::TcpStream;
use tokio_rustls::client::TlsStream;
use tokio_rustls::TlsConnector;

use super::{ConnectionSecurity, MailError, ServerConfig};

/// The byte stream of an IMAP session: plain (STARTTLS before, or this device) or TLS.
#[derive(Debug)]
pub enum MailStream {
    Plain(TcpStream),
    Tls(Box<TlsStream<TcpStream>>),
}

impl AsyncRead for MailStream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        match self.get_mut() {
            MailStream::Plain(s) => Pin::new(s).poll_read(cx, buf),
            MailStream::Tls(s) => Pin::new(s.as_mut()).poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for MailStream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        match self.get_mut() {
            MailStream::Plain(s) => Pin::new(s).poll_write(cx, buf),
            MailStream::Tls(s) => Pin::new(s.as_mut()).poll_write(cx, buf),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            MailStream::Plain(s) => Pin::new(s).poll_flush(cx),
            MailStream::Tls(s) => Pin::new(s.as_mut()).poll_flush(cx),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            MailStream::Plain(s) => Pin::new(s).poll_shutdown(cx),
            MailStream::Tls(s) => Pin::new(s.as_mut()).poll_shutdown(cx),
        }
    }
}

pub type ImapSession = async_imap::Session<MailStream>;

/// One TLS configuration for every mail connection.
fn tls_config() -> Result<Arc<ClientConfig>, MailError> {
    static CONFIG: OnceLock<Option<Arc<ClientConfig>>> = OnceLock::new();
    CONFIG
        .get_or_init(|| {
            let provider = Arc::new(rustls::crypto::ring::default_provider());
            ClientConfig::builder_with_provider(provider)
                .with_safe_default_protocol_versions()
                .ok()?
                .with_platform_verifier()
                .ok()
                .map(|b| Arc::new(b.with_no_client_auth()))
        })
        .clone()
        .ok_or_else(|| MailError::Tls("no TLS configuration on this device".into()))
}

/// Upgrades a TCP stream to TLS for the requested server name.
async fn tls(host: &str, tcp: TcpStream) -> Result<TlsStream<TcpStream>, MailError> {
    let name = ServerName::try_from(host.to_owned())
        .map_err(|_| MailError::Tls(format!("{host} is no valid server name")))?;
    TlsConnector::from(tls_config()?)
        .connect(name, tcp)
        .await
        .map_err(|error| MailError::Tls(format!("{host}: {error}")))
}

/// Opens the TCP connection described by a server configuration.
async fn tcp(config: &ServerConfig) -> Result<TcpStream, MailError> {
    TcpStream::connect((config.host.as_str(), config.port))
        .await
        .map_err(|error| MailError::Connect(format!("{}:{}: {error}", config.host, config.port)))
}

/// The server accepted the connection but sent no IMAP greeting (a wrong port or protocol).
fn greeting_failed(config: &ServerConfig, error: &impl std::fmt::Display) -> MailError {
    MailError::Connect(format!(
        "{}:{}: no IMAP greeting: {error}",
        config.host, config.port
    ))
}

/// Converts an async-IMAP protocol error into the mail boundary error.
fn imap_err(error: async_imap::error::Error) -> MailError {
    MailError::Imap(error.to_string())
}

/// Opens an IMAP session and logs in. The caller logs out.
pub async fn imap_login(config: &ServerConfig) -> Result<ImapSession, MailError> {
    super::check_server(&config.host, config.port, config.security)?;
    let tcp = tcp(config).await?;
    let client = match config.security {
        ConnectionSecurity::Tls => {
            let mut client =
                async_imap::Client::new(MailStream::Tls(Box::new(tls(&config.host, tcp).await?)));
            client
                .read_response()
                .await
                .map_err(|error| greeting_failed(config, &error))?
                .ok_or_else(|| greeting_failed(config, &"the server closed the connection"))?;
            client
        }
        ConnectionSecurity::None => {
            let mut client = async_imap::Client::new(MailStream::Plain(tcp));
            client
                .read_response()
                .await
                .map_err(|error| greeting_failed(config, &error))?
                .ok_or_else(|| greeting_failed(config, &"the server closed the connection"))?;
            client
        }
        ConnectionSecurity::StartTls => {
            let mut plain = async_imap::Client::new(MailStream::Plain(tcp));
            plain
                .read_response()
                .await
                .map_err(|error| greeting_failed(config, &error))?
                .ok_or_else(|| greeting_failed(config, &"the server closed the connection"))?;
            plain
                .run_command_and_check_ok("STARTTLS", None)
                .await
                .map_err(imap_err)?;
            let MailStream::Plain(tcp) = plain.into_inner() else {
                return Err(MailError::Tls(
                    "the connection was already encrypted".into(),
                ));
            };
            // No greeting after STARTTLS (RFC 3501 §6.2.1).
            async_imap::Client::new(MailStream::Tls(Box::new(tls(&config.host, tcp).await?)))
        }
    };
    // The login runs unencrypted only towards this device; `check_server` made sure. Only a NO or
    // BAD answer is a refused login; a lost connection may work the next time.
    client
        .login(&config.username, &config.password)
        .await
        .map_err(|(error, _)| match error {
            async_imap::error::Error::No(_) | async_imap::error::Error::Bad(_) => MailError::Auth,
            // Nothing was sent: a line break in the user name or password.
            async_imap::error::Error::Validate(_) => {
                MailError::Invalid("the login contains a line break".into())
            }
            other => MailError::Connect(format!(
                "{}:{}: the connection broke during the login: {other}",
                config.host, config.port
            )),
        })
}

/// Best effort: a failed logout changes nothing for the caller.
pub async fn imap_logout(mut session: ImapSession) {
    let _ = session.logout().await;
}
