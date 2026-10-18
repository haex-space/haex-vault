//! Where a model runs (spec 055, research R6): the answer to "does a byte leave this device, and
//! where to?" that the consent of the user rests on.

use std::net::IpAddr;

use serde::Serialize;

use crate::storage::providers::ProviderKind;

/// The place of a model, as shown to the extension and to the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Place {
    /// In this process or on this machine: no byte leaves the device.
    ThisDevice,
    /// Another device or a server in the own network (a GPU machine, a school server).
    Network,
    /// Everything else: a service on the internet.
    Cloud,
}

impl Place {
    /// Whether sending images or texts here needs the consent of the user.
    pub fn needs_consent(self) -> bool {
        self != Place::ThisDevice
    }
}

/// The place of a provider. A provider whose address cannot be read counts as `Cloud`: the safe
/// side is to ask. A command-line delegate is always `Cloud` (it talks to its vendor).
pub fn place_of(kind: ProviderKind, base_url: Option<&str>) -> Place {
    match kind {
        ProviderKind::Local => Place::ThisDevice,
        ProviderKind::CliDelegate => Place::Cloud,
        ProviderKind::ApiKey => base_url.map(place_of_address).unwrap_or(Place::Cloud),
    }
}

/// The place an address points to, judged by the address and not by the way to it: a name that
/// resolves into a private network but is reachable from outside is still told apart only by what
/// is written (the user sees it in the consent question).
fn place_of_address(address: &str) -> Place {
    let Ok(url) = url::Url::parse(address) else {
        return Place::Cloud;
    };
    match url.host() {
        Some(url::Host::Ipv4(ip)) => of_ip(IpAddr::V4(ip)),
        Some(url::Host::Ipv6(ip)) => of_ip(IpAddr::V6(ip)),
        Some(url::Host::Domain(name)) => of_name(&name.to_ascii_lowercase()),
        None => Place::Cloud,
    }
}

fn of_ip(ip: IpAddr) -> Place {
    // `::ffff:10.0.0.1` is the IPv4 address it carries.
    if let IpAddr::V6(v6) = ip {
        if let Some(v4) = v6.to_ipv4_mapped() {
            return of_ip(IpAddr::V4(v4));
        }
    }
    if ip.is_loopback() || ip.is_unspecified() {
        return Place::ThisDevice;
    }
    let private = match ip {
        // 100.64.0.0/10 is the shared space of carrier NAT and of Tailscale, never the internet.
        IpAddr::V4(v4) => {
            let [a, b, ..] = v4.octets();
            v4.is_private() || v4.is_link_local() || (a == 100 && (64..128).contains(&b))
        }
        IpAddr::V6(v6) => v6.is_unique_local() || v6.is_unicast_link_local(),
    };
    if private {
        Place::Network
    } else {
        Place::Cloud
    }
}

fn of_name(name: &str) -> Place {
    let name = name.trim_end_matches('.');
    if name == "localhost" {
        Place::ThisDevice
    } else if name.ends_with(".local")
        || name.ends_with(".lan")
        || name.ends_with(".localhost")
        || !name.contains('.')
    {
        // A name without a dot (`school-server`) or in a local zone lives in the own network.
        // `*.localhost` should be this device (RFC 6761), but not every resolver keeps to it.
        Place::Network
    } else {
        Place::Cloud
    }
}
