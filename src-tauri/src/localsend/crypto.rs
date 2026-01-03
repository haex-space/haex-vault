//! TLS certificate generation and fingerprinting for LocalSend
//!
//! LocalSend uses self-signed TLS certificates for HTTPS.
//! The SHA-256 fingerprint of the certificate is used as device identifier.

use rcgen::{CertificateParams, DistinguishedName, DnType, KeyPair, SanType};
use sha2::{Digest, Sha256};

use super::error::LocalSendError;

/// TLS identity (certificate + private key)
#[derive(Debug, Clone)]
pub struct TlsIdentity {
    /// DER-encoded certificate
    pub cert_der: Vec<u8>,
    /// PEM-encoded certificate
    pub cert_pem: String,
    /// DER-encoded private key
    pub key_der: Vec<u8>,
    /// PEM-encoded private key
    pub key_pem: String,
    /// SHA-256 fingerprint of the certificate (hex, uppercase, no colons)
    pub fingerprint: String,
}

impl TlsIdentity {
    /// Generate a new self-signed TLS certificate
    pub fn generate() -> Result<Self, LocalSendError> {
        // Generate a new key pair
        let key_pair = KeyPair::generate()
            .map_err(|e| LocalSendError::TlsError(format!("Failed to generate key pair: {e}")))?;

        // Create certificate parameters
        let mut params = CertificateParams::default();

        // Set distinguished name
        let mut dn = DistinguishedName::new();
        dn.push(DnType::CommonName, "LocalSend");
        dn.push(DnType::OrganizationName, "haex-vault");
        params.distinguished_name = dn;

        // Add Subject Alternative Names (localhost + common local IPs)
        params.subject_alt_names = vec![
            SanType::DnsName("localhost".try_into().unwrap()),
            SanType::IpAddress(std::net::IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 1))),
        ];

        // Add local IP addresses as SANs
        if let Ok(addrs) = get_local_ip_addresses() {
            for addr in addrs {
                if let Ok(ip) = addr.parse::<std::net::IpAddr>() {
                    params.subject_alt_names.push(SanType::IpAddress(ip));
                }
            }
        }

        // Set validity (1 year)
        params.not_before = time::OffsetDateTime::now_utc();
        params.not_after = params.not_before + time::Duration::days(365);

        // Generate the certificate
        let cert = params
            .self_signed(&key_pair)
            .map_err(|e| LocalSendError::TlsError(format!("Failed to generate certificate: {e}")))?;

        let cert_der = cert.der().to_vec();
        let cert_pem = cert.pem();
        let key_der = key_pair.serialize_der();
        let key_pem = key_pair.serialize_pem();

        // Calculate fingerprint
        let fingerprint = calculate_fingerprint(&cert_der);

        Ok(Self {
            cert_der,
            cert_pem,
            key_der,
            key_pem,
            fingerprint,
        })
    }
}

/// Calculate SHA-256 fingerprint of a DER-encoded certificate
pub fn calculate_fingerprint(cert_der: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(cert_der);
    let result = hasher.finalize();

    // Convert to uppercase hex without colons (LocalSend format)
    result
        .iter()
        .map(|b| format!("{:02X}", b))
        .collect::<String>()
}

/// Get all local IP addresses (IPv4 only for now)
pub fn get_local_ip_addresses() -> Result<Vec<String>, LocalSendError> {
    let mut addresses = Vec::new();

    // Use local_ip_address crate or manual approach
    if let Ok(interfaces) = get_if_addrs::get_if_addrs() {
        for iface in interfaces {
            // Skip loopback
            if iface.is_loopback() {
                continue;
            }

            // Only IPv4 for now (LocalSend prefers IPv4)
            if let get_if_addrs::IfAddr::V4(v4) = iface.addr {
                addresses.push(v4.ip.to_string());
            }
        }
    }

    // Fallback: try to get local IP via UDP socket
    if addresses.is_empty() {
        if let Ok(ip) = get_local_ip_fallback() {
            addresses.push(ip);
        }
    }

    Ok(addresses)
}

/// Fallback method to get local IP address
fn get_local_ip_fallback() -> Result<String, LocalSendError> {
    // Create a UDP socket and "connect" to a public IP
    // This doesn't actually send any data, but tells us which local IP would be used
    let socket = std::net::UdpSocket::bind("0.0.0.0:0")
        .map_err(|e| LocalSendError::NetworkError(e.to_string()))?;

    socket
        .connect("8.8.8.8:80")
        .map_err(|e| LocalSendError::NetworkError(e.to_string()))?;

    let local_addr = socket
        .local_addr()
        .map_err(|e| LocalSendError::NetworkError(e.to_string()))?;

    Ok(local_addr.ip().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_identity() {
        let identity = TlsIdentity::generate().unwrap();

        // Fingerprint should be 64 hex characters (SHA-256 = 32 bytes = 64 hex chars)
        assert_eq!(identity.fingerprint.len(), 64);

        // Should be uppercase hex
        assert!(identity.fingerprint.chars().all(|c| c.is_ascii_hexdigit() && (c.is_ascii_digit() || c.is_ascii_uppercase())));

        // PEM should contain certificate markers
        assert!(identity.cert_pem.contains("-----BEGIN CERTIFICATE-----"));
        assert!(identity.key_pem.contains("-----BEGIN PRIVATE KEY-----"));
    }

    #[test]
    fn test_get_local_ip() {
        let addrs = get_local_ip_addresses().unwrap();
        println!("Local addresses: {:?}", addrs);
        // Should have at least one address (unless running in a very restricted environment)
    }
}
