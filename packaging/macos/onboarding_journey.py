"""Prerequisite-based native journey; device secrets remain in Memory's Keychain.

No prerequisite downloads, builds, global config overlay or legacy service
control. Public CLI metadata is validated; batch process output is suppressed.
"""
import ipaddress
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import uuid
from urllib.parse import urlsplit

import onboarding_files as files
from onboarding_files import Refused
import onboard as operator
from onboarding_native import Native, clean_env


def origin(value, allow_loopback):
    if not isinstance(value, str) or len(value) > 2048 or any(c.isspace() or ord(c) < 32 for c in value):
        raise Refused("bounded canonical credential-free company origin required")
    parsed = urlsplit(value)
    if not parsed.hostname or parsed.username or parsed.password or parsed.path or parsed.query or parsed.fragment:
        raise Refused("exact company origin without credentials/path/query required")
    try:
        loopback = ipaddress.ip_address(parsed.hostname).is_loopback
    except ValueError:
        loopback = parsed.hostname in {"localhost", "localhost."}
    if loopback and not allow_loopback or parsed.scheme != "https" and not (allow_loopback and loopback and parsed.scheme == "http"):
        raise Refused("HTTPS required; loopback needs explicit development consent")
    # Match the CLI's canonical origin rather than rewriting an immutable input.
    host = "[" + parsed.hostname + "]" if ":" in parsed.hostname else parsed.hostname
    host = host.encode("idna").decode("ascii")
    port = parsed.port
    canonical = parsed.scheme + "://" + host + (":" + str(port) if port and port != {"https": 443, "http": 80}[parsed.scheme] else "")
    if value != canonical:
        raise Refused("company input must already be a canonical origin")
    return value


def plan(source, journal):
    operator.exact(source, {"schema_version", "company", "name", "no_browser", "allow_loopback_development", "native", "projects"})
    if type(source["schema_version"]) is not int or source["schema_version"] not in {1, 2} or any(type(source[k]) is not bool for k in ["no_browser", "allow_loopback_development"]):
        raise Refused("unsupported journey schema")
    origin(source["company"], source["allow_loopback_development"])
    if not isinstance(source["name"], str) or not 1 <= len(source["name"].encode()) <= 128 or source["name"].strip() != source["name"] or any(ord(c) < 32 for c in source["name"]):
        raise Refused("bounded public device name without controls required")
    if not isinstance(source["projects"], list) or not 1 <= len(source["projects"]) <= 8:
        raise Refused("one to eight explicit project output slots required")
    operator.exact(source["native"], {"schema_version", "root", "memory", "postgres", "pg_port", "areas"} | ({"curated_sync", "repositories"} if source["native"].get("schema_version") == 2 else set()))
    journal = files.path(journal)
    private_parent(journal)
    outputs, workspaces = [journal, Path(str(journal) + ".memory-lock")], []
    for slot in source["projects"]:
        operator.exact(slot, {"workspace", "journal", "subscription_output"} | ({"selection"} if source["schema_version"] == 2 else set()))
        if source["schema_version"] == 2:
            selected_project(slot["selection"])
        workspace = files.path(slot["workspace"])
        if not workspace.parent.is_dir():
            raise Refused("workspace parents must already exist; no directory creation during review")
        workspaces.append(workspace)
        for name in ["journal", "subscription_output"]:
            file = files.path(slot[name])
            if not re.fullmatch(r"[A-Za-z0-9_.-]{1,128}", file.name):
                raise Refused("journal/metadata leaves require bounded ASCII names")
            private_parent(file)
            outputs.extend([file, Path(str(file) + ".memory-lock")])
    native_input = dict(source["native"], subscriptions=[s["subscription_output"] for s in source["projects"]])
    prerequisite_plan = operator.make_plan(native_input, prerequisites_only=True)
    workspaces.append(Path(native_input["root"]))
    for output in outputs:
        for workspace in workspaces:
            files.outside(output, workspace)
    for paths in [outputs, workspaces]:
        for i, a in enumerate(paths):
            for b in paths[i + 1:]:
                if str(a) == str(b):
                    raise Refused("duplicate journey output/workspace paths")
                files.outside(a, b)
    return {"input": source, "prerequisites": prerequisite_plan,
            "connector": {"path": str(Path(__file__).absolute()), "sha256": files.sha(Path(__file__).absolute())}}


