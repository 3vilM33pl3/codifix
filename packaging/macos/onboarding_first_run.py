"""Interactive inputs for the existing native journey, not a second installer.

Uses an exact reviewed runtime bundle. Login secrets remain in Keychain. Only
public catalog metadata enters the private resumable first-run journal.
"""
import copy
import json
import os
from pathlib import Path
import platform
import re
import sys
import uuid

import onboard as operator
import onboarding_files as files
import onboarding_journey as journey
from onboarding_files import Refused
import runtime_bundle


CATALOG_SUFFIX = "Reviewed catalog metadata only; remote grants, installation and selective sync remain separate."


def catalog(text):
    try:
        value, end = json.JSONDecoder(object_pairs_hook=runtime_bundle.unique).raw_decode(text.lstrip())
    except (ValueError, TypeError, RecursionError):
        raise Refused("authorized project catalog unreadable; response body suppressed") from None
    if text.lstrip()[end:].strip() not in {"", CATALOG_SUFFIX}:
        raise Refused("project catalog includes unexpected output; body suppressed")
    operator.exact(value, {"schema_version", "projects"})
    if type(value["schema_version"]) is not int or value["schema_version"] != 1 or not isinstance(value["projects"], list) or len(value["projects"]) > 256:
        raise Refused("bounded versioned authorized project catalog required")
    identities = set()
    for entry in value["projects"]:
        operator.exact(entry, {"manifest", "allowed_roles", "offline_available"} | ({"curated_contributions_allowed"} if "curated_contributions_allowed" in entry else set()))
        if type(entry.get("curated_contributions_allowed", False)) is not bool:
            raise Refused("explicit boolean curated contribution permission required")
        journey.public_manifest(entry["manifest"], offline=False)
        manifest = entry["manifest"]
        metadata = journey.manifest_entry(manifest)
        identity = (metadata["area"], metadata["owner_instance_id"], manifest["project_id"])
        if identity in identities or type(entry["offline_available"]) is not bool or not isinstance(entry["allowed_roles"], list) or not 1 <= len(entry["allowed_roles"]) <= 2 or len(set(entry["allowed_roles"])) != len(entry["allowed_roles"]) or any(r not in {"reader", "writer"} for r in entry["allowed_roles"]):
            raise Refused("duplicate project identity or unsupported device permissions")
        if entry["offline_available"] and (not metadata["offline_allowed"] or identity[0] == "trust"):
            raise Refused("catalog offline policy is inconsistent; Trust stays CA-local")
        identities.add(identity)
    return value["projects"]


def ask(prompt):
    try:
        return input(prompt)
    except EOFError:
        raise Refused("first-run cancelled; retained state can be resumed") from None


def choose(entries, workspace_parent, *, question=ask):
    for index, entry in enumerate(entries, 1):
        manifest = entry["manifest"]
        metadata = journey.manifest_entry(manifest)
        area = metadata["area"]
        mode = "offline eligible" if entry["offline_available"] else "remote-only; not installed by this workflow"
        print(f"{index}. {area} / {manifest['project']} / owner {metadata['owner_instance_id']} ({mode})")
    answer = question("Select up to eight offline projects by number, separated by commas (blank cancels): ")
    if not re.fullmatch(r"[0-9]+(\s*,\s*[0-9]+){0,7}", answer):
        raise Refused("select one to eight explicit project numbers")
    indices = [int(n.strip()) - 1 for n in answer.split(",")]
    if len(set(indices)) != len(indices) or any(i < 0 or i >= len(entries) for i in indices):
        raise Refused("duplicate or unavailable project selection")
    selected = []
    for index in indices:
        entry = entries[index]
        if not entry["offline_available"]:
            raise Refused("selected project is not approved for offline installation; Trust remains remote-only")
        manifest = entry["manifest"]
        journey.public_manifest(manifest)
        metadata = journey.manifest_entry(manifest)
        composite = {"area": metadata["area"], "owner_instance_id": metadata["owner_instance_id"],
                     "project_id": manifest["project_id"], "revision": manifest["revision"], "manifest_digest": files.digest(manifest)}
        slug = re.sub(r"[^a-z0-9]+", "-", manifest["project"].lower()).strip("-")[:48] or "project"
        default = workspace_parent / f"{composite['area']}-{slug}-{files.digest(composite)[:12]}"
        location = question(f"Repository for {composite['area']} / {manifest['project']} (Enter for {default}; or an existing absolute path): ")
        workspace = files.path(location or default)
        if not workspace.parent.is_dir():
            raise Refused("repository parent must exist; existing repositories are never reset")
        selected.append({"selection": composite, "workspace": str(workspace)})
    return selected


