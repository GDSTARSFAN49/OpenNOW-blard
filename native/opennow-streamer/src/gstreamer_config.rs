// Always compiled so present-policy unit tests run without the optional
// `gstreamer` feature; production callers live behind that feature.
#![allow(dead_code)]

pub(crate) const EXTERNAL_RENDERER_ENV: &str = "OPENNOW_NATIVE_EXTERNAL_RENDERER";
pub(crate) const NATIVE_VIDEO_API_ENV: &str = "OPENNOW_NATIVE_VIDEO_API";
pub(crate) const NATIVE_VIDEO_BACKEND_ENV: &str = "OPENNOW_NATIVE_VIDEO_BACKEND";
pub(crate) const NATIVE_ZERO_COPY_ENV: &str = "OPENNOW_NATIVE_ZERO_COPY";
pub(crate) const NATIVE_PRESENT_MAX_FPS_ENV: &str = "OPENNOW_NATIVE_PRESENT_MAX_FPS";
pub(crate) const NATIVE_D3D_FULLSCREEN_ENV: &str = "OPENNOW_NATIVE_D3D_FULLSCREEN";
pub(crate) const NATIVE_AUDIO_LATENCY_ENV: &str = "OPENNOW_NATIVE_AUDIO_LATENCY_MS";
pub(crate) const NATIVE_AUDIO_SYNC_ENV: &str = "OPENNOW_NATIVE_AUDIO_SYNC";
/// Audio jitter-buffer budget. Video keeps the 2 ms webrtcbin default; audio
/// needs enough slack to absorb network jitter and scheduler stalls on slow
/// CPUs, otherwise late Opus packets are dropped and playback crackles.
pub(crate) const DEFAULT_AUDIO_LATENCY_MS: u32 = 60;
const MIN_AUDIO_LATENCY_MS: u32 = 10;
const MAX_AUDIO_LATENCY_MS: u32 = 500;
pub(crate) const PRESENT_LIMITER_AUTO_SENTINEL: u32 = u32::MAX;
pub(crate) const PRESENT_LIMITER_VRR_SENTINEL: u32 = u32::MAX - 1;
const VRR_REFRESH_HEADROOM_FPS: u32 = 3;

pub(crate) fn use_external_renderer_window() -> bool {
    std::env::var(EXTERNAL_RENDERER_ENV)
        .map(|value| {
            !matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "0" | "false" | "no" | "off"
            )
        })
        // Default to the internal child-surface renderer (single Electron window).
        .unwrap_or(false)
}

pub(crate) fn use_internal_renderer() -> bool {
    !use_external_renderer_window()
}

pub(crate) fn requested_video_backend() -> String {
    std::env::var(NATIVE_VIDEO_BACKEND_ENV)
        .or_else(|_| std::env::var(NATIVE_VIDEO_API_ENV))
        .unwrap_or_else(|_| "auto".to_owned())
        .to_ascii_lowercase()
}

pub(crate) fn zero_copy_requested() -> bool {
    matches!(
        std::env::var(NATIVE_ZERO_COPY_ENV)
            .unwrap_or_else(|_| "auto".to_owned())
            .to_ascii_lowercase()
            .as_str(),
        "1" | "true" | "yes" | "forced"
    )
}

pub(crate) fn resolve_present_max_fps(cloud_gsync_enabled: bool) -> u32 {
    if let Ok(value) = std::env::var(NATIVE_PRESENT_MAX_FPS_ENV) {
        let value = value.trim().to_ascii_lowercase();
        if value == "0" || value == "off" || value == "false" || value == "unlimited" {
            return 0;
        }
        if value == "auto" {
            return PRESENT_LIMITER_AUTO_SENTINEL;
        }
        if let Ok(fps) = value.parse::<u32>() {
            return fps;
        }
    }
    if cloud_gsync_enabled {
        PRESENT_LIMITER_VRR_SENTINEL
    } else {
        0
    }
}

pub(crate) fn automatic_present_max_fps(requested_fps: u32, display_hz: Option<u32>) -> u32 {
    display_hz
        .filter(|display_hz| *display_hz >= 30 && *display_hz < requested_fps)
        .unwrap_or(0)
}

pub(crate) fn vrr_present_max_fps(requested_fps: u32, display_hz: Option<u32>) -> u32 {
    display_hz
        .filter(|display_hz| *display_hz >= 30 && *display_hz <= requested_fps)
        .map(|display_hz| display_hz.saturating_sub(VRR_REFRESH_HEADROOM_FPS))
        .unwrap_or(0)
}

pub(crate) fn resolve_d3d_fullscreen_sink(cloud_gsync_enabled: bool) -> bool {
    resolve_d3d_fullscreen_sink_for(
        use_internal_renderer(),
        cloud_gsync_enabled,
        std::env::var(NATIVE_D3D_FULLSCREEN_ENV).ok(),
    )
}

