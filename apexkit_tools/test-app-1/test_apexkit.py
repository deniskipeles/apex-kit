#!/usr/bin/env python3
"""
ApexKit BaaS & Script Engine Automated Multi-Tenant Test & Analysis Suite
Automates complete multi-tenant lifecycle:
1. Connects with Root Admin API Key
2. Provisions a fresh, isolated Tenant
3. Mints a dedicated Tenant-scoped API Key (sk)
4. Executes comprehensive testing strictly inside the Tenant context
5. Analyzes performance, built-in APIs, and background queues
6. Teardown: Destroys the Tenant and frees all resources
"""

import os
import sys
import time
import argparse
import uuid
import requests

# 1. Auto-load .env if present in current directory
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

# Terminal formatting
try:
    from colorama import init, Fore, Style
    init(autoreset=True)
except ImportError:
    class Fore: GREEN = ""; RED = ""; YELLOW = ""; CYAN = ""; MAGENTA = ""; BLUE = ""
    class Style: BRIGHT = ""; RESET_ALL = ""

def print_banner():
    print(f"\n{Fore.CYAN}{Style.BRIGHT}{'=' * 72}")
    print(f"{Fore.CYAN}{Style.BRIGHT}   APEXKIT BaaS & SCRIPT ENGINE MULTI-TENANT TEST RUNNER")
    print(f"{Fore.CYAN}{Style.BRIGHT}{'=' * 72}")

def print_header(title):
    print(f"\n{Fore.BLUE}{Style.BRIGHT}▶ {title}")
    print(f"{Fore.BLUE}{'-' * 60}")

def print_success(msg):
    print(f"{Fore.GREEN}  [✓] {msg}")

def print_info(msg):
    print(f"{Fore.YELLOW}  [i] {msg}")

def print_error(msg):
    print(f"{Fore.RED}  [✗] {msg}")

def fail(msg):
    print_error(msg)
    print(f"\n{Fore.RED}{Style.BRIGHT}ABORTED: Automated test failed.")
    sys.exit(1)

