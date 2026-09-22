#!/usr/bin/python3
"""Assemble a fresh public CA support package without reading private datasets."""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import shutil
import tempfile
import zipfile

REPO = Path(__file__).resolve().parents[3]
GUIDES = (
    "docs/reviews/four-area-network/signed-release-handoff.md",
    "docs/reviews/four-area-network/staging-evidence.md",
    "docs/reviews/four-area-network/staging-cutover.md",
    "docs/reviews/four-area-network/ca-console-installation.md",
    "docs/reviews/four-area-network/ca-bookworm-release.md",
    "scripts/areas/releases/console-README.md",
    "scripts/areas/releases/instance-README.md",
    "scripts/areas/releases/README.md",
    "crates/mem-area-ops/README.md",
    "deploy/areas/instance.toml.example",
    "deploy/areas/ca-preflight.sh",
    "deploy/areas/CA-CONSOLE-HANDOFF.md",
)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--release-dir", required=True, type=Path)
    parser.add_argument("--output-parent", required=True, type=Path)
    args = parser.parse_args()
    spec = importlib.util.spec_from_file_location("preflight", Path(__file__).with_name("preflight.py"))
    verifier = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(verifier)
    verifier.verify_inventory(args.release_dir)
    os.umask(0o077)
    output = Path(tempfile.mkdtemp(prefix="memory-ca-agent-handoff-", dir=args.output_parent))
    bundle = output / "ca-agent-handoff"
    bundle.mkdir(mode=0o700)
    for name in ("README.md", "CA-AGENT.md", "NOTION-COMMUNICATION.md"):
        shutil.copyfile(REPO / "docs/user/ca-agent-handoff" / name, bundle / name)
    shutil.copyfile(Path(__file__).with_name("preflight.py"), bundle / "preflight.py")
    for relative in GUIDES:
        destination = bundle / "references" / relative
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(REPO / relative, destination)
    support = sorted(path for path in bundle.rglob("*") if path.is_file())
    inventory = "".join(f"{verifier.digest(path)}  {path.relative_to(bundle).as_posix()}\n" for path in support)
    (bundle / "SHA256SUMS").write_text(inventory, encoding="utf-8")
    support.append(bundle / "SHA256SUMS")
    archive = output / "ca-agent-support.zip"
    with zipfile.ZipFile(archive, "x", compression=zipfile.ZIP_DEFLATED) as packed:
        for path in support:
            packed.write(path, (Path("ca-agent-handoff") / path.relative_to(bundle)).as_posix())
    destination = bundle / "resources/release"
    destination.parent.mkdir()
    # Inventory was verified first; preserve all public original files, not a
    # selectively repacked release. Copy links as links so post-copy verification
    # rejects them rather than following a substituted link to private content.
    shutil.copytree(args.release_dir, destination, symlinks=True)
    verifier.verify_inventory(destination)
    receipt = {
        "format": "memory-ca-agent-package-v1", "bundle": str(bundle),
        "support_archive": str(archive), "support_bytes": archive.stat().st_size,
        "support_sha256": verifier.digest(archive),
        "support_inventory_sha256": verifier.digest(bundle / "SHA256SUMS"),
        "runbook_sha256": verifier.digest(bundle / "CA-AGENT.md"),
        "preflight_sha256": verifier.digest(bundle / "preflight.py"),
        "original_release_inventory_sha256": verifier.PINS["SHA256SUMS"],
        "support_file_count": len(support), "private_dataset_included": False,
        "ca_delivered": False, "ca_installed": False,
    }
    (output / "assembly-receipt.json").write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(receipt, sort_keys=True))


if __name__ == "__main__":
    main()
