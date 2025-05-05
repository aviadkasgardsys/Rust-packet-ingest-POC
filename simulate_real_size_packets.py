#!/usr/bin/env python3
import socket
import time
import argparse
import sys

# Simulate high-rate UDP traffic using a single pre-allocated, hardcoded payload
# By default, sends 60,000 packets/sec for the specified duration.

# Pre-generate a fixed payload of specified size (no per-packet allocation)
DEFAULT_PAYLOAD_SIZE = 512  # bytes


def run_simulation(target: str, port: int, pps: int, duration: float, payload_size: int):
    """
    Send UDP packets to target:port at approximately pps packets/sec
    for duration seconds, using a fixed payload of payload_size bytes.
    """
    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    # Increase send buffer to handle bursts
    sock.setsockopt(socket.SOL_SOCKET, socket.SO_SNDBUF, 4 * 1024 * 1024)

    # Prepare fixed payload once
    payload = b'A' * payload_size

    # Send in bursts every INTERVAL seconds
    INTERVAL = 0.01  # 10 ms
    burst_size = max(1, int(pps * INTERVAL))

    end_time = time.time() + duration
    total_sent = 0
    next_log = time.time() + 1.0

    print(f"Starting: {pps} pps → {burst_size} pkts/{INTERVAL*1000:.0f}ms bursts for {duration}s")

    while time.time() < end_time:
        t0 = time.perf_counter()
        sent = 0
        for _ in range(burst_size):
            try:
                sock.sendto(payload, (target, port))
                sent += 1
            except Exception as e:
                print(f"Send error: {e}", file=sys.stderr)
                break
        total_sent += sent

        # Sleep remainder of interval
        delta = time.perf_counter() - t0
        to_sleep = INTERVAL - delta
        if to_sleep > 0:
            time.sleep(to_sleep)

        # Log once per second
        now = time.time()
        if now >= next_log:
            elapsed = duration - (end_time - now)
            rate = total_sent / elapsed if elapsed > 0 else 0
            print(f"[{elapsed:.0f}s] Sent {total_sent} packets (~{rate:.0f} pps)")
            next_log += 1.0

    print(f"Done. Total packets sent: {total_sent} (~{total_sent/duration:.0f} pps)")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(
        description="Simulate high-rate UDP traffic with fixed payload"
    )
    parser.add_argument("--target", required=True, help="Destination IP or hostname")
    parser.add_argument("--port", type=int, required=True, help="Destination UDP port")
    parser.add_argument("--pps", type=int, default=60000, help="Packets per second (default 60000)")
    parser.add_argument("--duration", type=float, default=600, help="Duration in seconds (default 600)")
    parser.add_argument(
        "--size", type=int, default=DEFAULT_PAYLOAD_SIZE,
        help=f"Payload size in bytes (default {DEFAULT_PAYLOAD_SIZE})"
    )
    args = parser.parse_args()

    try:
        run_simulation(args.target, args.port, args.pps, args.duration, args.size)
    except KeyboardInterrupt:
        print("\nInterrupted by user")
