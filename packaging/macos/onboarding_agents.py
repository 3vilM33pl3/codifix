"""Explicit, resumable Codex/Claude setup after native journey provisioning.

Only public first-party CLI metadata enters this journal. Device/agent secrets
remain in Keychain; native profiles and custom settings are never copied here.
"""
import json
from pathlib import Path
import sys
import uuid

import onboarding_files as files
from onboarding_files import Refused
import onboarding_journey as journey
import onboard as operator

CALLERS = {"codex": "openai", "claude": "anthropic"}
PREVIEW_KEYS = {"schema_version", "plan_sha256", "agent_id", "caller", "provider_id", "device_id", "scope", "workspace", "settings_path", "before_sha256", "after_sha256", "command", "args", "allow_offline", "preserves_other_settings", "contains_credentials", "already_configured", "committed", "replayed"}


def binding_for(value, allow_offline):
    return {"journey_id": value["request_id"], "journey_sha256": value["plan_sha256"], "native": value["native"], "allow_offline": allow_offline,
            "connector_sha256": files.sha(Path(__file__).absolute())}


def inspect_saved(saved, value):
    operator.exact(saved, {"schema_version", "request_id", "binding", "plan_sha256", "agents", "superseded"})
    operator.identifier(saved["request_id"])
    offline = saved["binding"].get("allow_offline") if isinstance(saved["binding"], dict) else None
    if type(offline) is not bool or saved["schema_version"] != 1 or saved["binding"] != binding_for(value, offline) or saved["plan_sha256"] != files.digest(saved["binding"]) or not isinstance(saved["agents"], list) or len(saved["agents"]) != len(value["projects"]) * 2 or not isinstance(saved["superseded"], list) or len(saved["superseded"]) > 256:
        raise Refused("agent setup inputs/policy changed; no automatic adoption or renewal")
    for ordinal, entry in enumerate(saved["agents"]):
        operator.exact(entry, {"project_index", "caller", "approval", "receipt", "configuration", "configured"})
        index = entry["project_index"]
        if type(index) is not int or index != ordinal // 2 or entry["caller"] != list(CALLERS)[ordinal % 2] or type(entry["configured"]) is not bool:
            raise Refused("invalid agent setup stage metadata")
        selected = value["projects"][index]
        if entry["approval"] is not None:
            approval(entry["approval"], selected["selection"], entry["caller"])
            expected_mode = "curated_project_v1" if value["plan"]["input"]["native"].get("curated_sync") else "shared_publications_v1"
            if entry["approval"]["request"].get("replication_mode", "shared_publications_v1") != expected_mode:
                raise Refused("agent corpus mode differs from this native journey")
        if entry["receipt"] is not None:
            if entry["approval"] is None:
                raise Refused("agent receipt without an exact prepared request")
            verified(entry["receipt"], entry["approval"], selected["subscription"]["bootstrap"]["scope"]["company"])
        if entry["configured"] and any(entry[k] is None for k in ["approval", "receipt", "configuration"]):
            raise Refused("agent configuration lacks its review receipts")
    return saved


def status(journal, value=None):
    """Local recorded progress only; this never grants or checks remote access."""
    journal = files.path(journal)
    value = value or journey.inspect(journal)
    output = Path(str(journal) + ".agents.json")
    result = {"state": "not_started", "journal": str(output), "configured_agents": 0, "required_agents": len(value["projects"]) * 2,
              "current_authority": "not asserted by local status", "native_acceptance": "unverified"}
    if not output.exists():
        return result
    saved = inspect_saved(files.read(output), value)
    configured = sum(e["configured"] for e in saved["agents"])
    result.update(state="recorded_complete" if configured == len(saved["agents"]) else "recorded_partial", configured_agents=configured,
                  allow_offline=saved["binding"]["allow_offline"], replacements=len(saved["superseded"]))
    return result


class Memory(journey.Memory):
    def public(self, args):
        _, text = self.native.command(self.args(args))
        try:
            return json.loads(text)
        except (ValueError, TypeError):
            raise Refused("public agent metadata invalid; body suppressed") from None

    def prepare_agent(self, slot, caller, *, new_request=False):
        return self.public(["projects", "agents", "prepare", *self.company(), "--caller", caller,
                            *(["--replication-mode", "curated_project_v1"] if self.source["native"].get("curated_sync") else []),
                            "--workspace-journal", slot["journal"], *(["--new-request"] if new_request else []),
                            *([] if self.source["no_browser"] else ["--open-browser"])])

    def agent_status(self, agent_id):
        return self.public(["projects", "agents", "status", *self.company(), "--agent-id", agent_id])

    def config_args(self, slot, agent_id, profile, allow_offline):
        return ["projects", "agents", "configure", *self.company(), "--agent-id", agent_id,
                "--profile", profile, "--workspace-journal", slot["journal"], *(["--allow-offline"] if allow_offline else [])]

    def config_preview(self, slot, agent_id, profile, allow_offline):
        return self.public(self.config_args(slot, agent_id, profile, allow_offline))

    def config_apply(self, slot, agent_id, profile, allow_offline, digest):
        self.execute([*self.config_args(slot, agent_id, profile, allow_offline), "--plan-sha256", digest, "--confirm"])


