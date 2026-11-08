//! Share numeric version interpretation between compatibility and authentication.
pub(super) fn parse(raw: &str) -> Option<[u32; 3]> {
    let raw = raw.trim().strip_prefix('v').unwrap_or(raw.trim());
    let mut parts = raw.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.split(|c: char| !c.is_ascii_digit()).next()?;
    Some([major, minor, patch.parse().ok()?])
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn accepts_release_suffixes_and_additional_segments_without_guessing_unknown_versions() {
        for raw in ["3.3.9", "3.3.9.1", "3.3.9-beta.1", "v3.3.9+build"] {
            assert_eq!(parse(raw), Some([3, 3, 9]));
        }
        for raw in ["unknown", "3.3", "3.3.beta", ""] {
            assert_eq!(parse(raw), None);
        }
    }
}
