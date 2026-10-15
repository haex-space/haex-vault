//! The Tauri commands of the icon sets (spec 053, contracts/tauri-commands.md). They are commands
//! of the app's own window only: no bridge method for extensions and no action for agents gives
//! them (FR-016).

use tauri::ipc::Response;
use tauri::State;

use super::store::{self, Source};
use super::{
    builtin, resolve, IconAnswer, IconQuery, IconSelection, IconSetInfo, IconSetLicense,
    IconSetRef, Scheme,
};
use crate::error::{HolziError, Result};
use crate::state::AppState;
use crate::state_utils::active_database;

/// How many entries one call may ask about.
pub const MAX_QUERIES: usize = 500;

/// The answers for `queries`, in the same order. A set that is missing, still arriving, damaged or
/// of a newer app answers like the standard set: this never fails because of the set (FR-020).
#[tauri::command]
pub async fn files_icons_resolve(
    state: State<'_, AppState>,
    set: IconSetRef,
    scheme: Scheme,
    queries: Vec<IconQuery>,
) -> Result<Vec<IconAnswer>> {
    if queries.len() > MAX_QUERIES {
        return Err(HolziError::InvalidInput {
            reason: format!("at most {MAX_QUERIES} entries per call"),
        });
    }
    let db = active_database(&state)?;
    let in_use = db.read(move |q| store::in_use(q, &set)).await?;
    Ok(answers(&in_use.source, scheme, &queries))
}

/// The answers of one source; the core of [`files_icons_resolve`].
pub fn answers(source: &Source, scheme: Scheme, queries: &[IconQuery]) -> Vec<IconAnswer> {
    let theme = match source {
        Source::Lucide => None,
        Source::Pack(pack) => Some(pack.theme()),
    };
    queries
        .iter()
        .map(|query| resolve(theme, scheme, query))
        .collect()
}

/// The bytes of an image of a set: an SVG sanitized, a PNG or WebP checked. The window uses them
/// only as the source of an `<img>`.
#[tauri::command]
pub async fn files_icon_image(
    state: State<'_, AppState>,
    set: IconSetRef,
    id: String,
) -> Result<Response> {
    let db = active_database(&state)?;
    let in_use = db.read(move |q| store::in_use(q, &set)).await?;
    let Source::Pack(pack) = in_use.source else {
        return Err(HolziError::InvalidInput {
            reason: "this set has no images".to_owned(),
        });
    };
    let image = pack.image(&id).map_err(|error| HolziError::InvalidInput {
        reason: error.to_string(),
    })?;
    Ok(Response::new(image.bytes.clone()))
}

/// The sets for the settings: the built-in ones, then the custom ones by name, each with its
/// state (a custom set whose pack fails the checks is `damaged`).
#[tauri::command]
pub async fn file_icon_sets_list(state: State<'_, AppState>) -> Result<Vec<IconSetInfo>> {
    let db = active_database(&state)?;
    db.read(|q| list(q)).await
}

pub(crate) fn list(
    q: &mut impl crate::storage::query::Query,
) -> haex_crdt::Result<Vec<IconSetInfo>> {
    let mut sets = builtin::infos();
    for row in store::rows(q)? {
        let set_ref = IconSetRef::Custom { id: row.id.clone() };
        let state = store::in_use(q, &set_ref)?.state;
        sets.push(IconSetInfo {
            set_ref,
            name: row.name,
            author: row.author,
            license: row.license,
            source: row.source,
            image_count: row.image_count,
            state,
            removable: true,
        });
    }
    Ok(sets)
}

/// The set the vault has chosen and what it is in use as (`ready`, or why the standard set shows
/// instead): the window reads it for the group in the settings.
#[tauri::command]
pub async fn file_icon_set_selection(state: State<'_, AppState>) -> Result<IconSelection> {
    let db = active_database(&state)?;
    db.read(|q| {
        let (set, in_use) = store::chosen(q)?;
        Ok(IconSelection {
            set,
            state: in_use.state,
        })
    })
    .await
}

/// Author, license and text of the built-in sets, for the page "Lizenzen" (FR-017).
#[tauri::command]
pub fn file_icon_set_licenses() -> Vec<IconSetLicense> {
    builtin::licenses()
}
