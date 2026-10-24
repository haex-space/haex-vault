//! The allowlist of bridge methods (contracts/bridge.md §Methoden): every method an extension frame
//! can call, with its handler. [`super::dispatch::call`] runs nothing else.

use super::dispatch::Method;
use super::{database, methods, model_jobs, permissions};
use crate::extensions::{
    contacts, encrypted_folder, fs, kv, logs, mail, notifications, passwords, remote_storage,
    remote_storage_manage, remote_storage_propose, shell, web,
};

/// The allowlist (contracts/bridge.md §Methoden).
pub static METHODS: &[Method] = &[
    Method {
        name: "extension_model_job_start",
        handler: model_jobs::start,
        module: model_jobs::MODULE,
    },
    Method {
        name: "extension_model_job_cancel",
        handler: model_jobs::cancel,
        module: model_jobs::MODULE,
    },
    Method {
        name: "extension_model_list",
        handler: model_jobs::list,
        module: model_jobs::MODULE,
    },
    Method {
        name: "extension_model_access_request",
        handler: model_jobs::access_request,
        module: model_jobs::MODULE,
    },
    Method {
        name: "extension_model_settings_open",
        handler: model_jobs::open_settings,
        module: model_jobs::MODULE,
    },
    Method {
        name: "extension_context_get",
        handler: methods::context_get,
        module: methods::MODULE,
    },
    Method {
        name: "extension_get_info",
        handler: methods::get_info,
        module: methods::MODULE,
    },
    Method {
        name: "extension_tab_attention",
        handler: methods::tab_attention,
        module: methods::MODULE,
    },
    Method {
        name: "extension_dialog_confirm",
        handler: methods::dialog_confirm,
        module: methods::MODULE,
    },
    Method {
        name: "extension_database_query",
        handler: database::query,
        module: database::MODULE,
    },
    Method {
        name: "extension_database_execute",
        handler: database::execute,
        module: database::MODULE,
    },
    Method {
        name: "extension_database_transaction",
        handler: database::transaction,
        module: database::MODULE,
    },
    Method {
        name: "extension_permissions_check_database",
        handler: permissions::check_database,
        module: permissions::MODULE,
    },
    Method {
        name: "extension_database_register_migrations",
        handler: database::register_migrations,
        module: database::MODULE,
    },
    Method {
        name: "extension_web_storage_get_item",
        handler: kv::get_item,
        module: kv::MODULE,
    },
    Method {
        name: "extension_web_storage_set_item",
        handler: kv::set_item,
        module: kv::MODULE,
    },
    Method {
        name: "extension_web_storage_remove_item",
        handler: kv::remove_item,
        module: kv::MODULE,
    },
    Method {
        name: "extension_web_storage_clear",
        handler: kv::clear,
        module: kv::MODULE,
    },
    Method {
        name: "extension_web_storage_keys",
        handler: kv::keys,
        module: kv::MODULE,
    },
    Method {
        name: "extension_logging_write",
        handler: logs::write,
        module: logs::MODULE,
    },
    Method {
        name: "extension_logging_read",
        handler: logs::read_own,
        module: logs::MODULE,
    },
    Method {
        name: "extension_permissions_check_filesystem",
        handler: fs::check,
        module: fs::MODULE,
    },
    Method {
        name: "extension_filesystem_save_file",
        handler: fs::dialogs::save_file,
        module: fs::dialogs::MODULE,
    },
    Method {
        name: "extension_filesystem_open_file",
        handler: fs::dialogs::open_file,
        module: fs::dialogs::MODULE,
    },
    Method {
        name: "extension_filesystem_show_image",
        handler: fs::dialogs::show_image,
        module: fs::dialogs::MODULE,
    },
    Method {
        name: "extension_filesystem_select_folder",
        handler: fs::dialogs::select_folder,
        module: fs::dialogs::MODULE,
    },
    Method {
        name: "extension_filesystem_select_file",
        handler: fs::dialogs::select_file,
        module: fs::dialogs::MODULE,
    },
    Method {
        name: "extension_filesystem_read_file",
        handler: fs::ops::read_file,
        module: fs::ops::MODULE,
    },
    Method {
        name: "extension_filesystem_write_file",
        handler: fs::ops::write_file,
        module: fs::ops::MODULE,
    },
    Method {
        name: "extension_filesystem_read_dir",
        handler: fs::ops::read_dir,
        module: fs::ops::MODULE,
    },
    Method {
        name: "extension_filesystem_mkdir",
        handler: fs::ops::mkdir,
        module: fs::ops::MODULE,
    },
    Method {
        name: "extension_filesystem_remove",
        handler: fs::ops::remove,
        module: fs::ops::MODULE,
    },
    Method {
        name: "extension_filesystem_exists",
        handler: fs::ops::exists,
        module: fs::ops::MODULE,
    },
    Method {
        name: "extension_filesystem_stat",
        handler: fs::ops::stat,
        module: fs::ops::MODULE,
    },
    Method {
        name: "extension_filesystem_rename",
        handler: fs::ops::rename,
        module: fs::ops::MODULE,
    },
    Method {
        name: "extension_filesystem_copy",
        handler: fs::ops::copy,
        module: fs::ops::MODULE,
    },
    Method {
        name: "extension_filesystem_known_paths",
        handler: fs::ops::known_paths,
        module: fs::ops::MODULE,
    },
    Method {
        name: "extension_filesystem_watch",
        handler: fs::watch::watch,
        module: fs::watch::MODULE,
    },
    Method {
        name: "extension_filesystem_unwatch",
        handler: fs::watch::unwatch,
        module: fs::watch::MODULE,
    },
    Method {
        name: "extension_filesystem_is_watching",
        handler: fs::watch::is_watching,
        module: fs::watch::MODULE,
    },
    Method {
        name: "extension_web_fetch",
        handler: web::fetch,
        module: web::MODULE,
    },
    Method {
        name: "extension_web_open",
        handler: web::open,
        module: web::MODULE,
    },
    Method {
        name: "extension_notifications_show",
        handler: notifications::show,
        module: notifications::MODULE,
    },
    Method {
        name: "extension_notifications_dismiss",
        handler: notifications::dismiss,
        module: notifications::MODULE,
    },
    Method {
        name: "extension_permissions_check_web",
        handler: web::check_web,
        module: web::MODULE,
    },
    Method {
        name: "extension_contacts_list",
        handler: contacts::list,
        module: contacts::MODULE,
    },
    Method {
        name: "extension_contacts_read",
        handler: contacts::read,
        module: contacts::MODULE,
    },
    Method {
        name: "extension_contacts_search",
        handler: contacts::search,
        module: contacts::MODULE,
    },
    Method {
        name: "extension_contacts_photo",
        handler: contacts::photo,
        module: contacts::MODULE,
    },
    Method {
        name: "extension_password_list",
        handler: passwords::list,
        module: passwords::MODULE,
    },
    Method {
        name: "extension_password_read",
        handler: passwords::read,
        module: passwords::MODULE,
    },
    Method {
        name: "extension_password_create",
        handler: passwords::create,
        module: passwords::MODULE,
    },
    Method {
        name: "extension_password_update",
        handler: passwords::update,
        module: passwords::MODULE,
    },
    Method {
        name: "extension_password_delete",
        handler: passwords::delete,
        module: passwords::MODULE,
    },
    Method {
        name: "extension_mail_list_mailboxes",
        handler: mail::commands::list_mailboxes,
        module: mail::commands::MODULE,
    },
    Method {
        name: "extension_mail_fetch_envelopes",
        handler: mail::commands::fetch_envelopes,
        module: mail::commands::MODULE,
    },
    Method {
        name: "extension_mail_fetch_message",
        handler: mail::commands::fetch_message,
        module: mail::commands::MODULE,
    },
    Method {
        name: "extension_mail_fetch_attachment",
        handler: mail::commands::fetch_attachment,
        module: mail::commands::MODULE,
    },
    Method {
        name: "extension_mail_set_flags",
        handler: mail::commands::set_flags,
        module: mail::commands::MODULE,
    },
    Method {
        name: "extension_mail_move_messages",
        handler: mail::commands::move_messages,
        module: mail::commands::MODULE,
    },
    Method {
        name: "extension_mail_search",
        handler: mail::commands::search,
        module: mail::commands::MODULE,
    },
    Method {
        name: "extension_mail_create_mailbox",
        handler: mail::commands::create_mailbox,
        module: mail::commands::MODULE,
    },
    Method {
        name: "extension_mail_delete_messages",
        handler: mail::commands::delete_messages,
        module: mail::commands::MODULE,
    },
    Method {
        name: "extension_mail_append_message",
        handler: mail::commands::append_message,
        module: mail::commands::MODULE,
    },
    Method {
        name: "extension_mail_send_message",
        handler: mail::commands::send_message,
        module: mail::commands::MODULE,
    },
    Method {
        name: "extension_mail_build_rfc822",
        handler: mail::commands::build_rfc822,
        module: mail::commands::MODULE,
    },
    Method {
        name: "extension_mail_start_watch",
        handler: mail::commands::start_watch,
        module: mail::commands::MODULE,
    },
    Method {
        name: "extension_mail_stop_watch",
        handler: mail::commands::stop_watch,
        module: mail::commands::MODULE,
    },
    Method {
        name: "extension_shell_list_available",
        handler: shell::list_available,
        module: shell::MODULE,
    },
    Method {
        name: "extension_shell_create",
        handler: shell::create,
        module: shell::MODULE,
    },
    Method {
        name: "extension_shell_write",
        handler: shell::write,
        module: shell::MODULE,
    },
    Method {
        name: "extension_shell_ack",
        handler: shell::ack,
        module: shell::MODULE,
    },
    Method {
        name: "extension_shell_resize",
        handler: shell::resize,
        module: shell::MODULE,
    },
    Method {
        name: "extension_remote_storage_list_backends",
        handler: remote_storage::list_backends,
        module: remote_storage::MODULE,
    },
    Method {
        name: "extension_remote_storage_add_backend",
        handler: remote_storage_propose::add_backend,
        module: remote_storage_propose::MODULE,
    },
    Method {
        name: "extension_remote_storage_update_backend",
        handler: remote_storage_manage::update_backend,
        module: remote_storage_manage::MODULE,
    },
    Method {
        name: "extension_remote_storage_remove_backend",
        handler: remote_storage_manage::remove_backend,
        module: remote_storage_manage::MODULE,
    },
    Method {
        name: "extension_remote_storage_test_backend",
        handler: remote_storage_manage::test_backend,
        module: remote_storage_manage::MODULE,
    },
    Method {
        name: "extension_remote_storage_upload",
        handler: remote_storage::upload,
        module: remote_storage::MODULE,
    },
    Method {
        name: "extension_remote_storage_download",
        handler: remote_storage::download,
        module: remote_storage::MODULE,
    },
    Method {
        name: "extension_remote_storage_list",
        handler: remote_storage::list,
        module: remote_storage::MODULE,
    },
    Method {
        name: "extension_remote_storage_delete",
        handler: remote_storage::delete,
        module: remote_storage::MODULE,
    },
    Method {
        name: "extension_encrypted_folder_choose",
        handler: encrypted_folder::choose,
        module: encrypted_folder::MODULE,
    },
    Method {
        name: "extension_encrypted_folder_list",
        handler: encrypted_folder::list,
        module: encrypted_folder::MODULE,
    },
    Method {
        name: "extension_encrypted_folder_download",
        handler: encrypted_folder::download,
        module: encrypted_folder::MODULE,
    },
    Method {
        name: "extension_encrypted_folder_upload",
        handler: encrypted_folder::upload,
        module: encrypted_folder::MODULE,
    },
    Method {
        name: "extension_encrypted_folder_delete",
        handler: encrypted_folder::delete,
        module: encrypted_folder::MODULE,
    },
    Method {
        name: "extension_shell_close",
        handler: shell::close,
        module: shell::MODULE,
    },
];