def selected_project(value):
    """A v2 selection binds the full composite identity, never just a slug."""
    operator.exact(value, {"area", "owner_instance_id", "project_id", "revision", "manifest_digest"})
    if value["area"] not in operator.AREAS or type(value["revision"]) is not int or not 1 <= value["revision"] <= 2**63 - 1:
        raise Refused("exact offline area and catalog revision required; Trust remains remote-only")
    operator.identifier(value["owner_instance_id"])
    operator.identifier(value["project_id"])
    operator.pin(value["manifest_digest"])


def private_parent(file):
    if not file.parent.is_dir():
        raise Refused("existing private metadata parent prerequisite missing")
    files.directory(file.parent)  # Validate only; never seed parents during review.


def inspect(journal):
    value = files.read(journal)
    operator.exact(value, {"schema_version", "request_id", "plan", "plan_sha256", "stages", "projects", "native"})
    if value["schema_version"] != 1 or value["plan_sha256"] != files.digest(value["plan"]):
        raise Refused("journey immutable inputs changed; retain the journal")
    operator.identifier(value["request_id"])
    if plan(value["plan"]["input"], journal) != value["plan"]:
        raise Refused("journey prerequisites/inputs changed; no automatic replacement")
    slots = value["plan"]["input"]["projects"]
    stages = {"login", "native services", *[f"{kind}:{i}" for i in range(len(slots)) for kind in ["select", "repository/tools", "offline disclosure"]]}
    if not isinstance(value["stages"], dict) or not set(value["stages"]) <= stages or any(v not in {"intent", "done"} for v in value["stages"].values()) or not isinstance(value["projects"], list) or len(value["projects"]) != len(slots):
        raise Refused("journey stage receipts invalid")
    for entry in value["projects"]:
        operator.exact(entry, {"selection", "subscription_review", "subscription"})
        if entry["selection"] is not None:
            operator.exact(entry["selection"], {"request_id", "plan_sha256", "plan_digest", "device_id", "manifest_sha256", "manifest"})
            for key in ["request_id", "device_id"]:
                operator.identifier(entry["selection"][key])
            for key in ["plan_sha256", "plan_digest", "manifest_sha256"]:
                operator.pin(entry["selection"][key])
            public_manifest(entry["selection"]["manifest"])
        if entry["subscription"] is not None:
            operator.metadata(entry["subscription"])
    if value["native"] is not None:
        operator.exact(value["native"], {"request_id", "plan_sha256"})
        operator.identifier(value["native"]["request_id"])
        operator.pin(value["native"]["plan_sha256"])
    return value


class Memory:
    """Only fixed first-party commands, with a checked installed binary pin."""
    def __init__(self, source):
        self.source = source
        self.binary = source["native"]["memory"]
        self.native = Native()

    def args(self, args):
        if files.sha(self.binary["path"]) != self.binary["sha256"]:
            raise Refused("installed Memory changed; no command executed")
        return [self.binary["path"], *[str(a) for a in args]]

    def execute(self, args, *, interactive=False):
        try:
            result = subprocess.run(self.args(args), env=clean_env(),
                                    stdout=None if interactive else subprocess.DEVNULL,
                                    stderr=subprocess.DEVNULL, timeout=2400, check=False)
        except (OSError, subprocess.TimeoutExpired):
            raise Refused("Memory operation unconfirmed; exact journals retained (output suppressed)") from None
        if result.returncode != 0:
            raise Refused("Memory operation denied/unconfirmed; inspect retained journals (output suppressed)")

    def company(self):
        return ["--company", self.source["company"], *(["--allow-loopback"] if self.source["allow_loopback_development"] else [])]

    def login(self):
        # These reviewed CLI commands expose public codes/manifests only and need
        # their real TTY. Never forward stderr or batch/native output to the UI.
        self.execute(["auth", "login", *self.company(), "--name", self.source["name"], *(["--no-browser"] if self.source["no_browser"] else [])], interactive=True)

    def select(self, slot):
        args = ["projects", "select", *self.company()]
        if self.source["schema_version"] == 2:
            picked = slot["selection"]
            selected_project(picked)
            args = ["projects", "prepare", *self.company(), "--project", picked["project_id"],
                    "--area", picked["area"], "--owner-instance", picked["owner_instance_id"], "--revision", str(picked["revision"])]
        self.execute([*args, "--workspace", slot["workspace"], "--journal", slot["journal"]], interactive=True)

    def workspace_status(self, journal):
        self.execute(["projects", "status", "--journal", journal])
        return files.read(journal, 512 * 1024)

    def apply(self, journal, digest):
        self.execute(["projects", "apply", *self.company(), "--journal", journal, "--plan-sha256", digest, "--confirm"])

    def subscription_args(self, receipt, output):
        manifest = receipt["manifest"]
        entry = manifest_entry(manifest)
        args = ["projects", "subscription", *self.company(), "--project", manifest["project_id"], "--area", entry["area"],
                "--owner-instance", entry["owner_instance_id"], "--revision", manifest["revision"], "--output", output]
        if self.source["native"].get("curated_sync"):
            args.extend(["--replication-mode", "curated_project_v1"])
        return args

    def subscription_preview(self, receipt, output):
        _, text = self.native.command(self.args(self.subscription_args(receipt, output)))
        try:
            return json.loads(text)
        except (ValueError, TypeError):
            raise Refused("subscription preview invalid; response body suppressed") from None

    def subscription_confirm(self, receipt, output, digest):
        self.execute([*self.subscription_args(receipt, output), "--bootstrap-sha256", digest, "--confirm"])


