use serde::{Deserialize, Serialize};

pub(super) const LEGACY_COMMON_LOGIN_SETTING: &str = "common_login_profiles_json";
pub(super) const COMMON_LOGIN_SETTING: &str = "common_login_catalog_json";
pub(super) const LEGACY_PASSWORD_SCOPE: &str = "common_login_profile";
pub(super) const PASSWORD_SCOPE: &str = "common_login_password";
pub(super) const PASSWORD_KIND: &str = "password";

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct CommonLoginEmail {
    pub(super) id: String,
    pub(super) email: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct CommonLoginPassword {
    pub(super) id: String,
    pub(super) password_secret_id: String,
    pub(super) secret_scope: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct CommonLoginCatalog {
    pub(super) emails: Vec<CommonLoginEmail>,
    pub(super) passwords: Vec<CommonLoginPassword>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct LegacyCommonLoginProfile {
    pub(super) id: String,
    pub(super) email: String,
    pub(super) password_secret_id: Option<String>,
}

pub(super) fn is_common_login_setting(key: &str) -> bool {
    matches!(key, LEGACY_COMMON_LOGIN_SETTING | COMMON_LOGIN_SETTING)
}

pub(super) fn is_supported_password_scope(scope: &str) -> bool {
    matches!(scope, LEGACY_PASSWORD_SCOPE | PASSWORD_SCOPE)
}