def prerequisite(runtime, memory, native):
    arch = {"arm64": "aarch64", "x86_64": "x86_64"}.get(platform.machine())
    if not arch:
        raise Refused("unsupported native Mac architecture")
    descriptor = runtime_bundle.verify_installed(files.path(runtime), arch)
    python = descriptor["python"]
    if str(Path(sys.executable).absolute()) != python["path"] or ".".join(str(n) for n in sys.version_info[:3]) != python["version"]:
        raise Refused("run first-run with the exact reviewed bundled Python executable")
    memory = files.path(memory)
    with files.opened(memory, protected=False) as stream:
        if os.fstat(stream.fileno()).st_uid not in {0, os.getuid()}:
            raise Refused("installed Memory ownership is untrusted")
    pin = files.sha(memory)
    native.command(["/usr/bin/lipo", "-verify_arch", "arm64" if arch == "aarch64" else "x86_64", memory])
    version = native.command([memory, "--version"])[1]
    if files.sha(memory) != pin or not isinstance(version, str) or not re.fullmatch(r"memory [A-Za-z0-9_.+\-]{1,120}", version):
        raise Refused("installed Memory changed or returned unsupported version metadata")
    return descriptor, {"path": str(memory), "sha256": pin, "version": version}


def bindings(company, name, no_browser, allow_loopback, runtime, memory, state, workspace_parent, pg_port, api_base_port):
    journey.origin(company, allow_loopback)
    journey.public_text(name, 128)
    if name.strip() != name:
        raise Refused("device name must not contain surrounding whitespace")
    operator.port(pg_port)
    operator.port(api_base_port)
    if api_base_port > 65533:
        raise Refused("API base port must leave room for three distinct area ports")
    if pg_port in range(api_base_port, api_base_port + 3):
        raise Refused("PostgreSQL and possible area ports must be distinct")
    return {"company": company, "name": name, "no_browser": no_browser, "allow_loopback_development": allow_loopback,
            "runtime": str(files.path(runtime)), "memory": str(files.path(memory)), "state": str(files.path(state)),
            "workspace_parent": str(files.path(workspace_parent)), "pg_port": pg_port, "api_base_port": api_base_port}


def native_user():
    if not sys.stdin.isatty() or not sys.stdout.isatty():
        raise Refused("first-run requires an interactive terminal")
    if sys.platform != "darwin" or os.getuid() == 0 or os.geteuid() != os.getuid():
        raise Refused("first-run requires a non-root logged-in native Mac user")


