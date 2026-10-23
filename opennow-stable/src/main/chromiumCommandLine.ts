import type { VideoAccelerationPreference } from "@shared/gfn";
import {
  buildVideoAccelerationCommandLine,
  isAccelerationPreference,
  type VideoAccelerationCommandLine,
} from "./videoAcceleration";

export const MAC_STEAM_CONTROLLER_COMPATIBILITY_DISABLED_FEATURE =
  "XboxUseGameControllerDataFetcherMac";

export interface BootstrapChromiumPreferences {
  decoderPreference: VideoAccelerationPreference;
  encoderPreference: VideoAccelerationPreference;
  steamControllerCompatibilityMode: boolean;
}

export function normalizeBootstrapChromiumPreferences(raw: unknown): BootstrapChromiumPreferences {
  const defaults: BootstrapChromiumPreferences = {
    decoderPreference: "auto",
    encoderPreference: "auto",
    steamControllerCompatibilityMode: false,
  };
  if (!raw || typeof raw !== "object") {
    return defaults;
  }

  const parsed = raw as Partial<BootstrapChromiumPreferences>;
  return {
    decoderPreference: isAccelerationPreference(parsed.decoderPreference)
      ? parsed.decoderPreference
      : defaults.decoderPreference,
    encoderPreference: isAccelerationPreference(parsed.encoderPreference)
      ? parsed.encoderPreference
      : defaults.encoderPreference,
    steamControllerCompatibilityMode: parsed.steamControllerCompatibilityMode === true,
  };
}

/** Hosts with this many logical CPUs or fewer start with reduced motion. */
export const LOW_END_LOGICAL_CPU_THRESHOLD = 2;

/**
 * Reduced motion turns off the full-screen WebGL backdrop and UI motion that
 * otherwise keep a dual-core CPU busy while the app sits idle.
 * `OPENNOW_REDUCED_MOTION=1|0` forces it on or off.
 */
export function shouldForceReducedMotion(
  logicalCpuCount: number,
  envOverride: string | undefined,
): boolean {
  const override = envOverride?.trim().toLowerCase();
  if (override === "1" || override === "true") {
    return true;
  }
  if (override === "0" || override === "false") {
    return false;
  }
  return logicalCpuCount > 0 && logicalCpuCount <= LOW_END_LOGICAL_CPU_THRESHOLD;
}

export function buildChromiumCommandLine(
  preferences: BootstrapChromiumPreferences,
  platform: NodeJS.Platform,
  arch: NodeJS.Architecture,
): VideoAccelerationCommandLine {
  const commandLine = buildVideoAccelerationCommandLine(preferences, platform, arch);
  if (platform !== "darwin" || !preferences.steamControllerCompatibilityMode) {
    return commandLine;
  }

  return {
    ...commandLine,
    disableFeatures: [
      ...commandLine.disableFeatures,
      MAC_STEAM_CONTROLLER_COMPATIBILITY_DISABLED_FEATURE,
    ],
  };
}
