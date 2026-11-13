//! Images in tool results (spec 044 FR-035a, research R2): a model that accepts images (its
//! capabilities, spec 012) gets them as image blocks next to the result's text; any other model a
//! note in their place. The history keeps the text with a note, never the image, so later turns do
//! not send it again.

use crate::adapters::types::{Attachment, AttachmentKind, ChatRequest, ToolSpec};
use crate::chat::tools::{ToolImage, ToolResult};
use crate::model_capabilities::ModelCapabilities;

/// Whether the model of `request` is shown images; an undetermined model is not.
pub(super) fn shows_images(request: &ChatRequest) -> bool {
    accepts_images(request.capabilities.as_ref())
}

/// Whether a model with these capabilities accepts images; an undetermined model does not.
pub(crate) fn accepts_images(capabilities: Option<&ModelCapabilities>) -> bool {
    capabilities
        .and_then(|capabilities| capabilities.accepted_attachment_kinds.as_ref())
        .is_some_and(|kinds| kinds.contains(&AttachmentKind::Image))
}

/// The browser tools that are only of use to a model that sees pictures (spec 065 FR-063): the
/// screenshot, and the clicks and drags at a position of it. Without images the agent works with
/// element numbers. An external agent is not gated here; it gets every tool over MCP.
pub(crate) const NEED_IMAGES: [&str; 3] = [
    "browser_page_screenshot",
    "browser_click_at",
    "browser_drag",
];

/// Takes from `tools` those that need images when the model is not shown any.
pub(crate) fn retain_usable(tools: &mut Vec<ToolSpec>, shown: bool) {
    if !shown {
        tools.retain(|tool| !NEED_IMAGES.contains(&tool.name.as_str()));
    }
}

fn notes(images: &[ToolImage], shown: bool) -> String {
    images
        .iter()
        .map(|image| {
            if shown {
                format!("\n[Image {}: shown to the model.]", image.name)
            } else {
                format!(
                    "\n[Image {}: this model does not accept images, so it was not shown.]",
                    image.name
                )
            }
        })
        .collect()
}

/// The text the history keeps of `result`.
pub(super) fn kept(result: &ToolResult, shown: bool) -> String {
    format!("{}{}", result.content, notes(&result.images, shown))
}

/// The text and the images the model gets of `result`.
pub(super) fn sent(result: &ToolResult, shown: bool) -> (String, Vec<Attachment>) {
    if !shown {
        return (kept(result, false), Vec::new());
    }
    let images = result
        .images
        .iter()
        .map(|image| Attachment {
            name: image.name.clone(),
            kind: AttachmentKind::Image,
            media_type: image.media_type.clone(),
            bytes: image.bytes.clone(),
        })
        .collect();
    (result.content.clone(), images)
}

#[cfg(test)]
#[path = "images_tests.rs"]
mod tests;
