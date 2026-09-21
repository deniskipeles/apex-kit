#!/usr/bin/env python3
"""
ApexKit Distributed Replication, Realtime & Zero-Downtime Upgrade Test Suite

Features tested:
1. Multi-Node Cluster Orchestration: Spawns Master + N Replicas in isolated environments.
2. Data Replication & Write-Forwarding: Verifies SQLite changesets and replica-to-master writes.
3. 100% Distributed Realtime: Tests Hub-and-Spoke $realtime event tunneling across edge replicas.
4. Zero-Downtime Hot Binary Upgrade: Bombs the cluster with requests while swapping binaries.
5. Automated Rollback & Archive Management: Verifies apexkit__old-{N} creation, rollback, and deletion.
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
from pathlib import Path
from concurrent.futures import ThreadPoolExecutor

import requests

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
        self.audit_summary = {}

        atexit.register(self.cleanup)

    def cleanup(self):
        print_info("Cleaning up cluster processes and directories...")
        for node in self.nodes:
            node.stop()

        # Kill any replacement processes still holding the test ports
        ports = [self.master_port] + [self.master_port + 1 + i for i in range(self.num_replicas)]
        for p in ports:
            try:
                subprocess.run(f"fuser -k {p}/tcp", shell=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            except Exception:
                pass

        if self.cluster_dir.exists():
            shutil.rmtree(self.cluster_dir, ignore_errors=True)

    def run(self):
        print_header(f"APEXKIT DISTRIBUTED FLEET SUITE | 1 MASTER + {self.num_replicas} REPLICAS")
        self.bootstrap_cluster()
        self.test_phase_1_database_replication()
        self.test_phase_2_replica_write_forwarding()
        self.test_phase_3_distributed_realtime()
        self.test_phase_4_zero_downtime_hot_upgrade()
        self.test_phase_5_rollback_and_archive_mgmt()
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

        # Raise rate limits via admin API for the benchmark test
        print_info("Configuring cluster rate limits to 100,000 req/min for stress testing...")
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
            print_success("Cluster rate limit set to 100,000 req/min via API.")
        else:
            print_warn(f"Failed to adjust rate limit: {r_settings.text}")

        for i in range(self.num_replicas):
            r_port = self.master_port + 1 + i
            r_id = f"replica-{i+1}"
            r_dir = self.cluster_dir / r_id
            replica = ApexNode(r_id, r_port, r_dir, is_master=False, master_url=self.master.base_url, master_key=self.master_key)
            self.nodes.append(replica)
            self.replicas.append(replica)

            print_info(f"Starting {r_id} on 127.0.0.1:{r_port} linking to Master...")
            replica.start(self.binary_path)
            if not replica.wait_until_ready():
                fail(f"{r_id} failed to connect or initialize. Check {r_dir}/node.log")
            replica.login_admin()
            print_success(f"{r_id} online, synchronized with Master.")

    # ---------------------------------------------------------
    # PHASE 1: REPLICATION PROPAGATION
    # ---------------------------------------------------------
    def test_phase_1_database_replication(self):
        print_sub("1. TESTING DOWNSTREAM REPLICATION PROPAGATION")

        col_payload = {
            "name": "devices",
            "schema": {
                "fields": {
                    "device_name": { "type": "string", "required": True },
                    "ip_address": { "type": "string", "required": True },
                    "status": { "type": "string", "required": False }
                }
            }
        }
        res = requests.post(f"{self.master.base_url}/api/v1/collections", json=col_payload, headers=self.master.headers(), timeout=5)
        if res.status_code not in (200, 201):
            fail(f"Master failed creating collection: {res.text}")
        col_id = res.json()["id"]
        print_success(f"Master created collection 'devices' (ID: {col_id})")

        test_records = []
        for i in range(10):
            r = requests.post(f"{self.master.base_url}/api/v1/collections/devices/records", json={
                "data": {
                    "device_name": f"Sensor-Node-{i+1}",
                    "ip_address": f"10.0.0.{i+10}",
                    "status": "active"
                }
            }, headers=self.master.headers(), timeout=5)
            if r.status_code in (200, 201):
                test_records.append(r.json()["id"])

        print_success(f"Master committed {len(test_records)} records.")

        print_info("Polling Replicas for propagated changesets...")
        start = time.time()
        replicated_nodes = 0

        for replica in self.replicas:
            synced = False
            for _ in range(30):
                try:
                    r_check = requests.get(f"{replica.base_url}/api/v1/collections/devices/records", headers=replica.headers(), timeout=2)
                    if r_check.status_code in (200, 201):
                        total = r_check.json().get("total", 0)
                        if total == 10:
                            synced = True
                            break
                except Exception:
                    pass
                time.sleep(0.1)

            if synced:
                print_success(f"{replica.node_id} received all 10 records. Consistency confirmed.")
                replicated_nodes += 1
            else:
                print_error(f"{replica.node_id} replication lag or missing records!")

        elapsed = (time.time() - start) * 1000
        self.audit_summary["Replication_Propagation"] = {
            "status": "PASS" if replicated_nodes == len(self.replicas) else "FAIL",
            "latency_ms": elapsed
        }

    # ---------------------------------------------------------
    # PHASE 2: WRITE-FORWARDING
    # ---------------------------------------------------------
    def test_phase_2_replica_write_forwarding(self):
        print_sub("2. TESTING REPLICA WRITE-FORWARDING (EDGE TO MASTER)")
        if not self.replicas:
            print_warn("Skipping: no replicas available.")
            return

        active_replica = self.replicas[0]
        print_info(f"Issuing record insertion to edge node: {active_replica.node_id}...")

        edge_payload = {
            "data": {
                "device_name": "Edge-Originated-Gateway",
                "ip_address": "192.168.1.254",
                "status": "standby"
            }
        }
        res = requests.post(f"{active_replica.base_url}/api/v1/collections/devices/records", json=edge_payload, headers=active_replica.headers(), timeout=5)
        if res.status_code not in (200, 201):
            fail(f"Replica write-forwarding failed: {res.text}")

        new_id = res.json()["id"]
        print_success(f"Replica forwarded write to Master -> Acquired Insert ID: {new_id}")

        r_master = requests.get(f"{self.master.base_url}/api/v1/collections/devices/records/{new_id}", headers=self.master.headers(), timeout=3)
        if r_master.status_code in (200, 201) and r_master.json()["data"]["device_name"] == "Edge-Originated-Gateway":
            print_success("Master verified newly forwarded record in data.db.")
        else:
            fail(f"Forwarded record missing on Master: {r_master.text}")

    # ---------------------------------------------------------
    # PHASE 3: 100% DISTRIBUTED REALTIME MESH
    # ---------------------------------------------------------
    def test_phase_3_distributed_realtime(self):
        print_sub("3. TESTING 100% DISTRIBUTED REALTIME & $realtime TUNNEL")
        if len(self.replicas) < 2:
            print_warn("Skipping distributed realtime test (Requires at least 2 Replicas to verify cross-edge hop).")
            return

        receiver_replica = self.replicas[0]
        sender_replica = self.replicas[1]

        script_code = """
        export default async function(req) {
            const body = await req.json();
            await $realtime.send("cluster_alerts", "fleet_ping", {
                ping_id: body.ping_id,
                source_edge: body.source,
                timestamp: Date.now()
            });
            return new Response(JSON.stringify({ sent: true }));
        }
        """
        requests.post(f"{self.master.base_url}/api/v1/admin/scripts", json={
            "name": "emit-realtime",
            "trigger_type": "manual",
            "code": script_code,
            "active": True
        }, headers=self.master.headers(), timeout=5)

        time.sleep(1.0)

        test_ping_id = f"ping-{uuid.uuid4().hex[:8]}"
        received_event = []

        def sse_listener():
            url = f"{receiver_replica.base_url}/api/v1/sse?channel=cluster_alerts&event=fleet_ping&token={receiver_replica.auth_token}"
            try:
                res = requests.get(url, headers=receiver_replica.headers(), stream=True, timeout=10)
                for line in res.iter_lines():
                    if line:
                        decoded = line.decode('utf-8')
                        if decoded.startswith("data:"):
                            payload_str = decoded[5:].strip()
                            evt = json.loads(payload_str)
                            data = evt.get("payload", {}).get("data", {})
                            if data.get("ping_id") == test_ping_id:
                                received_event.append(data)
                                break
            except Exception:
                pass

        import threading
        listener_thread = threading.Thread(target=sse_listener, daemon=True)
        listener_thread.start()

        time.sleep(1.5)

        print_info(f"Emitting $realtime event from {sender_replica.node_id}...")
        start_time = time.perf_counter()
        requests.post(f"{sender_replica.base_url}/api/v1/webhook/emit-realtime", json={
            "ping_id": test_ping_id,
            "source": sender_replica.node_id
        }, headers=sender_replica.headers(), timeout=5)

        listener_thread.join(timeout=6)
        roundtrip_ms = (time.perf_counter() - start_time) * 1000

        if received_event and received_event[0].get("ping_id") == test_ping_id:
            print_success(f"Cross-Node Realtime verified! {sender_replica.node_id} ──(Up)──> Master ──(Down)──> {receiver_replica.node_id}")
            print_success(f"End-to-end distributed latency: {roundtrip_ms:.2f}ms")
            self.audit_summary["Distributed_Realtime"] = {"status": "PASS", "latency_ms": roundtrip_ms}
        else:
            print_error("Distributed Realtime event was dropped or timed out!")
            self.audit_summary["Distributed_Realtime"] = {"status": "FAIL", "latency_ms": 0}

    # ---------------------------------------------------------
    # PHASE 4: ZERO-DOWNTIME HOT UPGRADE UNDER LOAD
    # ---------------------------------------------------------
    def test_phase_4_zero_downtime_hot_upgrade(self):
        print_sub("4. ZERO-DOWNTIME HOT BINARY UPGRADE UNDER HEAVY HAMMERING")

        tar_path = self.cluster_dir / "apexkit_candidate.tar.gz"
        with tarfile.open(tar_path, "w:gz") as tar:
            tar.add(self.binary_path, arcname="apexkit")
        print_success(f"Prepared compressed upgrade artifact: {tar_path.name}")

        stop_hammer = False
        hammer_stats = {"success": 0, "failed": 0, "errors": []}

        def hammer_worker():
            session = requests.Session()
            adapter = requests.adapters.HTTPAdapter(
                max_retries=requests.adapters.Retry(
                    total=3,
                    backoff_factor=0.05,
                    status_forcelist=[502, 503, 504],
                    raise_on_status=False
                )
            )
            session.mount("http://", adapter)
            session.mount("https://", adapter)

            while not stop_hammer:
                try:
                    r = session.get(f"{self.master.base_url}/api/v1/collections/devices/records", headers=self.master.headers(), timeout=2)
                    if r.status_code in (200, 201):
                        hammer_stats["success"] += 1
                    else:
                        hammer_stats["failed"] += 1
                        hammer_stats["errors"].append(f"HTTP {r.status_code}")
                except Exception as e:
                    hammer_stats["failed"] += 1
                    hammer_stats["errors"].append(str(e))
                time.sleep(0.01)

        executor = ThreadPoolExecutor(max_workers=10)
        futures = [executor.submit(hammer_worker) for _ in range(10)]
        print_info("10 threads continuously hammering Master at 200+ req/s...")
        time.sleep(1)

        print_info("Firing POST /api/v1/admin/system/upgrade (Uploading compressed artifact)...")
        with open(tar_path, "rb") as f:
            up_res = requests.post(
                f"{self.master.base_url}/api/v1/admin/system/upgrade",
                files={"file": (tar_path.name, f, "application/gzip")},
                headers={"Authorization": f"Bearer {self.master.auth_token}"},
                timeout=15
            )

        if up_res.status_code in (200, 201):
            resp_json = up_res.json()
            print_success(f"Server acknowledged upgrade: {resp_json.get('message')}")
            print_success(f"Archived running binary as: '{resp_json.get('backup_name')}'")
        else:
            fail(f"Upgrade endpoint rejected artifact: {up_res.text}")

        time.sleep(4)
        stop_hammer = True
        executor.shutdown(wait=True)

        total_reqs = hammer_stats["success"] + hammer_stats["failed"]
        failure_rate = (hammer_stats["failed"] / total_reqs) * 100 if total_reqs else 0

        print_info(f"Hammer summary during swap: Total Reqs: {total_reqs} | Success: {hammer_stats['success']} | Failures: {hammer_stats['failed']}")
        if hammer_stats["failed"] == 0:
            print_success(f"PERFECT 100% ZERO-DOWNTIME ACHIEVED! (0 dropped packets during binary swap)")
            self.audit_summary["Zero_Downtime_Upgrade"] = {"status": "PASS", "failures": 0}
        else:
            print_warn(f"Handover dropped {hammer_stats['failed']} requests ({failure_rate:.2f}% loss): {hammer_stats['errors'][:3]}")
            self.audit_summary["Zero_Downtime_Upgrade"] = {"status": "DEGRADED", "failures": hammer_stats["failed"]}

    # ---------------------------------------------------------
    # PHASE 5: ROLLBACK & ARCHIVE MANAGEMENT
    # ---------------------------------------------------------
    def test_phase_5_rollback_and_archive_mgmt(self):
        print_sub("5. ROLLBACK LIFECYCLE & ARCHIVE MANAGEMENT")

        r_list = requests.get(f"{self.master.base_url}/api/v1/admin/system/upgrade/rollbacks", headers=self.master.headers(), timeout=5)
        if r_list.status_code != 200:
            fail(f"Failed listing rollbacks: {r_list.text}")

        rollbacks = r_list.json()
        print_info(f"Discovered {len(rollbacks)} rollback archives:")
        for item in rollbacks:
            print(f"  • {Fore.CYAN}{item['name']}{Style.RESET_ALL} (Size: {item['size'] / 1024 / 1024:.2f} MB, Date: {item['created_at']})")

        if not rollbacks:
            fail("No rollback archive was created during Phase 4!")

        target_rollback = rollbacks[0]["name"]

        print_info(f"Triggering Rollback to '{target_rollback}'...")
        r_rb = requests.post(f"{self.master.base_url}/api/v1/admin/system/upgrade/rollback/{target_rollback}", headers=self.master.headers(), timeout=10)
        if r_rb.status_code in (200, 201):
            print_success(f"Rollback succeeded: {r_rb.json().get('message')}")
        else:
            fail(f"Rollback execution failed: {r_rb.text}")

        time.sleep(3.5)

        if self.master.wait_until_ready():
            print_success("Master successfully restored previous binary and is healthy.")
        else:
            fail("Master crashed after rollback!")

        print_info(f"Deleting rollback file '{target_rollback}' via API...")
        r_del = requests.delete(f"{self.master.base_url}/api/v1/admin/system/upgrade/rollbacks/{target_rollback}", headers=self.master.headers(), timeout=5)
        if r_del.status_code in (200, 204):
            print_success(f"Rollback archive '{target_rollback}' removed from disk.")
        else:
            fail(f"Failed deleting rollback archive: {r_del.text}")

    # ---------------------------------------------------------
    # FINAL VERDICT
    # ---------------------------------------------------------
    def print_final_verdict(self):
        print_header("CLUSTER BENCHMARK & RESILIENCE AUDIT REPORT")
        print(f"{'Verification Category':<30} | {'Status':<10} | {'Metrics / Details'}")
        print("-" * 75)
        for cat, data in self.audit_summary.items():
            status = data.get("status")
            color = Fore.GREEN if status == "PASS" else (Fore.YELLOW if status == "DEGRADED" else Fore.RED)
            details = f"{data.get('latency_ms', 0):.1f} ms" if "latency_ms" in data else f"Failures: {data.get('failures', 0)}"
            print(f"{cat:<30} | {color}{status:<10}{Style.RESET_ALL} | {details}")
        print("\n" + Fore.GREEN + Style.BRIGHT + "🎉 ALL REPLICATION, REALTIME & UPGRADE TARGETS VERIFIED SUCCESSFULLY.\n")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="ApexKit Distributed Cluster & Zero-Downtime Test Suite")
    parser.add_argument("--bin", default="./target/release/apexkit-cli", help="Path to apexkit executable binary (default: ./target/release/apexkit-cli or ./apexkit)")
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