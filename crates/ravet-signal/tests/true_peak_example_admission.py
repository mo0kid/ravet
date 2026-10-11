#!/usr/bin/env python3
"""Admission checks for the worker-only ``true-peak-file`` example (A11).

Invocation (isolated interpreter; the script never builds anything):

    python3 -I true-peak-example-admission.py --binary /path/to/true-peak-file
    python3 -I true-peak-example-admission.py --binary /path/to/true-peak-file \
        --include-exact-16mib [--exact-timeout 900]

``--binary`` must be an explicit, already built example binary, e.g. the file
produced by ``cargo build --release --example true-peak-file`` (building is the
caller's job; nothing was built or run when this script was written, and no
log is supplied with it).

Every fixture is raw little-endian f64 PCM that this script generates itself in
a fresh temporary directory. Each subprocess has stdin closed, a minimal
environment, the temporary directory as working directory and a wall-clock
timeout. No audio device, playback, network or external proprietary data is
used. Exit status 0 means every selected check passed; 1 means a failure; 2
means a usage problem. Optional checks that were not requested are printed as
SKIP, never silently omitted.

What is asserted (oracle = this file's own pure-Python Kaiser-windowed cardinal
sinc, per-phase unity-DC rows, ``math.fsum`` dot products; coordinate t-L+p/F
exactly as in the guide):

* empty file: success, zero counts, ``-inf`` dB, empty stderr;
* partial samples (1, 7, 9, 15 bytes), 16 MiB+1 and 16 MiB+8 bytes: exit 1,
  empty stdout, exact stderr;
* invalid factor/rate/half/beta parsing, out-of-range profile values and wrong
  argument counts: exit 1, empty stdout, exact stderr (Rust ``Debug`` text of
  the error, which depends on the toolchain's std formatting);
* NaN/Inf flag propagation and measurement-silence peaks, and a genuine
  finite-sample amplitude overflow whose flag is distinct from ``nonfinite``;
* a short finite fixture whose emphasised final frames put the maximum beyond
  the last source frame, compared with the independent convolution and shown
  to need the finish tail (a truncated-tail oracle is lower);
* optional ``--include-exact-16mib``: exactly 16 MiB (2,097,152 frames) of
  sparse payload at the cheapest admitted profile (half-width 32, 4 phases).
  Work: one history scan of 65 values per frame, plus 3 weighted 65-tap rows
  only for windows that contain a nonzero sample (zero windows skip them);
  a dense payload would cost about 195 products per frame. Debug builds can be
  much slower than release, hence the separate ``--exact-timeout`` (default 600
  s). It is off by default because of this cost and has not been run here.
"""
import argparse
import math
import os
import struct
import subprocess
import sys
import tempfile

MAX_BYTES = 16 * 1024 * 1024
FIELDS = [
    "rate", "factor", "half", "beta", "source_frames", "tail_frames",
    "phase_observations", "sample_peak", "interpolated_peak", "db_tp",
    "nonfinite", "overflow",
]
PARTIAL_OR_OVERSIZE = (
    'Error: "raw f64 input must contain complete samples and at most 16 MiB"'
)
USAGE = 'Error: "usage: true-peak-file RAW_F64LE RATE FACTOR HALF_WIDTH BETA"'
FACTOR_ERROR = 'Error: "factor must be 4/8/16"'


# ---------------------------------------------------------------- oracle ----
def bessel_i0(x):
    term = total = 1.0
    for k in range(1, 500):
        term *= (x * x) / (4.0 * k * k)
        following = total + term
        if following == total:
            return total
        total = following
    raise AssertionError("I0 series did not converge")


def design_rows(half, factor, beta):
    """Newest-to-oldest rows h_p[j] at d = j - L + p/F, unity DC per row."""
    taps = 2 * half + 1
    denominator = bessel_i0(beta)
    rows = []
    for phase in range(factor):
        if phase == 0:
            row = [1.0 if j == half else 0.0 for j in range(taps)]
        else:
            row = []
            for j in range(taps):
                d = j - half + phase / factor
                if abs(d) > half:
                    row.append(0.0)
                    continue
                ratio = d / half
                sinc = math.sin(math.pi * d) / (math.pi * d)
                row.append(
                    sinc * bessel_i0(beta * math.sqrt(1.0 - ratio * ratio))
                    / denominator
                )
        norm = math.fsum(row)
        rows.append([v / norm for v in row])
    for row in rows:
        assert abs(math.fsum(row) - 1.0) < 1e-14, "oracle row is not unity DC"
    return rows