def approval(value, selected, caller):
    operator.exact(value, {"device_id", "approval_endpoint", "request", "agent_id", "contains_secret", "automatic_approval"})
    operator.identifier(value["agent_id"])
    if value["device_id"] != selected["device_id"] or value["approval_endpoint"] != f"/v1/onboarding/devices/{value['device_id']}/agents" or value["contains_secret"] is not False or value["automatic_approval"] is not False:
        raise Refused("agent request parent/disclosure metadata differs")
    r = value["request"]
    operator.exact(r, {"request_id", "agent_id", "principal_id", "token_id", "caller", "provider_id", "credential_sha256", "expires_at", "projects", "confirmed"} | ({"replication_mode"} if "replication_mode" in r else set()))
    if r.get("replication_mode", "shared_publications_v1") not in {"shared_publications_v1", "curated_project_v1"}:
        raise Refused("unsupported agent knowledge scope")
    for name in ["request_id", "agent_id", "principal_id", "token_id"]:
        operator.identifier(r[name])
    operator.pin(r["credential_sha256"])
    journey.public_text(r["expires_at"], 64)
    if r["agent_id"] != value["agent_id"] or r["caller"] != caller or r["provider_id"] != CALLERS[caller] or r["confirmed"] is not False or not isinstance(r["projects"], list) or len(r["projects"]) != 1:
        raise Refused("separate caller/provider and explicit human approval required")
    p, m = r["projects"][0], selected["manifest"]
    entry = journey.manifest_entry(m)
    operator.exact(p, {"area", "owner_instance_id", "project_id", "manifest_revision", "manifest_sha256", "offline_confirmed"})
    operator.pin(p["manifest_sha256"])
    if p["area"] != entry["area"] or p["owner_instance_id"] != entry["owner_instance_id"] or p["project_id"] != m["project_id"] or type(p["manifest_revision"]) is not int or p["manifest_revision"] != m["revision"] or p["offline_confirmed"] is not True:
        raise Refused("agent selection differs from the exact reviewed project")
    if {"name": caller, "provider_id": CALLERS[caller], "destination_id": CALLERS[caller]} not in entry["destinations"]:
        raise Refused("project operator has not approved this model/output destination")
    return value


def verified(receipt, prepared, company_id):
    operator.exact(receipt, {"schema_version", "request_id", "agent_id", "device_id", "human_id", "company_id", "principal_id", "token_id", "issuer", "subject", "caller", "provider_id", "credential_sha256", "expires_at", "projects", "approved_at", "revoked_at"} | ({"replication_mode"} if "replication_mode" in receipt else set()))
    r = prepared["request"]
    if receipt.get("replication_mode", "shared_publications_v1") != r.get("replication_mode", "shared_publications_v1"):
        raise Refused("agent knowledge scope changed from the explicit request")
    for name in ["request_id", "agent_id", "principal_id", "token_id", "caller", "provider_id", "credential_sha256", "expires_at"]:
        if receipt[name] != r[name]:
            raise Refused("approved agent receipt changed from the exact request")
    operator.identifier(receipt["human_id"])
    for field in ["issuer", "subject", "approved_at"]:
        journey.public_text(receipt[field], 2048)
    wanted = [{"selection": r["projects"][0], "destination": {"name": r["caller"], "provider_id": r["provider_id"], "destination_id": r["provider_id"]}}]
    if receipt["schema_version"] != 1 or receipt["device_id"] != prepared["device_id"] or receipt["company_id"] != company_id or receipt["projects"] != wanted or receipt["revoked_at"] is not None:
        raise Refused("current explicit agent disclosure grant required")
    # The pinned native CLI checks expiry, current parent authority and owner
    # trust online. This metadata journal is never an authorization mechanism.
    return receipt


