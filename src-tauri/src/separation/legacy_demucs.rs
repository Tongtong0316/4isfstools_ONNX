pub(crate) const LEGACY_ENGINE_ID: &str = "legacy_demucs";
pub(crate) const LEGACY_DEMUCS_ENV: &str = "MACARON_USE_LEGACY_DEMUCS";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LegacyDemucsStatus {
    AvailableFallback,
    Unavailable,
}

pub(crate) fn legacy_demucs_enabled() -> bool {
    std::env::var(LEGACY_DEMUCS_ENV)
        .map(|value| {
            let normalized = value.trim().to_ascii_lowercase();
            normalized == "1" || normalized == "true" || normalized == "yes" || normalized == "on"
        })
        .unwrap_or(false)
}