def selection(value, source, slot):
    operator.exact(value, {"schema_version", "request_id", "plan", "plan_sha256", "repository", "templates_done", "tools_done", "preserved_paths"})
    operator.identifier(value["request_id"])
    operator.pin(value["plan_sha256"])
    p = value["plan"]
    operator.exact(p, {"company", "device_id", "workspace", "manifest", "manifest_sha256", "repository_existing", "templates"})
    operator.pin(p["manifest_sha256"])
    operator.identifier(p["device_id"])
    manifest = p["manifest"]
    entry = manifest_entry(manifest)
    operator.identifier(manifest["project_id"])
    operator.identifier(entry["owner_instance_id"])
    if p["company"] != source["company"] or p["workspace"] != slot["workspace"] or entry["area"] not in {a["name"] for a in source["native"]["areas"]} or entry["offline_allowed"] is not True or type(manifest["revision"]) is not int or manifest["revision"] < 1:
        raise Refused("selected company/area/owner/project/offline scope differs from this journey")
    if source["schema_version"] == 2 and slot["selection"] != {
            "area": entry["area"], "owner_instance_id": entry["owner_instance_id"],
            "project_id": manifest["project_id"], "revision": manifest["revision"], "manifest_digest": files.digest(manifest)}:
        raise Refused("prepared project differs from the exact preselected identity/revision")
    public_manifest(manifest)
    # CLI status validates its own serialization digest/template inventory. Keep
    # that digest; don't invent a Python version of the Rust hash contract.
    return {"request_id": value["request_id"], "plan_sha256": value["plan_sha256"], "plan_digest": files.digest(p),
            "device_id": p["device_id"], "manifest_sha256": p["manifest_sha256"], "manifest": manifest}


def public_text(value, bound=256):
    if not isinstance(value, str) or not 1 <= len(value.encode()) <= bound or any(ord(c) < 32 or ord(c) == 127 for c in value):
        raise Refused("bounded public manifest metadata without controls required")


def public_url(value):
    public_text(value, 2048)
    url = urlsplit(value)
    if url.scheme != "https" or not url.hostname or url.username or url.password or url.query or url.fragment or any(c.isspace() for c in value):
        raise Refused("reviewed repository/tool URLs must be credential-free HTTPS")


def manifest_entry(manifest):
    """Read Rust's flattened wire manifest or an older nested journal unchanged.

    Never normalize the manifest itself: its original serialization is bound by
    the selection receipts. Exact field sets also reject mixed representations.
    """
    metadata = {"project_id", "project", "revision", "reviewed_by", "reviewed_at"}
    fields = {"area", "owner_instance_id", "repository_url", "default_branch", "offline_allowed", "tools", "destinations"}
    if isinstance(manifest, dict) and "entry" in manifest:
        operator.exact(manifest, metadata | {"entry"})
        operator.exact(manifest["entry"], fields)
        return manifest["entry"]
    operator.exact(manifest, metadata | fields)
    return {key: manifest[key] for key in fields}


