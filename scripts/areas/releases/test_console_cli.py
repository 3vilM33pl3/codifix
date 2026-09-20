#!/usr/bin/env python3
"""Explicit, offline Docker rehearsal of the real root-only console entrypoint.

Run with --image sha256:<an already present Python 3.11+/GPG image ID>.
The host runner never invokes console.py or systemctl itself. Only individual
source/tool files are bound read-only; every installation path is container
tmpfs. The generated key and ELF identity header are disposable test fixtures.
"""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import platform
import re
import shlex
import shutil
import stat
import subprocess
import sys
import tempfile
import unittest

sys.dont_write_bytecode = True
SOURCE_FILES = ("console.py", "instance.py", "release.py", "test_instance.py",
                "test_release.py", "test_console_cli.py")
FIELDS = ("ActiveState", "ControlPID", "DropInPaths", "FragmentPath", "Id", "Job",
          "LoadState", "MainPID", "Names", "SubState", "Transient", "UnitFileState")
FAKE_SYSTEMCTL = '''#!/usr/bin/python3 -I
import json
from pathlib import Path
import re
import sys

fields = FIELDS_LITERAL
arguments = sys.argv[1:]
with Path("/scratch/systemctl-calls.jsonl").open("a") as stream:
    stream.write(json.dumps(arguments) + "\\n")
if (len(arguments) != 5 or arguments[:3] != ["show", "--no-pager",
        "--property=" + ",".join(fields)] or arguments[3] != "--" or
        not re.fullmatch(r"memory-layer@[a-z][a-z0-9-]{0,18}\\.service", arguments[4])):
    sys.stderr.write("The fixture permits only an exact read-only systemctl show query\\n")
    sys.exit(77)
state_file = Path("/scratch/systemctl-state.json")
mode = json.loads(state_file.read_text()) if state_file.exists() else "inactive"
unit = arguments[4]
values = dict(LoadState="not-found", ActiveState="inactive", SubState="dead",
              MainPID="0", ControlPID="0", Job="", Id=unit, Names=unit,
              FragmentPath="", DropInPaths="", Transient="no", UnitFileState="")
if mode == "active":
    values.update(LoadState="loaded", ActiveState="active", SubState="running", MainPID="42")
elif mode == "pending":
    values["Job"] = "123"
elif mode == "incomplete":
    del values["ControlPID"]
elif mode != "inactive":
    sys.exit(78)
for name in sorted(values):
    print(name + "=" + values[name])
sys.exit(1 if values["LoadState"] == "not-found" else 0)
'''.replace("FIELDS_LITERAL", repr(FIELDS))


def emit(kind, **values):
    print(json.dumps({"kind": kind, **values}, sort_keys=True), flush=True)


def sha256(data):
    return hashlib.sha256(data).hexdigest()


def load(name, directory):
    spec = importlib.util.spec_from_file_location(name, directory / (name + ".py"))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def check_container():
    """Refuse the internal runner before creating fixture or installation files."""
    if os.geteuid() != 0 or not sys.flags.isolated or not sys.flags.no_site:
        raise RuntimeError("Container rehearsal requires root and python3 -I -S")
    status = dict(line.split(":", 1) for line in Path("/proc/self/status").read_text().splitlines())
    if int(status["CapEff"].strip(), 16) != 0 or status["NoNewPrivs"].strip() != "1":
        raise RuntimeError("Container must drop all capabilities and prohibit new privileges")
    mounts = {}
    for line in Path("/proc/self/mountinfo").read_text().splitlines():
        left, right = line.split(" - ", 1)
        parts = left.split()
        mounts[parts[4]] = (set(parts[5].split(",")), right.split()[0])
    if "ro" not in mounts["/"][0]:
        raise RuntimeError("The container root must be read-only")
    for path in ("/scratch", "/tmp", "/opt/memory-layer", "/etc/memory-layer",
                 "/var/lib/memory-layer"):
        if path not in mounts or mounts[path][1] != "tmpfs":
            raise RuntimeError("Every writable rehearsal path must be a disposable tmpfs")
    for path in [*("/source/" + name for name in SOURCE_FILES),
                 "/source/instance.toml.example", "/usr/bin/systemctl"]:
        if path not in mounts or "ro" not in mounts[path][0]:
            raise RuntimeError("Only read-only source and fake systemctl bindings are accepted")
    if {path.name for path in Path("/sys/class/net").iterdir()} != {"lo"}:
        raise RuntimeError("The rehearsal requires Docker network none")
    if Path("/var/run/docker.sock").exists() or Path("/run/dbus/system_bus_socket").exists():
        raise RuntimeError("Host daemon sockets must not be exposed")
    emit("isolation", effective_uid=0, effective_capabilities=0, no_new_privileges=True,
         root_read_only=True, network_interfaces=["lo"], installation_storage="disposable tmpfs")


class ConsoleCliTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        # Also protect accidental unittest discovery outside the explicit runner.
        check_container()
        cls.tools = Path("/scratch/tools")
        cls.tools.mkdir(mode=0o700)
        for name in SOURCE_FILES:
            destination = cls.tools / name
            destination.write_bytes((Path("/source") / name).read_bytes())
            destination.chmod(0o644)
            assert destination.stat().st_uid == 0
        emit("trusted_tools", sha256={name: sha256((cls.tools / name).read_bytes())
                                      for name in SOURCE_FILES})
        cls.fixture = load("test_instance", cls.tools)
        cls.signing = cls.fixture.FIXTURE.SignatureTests
        cls.signing.setUpClass()
        cls.addClassCleanup(cls.signing.tearDownClass)
        cls.architecture = {"x86_64": "amd64", "aarch64": "arm64"}[platform.machine()]
        cls.binary = bytearray(64)
        cls.binary[:6] = b"\x7fELF\x02\x01"
        cls.binary[18:20] = b"\x3e\x00" if cls.architecture == "amd64" else b"\xb7\x00"
        # ET_NONE, no program headers or instructions. No artifact is executed.
        cls.binary = bytes(cls.binary) + b"synthetic CLI fixture; never executed"
        cls.files = {"bin/memory": cls.binary, "web/index.html": b"<p>offline CLI fixture</p>",
                     "web/assets/deep/main.js": b"/* synthetic CLI fixture */"}
        cls.bundle = Path("/scratch/bundle")
        cls.bundle.mkdir(mode=0o700)
        cls.archive = "memory-" + cls.architecture + ".tar.gz"
        cls.fixture.archive(cls.bundle / cls.archive, cls.files)
        other = "arm64" if cls.architecture == "amd64" else "amd64"
        other_path = cls.bundle / ("memory-" + other)
        other_path.write_bytes(b"unselected synthetic architecture fixture")
        (cls.bundle / "signing-public-key.asc").write_bytes(cls.signing.public)
        cls.fixture.TOOL.release.create(cls.fixture.FIXTURE.creator(cls.bundle,
            schema_floor=60, schema_head=60,
            artifact=[f"{cls.architecture}:archive:{cls.bundle / cls.archive}",
                      f"{other}:binary:{other_path}"]))
        cls.signing.command(["--local-user", cls.signing.signer + "!", "--digest-algo", "SHA256",
            "--output", str(cls.bundle / "manifest.sig"), "--detach-sign", str(cls.bundle / "manifest.json")])
        cls.counter = 0
        emit("fixture", architecture=cls.architecture, disposable_signer=cls.signing.signer,
             binary_sha256=sha256(cls.binary), native_abi_proven=False, artifacts_executed=False)

    def setUp(self):
        type(self).counter += 1
        self.name = "cli-fixture-" + str(self.counter)
        self.work = Path("/scratch") / self.name
        self.work.mkdir(mode=0o700)
        stages = self.work / "stages"
        stages.mkdir(mode=0o700)
        prepared = self.fixture.TOOL.prepare(argparse.Namespace(
            manifest=str(self.bundle / "manifest.json"), signature=str(self.bundle / "manifest.sig"),
            public_key=str(self.bundle / "signing-public-key.asc"),
            trusted_signing_fingerprint=self.signing.signer, architecture=self.architecture,
            schema_current=60, artifacts_dir=str(self.bundle), staging_parent=str(stages),
            instance=self.name, release_archive=self.archive))
        self.prepared = Path(prepared["preparation_directory"])
        self.config = Path("/source/instance.toml.example").read_text().replace(
            'api_token = ""', 'api_token = "' + "synthetic-fixture-" * 3 + '"').replace(
            "instances/REVIEW/", "instances/" + self.name + "/").replace(
            "postgresql://REVIEW:REVIEW@127.0.0.1:5432/REVIEW",
            "postgresql://fixture:synthetic-password@127.0.0.1:5432/memory_fixture")
        self.config_path = self.work / "configuration.toml"
        self.config_path.write_text(self.config)
        self.config_path.chmod(0o600)
        self.release_root = Path("/opt/memory-layer/instances") / self.name
        self.config_root = Path("/etc/memory-layer/instances") / self.name
        self.set_systemd("inactive")

    def tearDown(self):
        self.set_systemd("inactive")

    def set_systemd(self, mode):
        Path("/scratch/systemctl-state.json").write_text(json.dumps(mode))

    def install_arguments(self, **overrides):
        values = dict(prepared=str(self.prepared), config=str(self.config_path),
                      config_sha256=sha256(self.config.encode()), database_name="memory_fixture",
                      instance=self.name, trusted_signing_fingerprint=self.signing.signer, schema_current=60)
        values.update(overrides)
        return ["install", *(item for name, value in values.items()
                for item in ("--" + name.replace("_", "-"), str(value)))]

    def recovery_arguments(self, action=None):
        return ["recover" if action else "inspect", "--instance", self.name,
                "--trusted-signing-fingerprint", self.signing.signer,
                *(["--action", action] if action else [])]

    def cli(self, arguments, status, *, isolated=True, no_site=True):
        command = ["/usr/bin/python3", *(["-I"] if isolated else []),
                   *(["-S"] if no_site else []), str(self.tools / "console.py"), *arguments]
        result = subprocess.run(command, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
            stderr=subprocess.PIPE, text=True, timeout=60,
            env={"PATH": "/usr/bin:/bin", "LC_ALL": "C", "TMPDIR": "/tmp"})
        emit("cli", test=self._testMethodName, command=command, exit_code=result.returncode,
             stdout=result.stdout, stderr=result.stderr)
        self.assertEqual(result.returncode, 1 if status == "rejected" else 0,
                         result.stdout + result.stderr)
        self.assertEqual(result.stdout if status == "rejected" else result.stderr, "")
        report = json.loads(result.stderr if status == "rejected" else result.stdout)
        self.assertEqual(report["status"], status)
        self.assertIs(report["activation_authorized"], False)
        if status != "rejected":
            for name in ("service_mutated", "database_contacted", "database_schema_verified",
                         "native_abi_or_sandbox_proven"):
                self.assertIs(report[name], False)
        return report

    def assert_tree(self, release_root=None, config_root=None):
        release_root = release_root or self.release_root
        config_root = config_root or self.config_root
        for name, contents in self.files.items():
            path = release_root / "release" / name
            self.assertEqual(path.read_bytes(), contents)
            self.assertEqual(stat.S_IMODE(path.stat().st_mode), 0o755 if name == "bin/memory" else 0o644)
            self.assertEqual(path.stat().st_uid, 0)
        for path in [release_root, *(p for p in release_root.rglob("*") if p.is_dir())]:
            self.assertEqual(stat.S_IMODE(path.stat().st_mode), 0o755)
        self.assertEqual(stat.S_IMODE(config_root.stat().st_mode), 0o700)
        for name, data in (("memory-layer.toml", self.config.encode()),
                           ("memory.sha256", (sha256(self.binary) + "\n").encode())):
            path = config_root / name
            self.assertEqual(path.read_bytes(), data)
            self.assertEqual(stat.S_IMODE(path.stat().st_mode), 0o600)
            self.assertEqual(path.stat().st_uid, 0)
        self.assertFalse((config_root / "activation-reviewed").exists())
        emit("verified_tree", instance=self.name, release_root=str(release_root),
             config_root=str(config_root), config_sha256=sha256(self.config.encode()),
             binary_sha256=sha256(self.binary), binary_mode="0755", config_mode="0600",
             release_directory_mode="0755", activation_marker=False)

    def test_install_inspect_finish_retain_and_refuse_restart(self):
        result = self.cli(self.install_arguments(), "installed_inactive")
        self.assert_tree()
        inspected = self.cli(self.recovery_arguments(), "installed")
        self.assertEqual(inspected["transaction"], result["transaction"])
        self.cli(self.recovery_arguments("finish"), "installed_inactive")
        self.cli(self.install_arguments(), "rejected")
        self.assert_tree()
        self.cli(self.recovery_arguments("retain"), "retained_inactive")
        retained = ".retained-" + self.name + "-" + result["transaction"]
        self.assertFalse(self.release_root.exists())
        self.assertFalse(self.config_root.exists())
        self.assert_tree(self.release_root.parent / retained, self.config_root.parent / retained)
        self.cli(self.recovery_arguments(), "retained_inactive")
        self.cli(self.recovery_arguments("retain"), "retained_inactive")
        self.cli(self.recovery_arguments("finish"), "rejected")

    def test_requires_isolated_no_site_python_and_trusted_tool_mode(self):
        for options in ({"isolated": False}, {"no_site": False}):
            self.cli(self.install_arguments(), "rejected", **options)
        tool = self.tools / "console.py"
        tool.chmod(0o664)
        try:
            self.cli(self.install_arguments(), "rejected")
        finally:
            tool.chmod(0o644)
        self.assertFalse(self.release_root.exists())

    def test_config_hash_permissions_and_closed_settings_refused(self):
        self.cli(self.install_arguments(config_sha256="0" * 64), "rejected")
        self.config_path.chmod(0o644)
        self.cli(self.install_arguments(), "rejected")
        self.config_path.chmod(0o600)
        unsafe = self.config.replace("read_only = true", "read_only = false", 1)
        self.config_path.write_text(unsafe)
        self.cli(self.install_arguments(config_sha256=sha256(unsafe.encode())), "rejected")
        self.assertFalse(self.release_root.exists())

    def test_wrong_signer_and_changed_prepared_bytes_refused(self):
        self.cli(self.install_arguments(trusted_signing_fingerprint="A" * 40), "rejected")
        binary = self.prepared / "release/bin/memory"
        binary.chmod(0o600)
        binary.write_bytes(self.binary[:-1] + b"X")
        binary.chmod(0o500)
        self.cli(self.install_arguments(), "rejected")
        self.assertFalse(self.release_root.exists())

    def test_active_pending_and_incomplete_systemd_refused(self):
        for mode in ("active", "pending", "incomplete"):
            with self.subTest(mode=mode):
                self.set_systemd(mode)
                self.cli(self.install_arguments(), "rejected")
                self.assertFalse(self.release_root.exists())
                self.assertFalse(self.config_root.exists())

    def test_existing_paths_preserved(self):
        self.release_root.mkdir(mode=0o755, parents=True)
        sentinel = self.release_root / "operator-file"
        sentinel.write_bytes(b"preserve this unrelated synthetic instance")
        self.cli(self.install_arguments(), "rejected")
        self.assertEqual(sentinel.read_bytes(), b"preserve this unrelated synthetic instance")

    def test_activation_marker_refuses_recovery_without_mutation(self):
        self.cli(self.install_arguments(), "installed_inactive")
        marker = self.config_root / "activation-reviewed"
        marker.write_bytes(b"synthetic refusal fixture")
        try:
            self.cli(self.recovery_arguments(), "rejected")
            self.cli(self.recovery_arguments("retain"), "rejected")
            self.assertEqual(marker.read_bytes(), b"synthetic refusal fixture")
            self.assertEqual((self.config_root / "memory-layer.toml").read_bytes(), self.config.encode())
        finally:
            # Remove only this test's deliberately injected synthetic marker.
            marker.unlink()
        self.assert_tree()

    def test_changed_installed_bytes_refuse_inspection_and_retention(self):
        self.cli(self.install_arguments(), "installed_inactive")
        target = self.release_root / "release/web/index.html"
        target.write_bytes(b"unexpected synthetic replacement")
        self.cli(self.recovery_arguments(), "rejected")
        self.cli(self.recovery_arguments("retain"), "rejected")
        self.assertEqual(target.read_bytes(), b"unexpected synthetic replacement")
        self.assertTrue(self.config_root.is_dir())


