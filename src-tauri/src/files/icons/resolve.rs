//! Which image an entry gets (spec 053 FR-003, FR-005, research R7). One function, the only place
//! where the order is written down:
//!
//! 0. a state (encryption, no access) comes first and is never covered by a name or an extension;
//! 1. the name: a folder by its name, a file by its name;
//! 2. a file by its extension, the longest first (`a.test.ts` tries `test.ts`, then `ts`);
//! 3. the role (`x-holzi.roles`: image, video, audio, pdf, text, folder …);
//! 4. the default of the theme (`file` or `folder`);
//! 5. the standard set (Lucide), which answers for every role.
//!
//! In the light scheme every step looks into the block `light` of the theme first.

use super::manifest::{extension_candidates, IconTheme};
use super::role::role_of;
use super::{IconAnswer, IconQuery, IconRole, ImageMime, Scheme};

/// The answer for `query` in `theme`; `None` for the standard set, which has no images.
pub fn resolve(theme: Option<&IconTheme>, scheme: Scheme, query: &IconQuery) -> IconAnswer {
    let role = role_of(query);
    let found = theme.and_then(|theme| image_path(theme, scheme, query, role));
    match found.and_then(answer_for) {
        Some(answer) => answer,
        None => symbol(role),
    }
}

fn symbol(role: IconRole) -> IconAnswer {
    IconAnswer::Builtin {
        name: role.fallback_symbol().to_owned(),
    }
}

fn image_path<'a>(
    theme: &'a IconTheme,
    scheme: Scheme,
    query: &IconQuery,
    role: IconRole,
) -> Option<&'a str> {
    if role.is_state() {
        return theme.role(scheme, role);
    }
    let name = query.name.to_lowercase();
    let by_name = if query.is_dir {
        theme.folder_name(scheme, &name)
    } else {
        theme.file_name(scheme, &name).or_else(|| {
            extension_candidates(&name).find_map(|extension| theme.extension(scheme, extension))
        })
    };
    by_name
        .or_else(|| theme.role(scheme, role))
        .or_else(|| theme.default_for(scheme, query.is_dir))
}

/// The answer for an image path: its id is the path, its media type comes from the extension.
fn answer_for(path: &str) -> Option<IconAnswer> {
    let mime = match crate::files::kind::extension(path).as_str() {
        "svg" => ImageMime::Svg,
        "png" => ImageMime::Png,
        "webp" => ImageMime::Webp,
        _ => return None,
    };
    Some(IconAnswer::Image {
        id: path.to_owned(),
        mime,
    })
}

#[cfg(test)]
#[path = "resolve_tests.rs"]
mod tests;
