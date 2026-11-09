//! What the executor needs from the app ([`super::BrowserTools`]).

use async_trait::async_trait;

use crate::agent_browser::error::BrowserError;
use crate::agent_browser::prompts::BrowserProfileNote;
use crate::agent_browser::sites::Site;
use crate::agent_browser::StartConfig;
use crate::caller::Caller;
use crate::files::access::AgentGrants;
use crate::files::local::OwnPlaces;
use crate::mcp_server::grants::{BrowserRights, PasswordRights, RequestRule, SiteLevel};
use crate::passwords::service::PasswordsService;

/// What the executor needs from the app.
#[async_trait]
pub trait BrowserEnv: Send + Sync + 'static {
    /// The tracker filters (FR-045) a new tab asks per request; `None` while blocking is off. The
    /// first call after a start may wait for the first download of the lists.
    async fn filter(&self) -> Option<crate::agent_browser::filters::LiveFilter> {
        None
    }
    /// The rights group `browser` of `caller` now; `None` when it is off.
    async fn rights(&self, caller: &Caller) -> Option<BrowserRights>;
    /// Adds `site` at `level` to the rights of `caller` ("Für diese Website merken").
    async fn remember(
        &self,
        caller: &Caller,
        site: &Site,
        level: SiteLevel,
        profile_id: Option<&str>,
    ) -> Result<(), String>;
    /// The name the dialogs show for `caller`.
    async fn agent_name(&self, caller: &Caller) -> String;
    /// The agent profile a remembered answer of `caller` is written into, for the dialogs (spec 083
    /// FR-015); `None` for the built-in agent, which keeps its own setting until PR C.
    async fn profile_note(&self, _caller: &Caller) -> Option<BrowserProfileNote> {
        None
    }
    /// The language of holzi's window, for the marker in the tabs.
    fn locale(&self) -> String;
    /// How to start the browser now.
    async fn start_config(&self) -> Result<StartConfig, BrowserError>;
    /// The folder downloads of agents land in: `HaexVault/<vault>` in the system's downloads
    /// folder (research R11).
    fn downloads_folder(&self) -> Result<std::path::PathBuf, String>;
    /// What the file rights (044) of `caller` reach: holzi's own places and its grants; `None`
    /// when they cannot be read.
    async fn file_rights(&self, caller: &Caller) -> Option<(OwnPlaces, AgentGrants)>;
    /// The rights group "Passwörter" of `caller` (021; for the built-in agent its own setting);
    /// `None` when it is off.
    async fn password_rights(&self, caller: &Caller) -> Option<PasswordRights>;
    /// The password manager of the open vault.
    fn passwords(&self) -> Option<PasswordsService>;
    /// "Für diese Website merken" of a fill: `site` fills without asking from now on (FR-021).
    async fn remember_fill(
        &self,
        caller: &Caller,
        site: &Site,
        profile_id: Option<&str>,
    ) -> Result<(), String>;
    /// "Für diese Website merken" of a page request: requests the pages send to `site` follow
    /// `rule` from now on, without asking (FR-044).
    async fn remember_request(
        &self,
        caller: &Caller,
        site: &Site,
        rule: RequestRule,
        profile_id: Option<&str>,
    ) -> Result<(), String>;
    /// Switches the rights group `browser` of `caller` on, with no website listed and every
    /// website at operating (FR-040, FR-043).
    async fn switch_on(&self, _caller: &Caller, _profile_id: Option<&str>) -> Result<(), String> {
        Err("the browser cannot be switched on here".to_owned())
    }
}
