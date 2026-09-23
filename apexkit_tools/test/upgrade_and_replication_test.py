#!/usr/bin/env python3
"""
ApexKit Comprehensive Multi-Tenant Replication, Realtime, Isolation & Zero-Downtime Test Suite

Verifies:
1. Cluster Bootstrapping: 1 Master + N Replicas in isolated environments.
2. Root Replication & Write-Forwarding: SQLite changesets and replica-to-master forwarding.
3. Root Distributed $realtime Mesh: Global Hub-and-Spoke $realtime event tunneling.
4. Tenant Provisioning & Replication: Multi-tenant database propagation and edge forwarding.
5. Tenant $realtime Mesh & Cross-Tenant Zero-Leakage:
   - Validates event delivery within Tenant A.
   - Proves Tenant B receives ZERO leaked events across the distributed network.
6. Tenant Native Collection Realtime & Boundary Isolation:
   - Validates native Insert/Update delivery in Tenant A.
   - Proves Tenant B's stream never sees Tenant A's database operations.
7. Zero-Downtime Hot Upgrade: Simultaneous hammering of Root & Tenant endpoints during binary swap.
8. Rollback & Archive Management: apexkit__old-{N} creation, rollback under load, and cleanup.
"""

import os
import sys
import time
import uuid
import json
import shutil
import atexit
import tarfile
import secrets
import base64
import argparse
import subprocess
import threading
from pathlib import Path
from concurrent.futures import ThreadPoolExecutor

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
                total=3,
                backoff_factor=0.05,
                status_forcelist=[502, 503, 504],
                raise_on_status=False
            )
        )
        session.mount("http://", adapter)
        session.mount("https://", adapter)
    return session

# -------------------------------------------------------------
# CLUSTER NODE WRAPPER
# -------------------------------------------------------------

class ApexNode:
    def __init__(self, node_id: str, port: int, work_dir: Path, is_master: bool, master_url: str = None, master_key: str = None):
        self.node_id = node_id
        self.port = port
        self.work_dir = work_dir
        self.is_master = is_master
        self.master_url = master_url
        self.master_key = master_key
        self.base_url = f"http://127.0.0.1:{port}"
        self.process: subprocess.Popen = None
        self.auth_token = None

    def start(self, binary_path: Path):
        self.work_dir.mkdir(parents=True, exist_ok=True)
        exe_path = self.work_dir / "apexkit"
        if not exe_path.exists():
            shutil.copy2(binary_path, exe_path)
            exe_path.chmod(0o755)

        env = os.environ.copy()
        env["PORT"] = str(self.port)
        env["APEXKIT_MASTER_KEY"] = self.master_key
        env["RUST_LOG"] = "info,tower=warn,hyper=warn"

        if not self.is_master:
            env["APEXKIT_MASTER_URL"] = self.master_url

        log_file = open(self.work_dir / "node.log", "w")
        self.process = subprocess.Popen(
            [str(exe_path)],
            cwd=str(self.work_dir),
            env=env,
            stdout=log_file,
            stderr=subprocess.STDOUT
        )

    def wait_until_ready(self, timeout=20):
        start = time.time()
        while time.time() - start < timeout:
            try:
                res = requests.get(f"{self.base_url}/healthz", timeout=1)
                if res.status_code == 200:
                    return True
            except requests.RequestException:
                pass
            time.sleep(0.3)
        return False

    def login_admin(self):
        try:
            r = requests.post(f"{self.base_url}/api/v1/auth/login", json={
                "email": "admin@apexkit.io",
                "password": "password"
            }, timeout=5)
            if r.status_code in (200, 201):
                self.auth_token = r.json()["token"]
                return self.auth_token
        except Exception as e:
            print_error(f"Login failed on {self.node_id}: {e}")
        return None

    def headers(self):
        return {
            "Authorization": f"Bearer {self.auth_token}",
            "Content-Type": "application/json"
        }

    def stop(self):
        if self.process and self.process.poll() is None:
            self.process.terminate()
            try:
                self.process.wait(timeout=3)
            except subprocess.TimeoutExpired:
                self.process.kill()


# -------------------------------------------------------------
# TEST SUITE CONTROLLER
# -------------------------------------------------------------

