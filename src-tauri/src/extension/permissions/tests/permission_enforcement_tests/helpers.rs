use crate::extension::core::manifest::{DisplayMode, ExtensionManifest, ExtensionPermissions};
use crate::extension::core::types::{Extension, ExtensionSource};
use crate::extension::permissions::types::{
    Action, DbAction, ExtensionPermission, FsAction, PermissionStatus, ResourceType, RwAction,
    WebAction,
};
use std::path::PathBuf;

pub(super) fn create_extension(public_key: &str, name: &str) -> Extension {
    Extension {
        id: format!("{}_{}", public_key, name),
        manifest: ExtensionManifest {
            name: name.to_string(),
            version: "0.1.0".to_string(),
            author: None,
            entry: Some("index.html".to_string()),
            icon: None,
            public_key: public_key.to_string(),
            signature: "test_sig".to_string(),
            permissions: ExtensionPermissions {
                database: None,
                filesystem: None,
                http: None,
                shell: None,
                sync_servers: None,
                cloud_storage: None,
                sync_rules: None,
                spaces: None,
                identities: None,
                passwords: None,
                mail: None,
                notifications: None,
            },
            homepage: None,
            description: None,
            single_instance: None,
            display_mode: Some(DisplayMode::Iframe),
            migrations_dir: None,
            i18n: None,
        },
        source: ExtensionSource::Production {
            path: PathBuf::from("/tmp/test"),
            version: "0.1.0".to_string(),
        },
        enabled: true,
        last_accessed: std::time::SystemTime::now(),
    }
}

pub(super) fn create_db_permission(
    extension_id: &str,
    action: DbAction,
    target: &str,
    status: PermissionStatus,
) -> ExtensionPermission {
    ExtensionPermission {
        id: uuid::Uuid::new_v4().to_string(),
        principal_id: extension_id.to_string(),
        resource_type: ResourceType::Db,
        action: Action::Database(action),
        target: target.to_string(),
        constraints: None,
        status,
        raw_constraints: None,
    }
}

pub(super) fn create_fs_permission(
    extension_id: &str,
    action: FsAction,
    target: &str,
    status: PermissionStatus,
) -> ExtensionPermission {
    ExtensionPermission {
        id: uuid::Uuid::new_v4().to_string(),
        principal_id: extension_id.to_string(),
        resource_type: ResourceType::Fs,
        action: Action::Filesystem(action),
        target: target.to_string(),
        constraints: None,
        status,
        raw_constraints: None,
    }
}

pub(super) fn create_web_permission(
    extension_id: &str,
    target: &str,
    status: PermissionStatus,
) -> ExtensionPermission {
    ExtensionPermission {
        id: uuid::Uuid::new_v4().to_string(),
        principal_id: extension_id.to_string(),
        resource_type: ResourceType::Web,
        action: Action::Web(WebAction::Get),
        target: target.to_string(),
        constraints: None,
        status,
        raw_constraints: None,
    }
}

pub(super) fn create_cloud_storage_permission(
    extension_id: &str,
    action: RwAction,
    target: &str,
    status: PermissionStatus,
) -> ExtensionPermission {
    ExtensionPermission {
        id: uuid::Uuid::new_v4().to_string(),
        principal_id: extension_id.to_string(),
        resource_type: ResourceType::CloudStorage,
        action: Action::CloudStorage(action),
        target: target.to_string(),
        constraints: None,
        status,
        raw_constraints: None,
    }
}
