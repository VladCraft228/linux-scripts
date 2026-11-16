#!/usr/bin/env python3
"""Linux process benchmark; no third-party Python packages. CPU=100% means one core."""
import argparse
import csv
import ctypes as C
import ctypes.util
import json
import math
import os
from pathlib import Path
import statistics
import time


def snapshot(pid):
    base = Path('/proc') / str(pid)
    raw = (base / 'stat').read_text()
    fields = raw[raw.rindex(')') + 2:].split()
    if fields[0] == 'Z':
        raise RuntimeError(f'PID {pid} exited (zombie)')
    mem = {}
    for line in (base / 'smaps_rollup').read_text().splitlines():
        if ':' in line:
            key, value = line.split(':', 1)
            if value.strip().endswith('kB'):
                mem[key] = int(value.split()[0]) / 1024
    status = {}
    for line in (base / 'status').read_text().splitlines():
        if ':' in line:
            key, value = line.split(':', 1)
            parts = value.strip().split()
            if parts:
                status[key] = parts[0]
    return dict(identity=fields[19], ticks=int(fields[11]) + int(fields[12]),
                rss_mib=mem.get('Rss', 0), pss_mib=mem.get('Pss', 0),
                uss_mib=mem.get('Private_Clean', 0) + mem.get('Private_Dirty', 0),
                swap_pss_mib=mem.get('SwapPss', 0), threads=int(fields[17]),
                voluntary=int(status.get('voluntary_ctxt_switches', 0)),
                involuntary=int(status.get('nonvoluntary_ctxt_switches', 0)))


class Pointer:
    """XTest workload only: move pointer, never click or launch applications."""
    def __init__(self):
        self.x = C.CDLL(ctypes.util.find_library('X11') or 'libX11.so.6')
        self.xt = C.CDLL(ctypes.util.find_library('Xtst') or 'libXtst.so.6')
        self.x.XOpenDisplay.argtypes = [C.c_char_p]
        self.x.XOpenDisplay.restype = C.c_void_p
        self.x.XDefaultRootWindow.argtypes = [C.c_void_p]
        self.x.XDefaultRootWindow.restype = C.c_ulong
        self.x.XDefaultScreen.argtypes = [C.c_void_p]
        self.x.XDefaultScreen.restype = C.c_int
        self.x.XFlush.argtypes = [C.c_void_p]
        self.x.XCloseDisplay.argtypes = [C.c_void_p]
        self.x.XQueryPointer.argtypes = [C.c_void_p, C.c_ulong, C.POINTER(C.c_ulong),
            C.POINTER(C.c_ulong), C.POINTER(C.c_int), C.POINTER(C.c_int),
            C.POINTER(C.c_int), C.POINTER(C.c_int), C.POINTER(C.c_uint)]
        self.xt.XTestFakeMotionEvent.argtypes = [C.c_void_p, C.c_int, C.c_int, C.c_int, C.c_ulong]
        self.xt.XTestFakeMotionEvent.restype = C.c_int
        self.xt.XTestQueryExtension.argtypes = [C.c_void_p] + [C.POINTER(C.c_int)] * 4
        self.xt.XTestQueryExtension.restype = C.c_int
        self.d = self.x.XOpenDisplay(None)
        if not self.d:
            raise RuntimeError('Cannot open X11 display')
        values = [C.c_int() for _ in range(4)]
        if not self.xt.XTestQueryExtension(self.d, *(C.byref(v) for v in values)):
            self.close()
            raise RuntimeError('XTest is not available on this X11 server')
        self.screen = self.x.XDefaultScreen(self.d)

    def position(self):
        root, child = C.c_ulong(), C.c_ulong()
        rx, ry, wx, wy, mask = C.c_int(), C.c_int(), C.c_int(), C.c_int(), C.c_uint()
        ok = self.x.XQueryPointer(self.d, self.x.XDefaultRootWindow(self.d),
            C.byref(root), C.byref(child), C.byref(rx), C.byref(ry),
            C.byref(wx), C.byref(wy), C.byref(mask))
        if not ok:
            raise RuntimeError('Pointer is on another X11 screen')
        return rx.value, ry.value

    def move(self, x, y):
        if not self.xt.XTestFakeMotionEvent(self.d, self.screen, int(x), int(y), 0):
            raise RuntimeError('XTest motion failed')
        self.x.XFlush(self.d)

    def close(self):
        if self.d:
            self.x.XCloseDisplay(self.d)
            self.d = None


