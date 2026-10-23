// Windows reports native crashes as NTSTATUS exit codes; name the common ones
// so "Process ended" logs say why the streamer died.
const KNOWN_NTSTATUS_EXIT_CODES: Record<number, string> = {
  0xc0000005: "access violation",
  0xc000001d: "illegal instruction (CPU lacks a required instruction set)",
  0xc00000fd: "stack overflow",
  0xc0000374: "heap corruption",
  0xc0000409: "fail-fast / abort (Rust panic or CRT abort)",
  0xc0000135: "missing DLL",
  0xc0000142: "DLL initialization failed",
  0xc0000017: "out of memory",
  0x40010004: "terminated by debugger/console close",
  0xc000013a: "terminated by Ctrl+C / console close",
};

export function describeNativeExitCode(code: number | null): string {
  if (code === null) {
    return "exit code unknown";
  }
  const unsigned = code >>> 0;
  const known = KNOWN_NTSTATUS_EXIT_CODES[unsigned];
  if (known) {
    return `exit code ${code} (0x${unsigned.toString(16).toUpperCase()}: ${known})`;
  }
  return `exit code ${code}`;
}
