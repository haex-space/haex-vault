//! The browser that agents drive (spec 065, ADR-0019): holzi starts a Chromium-based browser of its
//! own and steers it over a pipe; agents reach it through the native catalog actions `browser.*`
//! within the rights group `browser`. This delivery brings the websites; the browser itself
//! follows (tasks.md, PR C).

pub mod sites;

#[cfg(test)]
mod sites_tests;
