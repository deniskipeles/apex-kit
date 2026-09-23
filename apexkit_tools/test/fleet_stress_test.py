#!/usr/bin/env python3
"""
ApexKit BaaS Fleets & Script Tools Heavy Stress Testing Engine (Self-Bootstrapping)

Automated Capabilities:
1. Self-Bootstrapping: Automatically discovers or accepts --bin, provisions a sandbox environment, and launches the engine.
2. Port & Rate-Limit Optimization: Configures cluster rate limits to 100,000 req/min to allow heavy hammering.
3. Multi-Tenant Orchestration: Provisions isolated tenant workspaces with minted security keys.
4. Stress Test Pipeline:
   - Dynamic WASM: @silvia-odwyer/photon image transformation & memory buffer decoding.
   - Outbound Network: HTTP client connection pooling & scoped semaphore contention.
   - Background $queue: Quantum scheduler CPU slicing & background task polling.
   - Database Contention: High-concurrency SQLite WAL mutations & JSONB operations.
   - Chaos Workload: Simultaneous execution of all engine subsystems.
5. Forensic Reporting: Quantile latencies (p50, p95, max), request throughput (RPS), and failure audit.
6. Clean Lifecycle Management: Graceful shutdown and temporary directory cleanup on completion.
"""

import os
import sys
import time
import uuid
import json
import shutil
import atexit
import secrets
import base64
import argparse
import subprocess
from pathlib import Path
from concurrent.futures import ThreadPoolExecutor, as_completed

import requests
from requests.adapters import HTTPAdapter
try:
    from urllib3.util import Retry
except ImportError:
    Retry = None

# Auto-load .env if present
if os.path.exists(".env"):
    with open(".env", "r") as f:
        for line in f:
            line = line.strip()
            if line and not line.startswith("#") and "=" in line:
                k, v = line.split("=", 1)
                k = k.strip()
                v = v.split("#")[0].strip().strip('"').strip("'")
                if k not in os.environ:
                    os.environ[k] = v

try:
    from colorama import init, Fore, Style
    init(autoreset=True)
except ImportError:
    class Fore: GREEN = ""; RED = ""; YELLOW = ""; CYAN = ""; MAGENTA = ""; BLUE = ""
    class Style: BRIGHT = ""; RESET_ALL = ""

def print_header(title):
    print(f"\n{Fore.CYAN}{Style.BRIGHT}{'=' * 80}")
    print(f" {title}")
    print(f"{'=' * 80}")

def print_sub(title):
    print(f"\n{Fore.BLUE}{Style.BRIGHT}▶ {title}")
    print(f"{Fore.BLUE}{'-' * 65}")

def print_success(msg):
    print(f"{Fore.GREEN}  [✓] {msg}")

def print_info(msg):
    print(f"{Fore.YELLOW}  [i] {msg}")

def print_error(msg):
    print(f"{Fore.RED}  [✗] {msg}")

def print_warn(msg):
    print(f"{Fore.MAGENTA}  [!] {msg}")

def fail(msg):
    print_error(msg)
    sys.exit(1)

def build_http_session():
    session = requests.Session()
    if Retry is not None:
        adapter = HTTPAdapter(
            max_retries=Retry(
                total=3,
                backoff_factor=0.05,
                status_forcelist=[502, 503, 504],
                raise_on_status=False
            )
        )
        session.mount("http://", adapter)
        session.mount("https://", adapter)
    return session

# Verified valid 1x1 Red PNG binary
VERIFIED_VALID_PNG = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg=="

# -------------------------------------------------------------
# SCRIPT PAYLOADS
# -------------------------------------------------------------