def percentile(values, p):
    ordered = sorted(values)
    return ordered[max(0, math.ceil(len(ordered) * p) - 1)]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--pid', type=int, required=True)
    parser.add_argument('--extra-pid', type=int, action='append', default=[],
                        help='Optional compositor/Xorg PID; totals then include those processes')
    parser.add_argument('--label', required=True, help='e.g. rust-idle, qt-sweep; output file prefix')
    parser.add_argument('--seconds', type=float, default=30)
    parser.add_argument('--warmup', type=float, default=5)
    parser.add_argument('--sweep', action='store_true', help='Sweep around pointer position after warmup')
    parser.add_argument('--width', type=float, default=232, help='Full sweep width in pixels')
    parser.add_argument('--output', type=Path, default=Path('results'))
    parser.add_argument('--compare', type=Path, help='Compare with a previous summary JSON')
    args = parser.parse_args()
    if not (1 <= args.seconds <= 3600 and 0 <= args.warmup <= 300 and 1 <= args.width <= 2000):
        parser.error('seconds: 1..3600; warmup: 0..300; width: 1..2000')
    if not args.label or any(c not in 'abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_' for c in args.label):
        parser.error('label must contain only letters, numbers, hyphen, underscore')
    pids = list(dict.fromkeys([args.pid] + args.extra_pid))
    if any(pid <= 0 for pid in pids):
        parser.error('PIDs must be positive')
    initial = [snapshot(pid) for pid in pids]  # Fail before warmup if permissions are insufficient.
    pointer = Pointer() if args.sweep else None
    original = None
    try:
        print(f'Warmup {args.warmup:g}s. ' + ('Place pointer over the CENTER icon; then do not move it.' if pointer else 'Leave the pointer outside the dock.'))
        time.sleep(args.warmup)
        if pointer:
            original = pointer.position()
        previous = [snapshot(pid) for pid in pids]
        for old, new in zip(initial, previous):
            if old['identity'] != new['identity']:
                raise RuntimeError('PID was reused during warmup')
        first = previous
        start = last = time.monotonic()
        next_sample = start + 0.25
        rows = []
        hz = os.sysconf('SC_CLK_TCK')
        while True:
            now = time.monotonic()
            elapsed = now - start
            if pointer:
                # Two seconds per full left-right-left cycle, 60 requested movements/s.
                pointer.move(original[0] + args.width / 2 * math.sin(math.pi * elapsed), original[1])
            if now >= next_sample or elapsed >= args.seconds:
                current = [snapshot(pid) for pid in pids]
                for old, new in zip(previous, current):
                    if old['identity'] != new['identity']:
                        raise RuntimeError('PID was reused; refusing to combine different processes')
                dt = now - last
                cpu = sum(n['ticks'] - o['ticks'] for o, n in zip(previous, current)) / hz / dt * 100
                row = dict(seconds=elapsed, cpu_core_percent=cpu)
                for key in ('pss_mib', 'rss_mib', 'uss_mib', 'swap_pss_mib', 'threads'):
                    row[key] = sum(item[key] for item in current)
                rows.append(row)
                previous = current
                last = now
                next_sample = now + 0.25
            if elapsed >= args.seconds:
                break
            time.sleep(min(1 / 60 if pointer else 0.05, args.seconds - elapsed))
        cpu_seconds = sum(n['ticks'] - o['ticks'] for o, n in zip(first, previous)) / hz
        summary = dict(label=args.label, pids=pids, mode='sweep' if pointer else 'idle',
            seconds=rows[-1]['seconds'], samples=len(rows), sweep_width_px=args.width if pointer else None,
            cpu_seconds=cpu_seconds, cpu_core_percent=100 * cpu_seconds / rows[-1]['seconds'],
            cpu_sample_p95_percent=percentile([r['cpu_core_percent'] for r in rows], .95),
            pss_mean_mib=statistics.mean(r['pss_mib'] for r in rows),
            rss_mean_mib=statistics.mean(r['rss_mib'] for r in rows),
            rss_sample_max_mib=max(r['rss_mib'] for r in rows),
            uss_mean_mib=statistics.mean(r['uss_mib'] for r in rows),
            swap_pss_mean_mib=statistics.mean(r['swap_pss_mib'] for r in rows),
            main_thread_context_switches=sum(n['voluntary'] + n['involuntary'] - o['voluntary'] - o['involuntary'] for o,n in zip(first,previous)),
            kernel=os.uname().release)
        args.output.mkdir(parents=True, exist_ok=True)
        prefix = args.output / args.label
        with prefix.with_suffix('.csv').open('w', newline='') as f:
            writer = csv.DictWriter(f, fieldnames=list(rows[0]))
            writer.writeheader()
            writer.writerows(rows)
        prefix.with_suffix('.json').write_text(json.dumps(summary, indent=2) + '\n')
        print(f"{args.label}: CPU {summary['cpu_core_percent']:.2f}% of one core; PSS {summary['pss_mean_mib']:.2f} MiB; RSS {summary['rss_mean_mib']:.2f} MiB")
        print(f'Saved {prefix}.json and {prefix}.csv')
        if args.compare:
            other = json.loads(args.compare.read_text())
            if other['mode'] != summary['mode']:
                print('WARNING: different workloads; values are not directly comparable.')
            for key in ('cpu_core_percent', 'pss_mean_mib', 'rss_mean_mib'):
                print(f"{key}: {other['label']}={other[key]:.2f}; {args.label}={summary[key]:.2f}; delta={summary[key]-other[key]:+.2f}")
            print('No automatic winner: compare functionality, visual smoothness, and compositor/GPU cost too.')
    finally:
        if pointer:
            try:
                if original:
                    pointer.move(*original)
            finally:
                pointer.close()


if __name__ == '__main__':
    try:
        main()
    except (OSError, ValueError, RuntimeError, KeyboardInterrupt) as error:
        raise SystemExit(f'Benchmark stopped: {error}')
