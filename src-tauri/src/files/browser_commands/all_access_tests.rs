use tauri::test::{mock_builder, mock_context, noop_assets};

use super::*;

fn mock_app() -> AppHandle<tauri::test::MockRuntime> {
    mock_builder()
        .plugin(tauri_plugin_holzi_android::init())
        .build(mock_context(noop_assets()))
        .expect("mock app")
        .handle()
        .clone()
}

#[tokio::test]
async fn a_desktop_has_access_without_asking() {
    let app = mock_app();
    let granted = FilesAllAccess {
        granted: true,
        platform: FilesAccessPlatform::Other,
    };
    assert_eq!(files_all_access(app.clone()).await.unwrap(), granted);
    assert_eq!(files_request_all_access(app).await.unwrap(), granted);
}

#[test]
fn a_platform_that_cannot_tell_is_an_error_not_a_refusal() {
    let error = answer(Err("plugin gone".to_owned())).unwrap_err();
    assert_eq!(error.code, FilesErrorCode::Unsupported);
    assert_eq!(error.message, "plugin gone");
}