PHOTON_WASM_SCRIPT = f"""
export default async function(req) {{
    const startTime = Date.now();
    
    // Dynamic ESM Import for Photon
    const {{ default: initPhoton, resize, PhotonImage }} = await import("https://esm.sh/@silvia-odwyer/photon@0.3.3");
    await initPhoton("https://esm.sh/@silvia-odwyer/photon@0.3.3/es2022/photon_rs_bg.wasm");

    // Decode sample 1x1 valid PNG image
    const arrayBuffer = $util.base64DecodeBuffer("{VERIFIED_VALID_PNG}");
    const inputBytes = new Uint8Array(arrayBuffer);

    // Initialize Photon WASM struct
    const img = PhotonImage.new_from_byteslice(inputBytes);

    // Run WASM resize operation (Triangle Filter = 1)
    const resizedImg = resize(img, 32, 32, 1);

    // Encode to JPEG bytes
    const outputBytes = resizedImg.get_bytes_jpeg(85);
    const outBase64 = $util.base64Encode(outputBytes);

    // Save to $files storage
    const filename = "photon_" + $util.randomHex(6) + ".jpg";
    const fileMeta = await $files.save(filename, outBase64, "image/jpeg");

    // Insert record in tenant database
    const rec = await $db.records.create("stress_items", {{
        title: "Processed via Photon WASM",
        image_url: fileMeta.url,
        size: outputBytes.length,
        duration_ms: Date.now() - startTime
    }});

    return new Response(JSON.stringify({{
        success: true,
        record_id: rec.id,
        file_url: fileMeta.url,
        elapsed_ms: Date.now() - startTime
    }}), {{ status: 200, headers: {{ "Content-Type": "application/json" }} }});
}}
"""

FETCH_STRESS_SCRIPT = """
export default async function(req) {
    const startTime = Date.now();
    const body = await req.json().catch(() => ({}));
    const targetUrl = body.url || "https://dummyjson.com/quotes/random";

    const fetches = [
        fetch(targetUrl).then(r => r.status).catch(e => e.message),
        fetch(targetUrl).then(r => r.status).catch(e => e.message),
        fetch(targetUrl).then(r => r.status).catch(e => e.message)
    ];

    const results = await Promise.all(fetches);

    return new Response(JSON.stringify({
        success: true,
        fetch_results: results,
        elapsed_ms: Date.now() - startTime
    }), { status: 200, headers: { "Content-Type": "application/json" } });
}
"""

QUEUE_WORKLOAD_SCRIPT = """
export default async function(req) {
    const body = await req.json().catch(() => ({}));
    const targetLoops = body.loops || 200000;

    const job = await $queue.spawn(async (pid, jobReq) => {
        const payload = await jobReq.json().catch(() => ({}));
        const count = payload.loops || 200000;
        let acc = 0;
        for (let i = 0; i < count; i++) {
            acc = (acc + Math.sqrt(i) * 1.0001) % 1000000;
        }
        return { success: true, iterations: count, final_value: acc };
    }, { 
        timeoutMs: 30000,
        args: { loops: targetLoops }
    });

    return new Response(JSON.stringify(job), {
        status: 202,
        headers: { "Content-Type": "application/json" }
    });
}
"""

QUEUE_POLL_SCRIPT = """
export default async function(req) {
    const url = new URL(req.url);
    const body = await req.json().catch(() => ({}));
    const pid = body.pid || url.searchParams.get("pid");

    if (!pid) return new Response(JSON.stringify({ error: "No pid" }), { status: 400 });

    const status = await $queue.status(pid);
    let result = null;
    if (["completed", "failed", "timed_out"].includes(status.status)) {
        result = await $queue.result(pid);
    }

    return new Response(JSON.stringify({
        pid: pid,
        status: status.status,
        runtime_ms: status.runtime_ms,
        result: result ? result.result : null,
        error: status.error
    }), { status: 200, headers: { "Content-Type": "application/json" } });
}
"""