def public_manifest(manifest, *, offline=True):
    entry = manifest_entry(manifest)
    operator.identifier(manifest["project_id"])
    public_text(manifest["project"], 256)
    operator.identifier(manifest["reviewed_by"])
    public_text(manifest["reviewed_at"], 64)
    operator.identifier(entry["owner_instance_id"])
    if entry["area"] not in operator.AREAS | {"trust"} or type(entry["offline_allowed"]) is not bool or entry["area"] == "trust" and entry["offline_allowed"] or offline and not entry["offline_allowed"] or type(manifest["revision"]) is not int or not 1 <= manifest["revision"] <= 2**63 - 1:
        raise Refused("exact reviewed offline project scope required")
    public_url(entry["repository_url"])
    public_text(entry["default_branch"], 256)
    if not isinstance(entry["tools"], list) or len(entry["tools"]) > 32 or not isinstance(entry["destinations"], list) or len(entry["destinations"]) > 32:
        raise Refused("bounded reviewed tools/destinations required")
    for tool in entry["tools"]:
        operator.exact(tool, {"name", "version", "url", "sha256"})
        public_text(tool["name"], 128)
        public_text(tool["version"], 128)
        public_url(tool["url"])
        operator.pin(tool["sha256"])
    for destination in entry["destinations"]:
        operator.exact(destination, {"name", "provider_id", "destination_id"})
        for v in destination.values():
            public_text(v)


def curated_manifest_digest(manifest):
    """RFC 8785 for the bounded manifest schema, separate from CLI journal hashes."""
    try:
        public_manifest(manifest)
    except UnicodeError:
        raise Refused("curated manifest must contain valid Unicode scalar strings") from None
    if manifest["revision"] > 2**53 - 1:
        raise Refused("curated manifest revision exceeds exact JSON integer range")
    # Rust serializes OnboardingProjectEntry flattened. Nested old journals are
    # read without changing their stored data or opaque workspace digest.
    wire = {key: value for key, value in manifest.items() if key != "entry"}
    wire.update(manifest_entry(manifest))
    try:
        canonical = json.dumps(wire, sort_keys=True, separators=(",", ":"), ensure_ascii=False, allow_nan=False).encode("utf-8")
    except (ValueError, UnicodeError):
        raise Refused("curated manifest must contain valid Unicode scalar strings") from None
    return hashlib.sha256(canonical).hexdigest()


def bootstrap_preview(value, receipt, source, output):
    operator.exact(value, {"company", "bootstrap", "bootstrap_sha256", "output", "contains_database_credentials", "creates_services", "downloads_knowledge"} | ({"replication_mode"} if "replication_mode" in value else set()) | ({"manifest_sha256"} if "manifest_sha256" in value else set()))
    if source["native"].get("curated_sync") and value.get("replication_mode") != "curated_project_v1":
        raise Refused("curated local workflow requires an explicitly approved curated-project feed")
    if value["company"] != source["company"] or value["output"] != output or any(value[k] is not False for k in ["contains_database_credentials", "creates_services", "downloads_knowledge"]):
        raise Refused("subscription preview differs from the exact output/disclosure scope")
    operator.pin(value["bootstrap_sha256"])
    sample = {"schema_version": 1, "company": value["company"], "subscription": str(uuid.uuid4()), "bootstrap": value["bootstrap"], "allow_loopback_development": source["allow_loopback_development"]}
    if "replication_mode" in value:
        sample["replication_mode"] = value["replication_mode"]
    if "manifest_sha256" in value:
        sample["manifest_sha256"] = value["manifest_sha256"]
        if value["manifest_sha256"] != curated_manifest_digest(receipt["manifest"]):
            raise Refused("curated profile manifest differs from reviewed project")
    operator.metadata(sample)
    bootstrap = value["bootstrap"]
    scope = bootstrap["scope"]
    manifest = receipt["manifest"]
    entry = manifest_entry(manifest)
    if bootstrap["device"] != receipt["device_id"] or bootstrap["catalog_revision"] != manifest["revision"] or scope["area"] != entry["area"] or scope["owner"] != entry["owner_instance_id"] or scope["project"] != manifest["project_id"]:
        raise Refused("subscription bootstrap does not match the reviewed device/composite project/revision")
    return value


