use super::PermissionManager;

impl PermissionManager {
    /// Matches a filesystem path against a permission pattern with path traversal protection.
    ///
    /// This function normalizes paths to prevent directory traversal attacks.
    /// It handles:
    /// - Path traversal sequences (../, ..\)
    /// - URL-encoded traversal (%2e%2e%2f)
    /// - Null byte injection
    /// - Current directory references (./)
    ///
    /// Pattern types supported:
    /// - `*` - matches all paths (full wildcard)
    /// - `/path/to/dir/*` - matches all files under the directory
    /// - `*.ext` - matches all files with the given extension
    /// - `/path/*.ext` - matches files with extension under path
    /// - `/exact/path` - exact path match
    pub(crate) fn matches_path_pattern(pattern: &str, path: &str) -> bool {
        // Reject paths with null bytes (potential injection attack)
        if path.contains('\0') {
            return false;
        }

        // Reject empty paths (except for empty pattern == empty path exact match)
        if path.is_empty() && pattern != "" {
            return false;
        }

        // URL-decode the path to catch encoded traversal attempts
        let decoded_path = Self::url_decode_path(path);

        // Normalize the path to resolve . and .. components
        let normalized_path = Self::normalize_path(&decoded_path);

        // Full wildcard matches everything (after normalization)
        if pattern == "*" {
            return true;
        }

        // Directory wildcard: /path/to/dir/*
        if let Some(prefix) = pattern.strip_suffix("/*") {
            // Normalize the prefix pattern as well
            let normalized_prefix = Self::normalize_path(prefix);

            // The normalized path must start with the normalized prefix
            // AND must be either equal or have a path separator after the prefix
            if normalized_path == normalized_prefix {
                return true;
            }

            // Ensure proper directory boundary check
            let prefix_with_sep = if normalized_prefix.ends_with('/') {
                normalized_prefix.clone()
            } else {
                format!("{}/", normalized_prefix)
            };

            return normalized_path.starts_with(&prefix_with_sep);
        }

        // Extension wildcard: *.ext
        if pattern.starts_with("*.") {
            let suffix = &pattern[1..]; // includes the dot
                                        // For extension wildcards, the normalized path must end with the suffix
                                        // AND must not have originally contained traversal sequences (even if normalized away)
                                        // This prevents attacks where "../../../etc/secret.txt" normalizes to "/etc/secret.txt"
            let original_had_traversal = decoded_path.contains("..")
                || decoded_path.contains("./")
                || decoded_path.contains(".\\");
            return normalized_path.ends_with(suffix)
                && !Self::has_traversal(&normalized_path)
                && !original_had_traversal;
        }

        // Combined prefix and suffix: /path/*.ext
        if pattern.contains('*') {
            let parts: Vec<&str> = pattern.split('*').collect();
            if parts.len() == 2 {
                let prefix = parts[0];
                let suffix = parts[1];

                let normalized_prefix = Self::normalize_path(prefix);

                // The normalized path must:
                // 1. Start with the normalized prefix
                // 2. End with the suffix
                // 3. Not have traversal components
                return normalized_path.starts_with(&normalized_prefix)
                    && normalized_path.ends_with(suffix)
                    && !Self::has_traversal(&normalized_path);
            }
        }

        // Exact match: compare normalized paths
        let normalized_pattern = Self::normalize_path(pattern);
        normalized_path == normalized_pattern
    }

    /// URL-decode a path to catch encoded traversal attempts
    fn url_decode_path(path: &str) -> String {
        // Decode common URL-encoded sequences
        let mut result = path.to_string();

        // Decode %2e (.) and %2f (/) - case insensitive
        // We do this iteratively to catch double-encoding
        // First decode %25 (%) to handle double-encoding like %252e -> %2e -> .
        for _ in 0..5 {
            // Max 5 levels of encoding to catch deep nesting
            let prev = result.clone();

            // First handle double-encoding by decoding %25 -> %
            result = result.replace("%25", "%");

            // Then decode the actual characters
            result = result
                .replace("%2e", ".")
                .replace("%2E", ".")
                .replace("%2f", "/")
                .replace("%2F", "/")
                .replace("%5c", "\\")
                .replace("%5C", "\\")
                .replace("%00", "\0"); // Null byte

            if result == prev {
                break;
            }
        }

        result
    }

    /// Normalize a filesystem path by resolving . and .. components
    fn normalize_path(path: &str) -> String {
        // Replace backslashes with forward slashes for uniform handling
        let path = path.replace('\\', "/");

        // Handle empty path
        if path.is_empty() {
            return String::new();
        }

        let is_absolute = path.starts_with('/');
        let mut components: Vec<&str> = Vec::new();

        for component in path.split('/') {
            match component {
                "" | "." => {
                    // Skip empty components and current directory references
                }
                ".." => {
                    // Go up one directory, but don't go above root
                    if !components.is_empty() && components.last() != Some(&"..") {
                        components.pop();
                    } else if !is_absolute {
                        // For relative paths, keep the .. if we can't go up
                        components.push(component);
                    }
                    // For absolute paths, ignore .. at root level
                }
                _ => {
                    components.push(component);
                }
            }
        }

        let normalized = components.join("/");

        if is_absolute {
            format!("/{}", normalized)
        } else {
            normalized
        }
    }

    /// Check if a path contains traversal sequences (after normalization)
    fn has_traversal(path: &str) -> bool {
        // After proper normalization, these shouldn't exist in valid paths
        path.contains("../") || path.contains("..\\") || path.ends_with("..") || path.contains("\0")
    }
}