DB_HAMMER_SCRIPT = """
export default async function(req) {
    const startTime = Date.now();
    
    const ids = [];
    for (let i = 0; i < 5; i++) {
        const r = await $db.records.create("stress_items", {
            title: "Item " + $util.randomHex(4),
            image_url: "",
            size: i * 10,
            duration_ms: 0
        });
        ids.push(r.id);
    }

    for (const id of ids) {
        await $db.records.update("stress_items", id, { duration_ms: 999 });
        await $db.records.get("stress_items", id);
    }

    return new Response(JSON.stringify({
        processed: ids.length,
        elapsed_ms: Date.now() - startTime
    }), { status: 200, headers: { "Content-Type": "application/json" } });
}
"""

CHAOS_COMBO_SCRIPT = f"""
export default async function(req) {{
    const start = Date.now();

    // 1. WASM Photon Resize
    const {{ default: initPhoton, resize, PhotonImage }} = await import("https://esm.sh/@silvia-odwyer/photon@0.3.3");
    await initPhoton("https://esm.sh/@silvia-odwyer/photon@0.3.3/es2022/photon_rs_bg.wasm");
    const arrayBuffer = $util.base64DecodeBuffer("{VERIFIED_VALID_PNG}");
    const img = PhotonImage.new_from_byteslice(new Uint8Array(arrayBuffer));
    const resized = resize(img, 16, 16, 1);
    const jpegBytes = resized.get_bytes_jpeg(75);

    // 2. Outbound Fetch
    const pingStatus = await fetch("https://dummyjson.com/quotes/1").then(r => r.status).catch(() => 0);

    // 3. Database Write
    const rec = await $db.records.create("stress_items", {{
        title: "Chaos Combo Item",
        image_url: "in_memory",
        size: jpegBytes.length,
        duration_ms: Date.now() - start
    }});

    // 4. Background Queue Spawn
    const job = await $queue.spawn(async (pid, jobReq) => {{
        return {{ done: true, pid }};
    }}, {{ timeoutMs: 10000 }});

    return new Response(JSON.stringify({{
        success: true,
        record_id: rec.id,
        ping: pingStatus,
        job_id: job.pid,
        wall_time_ms: Date.now() - start
    }}), {{ status: 200, headers: {{ "Content-Type": "application/json" }} }});
}}
"""

class TenantFleet:
    def __init__(self, tenant_id, base_url, api_key):
        self.tenant_id = tenant_id
        self.base_url = f"{base_url}/tenant/{tenant_id}"
        self.api_key = api_key
        self.headers = {
            "x-api-key": api_key,
            "Authorization": f"Bearer {api_key}",
            "Content-Type": "application/json"
        }

