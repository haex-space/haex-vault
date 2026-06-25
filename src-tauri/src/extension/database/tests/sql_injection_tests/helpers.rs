use crate::extension::database::helpers::ExtensionSqlContext;

pub(super) fn create_test_context() -> ExtensionSqlContext {
    ExtensionSqlContext::new("testpublickey".to_string(), "testextension".to_string())
}

pub(super) fn get_expected_prefix() -> String {
    "testpublickey__testextension__".to_string()
}