def configuration(value, source, slot, prepared, subscription, profile, allow_offline):
    operator.exact(value, PREVIEW_KEYS)
    for name in ["plan_sha256", "after_sha256"]:
        operator.pin(value[name])
    if value["before_sha256"] is not None:
        operator.pin(value["before_sha256"])
    caller = prepared["request"]["caller"]
    expected_path = str(Path(slot["workspace"]) / (".codex/config.toml" if caller == "codex" else ".mcp.json"))
    expected_args = ["projects", "agents", "mcp", "--company", source["company"], "--agent-id", prepared["agent_id"], "--profile", str(profile)]
    if source["allow_loopback_development"]:
        expected_args.append("--allow-loopback")
    if allow_offline:
        expected_args.append("--allow-offline")
    if value["schema_version"] != 1 or value["agent_id"] != prepared["agent_id"] or value["caller"] != caller or value["provider_id"] != CALLERS[caller] or value["device_id"] != prepared["device_id"] or value["scope"] != subscription["bootstrap"]["scope"] or value["workspace"] != slot["workspace"] or value["settings_path"] != expected_path or value["args"] != expected_args or Path(value["command"]).resolve(strict=True) != Path(source["native"]["memory"]["path"]).resolve(strict=True) or value["allow_offline"] is not allow_offline or value["preserves_other_settings"] is not True or value["contains_credentials"] is not False:
        raise Refused("agent settings preview differs from the exact command/scope/disclosure plan")
    for flag in ["already_configured", "committed", "replayed"]:
        if type(value[flag]) is not bool:
            raise Refused("invalid agent configuration status")
    return value


def browser_confirmation(prepared):
    print(json.dumps(prepared, indent=2, sort_keys=True))
    print("Review and approve this separate project/model disclosure grant in the company browser. Nothing is automatically approved.")
    print("If signed out, sign in then paste/import this public JSON in Access → Devices → Agent access. It contains no token.")
    try:
        answer = input("Press Enter only after browser approval; type anything to cancel: ")
    except EOFError:
        raise Refused("agent approval cancelled; same request retained") from None
    if answer != "":
        raise Refused("agent approval cancelled; same request retained")


def run(journal, native, *, allow_offline=False, renew_grant=None, memory=None, prompt=journey.confirm, browser_prompt=browser_confirmation):
    journal = files.path(journal)
    value = journey.inspect(journal)
    if value["stages"].get("native services") != "done":
        raise Refused("finish the native journey before project agent configuration")
    source = value["plan"]["input"]
    _, native_journal = journey.native_paths(journal, value)
    native_value = operator.inspect(native_journal)
    native.preflight(native_value["plan"])
    operator.verify_files(native_value)
    if memory is None and (not sys.stdin.isatty() or not sys.stdout.isatty()):
        raise Refused("agent setup requires interactive human browser and settings review")
    memory = memory or Memory(source)
    output = Path(str(journal) + ".agents.json")
    journey.private_parent(output)
    for root in [Path(source["native"]["root"]), *[Path(p["workspace"]) for p in source["projects"]]]:
        files.outside(output, root)
        files.outside(Path(str(output) + ".memory-lock"), root)
    binding = binding_for(value, allow_offline)
    with files.Lock(output):
        if output.exists():
            saved = inspect_saved(files.read(output), value)
            if saved["binding"] != binding:
                raise Refused("agent setup inputs/policy changed; no automatic adoption or renewal")
        else:
            saved = {"schema_version": 1, "request_id": str(uuid.uuid4()), "binding": binding, "plan_sha256": files.digest(binding),
                     "agents": [{"project_index": i, "caller": caller, "approval": None, "receipt": None, "configuration": None, "configured": False} for i in range(len(source["projects"])) for caller in CALLERS], "superseded": []}
            files.missing(output, files.encoded(saved))
        if renew_grant is not None:
            replace_grant(saved, output, value, source, memory, prompt, renew_grant)
        for ordinal, entry in enumerate(saved["agents"]):
            operator.exact(entry, {"project_index", "caller", "approval", "receipt", "configuration", "configured"})
            i, caller = entry["project_index"], entry["caller"]
            if type(i) is not int or i != ordinal // 2 or caller != list(CALLERS)[ordinal % 2] or type(entry["configured"]) is not bool:
                raise Refused("invalid agent setup stage metadata")
            slot, selected, subscription = source["projects"][i], value["projects"][i]["selection"], value["projects"][i]["subscription"]
            if entry["approval"] is None:
                entry["approval"] = approval(memory.prepare_agent(slot, caller), selected, caller)
                files.save(output, saved)
            else:
                approval(entry["approval"], selected, caller)
            prepared = entry["approval"]
            expected_mode = "curated_project_v1" if source["native"].get("curated_sync") else "shared_publications_v1"
            if prepared["request"].get("replication_mode", "shared_publications_v1") != expected_mode:
                raise Refused("prepared agent corpus mode differs from the selected native workflow")
            if entry["receipt"] is None:
                browser_prompt(prepared)
            receipt = verified(memory.agent_status(prepared["agent_id"]), prepared, subscription["bootstrap"]["scope"]["company"])
            if entry["receipt"] is not None and entry["receipt"] != receipt:
                raise Refused("agent receipt changed; no automatic renewal")
            entry["receipt"] = receipt
            files.save(output, saved)
            profile = Path(source["native"]["root"]) / "profiles" / (subscription["subscription"] + ".json")
            preview = configuration(memory.config_preview(slot, prepared["agent_id"], profile, allow_offline), source, slot, prepared, subscription, profile, allow_offline)
            if entry["configuration"] is None:
                prompt(f"{caller} settings:{i}", preview["plan_sha256"], preview)
                entry["configuration"] = preview
                files.save(output, saved)  # Exact metadata-only intent before filesystem effects.
            original = configuration(entry["configuration"], source, slot, prepared, subscription, profile, allow_offline)
            if not preview["already_configured"] and preview != original:
                raise Refused("agent settings changed after review; existing settings preserved")
            if preview["already_configured"] and preview["after_sha256"] != original["after_sha256"]:
                raise Refused("configured project settings changed; explicit new review required")
            memory.config_apply(slot, prepared["agent_id"], profile, allow_offline, original["plan_sha256"])
            final = configuration(memory.config_preview(slot, prepared["agent_id"], profile, allow_offline), source, slot, prepared, subscription, profile, allow_offline)
            if not final["already_configured"] or final["after_sha256"] != original["after_sha256"]:
                raise Refused("agent configuration not confirmed; intent retained")
            entry["configured"] = True
            files.save(output, saved)
        result = {"agent_journal": str(output), "configured_agents": len(saved["agents"]), "native_acceptance": "unverified", "paid_calls": False,
                  "remote_expertise": "not enabled by cache bridge", "automatic_renewal": False}
        print(json.dumps(result, indent=2, sort_keys=True))
        return result


