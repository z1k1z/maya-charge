#!/usr/bin/env python3
"""Measure CLI CPU cost; run on the host with hardware to include real USB I/O.

Hardware mode polls at 5 s (no rapid-fire traffic). Error-path microbenchmarks
may use --interval 0 but do not represent working-device CPU or wall time.
ru_maxrss is deliberately omitted: fork/exec can include the parent's RSS.
"""
import argparse
import json
from pathlib import Path
import resource
import subprocess
import time

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('binary', nargs='?', default='./target/release/mouse-charge')
parser.add_argument('--samples', type=int, default=12)
parser.add_argument('--interval', type=float, default=5)
args = parser.parse_args()
if args.samples < 1 or args.interval < 0:
    parser.error('samples >= 1, interval >= 0')
cpu = 0.0
wall = 0.0
statuses = {}
binary = str(Path(args.binary).resolve())
for i in range(args.samples):
    before = resource.getrusage(resource.RUSAGE_CHILDREN)
    start = time.perf_counter()
    process = subprocess.run([binary, '--json'], capture_output=True, text=True, timeout=10)
    elapsed = time.perf_counter() - start
    after = resource.getrusage(resource.RUSAGE_CHILDREN)
    cpu += (after.ru_utime + after.ru_stime) - (before.ru_utime + before.ru_stime)
    wall += elapsed
    status = json.loads(process.stdout)['status']
    statuses[status] = statuses.get(status, 0) + 1
    if i + 1 < args.samples:
        time.sleep(max(0, args.interval - elapsed))
print(json.dumps({
    'samples': args.samples, 'statuses': statuses,
    'mean_cpu_ms': round(cpu * 1000 / args.samples, 4),
    'mean_wall_ms': round(wall * 1000 / args.samples, 4),
    'projected_one_core_percent_at_5s': round(cpu / args.samples / 5 * 100, 5),
    'scope': 'CLI only; excludes Noctalia rendering. Errors do not measure successful USB exchange.',
}, ensure_ascii=False, indent=2))