def oracle_peak(rows, half, factor, samples, n, tail=None):
    """max |y| over steps t = 0 .. n+tail-1, phases 0..F-1 (tail defaults to 2L).

    ``samples`` maps source index -> finite nonzero value; omitted indices are
    zero (and so are substituted nonfinite samples). y(t,p) = sum_k x[k] row_p[t-k].
    """
    if n == 0:
        return 0.0
    tail = 2 * half if tail is None else tail
    taps = 2 * half + 1
    steps = set()
    for k in samples:
        steps.update(t for t in range(k, k + taps) if t < n + tail)
    peak = 0.0
    for t in sorted(steps):
        for phase in range(factor):
            row = rows[phase]
            y = math.fsum(
                v * row[t - k] for k, v in samples.items() if 0 <= t - k < taps
            )
            peak = max(peak, abs(y))
    return peak


def expected_db(peak):
    if peak == 0.0:
        return -math.inf
    if peak == math.inf:
        return math.inf
    return 20.0 * math.log10(peak)


# ------------------------------------------------------------ subprocess ----
def run(binary, args, cwd, timeout):
    try:
        done = subprocess.run(
            [binary, *args],
            stdin=subprocess.DEVNULL,
            capture_output=True,
            cwd=cwd,
            env={"LC_ALL": "C"},
            timeout=timeout,
            check=False,
        )
    except subprocess.TimeoutExpired:
        raise AssertionError(f"timed out after {timeout} s (child killed)")
    return done.returncode, done.stdout.decode("utf-8"), done.stderr.decode("utf-8")


def parse_report(stdout):
    lines = stdout.split("\n")
    assert len(lines) == 2 and lines[1] == "", f"expected one stdout line: {stdout!r}"
    pairs = [part.split("=", 1) for part in lines[0].split(" ")]
    assert [p[0] for p in pairs] == FIELDS, f"unexpected fields: {lines[0]!r}"
    raw = dict(pairs)
    return {
        "rate": int(raw["rate"]),
        "factor": int(raw["factor"]),
        "half": int(raw["half"]),
        "beta": float(raw["beta"]),
        "source_frames": int(raw["source_frames"]),
        "tail_frames": int(raw["tail_frames"]),
        "phase_observations": int(raw["phase_observations"]),
        "sample_peak": float(raw["sample_peak"]),
        "interpolated_peak": float(raw["interpolated_peak"]),
        "db_tp": float(raw["db_tp"]),
        "nonfinite": {"true": True, "false": False}[raw["nonfinite"]],
        "overflow": {"true": True, "false": False}[raw["overflow"]],
    }


def same(actual, expected, rel_tol=1e-12, abs_tol=1e-15):
    if math.isinf(expected) or math.isinf(actual):
        return actual == expected
    return math.isclose(actual, expected, rel_tol=rel_tol, abs_tol=abs_tol)


def write_samples(path, values):
    with open(path, "wb") as handle:
        handle.write(b"".join(struct.pack("<d", v) for v in values))


def expect_report(binary, tmp, timeout, path, profile, n, samples,
                  nonfinite=False, overflow=False):
    rate, factor, half, beta_text = profile
    code, out, err = run(binary, [path, str(rate), str(factor), str(half), beta_text],
                         tmp, timeout)
    assert code == 0, f"exit {code}, stderr {err!r}"
    assert err == "", f"unexpected stderr {err!r}"
    report = parse_report(out)
    beta = float(beta_text)
    tail = 2 * half if n else 0
    assert (report["rate"], report["factor"], report["half"]) == (rate, factor, half)
    assert report["beta"] == beta
    assert report["source_frames"] == n, report
    assert report["tail_frames"] == tail, report
    assert report["phase_observations"] == (n + tail) * factor, report
    sample_peak = max((abs(v) for v in samples.values()), default=0.0)
    assert report["sample_peak"] == sample_peak, (report["sample_peak"], sample_peak)
    assert report["nonfinite"] is nonfinite and report["overflow"] is overflow, report
    if overflow:
        peak = math.inf
    else:
        peak = oracle_peak(design_rows(half, factor, beta), half, factor, samples, n)
    assert same(report["interpolated_peak"], peak), (report["interpolated_peak"], peak)
    assert same(report["db_tp"], expected_db(peak), 1e-12, 1e-10), (report["db_tp"], peak)
    return report, peak


def expect_failure(binary, tmp, timeout, args, stderr):
    code, out, err = run(binary, args, tmp, timeout)
    assert code == 1, f"exit {code}"
    assert out == "", f"stdout must be empty: {out!r}"
    assert err == stderr + "\n", f"stderr {err!r} != {stderr + chr(10)!r}"


