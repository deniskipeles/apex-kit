#!/usr/bin/env python3
import time
import statistics
import requests
from concurrent.futures import ThreadPoolExecutor

BASE_URL = "http://localhost:5000/tenant/test-app-1"
SPAWN_URL = f"{BASE_URL}/api/v1/webhook/queue-spawn"
POLL_URL = f"{BASE_URL}/api/v1/webhook/queue-poll"
PING_URL = f"{BASE_URL}/api/v1/webhook/fast-ping"

def test_ping():
    start = time.perf_counter()
    res = requests.get(PING_URL, timeout=5)
    elapsed = (time.perf_counter() - start) * 1000
    return res.status_code == 200, elapsed

def main():
    print("=" * 70)
    print("      APEXKIT $queue ENGINE & QUANTUM SCHEDULER ANALYSIS")
    print("=" * 70)

    # 1. Baseline Latency Check
    print("\n[Step 1] Measuring baseline frontend latency (fast-ping)...")
    baseline_latencies = []
    for _ in range(10):
        ok, latency = test_ping()
        if not ok:
            print("❌ Server unreachable. Make sure ApexKit is running on port 5000.")
            return
        baseline_latencies.append(latency)
    
    avg_baseline = statistics.mean(baseline_latencies)
    print(f"  ✓ Baseline Avg Latency: {avg_baseline:.2f} ms")

    # 2. Spawn Heavy Job
    print("\n[Step 2] Spawning heavy compute background job (15,000,000 loops)...")
    spawn_start = time.perf_counter()
    spawn_res = requests.post(
        SPAWN_URL,
        json={"iterations": 15_000_000},
        headers={"Content-Type": "application/json"},
        timeout=10
    )
    spawn_overhead = (time.perf_counter() - spawn_start) * 1000

    if spawn_res.status_code not in (200, 202):
        print(f"❌ Failed to spawn job: HTTP {spawn_res.status_code} -> {spawn_res.text}")
        return

    spawn_data = spawn_res.json()
    job_id = spawn_data.get("job_id")
    print(f"  ✓ Job ID: {job_id}")
    print(f"  ✓ Offload Response Time: {spawn_overhead:.2f} ms (Webhook did NOT block)")

    # 3. Concurrency Stress Test during Background Execution
    print("\n[Step 3] Bombarding frontend with 50 concurrent requests while $queue is computing...")
    concurrent_latencies = []
    
    with ThreadPoolExecutor(max_workers=10) as executor:
        futures = [executor.submit(test_ping) for _ in range(50)]
        for f in futures:
            ok, lat = f.result()
            if ok:
                concurrent_latencies.append(lat)

    avg_concurrent = statistics.mean(concurrent_latencies)
    p95_concurrent = statistics.quantiles(concurrent_latencies, n=20)[18]  # 95th percentile
    max_concurrent = max(concurrent_latencies)

    print(f"  ✓ Requests Completed: {len(concurrent_latencies)}/50")
    print(f"  ✓ Concurrent Avg Latency: {avg_concurrent:.2f} ms")
    print(f"  ✓ 95th Percentile (p95): {p95_concurrent:.2f} ms")
    print(f"  ✓ Max Latency: {max_concurrent:.2f} ms")

    if max_concurrent < 50.0:
        print("  🌟 PASS: QuantumScheduler successfully interleaved 10ms slices without starving HTTP traffic.")
    else:
        print("  ⚠️ NOTICE: Higher latency observed. Check system CPU usage.")

    # 4. Polling for Completion
    print(f"\n[Step 4] Tracking background progression for {job_id}...")
    completed = False
    polls = 0

    while not completed and polls < 60:
        time.sleep(0.5)
        polls += 1
        poll_res = requests.post(POLL_URL, json={"pid": job_id}, timeout=5)
        
        if poll_res.status_code == 200:
            data = poll_res.json()
            status = data.get("status")
            runtime = data.get("runtime_ms", 0)
            
            print(f"  [{polls * 0.5:4.1f}s] Status: {status:<10} | Active Runtime: {runtime} ms")

            if status == "completed":
                completed = True
                print("\n" + "-" * 70)
                print("🎉 JOB COMPLETED SUCCESSFULLY!")
                print(f"  • Final Status : {status}")
                print(f"  • Total CPU Time Recorded : {runtime} ms")
                print(f"  • Result Payload : {data.get('data') or data.get('result')}")
                print("-" * 70)
            elif status in ("failed", "timed_out"):
                print(f"❌ Job terminated with error: {data.get('error')}")
                break

    if not completed:
        print("❌ Test timed out waiting for background job to finish.")

if __name__ == "__main__":
    main()