def confirm(kind, digest, preview):
    print(json.dumps(preview, indent=2, sort_keys=True))
    print("Offline copies may persist after revocation. Model disclosure requires separate explicit consent; this installer enables no paid model calls.")
    try:
        answer = input(f"Type yes to confirm the displayed {kind} plan ({digest}), or cancel: ")
    except EOFError:
        raise Refused("confirmation cancelled; journals retained") from None
    if answer not in {"yes", digest}:
        raise Refused("confirmation cancelled; no new effects for this stage")


def persisted(journal, value):
    files.save(journal, value)


def confirmed_stage(journal, value, name, digest, preview, prompt, operation):
    if name not in value["stages"]:
        prompt(name, digest, preview)
        value["stages"][name] = "intent"  # Durable exact-input consent before effects.
        persisted(journal, value)
    operation()  # Recheck/replay even done effects using their same bound receipts.
    value["stages"][name] = "done"
    persisted(journal, value)


def apply_workspace(memory, slot, receipt, source):
    memory.apply(slot["journal"], receipt["plan_sha256"])
    complete = memory.workspace_status(slot["journal"])
    if selection(complete, source, slot) != receipt or complete["repository"] not in {"ready", "existing_preserved"} or complete["templates_done"] is not True or complete["tools_done"] is not True:
        raise Refused("workspace apply not verified complete; native stages remain blocked")


def publish_subscription(memory, slot, receipt, preview, source, entry):
    memory.subscription_confirm(receipt, slot["subscription_output"], preview["bootstrap_sha256"])
    subscription = files.read(slot["subscription_output"], 64 * 1024)
    operator.metadata(subscription)
    if subscription.get("replication_mode", "shared_publications_v1") != preview.get("replication_mode", "shared_publications_v1"):
        raise Refused("subscription feed changed after review; no native provisioning")
    if subscription.get("manifest_sha256") != preview.get("manifest_sha256"):
        raise Refused("subscription manifest changed after review; no native provisioning")
    if subscription["company"] != source["company"] or subscription["bootstrap"] != preview["bootstrap"] or subscription["allow_loopback_development"] != source["allow_loopback_development"] or entry["subscription"] is not None and entry["subscription"] != subscription:
        raise Refused("subscription receipt identity changed; no native provisioning")
    entry["subscription"] = subscription


def native_paths(journal, value):
    suffix = "." + value["request_id"]
    return Path(str(journal) + suffix + ".native-input.json"), Path(str(journal) + suffix + ".native-journal.json")


