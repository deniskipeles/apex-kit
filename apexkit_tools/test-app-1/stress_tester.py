import asyncio
import aiohttp
import time
from colorama import init, Fore, Style
from collections import Counter

# Initialize colors
init(autoreset=True)

# The Target Tenant App
BASE_URL = "http://localhost:5000/tenant/test-app-1"

async def fetch(session, endpoint, request_id):
    url = f"{BASE_URL}/api/v1/webhook/{endpoint}"
    start_time = time.time()
    try:
        async with session.post(url, json={"request_id": request_id}, ssl=False) as response:
            text = await response.text()
            elapsed = time.time() - start_time
            return {
                "id": request_id,
                "status": response.status,
                "text": text,
                "time": elapsed
            }
    except Exception as e:
        elapsed = time.time() - start_time
        return {
            "id": request_id,
            "status": 0,
            "text": str(e),
            "time": elapsed
        }

async def run_batch(endpoint, concurrent_requests, description):
    print(f"\n{Fore.CYAN}{'='*60}")
    print(f"{Fore.CYAN}🚀 RUNNING TEST: {description}")
    print(f"{Fore.CYAN}Endpoint: /api/v1/webhook/{endpoint}")
    print(f"{Fore.CYAN}Concurrency: {concurrent_requests} simultaneous requests")
    print(f"{Fore.CYAN}{'='*60}")

    async with aiohttp.ClientSession() as session:
        tasks = [fetch(session, endpoint, i) for i in range(concurrent_requests)]
        
        start_batch = time.time()
        results = await asyncio.gather(*tasks)
        total_time = time.time() - start_batch

        statuses = Counter(r['status'] for r in results)
        
        print(f"\n{Style.BRIGHT}--- RESULTS ---")
        print(f"Total Time Taken: {total_time:.2f} seconds")
        print(f"Status Codes: {dict(statuses)}")
        
        # Analyze Times
        times = [r['time'] for r in results]
        avg_time = sum(times) / len(times)
        max_time = max(times)
        min_time = min(times)
        print(f"Response Times -> Min: {min_time:.2f}s | Avg: {avg_time:.2f}s | Max: {max_time:.2f}s")

        print(f"\n{Style.BRIGHT}--- SAMPLE RESPONSES ---")
        
        # Group by status to show examples of successes vs rejections
        shown_statuses = set()
        for r in results:
            if r['status'] not in shown_statuses:
                shown_statuses.add(r['status'])
                color = Fore.GREEN if r['status'] == 200 else Fore.RED if r['status'] >= 500 else Fore.YELLOW
                snippet = r['text'][:200].replace('\n', ' ')
                print(f"{color}[Status {r['status']}] {Style.RESET_ALL}({r['time']:.2f}s) {snippet}...")

        return results

async def main():
    print(f"{Fore.MAGENTA}{Style.BRIGHT}Starting ApexKit Engine Stress & Resilience Analysis...")
    print(f"Target: {BASE_URL}")

    # TEST 1: Hono & Polyfill Validation
    # Proves the compiler and crypto.subtle overrides are working.
    await run_batch(
        endpoint="stress-hono",
        concurrent_requests=1,
        description="Hono/JSX & WebCrypto Polyfill Check"
    )

    # TEST 2: The Database Hammer
    # Proves the Mutex locks and O(1) Primary Key fixes allow rapid parallel execution
    # without deadlocking the SQLite threads.
    await run_batch(
        endpoint="stress-db",
        concurrent_requests=10,
        description="Parallel DB Mutex Hammer (1,000 Inserts/Updates/Deletes)"
    )

    # TEST 3: The Network Flood
    # Proves the Scoped HTTP Semaphores correctly queue and eventually abort greedy fetches
    # without crashing the Tokio thread pool or exhausting Linux sockets.
    await run_batch(
        endpoint="stress-network",
        concurrent_requests=10,  # 10 requests * 50 internal fetches = 500 hanging connections
        description="Network Socket Exhaustion & HTTP Semaphore Queuing"
    )

    # TEST 4: The CPU Meltdown
    # Proves the Quantum Scheduler successfully preempts infinite loops (while(true){}) 
    # and enforces the hard-cap CPU budget without freezing the web server.
    await run_batch(
        endpoint="stress-cpu",
        concurrent_requests=5,
        description="Quantum Scheduler CPU Meltdown (Infinite Loop Termination)"
    )

    print(f"\n{Fore.MAGENTA}{Style.BRIGHT}✅ Analysis Complete. Check your Admin Dashboard — it should still be loading instantly!")

if __name__ == "__main__":
    asyncio.run(main())