//! The entries for one page (spec 049 FR-019, spec 065 research R7): the headers of a caller's grants
//! whose address has the same site as the page. One rule for the External Bridge and the agent
//! browser.

use url::Url;

use super::{Headers, PasswordsService};
use crate::caller::Caller;
use crate::error::Result;
use crate::passwords::access::Grant;
use crate::passwords::model::ItemHeader;
use crate::passwords::site::{site_of, site_of_address};

impl PasswordsService {
    /// The headers within `grants` whose address has the site of `page`; none for a page without a
    /// site.
    // ponytail: filters all headers of the grants in memory. Ceiling: one header list per call
    // (049 SC-002: under 500 ms at 5 000 entries). Upgrade path: an index of sites in the vault.
    pub async fn headers_for_page(
        &self,
        caller: &Caller,
        grants: &[Grant],
        page: &Url,
    ) -> Result<Vec<ItemHeader>> {
        let Some(site) = page.host_str().and_then(site_of) else {
            return Ok(Vec::new());
        };
        let Headers::Items(headers) = self.list_headers(caller, grants).await? else {
            return Ok(Vec::new());
        };
        Ok(headers
            .into_iter()
            .filter(|header| {
                header
                    .url
                    .as_deref()
                    .and_then(site_of_address)
                    .is_some_and(|candidate| candidate == site)
            })
            .collect())
    }
}

#[cfg(test)]
#[path = "by_site_tests.rs"]
mod tests;
