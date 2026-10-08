#!/usr/bin/env python3
"""Compare deterministic guest execution across engines or saved/current binaries."""
import argparse
import hashlib
import json
import os
import platform
from pathlib import Path
import re
import statistics
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--image", type=Path, default=None)
    parser.add_argument("--guest", choices=["smalltalk", "squeak"], default="smalltalk")
    parser.add_argument("--baseline-binary", type=Path,
                        help="compare this saved JIT binary against the current build")
    parser.add_argument("--steps", type=int, default=50_000_000)
    parser.add_argument("--runs", type=int, default=5)
    parser.add_argument("--memory-words", type=int, default=16_777_216)
    parser.add_argument("--output", type=Path, default=Path("artifacts/jit-benchmark"))
    args = parser.parse_args()
    if args.runs < 1 or args.steps < 1:
        parser.error("--runs and --steps must be positive")
    root = Path(__file__).resolve().parent.parent
    image = (args.image or Path("artifacts/st80/VirtualImage" if args.guest == "smalltalk"
                                     else "artifacts/squeak-1.1/Squeak1.1.image")).resolve()
    baseline = args.baseline_binary.resolve() if args.baseline_binary else None
    if baseline is not None and not baseline.is_file():
        parser.error(f"missing baseline binary: {baseline}")
    output = args.output.resolve()
    if not image.is_file():
        parser.error(f"missing image: {image}; run scripts/fetch-smalltalk-image.py")
    output.mkdir(parents=True, exist_ok=True)
    subprocess.run(
        ["cargo", "build", "--release", "--locked", "-p", "rekursiv-emulator"],
        cwd=root, check=True,
    )
    # Honor Cargo's target directory, including configuration-file overrides.
    metadata = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--format-version", "1", "--no-deps", "--locked"], cwd=root,
    ))
    binary = Path(metadata["target_directory"]) / "release" / "rekursiv-emulator"
    names = ["baseline", "current"] if baseline else ["interpreter", "jit"]
    rows = []
    reference = None
    for run in range(args.runs):
        # Alternate order to reduce systematic warm-cache and temperature bias.
        engines = names if run % 2 == 0 else names[::-1]
        for engine in engines:
            frame = output / f"{run}-{engine}.ppm"
            result = subprocess.run([
                str(baseline if engine == "baseline" else binary), "--engine",
                "jit" if baseline else engine, "--headless", f"--{args.guest}", str(image),
                "--memory-words", str(args.memory_words), "--steps", str(args.steps),
                "--frame", str(frame),
            ], cwd=root, text=True, capture_output=True)
            log = result.stdout + result.stderr
            (output / f"{run}-{engine}.log").write_text(log)
            if result.returncode:
                raise RuntimeError(log)
            state = re.search(r"^PC .*", log, re.MULTILINE).group(0)
            frame_hash = hashlib.sha256(frame.read_bytes()).hexdigest()
            if reference is None:
                reference = (state, frame_hash)
            if (state, frame_hash) != reference:
                raise RuntimeError(f"nonmatching final counters or frame: {run}-{engine}")
            rate = float(re.search(r"; ([0-9.]+) microinstructions/s active", log).group(1))
            compile_time = re.search(r"compiled in ([0-9.]+) s", log)
            row = {"run": run, "engine": engine, "microinstructions_per_second": rate,
                   "compile_seconds": float(compile_time.group(1)) if compile_time else 0.0}
            rows.append(row)
            print(f"{engine:11s} {rate / 1e6:.3f} M instructions/s", flush=True)
    medians = {engine: statistics.median(
        row["microinstructions_per_second"] for row in rows if row["engine"] == engine
    ) for engine in names}
    summary = {"steps": args.steps, "memory_words": args.memory_words,
               "image_sha256": hashlib.sha256(image.read_bytes()).hexdigest(),
               "final_state": reference[0], "frame_sha256": reference[1], "runs": rows,
               "median_rates": medians, "speedup": medians[names[1]] / medians[names[0]],
               "guest": args.guest, "host": platform.platform(),
               "affinity": sorted(os.sched_getaffinity(0)) if hasattr(os, "sched_getaffinity") else None,
               "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
               "baseline_sha256": hashlib.sha256(baseline.read_bytes()).hexdigest() if baseline else None}
    (output / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    print(f"Median speedup: {summary['speedup']:.3f}x; results in {output}")


if __name__ == "__main__":
    main()
