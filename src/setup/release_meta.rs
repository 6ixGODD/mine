#![forbid(unsafe_code)]
//! Exact, bounded release resolution and stable-version comparison.
use super::delivery::{Client, failure, release_identity, validated_tag};
use crate::domain::error::MineResult;

pub fn latest_tag() -> MineResult<String> {
    if let Ok(tag) = std::env::var("MINE_REF") {
        if tag != "latest" {
            return Ok(validated_tag(&tag)?.to_owned());
        }
    }
    let (account, repo) = release_identity()?;
    let mut bytes = Vec::new();
    Client::new().fetch(
        &format!("https://api.github.com/repos/{account}/{repo}/releases/latest"),
        &mut bytes,
        1024 * 1024,
    )?;
    let body: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|_| failure("malformed release metadata JSON"))?;
    let tag = body
        .get("tag_name")
        .and_then(|t| t.as_str())
        .ok_or_else(|| failure("release metadata has no tag_name"))?;
    Ok(validated_tag(tag)?.to_owned())
}

pub fn is_newer(latest: &str, current: &str) -> bool {
    fn version(text: &str) -> Option<(u64, u64, u64)> {
        validated_tag(text).ok()?;
        let mut parts = text.strip_prefix('v').unwrap_or(text).split('.');
        Some((
            parts.next()?.parse().ok()?,
            parts.next()?.parse().ok()?,
            parts.next()?.parse().ok()?,
        ))
    }
    matches!((version(latest), version(current)), (Some(a),Some(b)) if a > b)
}
