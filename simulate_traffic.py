#!/usr/bin/env python3
import socket
import time
import argparse
import random
import string
import sys

def random_payload(size: int) -> bytes:
    """Generate a random ASCII payload of given size."""
    return ''.join(random.choices(string.ascii_letters + string.digits, k=size)).encode()

def run_simulation(target: str, port: int, pps: int, duration: float, pkt_size: int):
    """
    Send UDP packets to target:port at approximately pps packets/sec
    for duration seconds, with each packet of pkt_size bytes.
    """
    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    payload = random_payload(pkt_size)

    # We'll send in bursts every INTERVAL seconds
    INTERVAL = 0.01  # 10 ms
    burst_size = int(pps * INTERVAL)  # e.g., 600 packets per 10 ms
    if burst_size < 1:
        print("Error: INTERVAL too large for desired pps")
        return

    end_time = time.time() + duration
    total_sent = 0
    interval_count = 0

    print(f"Starting: {pps} pps → {burst_size} pkts/{INTERVAL*1000:.0f}ms bursts for {duration}s")
    next_log = time.time() + 1

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
        interval_count += 1

        # Sleep remainder of INTERVAL
        delta = time.perf_counter() - t0
        to_sleep = INTERVAL - delta
        if to_sleep > 0:
            time.sleep(to_sleep)

        # Log once per second
        now = time.time()
        if now >= next_log:
            elapsed = duration - (end_time - now)
            achieved_pps = total_sent / elapsed if elapsed > 0 else 0
            print(f"[{elapsed:.0f}s] Total sent: {total_sent}, Rate: {achieved_pps:.0f} pps")
            next_log += 1

    print(f"Done. Total packets sent: {total_sent} (~{total_sent/duration:.0f} pps)")

if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Simulate high-rate UDP traffic")
    parser.add_argument("--target", required=True, help="Destination IP or hostname")
    parser.add_argument("--port", type=int, required=True, help="Destination UDP port")
    parser.add_argument("--pps", type=int, default=60000, help="Packets per second (default 60000)")
    parser.add_argument("--duration", type=float, default=600, help="Duration in seconds (default 600)")
    parser.add_argument("--size", type=int, default=64, help="Packet payload size in bytes (default 64)")
    args = parser.parse_args()

    try:
        run_simulation(args.target, args.port, args.pps, args.duration, args.size)
    except KeyboardInterrupt:
        print("\nInterrupted by user")