def inside_container():
    check_container()
    os.umask(0o077)
    suite = unittest.defaultTestLoader.loadTestsFromTestCase(ConsoleCliTests)
    result = unittest.TextTestRunner(verbosity=2).run(suite)
    log = Path("/scratch/systemctl-calls.jsonl")
    calls = [json.loads(line) for line in log.read_text().splitlines()] if log.exists() else []
    only_show = bool(calls) and all(call[:3] == ["show", "--no-pager", "--property=" + ",".join(FIELDS)]
                                  and len(call) == 5 and call[3] == "--" for call in calls)
    markers = list(Path("/etc/memory-layer").rglob("activation-reviewed"))
    passed = result.wasSuccessful() and only_show and not markers
    emit("summary", status="passed" if passed else "failed", tests_run=result.testsRun,
         failures=len(result.failures), errors=len(result.errors), systemctl_queries=len(calls),
         systemctl_only_show=only_show, activation_markers=len(markers), artifacts_executed=False,
         database_contacted=False, real_service_manager_contacted=False)
    return 0 if passed else 1


def host_run(args):
    if not re.fullmatch(r"sha256:[a-f0-9]{64}", args.image):
        raise ValueError("Use an exact already-present Docker image ID; tags and pulls are forbidden")
    docker = shutil.which("docker")
    if not docker:
        raise RuntimeError("Docker is required for the explicitly requested container rehearsal")
    subprocess.run([docker, "image", "inspect", args.image, "--format", "{{.Id}}"],
                   check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=15)
    source = Path(__file__).resolve().parent
    template = source.parents[2] / "deploy/areas/instance.toml.example"
    inputs = {name: source / name for name in SOURCE_FILES}
    inputs["instance.toml.example"] = template
    for path in inputs.values():
        if not path.is_file() or path.is_symlink():
            raise RuntimeError("Expected a regular rehearsal source file: " + str(path))
    evidence = Path(tempfile.mkdtemp(prefix="memory-console-cli-", dir=args.evidence_parent))
    stub = evidence / "systemctl-fixture.py"
    stub.write_text(FAKE_SYSTEMCTL)
    stub.chmod(0o555)
    container_name = evidence.name
    cidfile = evidence / "container.cid"
    command = [docker, "run", "--rm", "--init", "--pull=never", "--network=none", "--read-only",
               "--name=" + container_name, "--cidfile=" + str(cidfile),
               "--cap-drop=ALL", "--security-opt=no-new-privileges:true", "--user=0:0",
               "--pids-limit=64", "--memory=512m", "--memory-swap=512m", "--cpus=1",
               "--ulimit=nofile=256:256", "--ipc=none", "--workdir=/", "--env=LC_ALL=C",
               "--env=PATH=/usr/bin:/bin", "--env=TMPDIR=/tmp", "--entrypoint=/usr/bin/python3"]
    for target, mode, size in (("/tmp", "1777", "128m"), ("/scratch", "0700", "128m"),
                               ("/opt/memory-layer", "0755", "64m"),
                               ("/etc/memory-layer", "0755", "16m"),
                               ("/var/lib/memory-layer", "0755", "64m")):
        command.extend(["--tmpfs", target + ":rw,nosuid,nodev,noexec,mode=" + mode + ",size=" + size])
    for name, path in inputs.items():
        if "," in str(path):
            raise ValueError("Docker source paths containing commas are unsupported")
        command.extend(["--mount", f"type=bind,src={path},dst=/source/{name},readonly"])
    command.extend(["--mount", f"type=bind,src={stub},dst=/usr/bin/systemctl,readonly", args.image,
                    "-I", "-S", "/source/test_console_cli.py", "--inside-container"])
    (evidence / "command.json").write_text(json.dumps(command, indent=2) + "\n")
    (evidence / "command.sh").write_text(shlex.join(command) + "\n")
    (evidence / "source-sha256.json").write_text(json.dumps(
        {name: sha256(path.read_bytes()) for name, path in inputs.items()}, indent=2, sort_keys=True) + "\n")
    emit("host_evidence", directory=str(evidence), command=command)
    timed_out = False
    with (evidence / "container.stdout.jsonl").open("w") as stdout, (evidence / "container.stderr.log").open("w") as stderr:
        try:
            result = subprocess.run(command, stdin=subprocess.DEVNULL, stdout=stdout, stderr=stderr, timeout=300)
            exit_code = result.returncode
        except subprocess.TimeoutExpired:
            timed_out = True
            exit_code = 124
        finally:
            # A disconnected docker client need not stop its container. Docker
            # records the exact new container ID here, so a name collision or
            # failed creation cannot cause removal of another container.
            if cidfile.exists():
                container_id = cidfile.read_text().strip()
                if not re.fullmatch(r"[a-f0-9]{64}", container_id):
                    raise RuntimeError("Invalid disposable container ID; cleanup refused")
                cleanup = subprocess.run([docker, "container", "rm", "--force", container_id],
                    stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                    text=True, timeout=20)
                (evidence / "cleanup.json").write_text(json.dumps({"container_id": container_id,
                    "container_name": container_name, "exit_code": cleanup.returncode,
                    "stdout": cleanup.stdout, "stderr": cleanup.stderr}, sort_keys=True) + "\n")
    (evidence / "result.json").write_text(json.dumps({"exit_code": exit_code, "timed_out": timed_out,
        "image": args.image, "evidence_directory": str(evidence)}, sort_keys=True) + "\n")
    sys.stdout.write((evidence / "container.stdout.jsonl").read_text())
    sys.stderr.write((evidence / "container.stderr.log").read_text())
    emit("host_result", exit_code=exit_code, timed_out=timed_out, evidence_directory=str(evidence))
    return exit_code


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--image", help="Exact local image ID, sha256: plus 64 hex digits")
    parser.add_argument("--evidence-parent", default="/data/tmp")
    parser.add_argument("--inside-container", action="store_true", help=argparse.SUPPRESS)
    args = parser.parse_args()
    if args.inside_container:
        return inside_container()
    if not args.image:
        parser.error("--image is required; this rehearsal never downloads an image")
    return host_run(args)


if __name__ == "__main__":
    sys.exit(main())