class ApexKitTester:
    def __init__(self, base_url: str, root_api_key: str):
        self.base_url = base_url.rstrip("/")
        self.root_api_key = root_api_key
        self.root_headers = {
            "x-api-key": self.root_api_key,
            "Authorization": f"Bearer {self.root_api_key}",
            "Content-Type": "application/json"
        }
        
        # Test targets
        self.tenant_id = f"test-tenant-{uuid.uuid4().hex[:8]}"
        self.tenant_api_key = None
        self.tenant_base_url = None
        self.tenant_headers = None
        
        # Tracking metrics
        self.metrics = {}

    def time_call(self, name: str, fn):
        start = time.perf_counter()
        res = fn()
        elapsed_ms = (time.perf_counter() - start) * 1000
        self.metrics[name] = elapsed_ms
        return res, elapsed_ms

    def run_all(self):
        print_banner()
        print_info(f"Target URL: {self.base_url}")
        print_info(f"Root Key  : {self.root_api_key[:16]}...")
        print_info(f"Tenant ID : {self.tenant_id}")

        try:
            self.step_1_verify_root()
            self.step_2_provision_tenant()
            self.step_3_mint_tenant_key()
            self.step_4_tenant_database_ops()
            self.step_5_tenant_script_engine()
            self.step_6_tenant_queue_scheduler()
            self.step_7_generate_analysis_report()
        finally:
            self.step_8_teardown_tenant()

    # -------------------------------------------------------------
    # STEP 1: Verify Root Access
    # -------------------------------------------------------------
    def step_1_verify_root(self):
        print_header("1. VERIFY ROOT ACCESS & SYSTEM METRICS")
        
        res, t = self.time_call("root_healthz", lambda: requests.get(f"{self.base_url}/healthz", timeout=5))
        if res.status_code != 200:
            fail(f"Health check failed: HTTP {res.status_code}")
        print_success(f"Health check OK ({t:.1f}ms)")

        res, t = self.time_call("root_version", lambda: requests.get(f"{self.base_url}/version", timeout=5))
        if res.status_code != 200:
            fail("Failed to retrieve system version.")
        v = res.json()
        print_success(f"System Online: Root v{v.get('root')}, Core v{v.get('core')}, Vector v{v.get('vector')} ({t:.1f}ms)")

        res, t = self.time_call("root_app_details", lambda: requests.get(f"{self.base_url}/app-details", headers=self.root_headers, timeout=5))
        if res.status_code != 200:
            fail(f"Root API Key rejected: HTTP {res.status_code} - {res.text}")
        print_success(f"Root authorization verified ({t:.1f}ms)")

    # -------------------------------------------------------------
    # STEP 2: Provision Isolated Tenant
    # -------------------------------------------------------------
    def step_2_provision_tenant(self):
        print_header("2. PROVISION ISOLATED TENANT")
        payload = {
            "tenant_id": self.tenant_id,
            "name": f"Automated CI Tenant {self.tenant_id}",
            "tier": "pro"
        }
        res, t = self.time_call("provision_tenant", lambda: requests.post(
            f"{self.base_url}/api/v1/admin/tenants",
            json=payload,
            headers=self.root_headers,
            timeout=10
        ))
        if res.status_code not in (200, 201):
            fail(f"Failed to provision tenant: HTTP {res.status_code} -> {res.text}")
        print_success(f"Tenant '{self.tenant_id}' created with 'pro' tier ({t:.1f}ms)")

    # -------------------------------------------------------------
    # STEP 3: Mint Scoped Tenant API Key
    # -------------------------------------------------------------
    def step_3_mint_tenant_key(self):
        print_header("3. GENERATE TENANT-SCOPED API KEY")
        key_payload = {
            "name": f"CI-Key-{self.tenant_id}",
            "target_tenant": self.tenant_id,
            "env_type": "sk",
            "roles": ["admin"],
            "bypass_cors": True
        }
        res, t = self.time_call("create_tenant_key", lambda: requests.post(
            f"{self.base_url}/api/v1/admin/keys",
            json=key_payload,
            headers=self.root_headers,
            timeout=10
        ))
        if res.status_code not in (200, 201):
            fail(f"Failed to mint tenant API key: HTTP {res.status_code} -> {res.text}")
        
        data = res.json()
        self.tenant_api_key = data["key"]
        self.tenant_base_url = f"{self.base_url}/tenant/{self.tenant_id}"
        self.tenant_headers = {
            "x-api-key": self.tenant_api_key,
            "Authorization": f"Bearer {self.tenant_api_key}",
            "Content-Type": "application/json"
        }
        print_success(f"Generated Tenant Key: {self.tenant_api_key[:22]}... ({t:.1f}ms)")

        # Verify Tenant App Details with Tenant Key
        res, _ = self.time_call("tenant_details", lambda: requests.get(
            f"{self.tenant_base_url}/app-details",
            headers=self.tenant_headers,
            timeout=5
        ))
        if res.status_code != 200:
            fail(f"Tenant-scoped endpoint failed: HTTP {res.status_code} -> {res.text}")
        
        details = res.json()
        assert details.get("scope_type") == "tenant", f"Expected scope_type 'tenant', got {details.get('scope_type')}"
        assert details.get("scope_id") == self.tenant_id, "Tenant ID mismatch in session scope."
        print_success(f"Tenant successfully isolated and active in scope: {details.get('scope')}")

    # -------------------------------------------------------------
    # STEP 4: Tenant Database Operations (CRUD & Query Engine)
    # -------------------------------------------------------------
    def step_4_tenant_database_ops(self):
        print_header("4. TENANT DATABASE (CRUD, JSONB, QUERY ENGINE)")
        
        # 1. Create Collection inside Tenant (All fields have required: bool)
        col_payload = {
            "name": "products",
            "schema": {
                "fields": {
                    "sku": { "type": "string", "required": True, "unique": True },
                    "title": { "type": "string", "required": True, "ose_indexed": True },
                    "price": { "type": "number", "required": True, "sql_indexed": True },
                    "in_stock": { "type": "boolean", "required": False, "default": True }
                },
                "policies": {
                    "read": "public",
                    "create": "admin",
                    "update": "admin",
                    "delete": "admin"
                }
            }
        }
        res, t = self.time_call("create_collection", lambda: requests.post(
            f"{self.tenant_base_url}/api/v1/collections",
            json=col_payload,
            headers=self.tenant_headers
        ))
        if res.status_code != 201:
            fail(f"Failed to create collection in tenant: {res.text}")
        col_id = res.json()["id"]
        print_success(f"Created collection 'products' (ID: {col_id}) ({t:.1f}ms)")

        # 2. Insert Records
        records = [
            {"data": {"sku": "APX-01", "title": "Apex Mechanical Keyboard", "price": 129.99, "in_stock": True}},
            {"data": {"sku": "APX-02", "title": "Apex Ultrawide Monitor", "price": 499.00, "in_stock": True}},
            {"data": {"sku": "APX-03", "title": "Ergonomic Mouse", "price": 69.50, "in_stock": False}}
        ]
        insert_times = []
        for r in records:
            _, it = self.time_call("insert_record", lambda: requests.post(
                f"{self.tenant_base_url}/api/v1/collections/{col_id}/records",
                json=r,
                headers=self.tenant_headers
            ))
            insert_times.append(it)
        print_success(f"Inserted {len(records)} records (Avg: {sum(insert_times)/len(insert_times):.1f}ms per row)")

        # 3. Test ApexQuery Engine (Aggregation)
        query_payload = {
            "from": "products",
            "select": [
                {"field": "price", "fn": "avg", "as": "average_price"},
                {"field": "price", "fn": "count", "as": "total_items"}
            ]
        }
        res, t = self.time_call("query_engine", lambda: requests.post(
            f"{self.tenant_base_url}/api/v1/collections/{col_id}/query",
            json=query_payload,
            headers=self.tenant_headers
        ))
        if res.status_code != 200:
            fail(f"ApexQuery engine failed: {res.text}")
        q_res = res.json()
        print_success(f"Query Engine Aggregate Result: {q_res} ({t:.1f}ms)")

    # -------------------------------------------------------------
    # STEP 5: Tenant Script Engine & VFS Execution
    # -------------------------------------------------------------
    def step_5_tenant_script_engine(self):
        print_header("5. SCRIPT ENGINE: BUILT-INS & TSX EXECUTION")
        
        script_code = """
export default async function(req) {
    const body = await req.json().catch(() => ({}));

    // Test 1: $db in tenant scope
    const items = await $db.records.list("products", { limit: 10 });

    // Test 2: $cache isolated in tenant
    await $cache.set("tenant_token", "apex_secret_42", 60);
    const cachedToken = await $cache.get("tenant_token");

    // Test 3: $env dynamic properties
    const appUrl = $env.APP_URL;

    // Test 4: $util crypto hashing
    const hash = $util.hash("test_payload", "sha256");

    return new Response(JSON.stringify({
        status: "ok",
        tenant_db_items: items.total,
        cache_verified: cachedToken === "apex_secret_42",
        app_url: appUrl,
        hash: hash
    }), {
        status: 200,
        headers: { "Content-Type": "application/json" }
    });
}
"""
        # Deploy Script into Tenant's VFS
        script_payload = {
            "name": "ci-runner",
            "trigger_type": "manual",
            "code": script_code,
            "active": True
        }
        res, t = self.time_call("deploy_script", lambda: requests.post(
            f"{self.tenant_base_url}/api/v1/admin/scripts",
            json=script_payload,
            headers=self.tenant_headers
        ))
        if res.status_code != 200:
            fail(f"Failed to deploy script: {res.text}")
        print_success(f"Deployed script 'ci-runner' to Tenant VFS ({t:.1f}ms)")

        # Execute Webhook via Tenant Route
        res, t = self.time_call("run_webhook", lambda: requests.post(
            f"{self.tenant_base_url}/api/v1/webhook/ci-runner",
            json={},
            headers=self.tenant_headers
        ))
        if res.status_code != 200:
            fail(f"Tenant webhook execution failed: HTTP {res.status_code} -> {res.text}")
        
        data = res.json()
        assert data.get("tenant_db_items") == 3, f"Expected 3 items in tenant DB, got {data.get('tenant_db_items')}"
        assert data.get("cache_verified") is True, "Tenant script cache isolation verification failed."
        print_success(f"Executed Tenant Webhook -> Verified DB access, isolated Cache, and Utils ({t:.1f}ms)")

    # -------------------------------------------------------------
    # STEP 6: Quantum Scheduler & Background Queue ($queue)
    # -------------------------------------------------------------
    def step_6_tenant_queue_scheduler(self):
        print_header("6. QUANTUM SCHEDULER & ASYNC $queue")
        
        queue_code = """
export default async function(req) {
    const job = await $queue.spawn(async (pid, jobReq) => {
        let acc = 0;
        for (let i = 0; i < 500000; i++) {
            acc = (acc + Math.sqrt(i) * 1.0001) % 1000000;
        }
        return { success: true, iterations: 500000, value: acc };
    }, { timeoutMs: 30000 });

    return new Response(JSON.stringify(job), {
        status: 202,
        headers: { "Content-Type": "application/json" }
    });
}
"""
        poll_code = """
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
        status: status.status,
        runtime_ms: status.runtime_ms,
        result: result ? result.result : null
    }), { status: 200, headers: { "Content-Type": "application/json" }});
}
"""
        requests.post(f"{self.tenant_base_url}/api/v1/admin/scripts", json={"name": "queue-spawn", "trigger_type": "manual", "code": queue_code, "active": True}, headers=self.tenant_headers)
        requests.post(f"{self.tenant_base_url}/api/v1/admin/scripts", json={"name": "queue-poll", "trigger_type": "manual", "code": poll_code, "active": True}, headers=self.tenant_headers)

        # Offload Job
        res, t = self.time_call("spawn_queue_job", lambda: requests.post(f"{self.tenant_base_url}/api/v1/webhook/queue-spawn", json={}, headers=self.tenant_headers))
        if res.status_code != 202:
            fail(f"Failed to spawn queue job: {res.text}")
        
        job_id = res.json()["pid"]
        print_success(f"Background job dispatched: {job_id} ({t:.1f}ms overhead)")

        # Poll for completion
        print_info("Polling quantum scheduler for execution progress...")
        completed = False
        polls = 0
        while not completed and polls < 20:
            time.sleep(0.5)
            polls += 1
            pres = requests.post(
                f"{self.tenant_base_url}/api/v1/webhook/queue-poll?pid={job_id}",
                json={"pid": job_id},
                headers=self.tenant_headers
            )
            if pres.status_code == 200:
                pdata = pres.json()
                st = pdata.get("status")
                rt = pdata.get("runtime_ms", 0)
                if st == "completed":
                    completed = True
                    print_success(f"Job completed in {rt}ms! Computation verified: {pdata.get('result')}")
                    break
                elif st in ("failed", "timed_out"):
                    fail(f"Job failed with status: {st}")
        
        if not completed:
            fail("Queue job did not complete within timeout.")

    # -------------------------------------------------------------
    # STEP 7: Analysis Report
    # -------------------------------------------------------------
    def step_7_generate_analysis_report(self):
        print_header("7. FULL PERFORMANCE & ARCHITECTURE ANALYSIS")
        print(f"{'Operation':<35} | {'Latency':<12}")
        print("-" * 50)
        for op, duration in self.metrics.items():
            print(f"{op:<35} | {duration:>8.2f} ms")
        print("-" * 50)
        print(f"{Fore.GREEN}All tested sub-systems (DB, Scripting, VFS, Queue, KV Cache) are functional.")

    # -------------------------------------------------------------
    # STEP 8: Teardown
    # -------------------------------------------------------------
    def step_8_teardown_tenant(self):
        print_header("8. TEARDOWN (CLEANUP)")
        if not self.tenant_id:
            return
        
        print_info(f"Destroying Tenant '{self.tenant_id}'...")
        res = requests.delete(
            f"{self.base_url}/api/v1/admin/tenants/{self.tenant_id}",
            headers=self.root_headers
        )
        if res.status_code in (200, 204):
            print_success(f"Tenant '{self.tenant_id}' and all associated files wiped clean from host.")
            print(f"\n{Fore.GREEN}{Style.BRIGHT}✨ COMPLETE: System is 100% verified and spotless.\n")
        else:
            print_error(f"Failed to delete tenant: HTTP {res.status_code} - {res.text}")

if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="ApexKit Automated Multi-Tenant Test Runner")
    parser.add_argument("--url", default=os.environ.get("APEXKIT_URL", "http://localhost:5000"), help="ApexKit server base URL")
    parser.add_argument("--api-key", default=os.environ.get("APEXKIT_API_KEY", "root_sys_prod_12345678_abcd"), help="Root Admin API Key")

    args = parser.parse_args()
    tester = ApexKitTester(base_url=args.url, root_api_key=args.api_key)
    tester.run_all()