/// Pure policy for exclusive D3D fullscreen present.
///
/// Internal (child HWND) always stays windowed — exclusive fullscreen fights
/// Electron parenting. External may enable it for Cloud G-Sync/VRR, or via
/// `OPENNOW_NATIVE_D3D_FULLSCREEN`.
pub(crate) fn resolve_d3d_fullscreen_sink_for(
    internal_renderer: bool,
    cloud_gsync_enabled: bool,
    env_override: Option<String>,
) -> bool {
    if internal_renderer {
        return false;
    }

    if let Some(value) = env_override {
        let value = value.trim().to_ascii_lowercase();
        if matches!(value.as_str(), "1" | "on" | "true" | "yes") {
            return true;
        }
        if matches!(value.as_str(), "0" | "off" | "false" | "no") {
            return false;
        }
    }

    cloud_gsync_enabled
}

pub(crate) fn audio_latency_ms() -> u32 {
    resolve_audio_latency_ms(std::env::var(NATIVE_AUDIO_LATENCY_ENV).ok())
}

pub(crate) fn resolve_audio_latency_ms(env_override: Option<String>) -> u32 {
    env_override
        .and_then(|value| value.trim().parse::<u32>().ok())
        .map(|value| value.clamp(MIN_AUDIO_LATENCY_MS, MAX_AUDIO_LATENCY_MS))
        .unwrap_or(DEFAULT_AUDIO_LATENCY_MS)
}

/// Clock-synced audio playout (default) lets the audio sink pace output from
/// the jitter buffer and correct sender/soundcard clock drift. `0` restores
/// the legacy unsynchronized playout.
pub(crate) fn audio_clock_sync_enabled() -> bool {
    resolve_audio_clock_sync(std::env::var(NATIVE_AUDIO_SYNC_ENV).ok())
}

pub(crate) fn resolve_audio_clock_sync(env_override: Option<String>) -> bool {
    !env_override.is_some_and(|value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "0" | "false" | "no" | "off"
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn automatic_present_limiter_uses_display_refresh_below_requested_fps() {
        assert_eq!(automatic_present_max_fps(240, Some(165)), 165);
        assert_eq!(automatic_present_max_fps(240, Some(240)), 0);
        assert_eq!(automatic_present_max_fps(240, Some(1)), 0);
        assert_eq!(automatic_present_max_fps(240, None), 0);
    }

    #[test]
    fn default_present_policy_is_uncapped_without_vrr() {
        assert_eq!(resolve_present_max_fps(false), 0);
        assert_eq!(
            resolve_present_max_fps(true),
            PRESENT_LIMITER_VRR_SENTINEL
        );
    }

    #[test]
    fn vrr_present_limiter_stays_below_refresh_ceiling() {
        assert_eq!(vrr_present_max_fps(240, Some(165)), 162);
        assert_eq!(vrr_present_max_fps(165, Some(165)), 162);
        assert_eq!(vrr_present_max_fps(120, Some(165)), 0);
        assert_eq!(vrr_present_max_fps(240, None), 0);
    }

    #[test]
    fn internal_renderer_never_enables_exclusive_d3d_fullscreen() {
        assert!(!resolve_d3d_fullscreen_sink_for(true, true, None));
        assert!(!resolve_d3d_fullscreen_sink_for(
            true,
            true,
            Some("1".to_owned())
        ));
        assert!(!resolve_d3d_fullscreen_sink_for(
            true,
            false,
            Some("on".to_owned())
        ));
    }

    #[test]
    fn external_renderer_follows_cloud_gsync_and_env_for_d3d_fullscreen() {
        assert!(resolve_d3d_fullscreen_sink_for(false, true, None));
        assert!(!resolve_d3d_fullscreen_sink_for(false, false, None));
        assert!(resolve_d3d_fullscreen_sink_for(
            false,
            false,
            Some("1".to_owned())
        ));
        assert!(!resolve_d3d_fullscreen_sink_for(
            false,
            true,
            Some("0".to_owned())
        ));
    }

    #[test]
    fn audio_latency_defaults_and_clamps_env_override() {
        assert_eq!(resolve_audio_latency_ms(None), DEFAULT_AUDIO_LATENCY_MS);
        assert_eq!(resolve_audio_latency_ms(Some("abc".to_owned())), DEFAULT_AUDIO_LATENCY_MS);
        assert_eq!(resolve_audio_latency_ms(Some(" 80 ".to_owned())), 80);
        assert_eq!(resolve_audio_latency_ms(Some("1".to_owned())), MIN_AUDIO_LATENCY_MS);
        assert_eq!(resolve_audio_latency_ms(Some("9000".to_owned())), MAX_AUDIO_LATENCY_MS);
    }

    #[test]
    fn audio_clock_sync_is_on_unless_disabled() {
        assert!(resolve_audio_clock_sync(None));
        assert!(resolve_audio_clock_sync(Some("1".to_owned())));
        assert!(!resolve_audio_clock_sync(Some("0".to_owned())));
        assert!(!resolve_audio_clock_sync(Some("Off".to_owned())));
    }
}