# ------------------------------------------------------------------ main ----
def main():
    parser = argparse.ArgumentParser(
        description="Admission checks for a prebuilt true-peak-file example binary.")
    parser.add_argument("--binary", required=True,
                        help="path of the prebuilt true-peak-file example binary")
    parser.add_argument("--timeout", type=float, default=120.0,
                        help="seconds allowed per ordinary subprocess (default 120)")
    parser.add_argument("--include-exact-16mib", action="store_true",
                        help="also run the costly exact 16 MiB success check")
    parser.add_argument("--exact-timeout", type=float, default=600.0,
                        help="seconds allowed for the exact 16 MiB run (default 600)")
    options = parser.parse_args()
    binary = os.path.abspath(options.binary)
    if not (os.path.isfile(binary) and os.access(binary, os.X_OK)):
        print(f"--binary {binary} is not an executable file", file=sys.stderr)
        return 2
    results = []

    with tempfile.TemporaryDirectory(prefix="true-peak-admission-") as tmp:
        def fixture(name, values=None, size=None):
            path = os.path.join(tmp, name)
            if values is not None:
                write_samples(path, values)
            else:
                with open(path, "wb") as handle:
                    handle.truncate(size)
            return path

        def case(name, body):
            try:
                body()
            except (AssertionError, KeyError, ValueError, OSError) as error:
                results.append((name, False, str(error)))
                print(f"FAIL {name}: {error}")
            else:
                results.append((name, True, ""))
                print(f"PASS {name}")

        t = options.timeout
        base = ["48000", "8", "32", "12"]
        profile = (48000, 8, 32, "12")
        empty = fixture("empty.f64")

        case("empty file: success with zero counts and -inf dB",
             lambda: expect_report(binary, tmp, t, empty, profile, 0, {}))

        for size in (1, 7, 9, 15):
            path = fixture(f"partial-{size}.f64", size=size)
            case(f"partial sample ({size} bytes) refused",
                 lambda p=path: expect_failure(binary, tmp, t, [p, *base],
                                               PARTIAL_OR_OVERSIZE))
        for size in (MAX_BYTES + 1, MAX_BYTES + 8):
            path = fixture(f"oversized-{size}.f64", size=size)
            case(f"oversized ({size} bytes, limit 16 MiB) refused",
                 lambda p=path: expect_failure(binary, tmp, t, [p, *base],
                                               PARTIAL_OR_OVERSIZE))
            os.remove(path)

        for label, args, stderr in [
            ("rate not a number", ["abc", "8", "32", "12"],
             "Error: ParseIntError { kind: InvalidDigit }"),
            ("rate negative", ["-1", "8", "32", "12"],
             "Error: ParseIntError { kind: InvalidDigit }"),
            ("rate empty", ["", "8", "32", "12"],
             "Error: ParseIntError { kind: Empty }"),
            ("rate overflows u32", ["4294967296", "8", "32", "12"],
             "Error: ParseIntError { kind: PosOverflow }"),
            ("rate below 8000", ["7999", "8", "32", "12"], "Error: SampleRate"),
            ("rate above 768000", ["768001", "8", "32", "12"], "Error: SampleRate"),
            ("factor 5", ["48000", "5", "32", "12"], FACTOR_ERROR),
            ("factor empty", ["48000", "", "32", "12"], FACTOR_ERROR),
            ("factor text", ["48000", "abc", "32", "12"], FACTOR_ERROR),
            ("factor 08", ["48000", "08", "32", "12"], FACTOR_ERROR),
            ("half not a number", ["48000", "8", "x", "12"],
             "Error: ParseIntError { kind: InvalidDigit }"),
            ("half negative", ["48000", "8", "-5", "12"],
             "Error: ParseIntError { kind: InvalidDigit }"),
            ("half 31", ["48000", "8", "31", "12"], "Error: HalfWidth"),
            ("half 513", ["48000", "8", "513", "12"], "Error: HalfWidth"),
            ("beta not a number", ["48000", "8", "32", "x"],
             "Error: ParseFloatError { kind: Invalid }"),
            ("beta empty", ["48000", "8", "32", ""],
             "Error: ParseFloatError { kind: Empty }"),
            ("beta above 16", ["48000", "8", "32", "16.000000001"], "Error: Beta"),
            ("beta negative", ["48000", "8", "32", "-0.5"], "Error: Beta"),
            ("beta nan", ["48000", "8", "32", "nan"], "Error: Beta"),
            ("beta inf", ["48000", "8", "32", "inf"], "Error: Beta"),
        ]:
            case(f"invalid profile: {label}",
                 lambda a=args, s=stderr: expect_failure(binary, tmp, t, [empty, *a], s))
        case("usage: too few arguments",
             lambda: expect_failure(binary, tmp, t, [empty, *base[:3]], USAGE))
        case("usage: too many arguments",
             lambda: expect_failure(binary, tmp, t, [empty, *base, "extra"], USAGE))

        def missing_file():
            # OS-specific message: only the exit status, empty stdout and the
            # "Error: " prefix are asserted here.
            code, out, err = run(binary, [os.path.join(tmp, "absent.f64"), *base], tmp, t)
            assert code == 1 and out == "" and err.startswith("Error: "), (code, out, err)
        case("missing input file refused (relaxed stderr)", missing_file)

        nan = float("nan")
        mixed = fixture("nonfinite-mixed.f64", [0.0, nan, math.inf, -math.inf, 0.1])
        case("NaN/+Inf/-Inf latch nonfinite; silence peak equals finite-only convolution",
             lambda: expect_report(binary, tmp, t, mixed, profile, 5, {4: 0.1},
                                   nonfinite=True))
        only_nan = fixture("nonfinite-only.f64", [nan, nan, nan])
        case("only NaN: zero peaks, -inf dB, nonfinite flag",
             lambda: expect_report(binary, tmp, t, only_nan, profile, 3, {},
                                   nonfinite=True))

        def genuine_overflow():
            rows = design_rows(32, 8, 12.0)
            assert math.fsum(abs(v) for v in rows[4]) > 1.0
            pattern = [sys.float_info.max if v >= 0 else -sys.float_info.max
                       for v in reversed(rows[4])]
            path = fixture("overflow.f64", pattern)
            samples = {k: v for k, v in enumerate(pattern)}
            expect_report(binary, tmp, t, path, profile, len(pattern), samples,
                          overflow=True)
        case("finite +-f64::MAX pattern overflows: overflow flag, not nonfinite",
             genuine_overflow)

        # Short finite fixture: 24 alternating-sign frames, magnitude ramping up
        # over the first 12 and flat at 0.9 afterwards, ending +0.9 on the last
        # frame. A truncated alternating run overshoots just outside its ends; the
        # nearly silent start makes the end overshoot (beyond the last frame, so
        # it needs the finish tail) the global maximum.
        short = [0.9 * min(1.0, (k + 1) / 12) * (1.0 if (23 - k) % 2 == 0 else -1.0)
                 for k in range(24)]
        short_path = fixture("short-trailing.f64", short)
        for rate, factor, half, beta_text in [(48000, 4, 32, "12"), (44100, 16, 48, "6.5")]:
            def trailing(rate=rate, factor=factor, half=half, beta_text=beta_text):
                samples = {k: v for k, v in enumerate(short)}
                report, peak = expect_report(
                    binary, tmp, t, short_path, (rate, factor, half, beta_text),
                    len(short), samples)
                rows = design_rows(half, factor, float(beta_text))
                shortened = oracle_peak(rows, half, factor, samples, len(short),
                                        tail=half - 1)
                assert report["interpolated_peak"] > report["sample_peak"] + 0.05, report
                assert shortened + 0.05 < peak, (
                    f"fixture must need the finish tail: {shortened} vs {peak}")
            case(f"finite short trailing fixture vs independent convolution "
                 f"(factor {factor}, half {half}, beta {beta_text})", trailing)

        if options.include_exact_16mib:
            def exact():
                n = MAX_BYTES // 8
                samples = {0: 0.5, 1: -0.25, n - 3: 0.75, n - 2: -0.75, n - 1: 0.9}
                path = os.path.join(tmp, "exact-16mib.f64")
                with open(path, "wb") as handle:
                    handle.truncate(MAX_BYTES)
                    for k, v in samples.items():
                        handle.seek(8 * k)
                        handle.write(struct.pack("<d", v))
                assert os.path.getsize(path) == MAX_BYTES
                expect_report(binary, tmp, options.exact_timeout, path,
                              (48000, 4, 32, "12"), n, samples)
            case("exactly 16 MiB sparse payload accepted at half 32, factor 4", exact)
        else:
            print("SKIP exactly-16-MiB success: not requested "
                  "(costly; use --include-exact-16mib, see the module docstring)")

    failed = [name for name, ok, _ in results if not ok]
    print(f"{len(results) - len(failed)}/{len(results)} selected checks passed"
          + ("" if options.include_exact_16mib else "; exact-16-MiB check NOT RUN"))
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
