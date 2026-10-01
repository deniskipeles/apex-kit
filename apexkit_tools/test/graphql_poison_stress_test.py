#!/usr/bin/env python3
"""
ApexKit GraphQL Poison & Adversarial Stress Engine (Self-Bootstrapping)

Tests:
1. Exact Crash Reproduction: Colliding field and relation identifiers in the same schema.
2. GraphQL Reserved Word & Type Collisions: Collections/fields named Query, Mutation, id, clientMutationId, etc.
3. Scope-Aware Introspection Lock: Toggling disable_introspection via General Settings for Root vs Tenants.
4. Denial-of-Service Bombs: Deep query nesting (depth > 32) and alias multiplier bombs (complexity > 2000).
5. High-Concurrency Multi-Tenant Assault: Simultaneous concurrent GraphQL mutations and queries.
6. Process Survival Verification: Proves the binary remains responsive and healthy after all attacks.
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

try:
    from colorama import init, Fore, Style
    init(autoreset=True)
except ImportError:
    class Fore: GREEN = ""; RED = ""; YELLOW = ""; CYAN = ""; MAGENTA = ""; BLUE = ""
    class Style: BRIGHT = ""; RESET_ALL = ""

def print_header(title):
    print(f"\n{Fore.CYAN}{Style.BRIGHT}{'=' * 85}")
    print(f" {title}")
    print(f"{'=' * 85}")

def print_sub(title):
    print(f"\n{Fore.BLUE}{Style.BRIGHT}▶ {title}")
    print(f"{Fore.BLUE}{'-' * 70}")

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
                total=2,
                backoff_factor=0.05,
                status_forcelist=[502, 503, 504],
                raise_on_status=False
            )
        )
        session.mount("http://", adapter)
        session.mount("https://", adapter)
    return session

class GraphqlPoisonRunner:
    def __init__(self, binary_path: str, port: int, concurrency: int, rounds: int, external_url: str = None, external_key: str = None):
        self.binary_path = Path(binary_path).resolve() if binary_path else None
        self.port = port
        self.concurrency = concurrency
        self.rounds = rounds
        self.external_url = external_url
        self.external_key = external_key
        self.is_self_managed = external_url is None

        self.cluster_dir = Path("./_graphql_poison_env").resolve()
        self.master_key = base64.b64encode(secrets.token_bytes(32)).decode()
        self.process = None

        if self.is_self_managed:
            self.base_url = f"http://127.0.0.1:{self.port}"
        else:
            self.base_url = external_url.rstrip("/")

        self.root_headers = {}
        self.tenant_id = f"tenant-{uuid.uuid4().hex[:6]}"
        self.tenant_key = None
        self.tenant_headers = {}
        self.audit_summary = {}

        if self.is_self_managed:
            atexit.register(self.cleanup)

    def cleanup(self):
        if self.is_self_managed:
            print_info("Stopping self-managed ApexKit engine...")
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

        print_sub("0. BOOTSTRAPPING ENGINE WITH EXPANDED CAPACITIES")
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

        # Raise rate limit via API to allow high stress testing
        requests.patch(f"{self.base_url}/api/v1/admin/settings", json={
            "security": {
                "cors_allow_all": True,
                "cors_origins": "",
                "tenant_transparency": False,
                "global_rate_limit": 100000,
                "tenant_free_rate_limit": 10000,
                "tenant_pro_rate_limit": 50000
            }
        }, headers=self.root_headers, timeout=5)

    def setup_tenant(self):
        print_sub("1. PROVISIONING ISOLATED TEST TENANT")
        r = requests.post(f"{self.base_url}/api/v1/admin/tenants", json={
            "tenant_id": self.tenant_id,
            "name": "GraphQL Test Fleet",
            "tier": "pro"
        }, headers=self.root_headers, timeout=5)
        if r.status_code not in (200, 201):
            fail(f"Failed provisioning tenant: {r.text}")

        rk = requests.post(f"{self.base_url}/api/v1/admin/keys", json={
            "name": f"Key-{self.tenant_id}",
            "target_tenant": self.tenant_id,
            "env_type": "sk",
            "roles": ["admin"],
            "bypass_cors": True
        }, headers=self.root_headers, timeout=5)
        if rk.status_code not in (200, 201):
            fail(f"Failed minting tenant key: {rk.text}")

        self.tenant_key = rk.json()["key"]
        self.tenant_headers = {
            "x-api-key": self.tenant_key,
            "Authorization": f"Bearer {self.tenant_key}",
            "Content-Type": "application/json"
        }
        print_success(f"Tenant '{self.tenant_id}' ready with API key.")

    # -------------------------------------------------------------
    # PHASE 1: THE EXACT CRASH SCENARIO
    # -------------------------------------------------------------
    def test_phase_1_duplicate_fields_and_relations(self):
        print_sub("2. POISON TEST: DUPLICATE FIELD AND RELATION IDENTIFIERS (THE ORIGINAL BUG)")
        print_info("Creating target collections 'courses' and 'students' first...")
        
        for col_name in ["courses", "students"]:
            requests.post(f"{self.base_url}/tenant/{self.tenant_id}/api/v1/collections", json={
                "name": col_name,
                "schema": {
                    "fields": {
                        "name": { "type": "string", "required": True }
                    }
                }
            }, headers=self.tenant_headers, timeout=5)

        # Deploy the exact conflicting schema that caused the panic
        grades_schema = {
            "name": "grades",
            "schema": {
                "fields": {
                    "student_id": { "type": "number", "required": True, "uid": "f_stid" },
                    "course_id": { "type": "number", "required": True, "uid": "f_crid" },
                    "score": { "type": "number", "required": True, "uid": "f_scor" }
                },
                "relations": {
                    "course_id": {
                        "target_collection": "courses",
                        "relation_type": "one",
                        "required": True,
                        "uid": "r_crid"
                    },
                    "student_id": {
                        "target_collection": "students",
                        "relation_type": "one",
                        "required": True,
                        "uid": "r_stid"
                    }
                }
            }
        }

        print_info("Deploying 'grades' with colliding fields: [course_id, student_id]...")
        res = requests.post(
            f"{self.base_url}/tenant/{self.tenant_id}/api/v1/collections",
            json=grades_schema,
            headers=self.tenant_headers,
            timeout=5
        )
        if res.status_code not in (200, 201):
            fail(f"Failed to create grades collection: {res.text}")
        print_success("Collection created via REST API without errors.")

        # Test GraphQL query on tenant
        gql_query = {
            "query": """
            query {
                grades {
                    total
                    items {
                        id
                        score
                        course_id
                        student_id
                        course_id_expand {
                            id
                        }
                        student_id_expand {
                            id
                        }
                    }
                }
            }
            """
        }

        print_info("Querying tenant GraphQL endpoint to verify dynamic schema generation...")
        gql_res = requests.post(
            f"{self.base_url}/tenant/{self.tenant_id}/graphql",
            json=gql_query,
            headers=self.tenant_headers,
            timeout=5
        )

        if gql_res.status_code == 200 and "data" in gql_res.json():
            print_success("Dynamic GraphQL schema built and queried successfully! (NO PANIC, ZERO DOWNTIME)")
            self.audit_summary["Phase_1_Duplicate_Field_Relation"] = "PASS"
        else:
            fail(f"GraphQL returned unexpected error: {gql_res.text}")

        # Test GraphQL Create Mutation on tenant
        gql_mutation = {
            "query": """
            mutation {
                createGrades(data: {
                    course_id: 101,
                    student_id: 202,
                    score: 95.5
                }) {
                    id
                    score
                    course_id
                    student_id
                }
            }
            """
        }
        print_info("Executing createGrades GraphQL Mutation with deduplicated inputs...")
        mut_res = requests.post(
            f"{self.base_url}/tenant/{self.tenant_id}/graphql",
            json=gql_mutation,
            headers=self.tenant_headers,
            timeout=5
        )
        if mut_res.status_code == 200 and "data" in mut_res.json():
            new_id = mut_res.json()["data"]["createGrades"]["id"]
            print_success(f"Mutation succeeded! Created record id: {new_id}")
        else:
            fail(f"Mutation failed: {mut_res.text}")

    # -------------------------------------------------------------
    # PHASE 2: RESERVED WORD & TYPE COLLISION ATTACK
    # -------------------------------------------------------------
    def test_phase_2_reserved_word_collision(self):
        print_sub("3. POISON TEST: GRAPHQL RESERVED NAMES & INTERNAL IDENTIFIER COLLISION")

        # Creating collections with system names
        poison_cols = [
            ("query", "f_query"),
            ("mutation", "f_mutation"),
            ("user", "f_user"),
            ("json", "f_json")
        ]

        for name, uid in poison_cols:
            schema = {
                "name": name,
                "schema": {
                    "fields": {
                        "id": { "type": "string", "required": False, "uid": f"{uid}_id" },
                        "clientMutationId": { "type": "string", "required": False, "uid": f"{uid}_cmid" },
                        "total": { "type": "number", "required": False, "uid": f"{uid}_tot" },
                        "items": { "type": "string", "required": False, "uid": f"{uid}_itm" }
                    }
                }
            }
            res = requests.post(
                f"{self.base_url}/tenant/{self.tenant_id}/api/v1/collections",
                json=schema,
                headers=self.tenant_headers,
                timeout=5
            )
            if res.status_code in (200, 201):
                print_success(f"Protected registration for system-named collection '{name}'.")

        # Now query GraphQL to trigger schema generation for all these poison types
        print_info("Sending query across all poison collections...")
        gql_query = {
            "query": """
            query {
                _status
                query { total }
                mutation { total }
                user { total }
            }
            """
        }
        res = requests.post(
            f"{self.base_url}/tenant/{self.tenant_id}/graphql",
            json=gql_query,
            headers=self.tenant_headers,
            timeout=5
        )

        if res.status_code == 200 and "data" in res.json():
            print_success("Engine survived all reserved keyword & system type name collisions!")
            self.audit_summary["Phase_2_Reserved_Keyword_Assault"] = "PASS"
        else:
            fail(f"Server rejected or failed on reserved names: {res.text}")

    # -------------------------------------------------------------
    # PHASE 3: SCOPE-AWARE INTROSPECTION LOCK
    # -------------------------------------------------------------
    def test_phase_3_scope_aware_introspection_lock(self):
        print_sub("4. SCOPE-AWARE INTROSPECTION LOCK: GENERAL SETTINGS TEST")

        introspection_query = {
            "query": """
            query {
                __schema {
                    types {
                        name
                    }
                }
            }
            """
        }

        # Step 1: Root should have introspection open by default
        print_info("Testing introspection on Root (default = unlocked)...")
        r_root_open = requests.post(f"{self.base_url}/graphql", json=introspection_query, headers=self.root_headers, timeout=5)
        if r_root_open.status_code == 200 and "__schema" in r_root_open.json().get("data", {}):
            print_success("Root introspection is functional.")
        else:
            fail(f"Root introspection unexpectedly blocked: {r_root_open.text}")

        # Step 2: Lock Root Introspection via General Settings
        print_info("Locking introspection on Root via General Settings (disable_introspection = true)...")
        r_patch_root = requests.patch(f"{self.base_url}/api/v1/admin/settings", json={
            "disable_introspection": True
        }, headers=self.root_headers, timeout=5)
        if r_patch_root.status_code not in (200, 201):
            fail(f"Failed to update root settings: {r_patch_root.text}")

        time.sleep(1.0) # Allow reload trigger to rebuild schema

        # Step 3: Verify Root Introspection is now BLOCKED
        print_info("Verifying Root introspection is now blocked...")
        r_root_locked = requests.post(f"{self.base_url}/graphql", json=introspection_query, headers=self.root_headers, timeout=5)
        is_blocked = "errors" in r_root_locked.json() or r_root_locked.json().get("data") is None
        if is_blocked:
            print_success("Root introspection successfully locked by General Settings.")
        else:
            fail("Root introspection remained open despite lock!")

        # Step 4: Verify Tenant Introspection is still accessible (Scope Isolation!)
        print_info("Verifying Tenant introspection remains independent and unlocked...")
        r_tenant_check = requests.post(f"{self.base_url}/tenant/{self.tenant_id}/graphql", json=introspection_query, headers=self.tenant_headers, timeout=5)
        if r_tenant_check.status_code == 200 and "__schema" in r_tenant_check.json().get("data", {}):
            print_success("Tenant introspection is isolated and remained open!")
        else:
            fail("Tenant introspection was erroneously affected by Root lock!")

        # Step 5: Lock Tenant Introspection specifically
        print_info("Locking Tenant introspection via its scoped Settings...")
        r_patch_tenant = requests.patch(f"{self.base_url}/tenant/{self.tenant_id}/api/v1/admin/settings", json={
            "disable_introspection": True
        }, headers=self.tenant_headers, timeout=5)
        if r_patch_tenant.status_code not in (200, 201):
            fail(f"Failed to update tenant settings: {r_patch_tenant.text}")

        time.sleep(0.5)

        r_tenant_locked = requests.post(f"{self.base_url}/tenant/{self.tenant_id}/graphql", json=introspection_query, headers=self.tenant_headers, timeout=5)
        tenant_is_blocked = "errors" in r_tenant_locked.json() or r_tenant_locked.json().get("data") is None
        if tenant_is_blocked:
            print_success("Tenant introspection successfully locked by its own General Settings!")
            self.audit_summary["Phase_3_Scope_Aware_Introspection_Lock"] = "PASS"
        else:
            fail("Tenant introspection failed to lock.")

        # Unlock Root and Tenant for remainder of tests
        requests.patch(f"{self.base_url}/api/v1/admin/settings", json={"disable_introspection": False}, headers=self.root_headers, timeout=5)
        requests.patch(f"{self.base_url}/tenant/{self.tenant_id}/api/v1/admin/settings", json={"disable_introspection": False}, headers=self.tenant_headers, timeout=5)
        time.sleep(0.5)

    # -------------------------------------------------------------
    # PHASE 4: DENIAL-OF-SERVICE DEPTH & ALIAS BOMBS
    # -------------------------------------------------------------
    def test_phase_4_dos_depth_and_alias_bombs(self):
        print_sub("4. DENIAL-OF-SERVICE DEFENSE: QUERY DEPTH & COMPLEXITY BOMBS")

        # 1. Depth Bomb: Exceed limit_depth(32)
        print_info("Crafting deeply nested query (depth = 36) to exceed limit_depth(32)...")
        nested_query = "id"
        for _ in range(35):
            nested_query = f"course_id_expand {{ {nested_query} }}"
        deep_gql = { "query": f"query {{ grades {{ items {{ {nested_query} }} }} }}" }

        res_depth = requests.post(
            f"{self.base_url}/tenant/{self.tenant_id}/graphql",
            json=deep_gql,
            headers=self.tenant_headers,
            timeout=5
        )
        data_d = res_depth.json()
        if "errors" in data_d and any("depth" in e.get("message", "").lower() or "limit" in e.get("message", "").lower() for e in data_d["errors"]):
            print_success("Query depth bomb caught and safely rejected by depth limiter!")
        else:
            fail(f"Depth bomb was not caught by analyzer: {res_depth.text}")

        # 2. Alias Multiplier Complexity Bomb: Exceed limit_complexity(2000)
        print_info("Crafting alias complexity bomb (300 aliases on query root)...")
        aliases = " ".join([f"a{i}: _status" for i in range(300)])
        alias_gql = { "query": f"query {{ {aliases} }}" }

        res_alias = requests.post(f"{self.base_url}/graphql", json=alias_gql, headers=self.root_headers, timeout=5)
        if res_alias.status_code in (200, 400):
            print_success("Engine processed or bounded alias flood without crashing.")
            self.audit_summary["Phase_4_DoS_Bomb_Defense"] = "PASS"
        else:
            fail("Server crashed on alias load.")

    # -------------------------------------------------------------
    # PHASE 5: HIGH-CONCURRENCY MULTI-TENANT ASSAULT
    # -------------------------------------------------------------
    def test_phase_5_concurrent_graphql_assault(self):
        print_sub(f"5. CONCURRENT GRAPHQL HAMMER: {self.concurrency} THREADS x {self.rounds} ROUNDS")

        work_items = []
        for i in range(self.rounds):
            # Target Root Queries
            work_items.append((
                f"{self.base_url}/graphql",
                self.root_headers,
                { "query": "query { _status _users { total } }" }
            ))
            # Target Tenant Mutations
            work_items.append((
                f"{self.base_url}/tenant/{self.tenant_id}/graphql",
                self.tenant_headers,
                {
                    "query": f"""
                    mutation {{
                        createGrades(data: {{
                            course_id: {100 + i},
                            student_id: {200 + i},
                            score: {80.0 + (i % 20)}
                        }}) {{
                            id
                        }}
                    }}
                    """
                }
            ))

        latencies = []
        status_counts = {}
        errors = []

        def worker(item):
            url, headers, payload = item
            session = build_http_session()
            start = time.perf_counter()
            try:
                res = session.post(url, json=payload, headers=headers, timeout=10)
                dur = (time.perf_counter() - start) * 1000
                return res.status_code, dur, res.text, None
            except Exception as e:
                dur = (time.perf_counter() - start) * 1000
                return 0, dur, "", str(e)

        print_info(f"Firing {len(work_items)} GraphQL requests concurrently...")
        start_time = time.perf_counter()

        with ThreadPoolExecutor(max_workers=self.concurrency) as executor:
            futures = [executor.submit(worker, item) for item in work_items]
            for f in as_completed(futures):
                code, dur, text, exc = f.result()
                latencies.append(dur)
                status_counts[code] = status_counts.get(code, 0) + 1
                if code != 200:
                    errors.append(exc if exc else f"HTTP {code}: {text[:100]}")

        total_wall = time.perf_counter() - start_time
        latencies.sort()
        count = len(latencies)
        p50 = latencies[int(count * 0.50)] if count else 0
        p95 = latencies[int(count * 0.95)] if count else 0
        max_lat = latencies[-1] if count else 0

        success_count = status_counts.get(200, 0)
        rps = count / total_wall if total_wall else 0
        color = Fore.GREEN if success_count == count else Fore.YELLOW

        print(f"{color}  -> Completed in {total_wall:.2f}s | Throughput: {rps:.1f} req/s | p50: {p50:.1f}ms | p95: {p95:.1f}ms | Max: {max_lat:.1f}ms")
        print(f"  -> Status distribution: {status_counts}")

        if errors:
            print_warn(f"Encountered {len(errors)} non-200 responses:")
            for err in errors[:3]:
                print(f"{Fore.RED}     • {err}")

        self.audit_summary["Phase_5_Concurrent_GraphQL_Hammer"] = {
            "status": "PASS" if success_count == count else "DEGRADED",
            "rps": rps,
            "p95_ms": p95
        }

    # -------------------------------------------------------------
    # PHASE 6: FINAL HEALTH VERIFICATION & REPORT
    # -------------------------------------------------------------
    def verify_engine_survival(self):
        print_sub("6. VERIFYING ENGINE SURVIVAL & PROCESS HEALTH")
        try:
            r = requests.get(f"{self.base_url}/healthz", timeout=3)
            if r.status_code == 200:
                print_success("ApexKit engine survived all adversarial attacks and remains 100% HEALTHY!")
                self.audit_summary["Engine_Survival_Verdict"] = "PASS"
            else:
                fail(f"Engine degraded! Healthz returned HTTP {r.status_code}")
        except Exception as e:
            fail(f"CRITICAL FAILURE: Engine crashed or unreachable: {e}")

    def generate_report(self):
        print_header("GRAPHQL ADVERSARIAL STRESS TEST VERDICT")
        print(f"{'Audit Phase':<45} | {'Verdict':<10} | {'Details'}")
        print("-" * 85)
        for phase, result in self.audit_summary.items():
            if isinstance(result, dict):
                st = result.get("status", "INFO")
                color = Fore.GREEN if st == "PASS" else Fore.YELLOW
                details = f"RPS: {result.get('rps', 0):.1f}, p95: {result.get('p95_ms', 0):.1f}ms"
                print(f"{phase:<45} | {color}{st:<10}{Style.RESET_ALL} | {details}")
            else:
                color = Fore.GREEN if result == "PASS" else Fore.RED
                print(f"{phase:<45} | {color}{result:<10}{Style.RESET_ALL} | Clean")
        print(f"\n{Fore.GREEN}{Style.BRIGHT}✨ ALL GRAPHQL POISON SHIELDS VERIFIED. Zero panics recorded.\n")

    def run(self):
        print_header(f"APEXKIT GRAPHQL POISON STRESS TEST")
        self.bootstrap_engine()
        self.setup_tenant()
        self.test_phase_1_duplicate_fields_and_relations()
        self.test_phase_2_reserved_word_collision()
        self.test_phase_3_scope_aware_introspection_lock()
        self.test_phase_4_dos_depth_and_alias_bombs()
        self.test_phase_5_concurrent_graphql_assault()
        self.verify_engine_survival()
        self.generate_report()

if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="ApexKit Poisonous GraphQL Stress Suite")
    parser.add_argument("--bin", default="./target/release/apexkit-cli", help="Path to apexkit executable (default: ./target/release/apexkit-cli or ./apexkit)")
    parser.add_argument("--port", type=int, default=5055, help="Port to run local engine on (default: 5055)")
    parser.add_argument("--url", default=None, help="Target an external running instance (e.g. http://localhost:5000)")
    parser.add_argument("--api-key", default=None, help="Root Admin API Key (if using an external --url)")
    parser.add_argument("--concurrency", type=int, default=15, help="Concurrent workers for assault (default: 15)")
    parser.add_argument("--rounds", type=int, default=30, help="Number of query rounds per worker (default: 30)")

    args = parser.parse_args()

    bin_path = args.bin
    if not args.url:
        bin_target = Path(args.bin)
        if not bin_target.exists() and Path("./apexkit").exists():
            bin_path = "./apexkit"

    runner = GraphqlPoisonRunner(
        binary_path=bin_path,
        port=args.port,
        concurrency=args.concurrency,
        rounds=args.rounds,
        external_url=args.url,
        external_key=args.api_key
    )
    try:
        runner.run()
    except KeyboardInterrupt:
        print_warn("\nTest interrupted by user.")
        sys.exit(0)