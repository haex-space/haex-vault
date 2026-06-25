use super::PermissionManager;
use crate::database::error::DatabaseError;
use crate::extension::permissions::types::ResourceType;

impl PermissionManager {
    // Helper-Methoden - müssen DatabaseError statt ExtensionError zurückgeben
    #[allow(dead_code)]
    pub fn parse_resource_type(s: &str) -> Result<ResourceType, DatabaseError> {
        match s {
            "fs" => Ok(ResourceType::Fs),
            "web" => Ok(ResourceType::Web),
            "db" => Ok(ResourceType::Db),
            "shell" => Ok(ResourceType::Shell),
            "syncServers" => Ok(ResourceType::SyncServers),
            "cloudStorage" => Ok(ResourceType::CloudStorage),
            "syncRules" => Ok(ResourceType::SyncRules),
            "spaces" => Ok(ResourceType::Spaces),
            "identities" => Ok(ResourceType::Identities),
            _ => Err(DatabaseError::SerializationError {
                reason: format!("Unknown resource type: {s}"),
            }),
        }
    }

    /// Matches a URL against a URL pattern
    /// Supports:
    /// - Path wildcards: "https://domain.com/*"
    /// - Subdomain wildcards: "https://*.domain.com/*"
    pub(crate) fn matches_url_pattern(pattern: &str, url: &str) -> bool {
        // Parse the actual URL
        let Ok(url_parsed) = url::Url::parse(url) else {
            return false;
        };

        // Check if pattern contains subdomain wildcard
        let has_subdomain_wildcard = pattern.contains("://*.") || pattern.starts_with("*.");

        if has_subdomain_wildcard {
            // Extract components for wildcard matching
            // Pattern: "https://*.example.com/*"

            // Get protocol from pattern
            let protocol_end = pattern.find("://").unwrap_or(0);
            let pattern_protocol = if protocol_end > 0 {
                &pattern[..protocol_end]
            } else {
                ""
            };

            // Protocol must match if specified
            if !pattern_protocol.is_empty() && pattern_protocol != url_parsed.scheme() {
                return false;
            }

            // Extract the domain pattern (after *.  )
            let domain_start = if pattern.contains("://*.") {
                // invariant: pattern.contains("://*.") was checked on the
                // line above, so find() must return Some. (Not // SAFETY:
                // — that prefix is reserved for unsafe blocks.)
                pattern
                    .find("://*.")
                    .expect("invariant: contains() guard above guarantees a match")
                    + 5
            } else if pattern.starts_with("*.") {
                2 // length of "*."
            } else {
                return false;
            };

            // Find where the domain pattern ends (at / or end of string)
            let domain_pattern_end = pattern[domain_start..]
                .find('/')
                .map(|i| domain_start + i)
                .unwrap_or(pattern.len());
            let domain_pattern = &pattern[domain_start..domain_pattern_end];

            // Check if the URL's host ends with the domain pattern
            let Some(url_host) = url_parsed.host_str() else {
                return false;
            };

            // For subdomain wildcard (*.example.com), the host must:
            // 1. End with ".example.com" (note the leading dot!) OR
            // 2. NOT match if it's just "example.com" (no subdomain)
            // This prevents attacks like "evil-example.com" matching "*.example.com"
            if pattern.contains("*.") {
                // Subdomain wildcard: require ".domain_pattern" suffix
                let required_suffix = format!(".{}", domain_pattern);
                if !url_host.ends_with(&required_suffix) {
                    return false;
                }
            } else {
                // No subdomain wildcard: exact match or ends_with
                if !url_host.ends_with(domain_pattern) && url_host != domain_pattern {
                    return false;
                }
            }

            // Path matching — wildcard or exact, mirroring the full-URL branch below.
            let pattern_path_start = domain_pattern_end;
            if pattern_path_start >= pattern.len() {
                // No path component in pattern → any path allowed
                return true;
            }
            let pattern_path = &pattern[pattern_path_start..];

            if let Some(wildcard_pos) = pattern_path.find("/*") {
                let path_prefix = &pattern_path[..wildcard_pos + 1]; // include trailing /
                let url_path = url_parsed.path();
                let normalized_url_path = Self::normalize_url_path(url_path);
                return normalized_url_path.starts_with(path_prefix)
                    || normalized_url_path == path_prefix[..path_prefix.len() - 1];
            }

            return url_parsed.path() == pattern_path;
        }

        // No subdomain wildcard - parse as full URL
        let Ok(pattern_url) = url::Url::parse(pattern) else {
            return false;
        };

        // Protocol must match
        if pattern_url.scheme() != url_parsed.scheme() {
            return false;
        }

        // Host must match
        if pattern_url.host_str() != url_parsed.host_str() {
            return false;
        }

        // Port must match (if specified)
        if pattern_url.port() != url_parsed.port() {
            return false;
        }

        // Path matching with wildcard support
        if pattern.contains("/*") {
            // Extract the path pattern before the wildcard
            let pattern_path = pattern_url.path();
            if let Some(wildcard_pos) = pattern_path.find("/*") {
                let path_prefix = &pattern_path[..wildcard_pos + 1]; // Include trailing /

                // Normalize the URL path to prevent traversal bypass
                let url_path = url_parsed.path();
                let normalized_url_path = Self::normalize_url_path(url_path);

                // Check if the normalized path starts with the pattern prefix
                return normalized_url_path.starts_with(path_prefix)
                    || normalized_url_path == &path_prefix[..path_prefix.len() - 1];
                // Allow exact match without trailing /
            }
        }

        // Exact path match (no wildcard)
        pattern_url.path() == url_parsed.path()
    }

    /// Normalize a URL path by resolving . and .. components
    fn normalize_url_path(path: &str) -> String {
        let mut components: Vec<&str> = Vec::new();

        for component in path.split('/') {
            match component {
                "" | "." => {
                    // Skip empty components and current directory
                    if components.is_empty() {
                        components.push(""); // Keep leading empty for absolute path
                    }
                }
                ".." => {
                    // Go up one directory, but don't go above root
                    if components.len() > 1 {
                        components.pop();
                    }
                }
                _ => {
                    components.push(component);
                }
            }
        }

        if components.is_empty() {
            return "/".to_string();
        }

        components.join("/")
    }
}