class FleetStressRunner:
    def __init__(self, binary_path: str, port: int, num_tenants: int, concurrency: int, rounds: int, external_url: str = None, external_key: str = None):
        self.binary_path = Path(binary_path).resolve() if binary_path else None
        self.port = port
        self.num_tenants = num_tenants
        self.concurrency = concurrency
        self.rounds = rounds
        self.external_url = external_url
        self.external_key = external_key
        self.is_self_managed = external_url is None

        self.cluster_dir = Path("./_fleet_stress_env").resolve()
        self.master_key = base64.b64encode(secrets.token_bytes(32)).decode()
        self.process = None

        if self.is_self_managed:
            self.base_url = f"http://127.0.0.1:{self.port}"
        else:
            self.base_url = external_url.rstrip("/")

        self.root_headers = {}
        self.fleets: list[TenantFleet] = []
        self.audit_results = {}

        if self.is_self_managed:
            atexit.register(self.cleanup)

    def cleanup(self):
        if self.is_self_managed:
            print_info("Stopping self-managed ApexKit instance...")
            if self.process and self.process.poll() is None:
                self.process.terminate()
                try:
                    self.process.wait(timeout=3)
                except subprocess.TimeoutExpired:
                    self.process.kill()

            try:
                subprocess.run(f"fuser -k {self.port}/tcp", shell=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            except Exception:
                pass

            if self.cluster_dir.exists():
                shutil.rmtree(self.cluster_dir, ignore_errors=True)

    def bootstrap_engine(self):
        if not self.is_self_managed:
            print_info(f"Targeting external ApexKit engine at {self.base_url}")
            self.root_headers = {
                "x-api-key": self.external_key or "",
                "Authorization": f"Bearer {self.external_key or ''}",
                "Content-Type": "application/json"
            }
            return

        print_sub("0. BOOTSTRAPPING LOCAL APEXKIT ENGINE")
        if not self.binary_path or not self.binary_path.exists():
            fail(f"ApexKit binary not found at '{self.binary_path}'")

        if self.cluster_dir.exists():
            shutil.rmtree(self.cluster_dir, ignore_errors=True)
        self.cluster_dir.mkdir(parents=True, exist_ok=True)

        exe_target = self.cluster_dir / "apexkit"
        shutil.copy2(self.binary_path, exe_target)
        exe_target.chmod(0o755)

        env = os.environ.copy()
        env["PORT"] = str(self.port)
        env["APEXKIT_MASTER_KEY"] = self.master_key
        env["RUST_LOG"] = "info,tower=warn,hyper=warn"

        log_file = open(self.cluster_dir / "engine.log", "w")
        self.process = subprocess.Popen(
            [str(exe_target)],
            cwd=str(self.cluster_dir),
            env=env,
            stdout=log_file,
            stderr=subprocess.STDOUT
        )

        print_info(f"Starting ApexKit engine on 127.0.0.1:{self.port}...")
        start_wait = time.time()
        ready = False
        while time.time() - start_wait < 20:
            try:
                r = requests.get(f"{self.base_url}/healthz", timeout=1)
                if r.status_code == 200:
                    ready = True
                    break
            except Exception:
                pass
            time.sleep(0.3)

        if not ready:
            fail(f"Engine failed to start in time. Check {self.cluster_dir}/engine.log")

        # Authenticate as seeded admin
        r_login = requests.post(f"{self.base_url}/api/v1/auth/login", json={
            "email": "admin@apexkit.io",
            "password": "password"
        }, timeout=5)

        if r_login.status_code not in (200, 201):
            fail(f"Failed logging in as default admin: {r_login.text}")

        token = r_login.json()["token"]
        self.root_headers = {
            "Authorization": f"Bearer {token}",
            "Content-Type": "application/json"
        }
        print_success(f"Engine online & authenticated (Token: {token[:15]}...)")

        # Raise rate limit via API to 100,000 req/min for stress testing
        print_info("Configuring rate limit to 100,000 req/min via Admin Settings API...")
        r_set = requests.patch(f"{self.base_url}/api/v1/admin/settings", json={
            "security": {
                "cors_allow_all": True,
                "cors_origins": "",
                "tenant_transparency": False,
                "global_rate_limit": 100000,
                "tenant_free_rate_limit": 10000,
                "tenant_pro_rate_limit": 50000
            }
        }, headers=self.root_headers, timeout=5)

        if r_set.status_code in (200, 201):
            print_success("Engine rate limits raised for stress testing.")
        else:
            print_warn(f"Could not adjust rate limit: {r_set.text}")

    def run(self):
        print_header(f"APEXKIT FLEET ASSAULT: {self.num_tenants} TENANTS | {self.concurrency} THREADS | {self.rounds} ROUNDS")
        try:
            self.bootstrap_engine()
            self.setup_fleet()
            self.deploy_tools_to_fleet()
            
            # Stress Phases
            self.test_phase_wasm_photon()
            self.test_phase_fetch_concurrency()
            self.test_phase_queue_orchestration()
            self.test_phase_db_contention()
            self.test_phase_chaos_all_tools()
            
            self.generate_forensic_report()
        finally:
            self.teardown_fleet()

    def setup_fleet(self):
        print_sub(f"1. PROVISIONING {self.num_tenants} INDEPENDENT TENANTS & MINTING KEYS")
        for i in range(self.num_tenants):
            t_id = f"fleet-{uuid.uuid4().hex[:6]}"
            r = requests.post(f"{self.base_url}/api/v1/admin/tenants", json={
                "tenant_id": t_id,
                "name": f"Fleet Node {i+1}",
                "tier": "pro"
            }, headers=self.root_headers, timeout=10)
            
            if r.status_code not in (200, 201):
                fail(f"Failed provisioning tenant {t_id}: {r.text}")

            k_res = requests.post(f"{self.base_url}/api/v1/admin/keys", json={
                "name": f"Key-{t_id}",
                "target_tenant": t_id,
                "env_type": "sk",
                "roles": ["admin"],
                "bypass_cors": True
            }, headers=self.root_headers, timeout=10)

            if k_res.status_code not in (200, 201):
                fail(f"Failed to mint API Key for {t_id}: {k_res.text}")

            t_key = k_res.json()["key"]
            fleet_node = TenantFleet(t_id, self.base_url, t_key)
            self.fleets.append(fleet_node)
            print_success(f"Fleet Node {i+1}/{self.num_tenants}: Tenant '{t_id}' online")

    def deploy_tools_to_fleet(self):
        print_sub("2. DEPLOYING STRESS TEST TOOLS & COLLECTIONS TO ALL TENANTS")
        
        col_schema = {
            "name": "stress_items",
            "schema": {
                "fields": {
                    "title": { "type": "string", "required": True },
                    "image_url": { "type": "string", "required": False },
                    "size": { "type": "number", "required": True },
                    "duration_ms": { "type": "number", "required": False }
                },
                "policies": {
                    "read": "public",
                    "create": "admin",
                    "update": "admin",
                    "delete": "admin"
                }
            }
        }

        tools = [
            ("stress-wasm", PHOTON_WASM_SCRIPT),
            ("stress-fetch", FETCH_STRESS_SCRIPT),
            ("stress-queue-spawn", QUEUE_WORKLOAD_SCRIPT),
            ("stress-queue-poll", QUEUE_POLL_SCRIPT),
            ("stress-db", DB_HAMMER_SCRIPT),
            ("stress-chaos", CHAOS_COMBO_SCRIPT)
        ]

        for fleet in self.fleets:
            r_col = requests.post(f"{fleet.base_url}/api/v1/collections", json=col_schema, headers=fleet.headers, timeout=5)
            if r_col.status_code not in (200, 201):
                fail(f"Failed to create collection in {fleet.tenant_id}: {r_col.text}")

            for name, code in tools:
                r_sc = requests.post(f"{fleet.base_url}/api/v1/admin/scripts", json={
                    "name": name,
                    "trigger_type": "manual",
                    "code": code,
                    "active": True
                }, headers=fleet.headers, timeout=5)
                if r_sc.status_code not in (200, 201):
                    fail(f"Failed deploying script '{name}' to {fleet.tenant_id}: {r_sc.text}")
            
            print_success(f"Deployed endpoints & 'stress_items' DB to {fleet.tenant_id}")

    def execute_concurrent_burst(self, test_name: str, endpoint: str, payload_builder=None):
        print_info(f"Bombarding /webhook/{endpoint} across all {len(self.fleets)} tenants...")
        
        requests_list = []
        for fleet in self.fleets:
            for i in range(self.rounds):
                payload = payload_builder(i) if payload_builder else {}
                url = f"{fleet.base_url}/api/v1/webhook/{endpoint}"
                requests_list.append((fleet.tenant_id, url, fleet.headers, payload))

        latencies = []
        status_counts = {}
        error_samples = []

        def worker(item):
            tid, url, headers, p = item
            session = build_http_session()
            start = time.perf_counter()
            try:
                res = session.post(url, json=p, headers=headers, timeout=35)
                duration = (time.perf_counter() - start) * 1000
                return res.status_code, duration, res.text, None
            except Exception as e:
                duration = (time.perf_counter() - start) * 1000
                return 0, duration, "", str(e)

        start_time = time.perf_counter()
        with ThreadPoolExecutor(max_workers=self.concurrency) as executor:
            futures = [executor.submit(worker, req) for req in requests_list]
            for f in as_completed(futures):
                status, lat, text, exc = f.result()
                latencies.append(lat)
                status_counts[status] = status_counts.get(status, 0) + 1
                if status not in (200, 201, 202):
                    if exc:
                        error_samples.append(f"NetworkException: {exc}")
                    else:
                        error_samples.append(f"HTTP {status}: {text}")

        total_wall_sec = time.perf_counter() - start_time
        latencies.sort()
        count = len(latencies)
        p50 = latencies[int(count * 0.50)] if count else 0
        p95 = latencies[int(count * 0.95)] if count else 0
        max_lat = latencies[-1] if count else 0

        self.audit_results[test_name] = {
            "total_calls": count,
            "wall_sec": total_wall_sec,
            "rps": count / total_wall_sec if total_wall_sec else 0,
            "status_counts": status_counts,
            "p50_ms": p50,
            "p95_ms": p95,
            "max_ms": max_lat,
            "errors": error_samples[:5]
        }

        success_count = sum(status_counts.get(c, 0) for c in (200, 201, 202))
        color = Fore.GREEN if success_count == count else Fore.YELLOW
        print(f"{color}  -> Done in {total_wall_sec:.2f}s | Throughput: {count/total_wall_sec:.1f} req/s | p50: {p50:.1f}ms | p95: {p95:.1f}ms | Max: {max_lat:.1f}ms")
        print(f"  -> Status Codes: {status_counts}")
        if error_samples:
            print_warn(f"Full Error Samples ({len(error_samples)} total failures):")
            for err in error_samples[:3]:
                print(f"{Fore.RED}     • {err}")

    def test_phase_wasm_photon(self):
        print_sub("3. WASM & PHOTON STRESS: COMPETING INSTANTIATION & PARALLEL IMAGE PROCESSING")
        self.execute_concurrent_burst("Photon_WASM_Resize", "stress-wasm")

    def test_phase_fetch_concurrency(self):
        print_sub("4. NETWORK & FETCH STRESS: SCOPED HTTP SEMAPHORE & CLIENT POOLING")
        self.execute_concurrent_burst("HTTP_Fetch_Flood", "stress-fetch")

    def test_phase_queue_orchestration(self):
        print_sub("5. BACKGROUND $queue ORCHESTRATION & QUANTUM SCHEDULER")
        
        spawned_jobs = []
        for fleet in self.fleets:
            res = requests.post(f"{fleet.base_url}/api/v1/webhook/stress-queue-spawn", json={"loops": 2000000}, headers=fleet.headers, timeout=10)
            if res.status_code in (200, 201, 202):
                pid = res.json().get("pid")
                spawned_jobs.append((fleet, pid))
        
        total_spawned = len(spawned_jobs)
        print_success(f"Spawned {total_spawned} parallel compute tasks across tenants. Tracking CPU quantum times...")

        start_poll = time.time()
        completed_pids = set()
        failed_pids = set()

        while time.time() - start_poll < 25 and (len(completed_pids) + len(failed_pids)) < total_spawned:
            time.sleep(0.5)
            for fleet, pid in spawned_jobs:
                if pid in completed_pids or pid in failed_pids:
                    continue
                r = requests.post(f"{fleet.base_url}/api/v1/webhook/stress-queue-poll", json={"pid": pid}, headers=fleet.headers, timeout=5)
                if r.status_code in (200, 201, 202):
                    d = r.json()
                    st = d.get("status")
                    if st == "completed":
                        completed_pids.add(pid)
                    elif st in ("failed", "timed_out"):
                        print_warn(f"Job {pid} failed: {d.get('error')}")
                        failed_pids.add(pid)

        if len(completed_pids) == total_spawned:
            print_success(f"All {len(completed_pids)} background queue tasks completed under preemptive CPU slicing.")
        elif failed_pids:
            print_error(f"{len(failed_pids)} tasks failed during execution.")
        else:
            print_warn(f"{total_spawned - len(completed_pids)} background tasks were still pending.")

    def test_phase_db_contention(self):
        print_sub("6. DATABASE MUTEX & PARALLEL RECORD HAMMER")
        self.execute_concurrent_burst("Database_Contention", "stress-db")

    def test_phase_chaos_all_tools(self):
        print_sub("7. CHAOS MULTI-TOOL ROUND (WASM + FETCH + DB + QUEUE CONCURRENTLY)")
        self.execute_concurrent_burst("Chaos_Combined_Assault", "stress-chaos")

    def generate_forensic_report(self):
        print_header("DIAGNOSTIC & PERFORMANCE ANALYSIS REPORT")
        print(f"{'Benchmark Phase':<25} | {'Reqs':<5} | {'Req/s':<7} | {'p50 (ms)':<9} | {'p95 (ms)':<9} | {'Max (ms)':<9} | {'Success Status'}")
        print("-" * 88)
        
        for name, data in self.audit_results.items():
            success_count = sum(data["status_counts"].get(c, 0) for c in (200, 201, 202))
            rate = (success_count / data["total_calls"]) * 100 if data["total_calls"] else 0
            rate_str = f"{rate:.1f}% OK"
            color = Fore.GREEN if rate == 100.0 else Fore.RED
            
            print(f"{name:<25} | {data['total_calls']:<5} | {data['rps']:<7.1f} | {data['p50_ms']:<9.1f} | {data['p95_ms']:<9.1f} | {data['max_ms']:<9.1f} | {color}{rate_str}{Style.RESET_ALL}")

    def teardown_fleet(self):
        print_sub("8. TEARDOWN: DELETING ALL FLEET TENANTS & DESTROYING DATABASES")
        for fleet in self.fleets:
            try:
                requests.delete(f"{self.base_url}/api/v1/admin/tenants/{fleet.tenant_id}", headers=self.root_headers, timeout=5)
                print_success(f"Tenant '{fleet.tenant_id}' wiped from host.")
            except Exception as e:
                print_error(f"Failed deleting {fleet.tenant_id}: {e}")
        print(f"\n{Fore.GREEN}{Style.BRIGHT}✨ FLEET CLEANUP COMPLETE. System is pristine.\n")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="ApexKit Fleet Multi-Tenant Stress Engine")
    parser.add_argument("--bin", default="./target/release/apexkit-cli", help="Path to apexkit executable binary (default: ./target/release/apexkit-cli or ./apexkit)")
    parser.add_argument("--port", type=int, default=5000, help="Port to run local engine on (default: 5000)")
    parser.add_argument("--url", default=None, help="Target an already-running ApexKit URL instead of auto-spawning (e.g. http://localhost:5000)")
    parser.add_argument("--api-key", default=None, help="Root Admin API Key (only required when using an external --url)")
    parser.add_argument("--tenants", type=int, default=3, help="Number of concurrent tenants to launch (default: 3)")
    parser.add_argument("--concurrency", type=int, default=15, help="Concurrent HTTP worker threads (default: 15)")
    parser.add_argument("--rounds", type=int, default=10, help="Rounds of requests per tenant per phase (default: 10)")

    args = parser.parse_args()

    bin_path = args.bin
    if not args.url:
        bin_target = Path(args.bin)
        if not bin_target.exists() and Path("./apexkit").exists():
            bin_path = "./apexkit"

    runner = FleetStressRunner(
        binary_path=bin_path,
        port=args.port,
        num_tenants=args.tenants,
        concurrency=args.concurrency,
        rounds=args.rounds,
        external_url=args.url,
        external_key=args.api_key
    )
    try:
        runner.run()
    except KeyboardInterrupt:
        print_warn("\nStress test interrupted by user.")
        sys.exit(0)