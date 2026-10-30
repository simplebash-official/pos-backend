/// `"1.13.0"` (a `-suffix` such as `-dev` is ignored) as comparable numbers;
/// `None` for anything that is not three dot-separated numbers.
pub(super) fn parse_app_version(version: &str) -> Option<(u64, u64, u64)> {
    let core = version.trim().split('-').next()?;
    let mut parts = core.split('.').map(|p| p.parse::<u64>().ok());
    let parsed = (parts.next()??, parts.next()??, parts.next()??);
    parts.next().is_none().then_some(parsed)
}