def run(*, company, name, runtime, memory, state, workspace_parent, native, no_browser=False,
        allow_loopback=False, pg_port=55432, api_base_port=55440, prepare_only=False, question=ask,
        memory_factory=journey.Memory, journey_runner=journey.run, curated_sync=False):
    # Test dependencies are Python-only injections. The command-line has no
    # platform/Keychain/architecture bypass flag and no arbitrary executable hook.
    native_user()
    bound = bindings(company, name, no_browser, allow_loopback, runtime, memory, state, workspace_parent, pg_port, api_base_port)
    if curated_sync:
        bound["curated_sync"] = True
    descriptor, installed = prerequisite(runtime, memory, native)
    state, workspace_parent = files.path(state), files.path(workspace_parent)
    if not state.parent.is_dir() or not workspace_parent.is_dir():
        raise Refused("state parent and repository parent must already exist")
    files.outside(state, workspace_parent)
    files.directory(state)  # New private state only; never chmod/adopt public state.
    receipt_path, input_path, journal = state / "first-run.json", state / "journey-input.json", state / "journey.json"
    with files.Lock(receipt_path):
        if receipt_path.exists():
            receipt = files.read(receipt_path)
            operator.exact(receipt, {"schema_version", "request_id", "plan_sha256", "bindings", "descriptor_digest", "memory", "input_digest", "prepared_input"})
            if type(receipt["schema_version"]) is not int or receipt["schema_version"] != 1 or receipt["bindings"] != bound or receipt["descriptor_digest"] != files.digest(descriptor) or receipt["memory"] != installed:
                raise Refused("first-run inputs/runtime changed; retain state and review a separate installation")
            operator.identifier(receipt["request_id"])
        else:
            if input_path.exists() or journal.exists() or (state / "native").exists():
                raise Refused("first-run does not adopt existing installations or unbound journals")
            receipt = {"schema_version": 1, "request_id": str(uuid.uuid4()), "bindings": bound,
                       "descriptor_digest": files.digest(descriptor), "memory": installed, "input_digest": None, "prepared_input": None}
            receipt["plan_sha256"] = files.digest({k: receipt[k] for k in ["bindings", "descriptor_digest", "memory"]})
            files.missing(receipt_path, files.encoded(receipt))
        if receipt["plan_sha256"] != files.digest({k: receipt[k] for k in ["bindings", "descriptor_digest", "memory"]}):
            raise Refused("first-run immutable prerequisite binding changed")
        native_input = {"schema_version": 1, "root": str(state / "native"), "memory": installed,
                        "postgres": descriptor["postgres"], "pg_port": pg_port,
                        "areas": [{"name": "workshop", "instance_id": receipt["request_id"], "api_port": api_base_port}]}
        skeleton = {"schema_version": 2, "company": company, "name": name, "no_browser": no_browser,
                    "allow_loopback_development": allow_loopback, "native": native_input}
        native.preflight(operator.make_plan(dict(native_input, subscriptions=[str(state / "unused-subscription.json")]), prerequisites_only=True))
        if receipt["prepared_input"] is not None:
            source = receipt["prepared_input"]
            if receipt["input_digest"] != files.digest(source):
                raise Refused("first-run immutable prepared input changed")
            journey.plan(source, journal)
            for key in ["company", "name", "no_browser", "allow_loopback_development"]:
                if source[key] != skeleton[key]:
                    raise Refused("saved first-run input differs from original bindings")
            if source["native"]["memory"] != installed or source["native"]["postgres"] != descriptor["postgres"] or source["native"]["root"] != native_input["root"] or source["native"]["pg_port"] != pg_port:
                raise Refused("saved native prerequisite inputs changed")
            if input_path.exists() and files.read(input_path, 64 * 1024) != source:
                raise Refused("saved first-run selection changed")
        else:
            if receipt["input_digest"] is not None or input_path.exists() or journal.exists():
                raise Refused("unbound first-run input/journey appeared; preserve recovery state")
            cli = memory_factory(skeleton)
            cli.login()
            _, text = native.command(cli.args(["projects", "available", *cli.company()]))
            picked = choose(catalog(text), workspace_parent, question=question)
            if curated_sync:
                projects = catalog(text)
                for item in picked:
                    eligible = [entry for entry in projects if entry["manifest"]["project_id"] == item["selection"]["project_id"] and journey.manifest_entry(entry["manifest"])["area"] == item["selection"]["area"] and journey.manifest_entry(entry["manifest"])["owner_instance_id"] == item["selection"]["owner_instance_id"]]
                    if len(eligible) != 1 or eligible[0].get("curated_contributions_allowed") is not True:
                        raise Refused("curated contributions require explicit current device catalog permission")
                native_input.update(schema_version=2, curated_sync=True, repositories=[
                    {"area": item["selection"]["area"], "project_id": item["selection"]["project_id"], "repository": item["workspace"], "project": next(
                        entry["manifest"]["project"] for entry in projects
                        if entry["manifest"]["project_id"] == item["selection"]["project_id"]
                        and journey.manifest_entry(entry["manifest"])["area"] == item["selection"]["area"]
                        and journey.manifest_entry(entry["manifest"])["owner_instance_id"] == item["selection"]["owner_instance_id"])}
                    for item in picked])
            areas = sorted({item["selection"]["area"] for item in picked})
            native_input["areas"] = [{"name": area, "instance_id": str(uuid.uuid4()), "api_port": api_base_port + index} for index, area in enumerate(areas)]
            source = copy.deepcopy(skeleton)
            source["projects"] = [dict(item, journal=str(state / f"workspace-{i}.json"), subscription_output=str(state / f"subscription-{i}.json")) for i, item in enumerate(picked)]
            journey.plan(source, journal)
            # Durable exact selection before publishing the immutable input. A
            # lost acknowledgement recovers it; it never invokes a new picker.
            receipt["prepared_input"] = source
            receipt["input_digest"] = files.digest(source)
            files.save(receipt_path, receipt)
        files.missing(input_path, files.encoded(source))
    print(f"Selected project input saved. Resume with the same first-run command. Status: journey-status --journal {journal}")
    if prepare_only:
        return {"input": str(input_path), "journal": str(journal), "input_digest": receipt["input_digest"], "native_acceptance": "unverified"}
    return journey_runner(input_path, journal, native)