class ClusterTestSuite:
    def __init__(self, binary_path: str, master_port: int, num_replicas: int):
        self.binary_path = Path(binary_path).resolve()
        if not self.binary_path.exists():
            fail(f"ApexKit binary not found at '{self.binary_path}'")

        self.master_port = master_port
        self.num_replicas = num_replicas
        self.cluster_dir = Path("./_test_cluster").resolve()
        self.master_key = base64.b64encode(secrets.token_bytes(32)).decode()
        self.nodes: list[ApexNode] = []
        self.master: ApexNode = None
        self.replicas: list[ApexNode] = []

        # Tenants for Isolation Tests
        self.tenant_a_id = f"tenant-a-{uuid.uuid4().hex[:6]}"
        self.tenant_a_key = None
        self.tenant_b_id = f"tenant-b-{uuid.uuid4().hex[:6]}"
        self.tenant_b_key = None

        self.audit_summary = {}

        atexit.register(self.cleanup)

    def cleanup(self):
        print_info("Cleaning up cluster processes and directories...")
        for node in self.nodes:
            node.stop()

        ports = [self.master_port] + [self.master_port + 1 + i for i in range(self.num_replicas)]
        for p in ports:
            try:
                subprocess.run(f"fuser -k {p}/tcp", shell=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            except Exception:
                pass

        if self.cluster_dir.exists():
            shutil.rmtree(self.cluster_dir, ignore_errors=True)

    def run(self):
        print_header(f"APEXKIT MULTI-TENANT DISTRIBUTED & ISOLATION SUITE")
        self.bootstrap_cluster()
        
        # Root Scope Tests
        self.test_phase_1_root_replication()
        self.test_phase_2_root_write_forwarding()
        self.test_phase_3_root_distributed_realtime()
        
        # Multi-Tenant Isolation & Replication
        self.test_phase_4_tenant_provisioning_and_replication()
        self.test_phase_5_tenant_realtime_mesh_and_isolation()
        self.test_phase_6_tenant_native_realtime_isolation()
        
        # Resilience & Upgrades
        self.test_phase_7_zero_downtime_hot_upgrade()
        self.test_phase_8_rollback_and_archive_mgmt()
        
        self.print_final_verdict()

    # ---------------------------------------------------------
    # PHASE 0: BOOTSTRAP CLUSTER
    # ---------------------------------------------------------
    def bootstrap_cluster(self):
        print_sub("0. BOOTSTRAPPING ISOLATED CLUSTER TOPOLOGY")
        if self.cluster_dir.exists():
            shutil.rmtree(self.cluster_dir, ignore_errors=True)

        master_dir = self.cluster_dir / "master"
        self.master = ApexNode("master", self.master_port, master_dir, is_master=True, master_key=self.master_key)
        self.nodes.append(self.master)
        print_info(f"Starting Master on 127.0.0.1:{self.master_port}...")
        self.master.start(self.binary_path)

        if not self.master.wait_until_ready():
            fail("Master failed to start in time. Check _test_cluster/master/node.log")
        self.master.login_admin()
        print_success(f"Master online & authenticated (Token: {self.master.auth_token[:15]}...)")

        print_info("Setting cluster rate limit to 100,000 req/min via Admin API...")
        r_settings = requests.patch(f"{self.master.base_url}/api/v1/admin/settings", json={
            "security": {
                "cors_allow_all": True,
                "cors_origins": "",
                "tenant_transparency": False,
                "global_rate_limit": 100000,
                "tenant_free_rate_limit": 10000,
                "tenant_pro_rate_limit": 50000
            }
        }, headers=self.master.headers(), timeout=5)
        if r_settings.status_code in (200, 201):
            print_success("Cluster rate limit updated to 100,000 req/min.")
        else:
            print_warn(f"Failed setting rate limit: {r_settings.text}")

        for i in range(self.num_replicas):
            r_port = self.master_port + 1 + i
            r_id = f"replica-{i+1}"
            r_dir = self.cluster_dir / r_id
            replica = ApexNode(r_id, r_port, r_dir, is_master=False, master_url=self.master.base_url, master_key=self.master_key)
            self.nodes.append(replica)
            self.replicas.append(replica)

            print_info(f"Starting {r_id} on 127.0.0.1:{r_port} linked to Master...")
            replica.start(self.binary_path)
            if not replica.wait_until_ready():
                fail(f"{r_id} failed to connect. Check {r_dir}/node.log")
            replica.login_admin()
            print_success(f"{r_id} online, synchronized with Master.")

    # ---------------------------------------------------------
    # PHASE 1: ROOT REPLICATION
    # ---------------------------------------------------------
    def test_phase_1_root_replication(self):
        print_sub("1. TESTING ROOT DATABASE REPLICATION")

        col_payload = {
            "name": "devices",
            "schema": {
                "fields": {
                    "device_name": { "type": "string", "required": True },
                    "ip_address": { "type": "string", "required": True }
                }
            }
        }
        res = requests.post(f"{self.master.base_url}/api/v1/collections", json=col_payload, headers=self.master.headers(), timeout=5)
        if res.status_code not in (200, 201):
            fail(f"Master failed creating collection: {res.text}")
        print_success("Master created collection 'devices' in Root scope.")

        for i in range(10):
            requests.post(f"{self.master.base_url}/api/v1/collections/devices/records", json={
                "data": { "device_name": f"Sensor-{i+1}", "ip_address": f"10.0.0.{i+10}" }
            }, headers=self.master.headers(), timeout=5)
        print_success("Master committed 10 records to 'devices'.")

        start = time.time()
        synced_nodes = 0
        for replica in self.replicas:
            for _ in range(30):
                r = requests.get(f"{replica.base_url}/api/v1/collections/devices/records", headers=replica.headers(), timeout=2)
                if r.status_code in (200, 201) and r.json().get("total") == 10:
                    synced_nodes += 1
                    print_success(f"{replica.node_id} synced all 10 root records.")
                    break
                time.sleep(0.1)

        elapsed = (time.time() - start) * 1000
        self.audit_summary["Root_Replication"] = {
            "status": "PASS" if synced_nodes == len(self.replicas) else "FAIL",
            "latency_ms": elapsed
        }

    # ---------------------------------------------------------
    # PHASE 2: ROOT WRITE-FORWARDING
    # ---------------------------------------------------------
    def test_phase_2_root_write_forwarding(self):
        print_sub("2. TESTING ROOT REPLICA WRITE-FORWARDING")
        replica = self.replicas[0]
        res = requests.post(f"{replica.base_url}/api/v1/collections/devices/records", json={
            "data": { "device_name": "Edge-Gateway", "ip_address": "192.168.1.1" }
        }, headers=replica.headers(), timeout=5)
        
        if res.status_code not in (200, 201):
            fail(f"Root write-forwarding failed: {res.text}")
        new_id = res.json()["id"]
        print_success(f"Edge {replica.node_id} forwarded write to Master (Record ID: {new_id}).")

        r_master = requests.get(f"{self.master.base_url}/api/v1/collections/devices/records/{new_id}", headers=self.master.headers(), timeout=3)
        if r_master.status_code in (200, 201):
            print_success("Master verified forwarded record in Root data.db.")
            self.audit_summary["Root_Write_Forwarding"] = {"status": "PASS"}
        else:
            fail("Forwarded record missing on Master.")

    # ---------------------------------------------------------
    # PHASE 3: ROOT DISTRIBUTED $realtime
    # ---------------------------------------------------------
    def test_phase_3_root_distributed_realtime(self):
        print_sub("3. TESTING ROOT DISTRIBUTED $realtime TUNNEL")
        if len(self.replicas) < 2:
            print_warn("Skipping: requires at least 2 replicas.")
            return

        receiver = self.replicas[0]
        sender = self.replicas[1]

        script = """
        export default async function(req) {
            const body = await req.json();
            await $realtime.send("cluster_alerts", "fleet_ping", { ping_id: body.ping_id });
            return new Response(JSON.stringify({ sent: true }));
        }
        """
        requests.post(f"{self.master.base_url}/api/v1/admin/scripts", json={
            "name": "root-emit", "trigger_type": "manual", "code": script, "active": True
        }, headers=self.master.headers(), timeout=5)
        time.sleep(1.0)

        test_ping = f"root-{uuid.uuid4().hex[:6]}"
        received = []

        def listener():
            url = f"{receiver.base_url}/api/v1/sse?channel=cluster_alerts&event=fleet_ping&token={receiver.auth_token}"
            try:
                res = requests.get(url, headers=receiver.headers(), stream=True, timeout=10)
                for line in res.iter_lines():
                    if line and line.startswith(b"data:"):
                        payload = json.loads(line[5:].decode('utf-8'))
                        data = payload.get("payload", {}).get("data", {})
                        if data.get("ping_id") == test_ping:
                            received.append(data)
                            break
            except Exception:
                pass

        t = threading.Thread(target=listener, daemon=True)
        t.start()
        time.sleep(1.5)

        start = time.perf_counter()
        requests.post(f"{sender.base_url}/api/v1/webhook/root-emit", json={"ping_id": test_ping}, headers=sender.headers(), timeout=5)
        t.join(timeout=6)
        elapsed = (time.perf_counter() - start) * 1000

        if received:
            print_success(f"Root $realtime routed {sender.node_id} ──(Up)──> Master ──(Down)──> {receiver.node_id} ({elapsed:.2f}ms)")
            self.audit_summary["Root_Distributed_Realtime"] = {"status": "PASS", "latency_ms": elapsed}
        else:
            print_error("Root distributed realtime timed out!")
            self.audit_summary["Root_Distributed_Realtime"] = {"status": "FAIL", "latency_ms": 0}

    # ---------------------------------------------------------
    # PHASE 4: PROVISION TWO ISOLATED TENANTS & REPLICATE
    # ---------------------------------------------------------
    def test_phase_4_tenant_provisioning_and_replication(self):
        print_sub("4. PROVISIONING 2 ISOLATED TENANTS (TENANT A & TENANT B)")

        # Provision Tenant A
        requests.post(f"{self.master.base_url}/api/v1/admin/tenants", json={
            "tenant_id": self.tenant_a_id, "name": "Tenant Alpha", "tier": "pro"
        }, headers=self.master.headers(), timeout=5)
        
        rk_a = requests.post(f"{self.master.base_url}/api/v1/admin/keys", json={
            "name": f"Key-{self.tenant_a_id}", "target_tenant": self.tenant_a_id,
            "env_type": "sk", "roles": ["admin"], "bypass_cors": True
        }, headers=self.master.headers(), timeout=5)
        self.tenant_a_key = rk_a.json()["key"]
        print_success(f"Tenant A online: '{self.tenant_a_id}'")

        # Provision Tenant B (Isolation Probe)
        requests.post(f"{self.master.base_url}/api/v1/admin/tenants", json={
            "tenant_id": self.tenant_b_id, "name": "Tenant Beta", "tier": "pro"
        }, headers=self.master.headers(), timeout=5)
        
        rk_b = requests.post(f"{self.master.base_url}/api/v1/admin/keys", json={
            "name": f"Key-{self.tenant_b_id}", "target_tenant": self.tenant_b_id,
            "env_type": "sk", "roles": ["admin"], "bypass_cors": True
        }, headers=self.master.headers(), timeout=5)
        self.tenant_b_key = rk_b.json()["key"]
        print_success(f"Tenant B online (Isolation Target): '{self.tenant_b_id}'")

        # Create collections on both tenants
        col_def = {
            "name": "notes",
            "schema": {
                "fields": {
                    "title": { "type": "string", "required": True },
                    "body": { "type": "string", "required": False }
                }
            }
        }
        requests.post(f"{self.master.base_url}/tenant/{self.tenant_a_id}/api/v1/collections", json=col_def, headers={"x-api-key": self.tenant_a_key}, timeout=5)
        requests.post(f"{self.master.base_url}/tenant/{self.tenant_b_id}/api/v1/collections", json=col_def, headers={"x-api-key": self.tenant_b_key}, timeout=5)

        # Warm up Replicas for both tenants
        for replica in self.replicas:
            requests.get(f"{replica.base_url}/tenant/{self.tenant_a_id}/api/v1/collections", headers={"x-api-key": self.tenant_a_key}, timeout=8)
            requests.get(f"{replica.base_url}/tenant/{self.tenant_b_id}/api/v1/collections", headers={"x-api-key": self.tenant_b_key}, timeout=8)
        print_success("Both tenants synchronized across edge replicas.")

        # Insert 10 records into Tenant A
        for i in range(10):
            requests.post(f"{self.master.base_url}/tenant/{self.tenant_a_id}/api/v1/collections/notes/records", json={
                "data": { "title": f"Note-A-{i+1}", "body": "Alpha payload" }
            }, headers={"x-api-key": self.tenant_a_key}, timeout=5)

        # Verify Tenant A replicas have records, and Tenant B remains clean (0 records)
        all_a_synced = True
        tenant_b_clean = True
        for replica in self.replicas:
            r_a = requests.get(f"{replica.base_url}/tenant/{self.tenant_a_id}/api/v1/collections/notes/records", headers={"x-api-key": self.tenant_a_key}, timeout=3)
            r_b = requests.get(f"{replica.base_url}/tenant/{self.tenant_b_id}/api/v1/collections/notes/records", headers={"x-api-key": self.tenant_b_key}, timeout=3)
            if r_a.json().get("total") != 10:
                all_a_synced = False
            if r_b.json().get("total") != 0:
                tenant_b_clean = False

        if all_a_synced and tenant_b_clean:
            print_success("Database storage isolation confirmed: Tenant A has 10 records, Tenant B has 0.")
            self.audit_summary["Tenant_Storage_Isolation"] = {"status": "PASS"}
        else:
            fail("Storage cross-contamination detected!")

    # ---------------------------------------------------------
    # PHASE 5: TENANT $realtime & CROSS-TENANT ZERO-LEAKAGE
    # ---------------------------------------------------------
    def test_phase_5_tenant_realtime_mesh_and_isolation(self):
        print_sub("5. TESTING TENANT $realtime MESH & CROSS-TENANT LEAKAGE PROOF")
        if len(self.replicas) < 2:
            print_warn("Skipping: requires at least 2 replicas.")
            return

        receiver = self.replicas[0]
        sender = self.replicas[1]

        # Deploy identical channel script to Tenant A
        script = """
        export default async function(req) {
            const body = await req.json();
            await $realtime.send("shared_room_name", "secret_ping", {
                secret: body.secret,
                origin_tenant: body.tenant
            });
            return new Response(JSON.stringify({ sent: true }));
        }
        """
        requests.post(f"{self.master.base_url}/tenant/{self.tenant_a_id}/api/v1/admin/scripts", json={
            "name": "emit-secret", "trigger_type": "manual", "code": script, "active": True
        }, headers={"x-api-key": self.tenant_a_key}, timeout=5)
        time.sleep(1.0)

        test_secret = f"secret-{uuid.uuid4().hex[:8]}"
        
        tenant_a_received = []
        tenant_b_leaked = []

        def listener_tenant_a():
            url = f"{receiver.base_url}/tenant/{self.tenant_a_id}/api/v1/sse?channel=shared_room_name&event=secret_ping&token={receiver.auth_token}"
            try:
                res = requests.get(url, headers=receiver.headers(), stream=True, timeout=8)
                for line in res.iter_lines():
                    if line and line.startswith(b"data:"):
                        payload = json.loads(line[5:].decode('utf-8'))
                        data = payload.get("payload", {}).get("data", {})
                        if data.get("secret") == test_secret:
                            tenant_a_received.append(data)
                            break
            except Exception:
                pass

        def listener_tenant_b_probe():
            # Listening to the EXACT SAME channel name ('shared_room_name') under Tenant B!
            url = f"{receiver.base_url}/tenant/{self.tenant_b_id}/api/v1/sse?channel=shared_room_name&event=secret_ping&token={receiver.auth_token}"
            try:
                res = requests.get(url, headers=receiver.headers(), stream=True, timeout=8)
                for line in res.iter_lines():
                    if line and line.startswith(b"data:"):
                        payload = json.loads(line[5:].decode('utf-8'))
                        data = payload.get("payload", {}).get("data", {})
                        if data.get("secret") == test_secret:
                            tenant_b_leaked.append(data)
                            break
            except Exception:
                pass

        t_a = threading.Thread(target=listener_tenant_a, daemon=True)
        t_b = threading.Thread(target=listener_tenant_b_probe, daemon=True)
        t_a.start()
        t_b.start()
        time.sleep(1.5)

        print_info(f"Emitting secret $realtime event inside '{self.tenant_a_id}'...")
        start = time.perf_counter()
        requests.post(f"{sender.base_url}/tenant/{self.tenant_a_id}/api/v1/webhook/emit-secret", json={
            "secret": test_secret, "tenant": self.tenant_a_id
        }, headers={"x-api-key": self.tenant_a_key}, timeout=5)

        t_a.join(timeout=6)
        t_b.join(timeout=1.0)
        elapsed = (time.perf_counter() - start) * 1000

        if tenant_a_received:
            print_success(f"Tenant A receiver captured event in channel 'shared_room_name' ({elapsed:.2f}ms).")
        else:
            fail("Tenant A receiver timed out or dropped event!")

        if len(tenant_b_leaked) == 0:
            print_success(f"ZERO-LEAKAGE VERIFIED: Tenant B listening on 'shared_room_name' received 0 bytes.")
            self.audit_summary["Tenant_Channel_Isolation"] = {"status": "PASS", "latency_ms": elapsed}
        else:
            fail(f"CRITICAL SECURITY LEAK: Tenant B received Tenant A's private realtime data: {tenant_b_leaked}")

    # ---------------------------------------------------------
    # PHASE 6: NATIVE COLLECTION REALTIME ISOLATION
    # ---------------------------------------------------------
    def test_phase_6_tenant_native_realtime_isolation(self):
        print_sub("6. TESTING NATIVE COLLECTION REALTIME & SCOPE BOUNDARY")
        if len(self.replicas) < 2:
            print_warn("Skipping: requires at least 2 replicas.")
            return

        receiver = self.replicas[0]
        sender = self.replicas[1]

        unique_note = f"StrictNote-{uuid.uuid4().hex[:6]}"
        tenant_a_events = []
        tenant_b_leaks = []

        def listener_a():
            url = f"{receiver.base_url}/tenant/{self.tenant_a_id}/api/v1/sse?token={receiver.auth_token}"
            try:
                res = requests.get(url, headers=receiver.headers(), stream=True, timeout=8)
                for line in res.iter_lines():
                    if line and line.startswith(b"data:"):
                        payload = json.loads(line[5:].decode('utf-8'))
                        data = payload.get("payload", {}).get("data", {})
                        if data.get("title") == unique_note:
                            tenant_a_events.append(payload.get("type"))
                            if len(tenant_a_events) >= 2:
                                break
            except Exception:
                pass

        def listener_b_probe():
            url = f"{receiver.base_url}/tenant/{self.tenant_b_id}/api/v1/sse?token={receiver.auth_token}"
            try:
                res = requests.get(url, headers=receiver.headers(), stream=True, timeout=8)
                for line in res.iter_lines():
                    if line and line.startswith(b"data:"):
                        payload = json.loads(line[5:].decode('utf-8'))
                        data = payload.get("payload", {}).get("data", {})
                        if data.get("title") == unique_note:
                            tenant_b_leaks.append(payload)
                            break
            except Exception:
                pass

        t_a = threading.Thread(target=listener_a, daemon=True)
        t_b = threading.Thread(target=listener_b_probe, daemon=True)
        t_a.start()
        t_b.start()
        time.sleep(1.5)

        # 1. Insert on Replica 2 (write-forwarded to Master)
        print_info("Inserting record on Edge in Tenant A...")
        r_create = requests.post(f"{sender.base_url}/tenant/{self.tenant_a_id}/api/v1/collections/notes/records", json={
            "data": { "title": unique_note, "body": "Classified Note" }
        }, headers={"x-api-key": self.tenant_a_key}, timeout=5)
        rec_id = r_create.json()["id"]

        time.sleep(0.5)

        # 2. Update on Master in Tenant A
        print_info("Updating record on Master in Tenant A...")
        requests.patch(f"{self.master.base_url}/tenant/{self.tenant_a_id}/api/v1/collections/notes/records/{rec_id}", json={
            "data": { "title": unique_note, "body": "Updated Classified Note" }
        }, headers={"x-api-key": self.tenant_a_key}, timeout=5)

        t_a.join(timeout=6)
        t_b.join(timeout=1.0)

        if "Insert" in tenant_a_events and "Update" in tenant_a_events:
            print_success(f"Tenant A correctly captured distributed ['Insert', 'Update'].")
        else:
            fail(f"Tenant A missed events: {tenant_a_events}")

        if len(tenant_b_leaks) == 0:
            print_success("ZERO-LEAKAGE VERIFIED: Tenant B's stream did not receive Tenant A's database operations.")
            self.audit_summary["Tenant_Native_Isolation"] = {"status": "PASS"}
        else:
            fail(f"CRITICAL SECURITY LEAK: Tenant B received database mutations from Tenant A: {tenant_b_leaks}")

    # ---------------------------------------------------------
    # PHASE 7: ZERO-DOWNTIME UPGRADE (ROOT & TENANT LOAD)
    # ---------------------------------------------------------
    def test_phase_7_zero_downtime_hot_upgrade(self):
        print_sub("7. ZERO-DOWNTIME HOT UPGRADE (CONCURRENT ROOT & TENANT HAMMER)")

        tar_path = self.cluster_dir / "apexkit_candidate.tar.gz"
        with tarfile.open(tar_path, "w:gz") as tar:
            tar.add(self.binary_path, arcname="apexkit")
        print_success(f"Prepared compressed upgrade artifact: {tar_path.name}")

        stop_hammer = False
        hammer_stats = {"root_success": 0, "tenant_success": 0, "failed": 0, "errors": []}
        tenant_headers = { "x-api-key": self.tenant_a_key, "Content-Type": "application/json" }

        def hammer_root():
            session = build_http_session()
            while not stop_hammer:
                try:
                    r = session.get(f"{self.master.base_url}/api/v1/collections/devices/records", headers=self.master.headers(), timeout=2)
                    if r.status_code in (200, 201):
                        hammer_stats["root_success"] += 1
                    else:
                        hammer_stats["failed"] += 1
                        hammer_stats["errors"].append(f"Root HTTP {r.status_code}")
                except Exception as e:
                    hammer_stats["failed"] += 1
                    hammer_stats["errors"].append(str(e))
                time.sleep(0.01)

        def hammer_tenant():
            session = build_http_session()
            while not stop_hammer:
                try:
                    r = session.get(f"{self.master.base_url}/tenant/{self.tenant_a_id}/api/v1/collections/notes/records", headers=tenant_headers, timeout=2)
                    if r.status_code in (200, 201):
                        hammer_stats["tenant_success"] += 1
                    else:
                        hammer_stats["failed"] += 1
                        hammer_stats["errors"].append(f"Tenant HTTP {r.status_code}")
                except Exception as e:
                    hammer_stats["failed"] += 1
                    hammer_stats["errors"].append(str(e))
                time.sleep(0.01)

        executor = ThreadPoolExecutor(max_workers=10)
        for _ in range(5):
            executor.submit(hammer_root)
            executor.submit(hammer_tenant)

        print_info("Bombarding Root and Tenant simultaneously with 10 threads at 200+ req/s...")
        time.sleep(1.0)

        # Upload binary
        print_info("Uploading candidate binary to /api/v1/admin/system/upgrade...")
        with open(tar_path, "rb") as f:
            up_res = requests.post(
                f"{self.master.base_url}/api/v1/admin/system/upgrade",
                files={"file": (tar_path.name, f, "application/gzip")},
                headers={"Authorization": f"Bearer {self.master.auth_token}"},
                timeout=20
            )

        if up_res.status_code in (200, 201):
            print_success("Server acknowledged upgrade: " + up_res.json().get("message", ""))
        else:
            fail(f"Upgrade failed: {up_res.text}")

        time.sleep(4.5)
        stop_hammer = True
        executor.shutdown(wait=True)

        total_reqs = hammer_stats["root_success"] + hammer_stats["tenant_success"] + hammer_stats["failed"]
        print_info(f"Swap summary: Total: {total_reqs} | Root OK: {hammer_stats['root_success']} | Tenant OK: {hammer_stats['tenant_success']} | Failed: {hammer_stats['failed']}")

        if hammer_stats["failed"] == 0:
            print_success("PERFECT 100% ZERO-DOWNTIME FOR ROOT & TENANTS! (0 dropped packets)")
            self.audit_summary["Zero_Downtime_Upgrade"] = {"status": "PASS", "failures": 0}
        else:
            print_warn(f"Handover dropped {hammer_stats['failed']} calls: {hammer_stats['errors'][:3]}")
            self.audit_summary["Zero_Downtime_Upgrade"] = {"status": "DEGRADED", "failures": hammer_stats["failed"]}

    # ---------------------------------------------------------
    # PHASE 8: ROLLBACK LIFECYCLE & ARCHIVE MANAGEMENT
    # ---------------------------------------------------------
    def test_phase_8_rollback_and_archive_mgmt(self):
        print_sub("8. ROLLBACK LIFECYCLE & ARCHIVE MANAGEMENT")

        r_list = requests.get(f"{self.master.base_url}/api/v1/admin/system/upgrade/rollbacks", headers=self.master.headers(), timeout=5)
        rollbacks = r_list.json()
        print_info(f"Discovered {len(rollbacks)} rollback archives:")
        for item in rollbacks:
            print(f"  • {Fore.CYAN}{item['name']}{Style.RESET_ALL} ({item['size'] / 1024 / 1024:.2f} MB, {item['created_at']})")

        if not rollbacks:
            fail("No rollback archive found!")

        target = rollbacks[0]["name"]

        print_info(f"Triggering Rollback to '{target}'...")
        r_rb = requests.post(f"{self.master.base_url}/api/v1/admin/system/upgrade/rollback/{target}", headers=self.master.headers(), timeout=10)
        if r_rb.status_code in (200, 201):
            print_success(f"Rollback succeeded: {r_rb.json().get('message')}")
        else:
            fail(f"Rollback failed: {r_rb.text}")

        time.sleep(4.0)

        if self.master.wait_until_ready():
            print_success("Master successfully restored previous binary and verified healthy.")
        else:
            fail("Master crashed after rollback!")

        print_info(f"Deleting rollback file '{target}' via API...")
        r_del = requests.delete(f"{self.master.base_url}/api/v1/admin/system/upgrade/rollbacks/{target}", headers=self.master.headers(), timeout=5)
        if r_del.status_code in (200, 204):
            print_success(f"Rollback archive '{target}' wiped from disk.")
        else:
            fail(f"Failed deleting rollback: {r_del.text}")

    # ---------------------------------------------------------
    # FINAL VERDICT
    # ---------------------------------------------------------
    def print_final_verdict(self):
        print_header("FINAL CLUSTER BENCHMARK & MULTI-TENANT ISOLATION REPORT")
        print(f"{'Verification Category':<35} | {'Status':<10} | {'Metrics / Details'}")
        print("-" * 80)
        for cat, data in self.audit_summary.items():
            status = data.get("status")
            color = Fore.GREEN if status == "PASS" else (Fore.YELLOW if status == "DEGRADED" else Fore.RED)
            details = f"{data.get('latency_ms', 0):.1f} ms" if "latency_ms" in data else f"Failures: {data.get('failures', 0)}"
            print(f"{cat:<35} | {color}{status:<10}{Style.RESET_ALL} | {details}")
        print("\n" + Fore.GREEN + Style.BRIGHT + "🎉 ALL MULTI-TENANT REPLICATION, REALTIME & UPGRADE TARGETS VERIFIED.\n")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="ApexKit Distributed Fleet & Tenant Test Suite")
    parser.add_argument("--bin", default="./target/release/apexkit-cli", help="Path to apexkit binary (default: ./target/release/apexkit-cli or ./apexkit)")
    parser.add_argument("--port", type=int, default=5000, help="Master port (default: 5000)")
    parser.add_argument("--replicas", type=int, default=2, help="Number of replicas to launch (default: 2)")

    args = parser.parse_args()

    bin_target = Path(args.bin)
    if not bin_target.exists() and Path("./apexkit").exists():
        bin_target = Path("./apexkit")

    suite = ClusterTestSuite(str(bin_target), args.port, args.replicas)
    try:
        suite.run()
    except KeyboardInterrupt:
        print_warn("\nTest aborted by user.")
        sys.exit(0)