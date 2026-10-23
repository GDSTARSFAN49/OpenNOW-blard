import assert from "node:assert/strict";
import test from "node:test";

import {
  buildChromiumCommandLine,
  MAC_STEAM_CONTROLLER_COMPATIBILITY_DISABLED_FEATURE,
  normalizeBootstrapChromiumPreferences,
  shouldForceReducedMotion,
} from "./chromiumCommandLine";

test("normalizes bootstrap Chromium preferences", () => {
  assert.deepEqual(
    normalizeBootstrapChromiumPreferences({
      decoderPreference: "hardware",
      encoderPreference: "software",
      steamControllerCompatibilityMode: true,
    }),
    {
      decoderPreference: "hardware",
      encoderPreference: "software",
      steamControllerCompatibilityMode: true,
    },
  );

  assert.deepEqual(
    normalizeBootstrapChromiumPreferences({
      decoderPreference: "invalid",
      encoderPreference: false,
      steamControllerCompatibilityMode: "true",
    }),
    {
      decoderPreference: "auto",
      encoderPreference: "auto",
      steamControllerCompatibilityMode: false,
    },
  );
});

test("adds Steam Controller compatibility feature disable only on macOS when enabled", () => {
  const preferences = normalizeBootstrapChromiumPreferences({
    steamControllerCompatibilityMode: true,
  });

  assert.equal(
    buildChromiumCommandLine(preferences, "darwin", "arm64").disableFeatures.includes(
      MAC_STEAM_CONTROLLER_COMPATIBILITY_DISABLED_FEATURE,
    ),
    true,
  );
  assert.equal(
    buildChromiumCommandLine(preferences, "linux", "x64").disableFeatures.includes(
      MAC_STEAM_CONTROLLER_COMPATIBILITY_DISABLED_FEATURE,
    ),
    false,
  );
  assert.equal(
    buildChromiumCommandLine(
      { ...preferences, steamControllerCompatibilityMode: false },
      "darwin",
      "arm64",
    ).disableFeatures.includes(MAC_STEAM_CONTROLLER_COMPATIBILITY_DISABLED_FEATURE),
    false,
  );
});

test("forces reduced motion on dual-core hosts unless overridden", () => {
  assert.equal(shouldForceReducedMotion(2, undefined), true);
  assert.equal(shouldForceReducedMotion(1, undefined), true);
  assert.equal(shouldForceReducedMotion(4, undefined), false);
  assert.equal(shouldForceReducedMotion(0, undefined), false);
  assert.equal(shouldForceReducedMotion(8, "1"), true);
  assert.equal(shouldForceReducedMotion(2, "0"), false);
});