def run(input_file, journal, native, *, memory=None, prompt=confirm):
    source = files.read(input_file, 64 * 1024)
    journal = files.path(journal)
    if memory is None and (not sys.stdin.isatty() or not sys.stdout.isatty()):
        raise Refused("journey requires an interactive terminal; no enrollment/native effects")
    prepared = plan(source, journal)
    native.preflight(prepared["prerequisites"])  # Pins/platform only, no scope implied.
    memory = memory or Memory(source)
    with files.Lock(journal):
        if journal.exists():
            value = inspect(journal)
            if value["plan"] != prepared:
                raise Refused("journey inputs changed; preserve this journal and review separately")
        else:
            if Path(source["native"]["root"]).exists() or any(Path(s[k]).exists() for s in source["projects"] for k in ["journal", "subscription_output"]):
                raise Refused("new journey never adopts existing journals/subscriptions/installations")
            value = {"schema_version": 1, "request_id": str(uuid.uuid4()), "plan": prepared, "plan_sha256": files.digest(prepared),
                     "stages": {}, "projects": [{"selection": None, "subscription_review": None, "subscription": None} for _ in source["projects"]], "native": None}
            files.missing(journal, files.encoded(value))
        if value["stages"].get("login") != "done":
            value["stages"]["login"] = "intent"
            persisted(journal, value)
            memory.login()
            value["stages"]["login"] = "done"
            persisted(journal, value)
        for index, slot in enumerate(source["projects"]):
            entry = value["projects"][index]
            if entry["selection"] is None:
                if Path(slot["journal"]).exists() and value["stages"].get(f"select:{index}") != "intent":
                    raise Refused("unbound workspace journal appeared; no automatic adoption")
                if not Path(slot["journal"]).exists():
                    value["stages"][f"select:{index}"] = "intent"
                    persisted(journal, value)
                    memory.select(slot)
                if not Path(slot["journal"]).exists():
                    raise Refused("project selection cancelled; exact journey retained without preparation")
            receipt = selection(memory.workspace_status(slot["journal"]), source, slot)
            if entry["selection"] is not None and entry["selection"] != receipt:
                raise Refused("confirmed workspace selection changed; no automatic re-selection")
            entry["selection"] = receipt
            value["stages"][f"select:{index}"] = "done"
            persisted(journal, value)
        selected = [entry["selection"] for entry in value["projects"]]
        identities = [(manifest_entry(r["manifest"])["area"], manifest_entry(r["manifest"])["owner_instance_id"], r["manifest"]["project_id"]) for r in selected]
        if len(set(identities)) != len(identities) or len({r["device_id"] for r in selected}) != 1:
            raise Refused("duplicate composite projects or changed enrolled device; no repository/native effects")
        if {key[0] for key in identities} != {a["name"] for a in source["native"]["areas"]}:
            raise Refused("selected project areas differ from the exact native area plan; no repository/native effects")
        for index, slot in enumerate(source["projects"]):
            entry = value["projects"][index]
            receipt = entry["selection"]
            confirmed_stage(journal, value, f"repository/tools:{index}", receipt["plan_sha256"],
                            {"company": source["company"], "workspace": slot["workspace"], "journal": slot["journal"], "manifest": receipt["manifest"], "preserves_existing_settings": True},
                            prompt, lambda: apply_workspace(memory, slot, receipt, source))
            preview = bootstrap_preview(memory.subscription_preview(receipt, slot["subscription_output"]), receipt, source, slot["subscription_output"])
            if entry["subscription_review"] is not None and preview != entry["subscription_review"]:
                raise Refused("reviewed subscription changed; no automatic scope/digest replacement")
            entry["subscription_review"] = preview
            # Store the exact public preview before confirmation/replay. No bearer
            # or device/poll secret is read from Keychain or persisted by Python.
            persisted(journal, value)
            confirmed_stage(journal, value, f"offline disclosure:{index}", preview["bootstrap_sha256"], preview,
                            prompt, lambda: publish_subscription(memory, slot, receipt, preview, source, entry))
        if len({e["subscription"]["bootstrap"]["scope"]["company"] for e in value["projects"]}) != 1:
            raise Refused("subscription company identities disagree; no native effects")
        native_input, native_journal = native_paths(journal, value)
        for output in [native_input, native_journal, Path(str(native_journal) + ".memory-lock")]:
            for root in [Path(source["native"]["root"]), *[Path(s["workspace"]) for s in source["projects"]]]:
                files.outside(output, root)
        files.missing(native_input, files.encoded(dict(source["native"], subscriptions=[s["subscription_output"] for s in source["projects"]])))
        native_plan = operator.prepare(native_input, native_journal, native)  # Full receipts mandatory.
        native_receipt = {"request_id": native_plan["request_id"], "plan_sha256": native_plan["plan_sha256"]}
        if value["native"] is not None and value["native"] != native_receipt:
            raise Refused("native immutable plan identity changed")
        value["native"] = native_receipt
        persisted(journal, value)
        confirmed_stage(journal, value, "native services", native_plan["plan_sha256"], native_plan,
                        prompt, lambda: operator.apply(native_journal, native_plan["plan_sha256"], True, native))
        result = status(journal)
        print(json.dumps(result, indent=2, sort_keys=True))
        print("Selected prerequisites/native stages recorded, not clean-Mac acceptance. Run journey-agents for separate Codex/Claude setup; no self-contained prerequisite installation or paid model calls were performed.")
        return result


def status(journal):
    value = inspect(journal)
    result = {"request_id": value["request_id"], "plan_sha256": value["plan_sha256"], "stages": value["stages"],
              "projects": [{"selection": entry["selection"], "subscription_id": entry["subscription"]["subscription"] if entry["subscription"] else None} for entry in value["projects"]],
              "agent_configuration": "not started; run journey-agents", "native_acceptance": "unverified", "current_authority": "not asserted by local status"}
    agent_journal = Path(str(journal) + ".agents.json")
    if agent_journal.exists():
        import onboarding_agents
        result["agent_configuration"] = onboarding_agents.status(journal, value)
    if value["native"] is not None:
        _, native_journal = native_paths(journal, value)
        native = operator.inspect(native_journal)
        if {"request_id": native["request_id"], "plan_sha256": native["plan_sha256"]} != value["native"]:
            raise Refused("native status receipt mismatch")
        result["native_stages"] = native["stages"]
    return result
