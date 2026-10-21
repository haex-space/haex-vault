//! Generator preset methods of the service (spec 034, US3, FR-014): for the user alone (Z11), except
//! the list, which every caller with a password grant reads (spec 049 FR-027); presets hold no
//! secret.

use super::{require_user, PasswordsService};
use crate::error::Result;
use crate::passwords::access::{authorize_any_grant, Caller, Grant};
use crate::passwords::model::{Preset, PresetInput};
use crate::passwords::presets;

impl PasswordsService {
    pub async fn preset_list(&self, caller: &Caller, grants: &[Grant]) -> Result<Vec<Preset>> {
        authorize_any_grant(caller, grants)?;
        self.db()
            .read(|q| presets::list(q).map_err(Into::into))
            .await
    }

    /// Saves a preset (an empty id creates one) and returns its id.
    pub async fn preset_save(&self, caller: &Caller, input: PresetInput) -> Result<String> {
        require_user(caller)?;
        self.db()
            .write(move |tx| presets::save(tx, &input).map_err(Into::into))
            .await
    }

    pub async fn preset_delete(&self, caller: &Caller, preset_id: String) -> Result<()> {
        require_user(caller)?;
        self.db()
            .write(move |tx| presets::delete(tx, &preset_id).map_err(Into::into))
            .await
    }
}
