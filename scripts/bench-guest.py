#!/usr/bin/env python3
"""Compare guest throughput across microcode revisions at a fixed dispatch count.

Both binaries must support --bytecodes. Compilation and image loading are outside
the reported active rate. Unlike bench-emulator.py, micro-PCs and microinstruction
counts may differ. Device clocks still advance with microinstructions, so guest
counts alone are not proof of identical work; keep semantic regressions and traces.
"""
import argparse
import hashlib
import json
from pathlib import Path
import platform
import re
import statistics
import subprocess


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--guest", choices=["smalltalk", "squeak"], required=True)
    parser.add_argument("--image", type=Path)
    parser.add_argument("--baseline-binary", type=Path, required=True)
    parser.add_argument("--current-binary", type=Path, default=Path("target/release/rekursiv-emulator"))
    parser.add_argument("--bytecodes", type=int, default=2_000_000)
    parser.add_argument("--steps", type=int, default=2_000_000_000, help="safety cap per run")
    parser.add_argument("--runs", type=int, default=3)
    parser.add_argument("--cpu", type=int, help="pin each run with taskset")
    parser.add_argument("--estimate-cycles", choices=["sram", "dram"],
                        help="measure hardware work separately; this slows native execution")
    parser.add_argument("--output", type=Path, default=Path("artifacts/guest-benchmark"))
    args = parser.parse_args()
    if min(args.bytecodes, args.steps, args.runs) <= 0:
        parser.error("counts must be positive")
    image = args.image or Path({
        "smalltalk": "artifacts/st80/VirtualImage",
        "squeak": "artifacts/squeak-1.1/Squeak1.1.image",
    }[args.guest])
    binaries = {"baseline": args.baseline_binary.resolve(), "current": args.current_binary.resolve()}
    args.output.mkdir(parents=True, exist_ok=True)
    results = []
    for pair in range(args.runs):
        for name in (["baseline", "current"] if pair % 2 == 0 else ["current", "baseline"]):
            stem = args.output / f"{args.guest}-{pair + 1}-{name}"
            command = [str(binaries[name]), "--headless", f"--{args.guest}", str(image),
                       "--memory-words", "16777216", "--bytecodes", str(args.bytecodes),
                       "--steps", str(args.steps), "--frame", str(stem.with_suffix(".ppm"))]
            if args.cpu is not None:
                command = ["taskset", "-c", str(args.cpu), *command]
            if args.estimate_cycles:
                command += ["--estimate-cycles", args.estimate_cycles]
            run = subprocess.run(command, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
            log = run.stdout
            stem.with_suffix(".log").write_text(log)
            run.check_returncode()
            guest = re.search(r"Guest VM: (\d+) bytecodes dispatched; ([0-9.]+) bytecodes/s active", log)
            work = re.search(r"(\d+) mutator instructions, (\d+) collector instructions", log)
            if not guest or not work or int(guest[1]) != args.bytecodes or "halted=false, service=false" not in log:
                raise RuntimeError(f"guest did not reach the requested running checkpoint: {stem}")
            result = {"pair": pair + 1, "binary": name, "bytecodes": int(guest[1]),
                      "bytecodes_per_second": float(guest[2]),
                      "mutator_instructions": int(work[1]), "collector_instructions": int(work[2]),
                      "frame_sha256": digest(stem.with_suffix(".ppm"))}
            results.append(result)
            if args.estimate_cycles:
                cycles = int(re.search(r"elapsed: (\d+) cycles;", log)[1])
                result["estimated_cycles"] = cycles
                result["cycles_per_bytecode"] = cycles / args.bytecodes
            print(json.dumps(result), flush=True)
    medians = {name: statistics.median(r["bytecodes_per_second"] for r in results if r["binary"] == name)
               for name in binaries}
    summary = {"guest": args.guest, "platform": platform.platform(), "cpu": args.cpu,
               "image_sha256": digest(image), "binary_sha256": {n: digest(p) for n, p in binaries.items()},
               "results": results, "median_bytecodes_per_second": medians,
               "speedup": medians["current"] / medians["baseline"],
               "frames_match": len({r["frame_sha256"] for r in results}) == 1}
    if args.estimate_cycles:
        cycles = {name: statistics.median(r["estimated_cycles"] for r in results if r["binary"] == name)
                  for name in binaries}
        summary["cycle_profile"] = args.estimate_cycles
        summary["estimated_cycles"] = cycles
        summary["estimated_hardware_speedup"] = cycles["baseline"] / cycles["current"]
    (args.output / f"{args.guest}-summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    print(json.dumps({k: v for k, v in summary.items() if k != "results"}, indent=2))


if __name__ == "__main__":
    main()
