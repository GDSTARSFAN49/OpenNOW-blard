import assert from "node:assert/strict";
import test from "node:test";

import { describeNativeExitCode } from "./exitCode";

test("names known Windows NTSTATUS crash codes", () => {
  assert.equal(
    describeNativeExitCode(3221225477),
    "exit code 3221225477 (0xC0000005: access violation)",
  );
  assert.match(describeNativeExitCode(3221225501), /0xC000001D: illegal instruction/);
  assert.match(describeNativeExitCode(-1073740791), /0xC0000409: fail-fast/);
});

test("leaves ordinary and unknown exit codes unchanged", () => {
  assert.equal(describeNativeExitCode(0), "exit code 0");
  assert.equal(describeNativeExitCode(1), "exit code 1");
  assert.equal(describeNativeExitCode(null), "exit code unknown");
});