def replace_grant(saved, output, journey_value, source, memory, prompt, old_id):
    """Explicit replacement; preserve the denied/expired ID and its history.

    If preparation was interrupted, normal prepare returns the already-created
    new Keychain identity. Never repeatedly generate --new-request credentials.
    """
    operator.identifier(old_id)
    for prior in saved["superseded"]:
        operator.exact(prior, {"entry", "renewed_to"})
        if prior["entry"]["approval"]["agent_id"] == old_id:
            if any(e["approval"] and e["approval"]["agent_id"] == prior["renewed_to"] for e in saved["agents"]):
                return  # Exact replacement replay, still verified online below.
            raise Refused("this replacement was superseded again; use the current exact agent ID")
    candidates = [e for e in saved["agents"] if e["approval"] and e["approval"]["agent_id"] == old_id]
    if len(candidates) != 1 or len(saved["superseded"]) >= 256:
        raise Refused("one exact existing grant and bounded replacement history required")
    entry = candidates[0]
    i, caller = entry["project_index"], entry["caller"]
    if type(i) is not int or not 0 <= i < len(source["projects"]) or caller not in CALLERS:
        raise Refused("invalid replacement project/caller")
    slot, selected = source["projects"][i], journey_value["projects"][i]["selection"]
    approval(entry["approval"], selected, caller)
    try:
        current = memory.prepare_agent(slot, caller)
    except Refused:
        # An expired/denied index cannot be prepared normally. This fallback is
        # permitted ONLY inside an explicitly requested renewal; the native CLI
        # still checks current parent/project authority and prior unusability.
        current = memory.prepare_agent(slot, caller, new_request=True)
    fresh = approval(current, selected, caller)
    if fresh["agent_id"] == old_id:
        # Native CLI refuses unless the prior request is expired or definitively
        # denied, and retains its monotonic fence. No silent consent extension.
        fresh = approval(memory.prepare_agent(slot, caller, new_request=True), selected, caller)
    if fresh["agent_id"] == old_id:
        raise Refused("replacement must have independent fresh IDs and credentials")
    preview = {"old_agent_id": old_id, "fresh_public_request": fresh, "retains_old_history": True, "inherits_approval": False}
    prompt("fresh agent grant", files.digest(preview), preview)
    import copy
    saved["superseded"].append({"entry": copy.deepcopy(entry), "renewed_to": fresh["agent_id"]})
    entry.update(approval=fresh, receipt=None, configuration=None, configured=False)
    files.save(output, saved)
