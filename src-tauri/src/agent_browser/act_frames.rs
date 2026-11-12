//! Elements of frames (spec 065 FR-053): a call that names a number of a frame runs the page
//! script in that frame, and the point it reports, which lies in the frame's own view, is moved to
//! where the frame is on the page, so a real click or hover lands on it. The website of the frame
//! is the one the rights are checked against (`Target::url`).

use serde_json::{json, Value};

use super::{element_gone, Acting, Target};
use crate::agent_browser::calls::browser_error;
use crate::agent_browser::frames::{self, Attached, FrameSlice};
use crate::agent_browser::tab::Tab;
use crate::chat::tools::native_action::NativeError;

impl Acting<'_> {
    /// What the script says about the element of `args` in the frame of `slice`, its point moved by
    /// the place of the frame once the script has brought the element into view.
    pub(super) async fn target_in_frame(
        &self,
        tab: &Tab,
        slice: &FrameSlice,
        args: &Value,
    ) -> Result<Target, NativeError> {
        let mut attached = Attached::new(self.connection);
        let reply = frames::script(&mut attached, tab, slice, args).await;
        let place = match &reply {
            Ok(reply) if reply.get("x").is_some() => {
                Some(frames::origin(&mut attached, tab, &slice.chain).await)
            }
            _ => None,
        };
        attached.release().await;
        let mut target: Target =
            serde_json::from_value(reply.map_err(browser_error)?).map_err(|error| {
                browser_error(crate::agent_browser::error::BrowserError::Protocol(
                    error.to_string(),
                ))
            })?;
        if target.gone {
            return Err(element_gone());
        }
        if let Some(place) = place {
            let (x, y) = place.map_err(browser_error)?;
            target.x += x;
            target.y += y;
        }
        Ok(target)
    }

    /// Brings element `element` into view, in its frame when it belongs to one.
    pub(super) async fn bring_into_view(&self, tab: &Tab, element: u32) -> Result<(), NativeError> {
        let mut args = json!({ "scrollTo": element });
        let Some(slice) = frames::route(&tab.frames(), &mut args) else {
            return tab
                .scroll(true, Some(element))
                .await
                .map(drop)
                .map_err(browser_error);
        };
        let mut attached = Attached::new(self.connection);
        let reply = frames::script(&mut attached, tab, &slice, &args).await;
        attached.release().await;
        if reply.map_err(browser_error)?["gone"] == true {
            return Err(element_gone());
        }
        Ok(())
    }
}
