# CA release-transfer USB — 2026-09-22

Status: operator-approved reformat and public-handoff copy in progress. No CA
installation, key enrollment, activation or dataset transfer is authorized here.
Olivier confirmed erasing the exact device after its identity, all three
partitions and visible filenames were disclosed. This is a quick format, not
secure erasure or a backup of discarded files.

Task checkpoint `9897fbf1-9d13-4d33-b733-922bb1178c4b`, memory
`ed7715bd-d3a4-4e56-a1f5-0becb444d1a6`, starting at `2a31052d`.
The original user confirmation was `yes`; the task-start capture expands that
into the explicitly confirmed scope rather than a verbatim quotation.

## Exact approved device

- `/dev/sdc`, 62,534,975,488 bytes (58.2 GiB), removable USB `STORAGE DEVICE`.
- Device 8:32, kernel disk sequence 126; generic serial
  `Generic_STORAGE_DEVICE-0:0` is not a globally unique identity.
- Port path `pci-0000:06:00.0-usb-0:1.2:1.0-scsi-0:0:0:0`.
- Existing DOS partition-table identity `24a1e612`.
- `/dev/sdc1`: 3,091,628,032 bytes, ext4 `TransferDisk`, UUID
  `5e18bd6e-363c-47db-9d01-9eecf82ca7aa`.
- `/dev/sdc2`: 5,242,880 bytes, no recognized filesystem.
- `/dev/sdc3`: 59,441,676,288 bytes, ext4 `writable`, UUID
  `2ce0e5ad-4f31-4d97-bdac-33a94271b931`.

Top-level filenames include `memory-layer.env`, `ca.json` and two installation-
log directories. Their contents were not read or backed up. The earlier signing
recovery USB filesystem (`BIOS UPGRAD`, UUID `66F0-75AD`) is not the current
partition layout; its backup directory was not an input to this operation.

Recheck device identity after administrative authentication and before writes.
Unmount only the two observed USB filesystems without force; stop on busy state,
unexpected partitions, changed disk sequence or device holders. Replace the
partition table with one full-size FAT32/LBA partition at the 1-MiB offset,
labelled `MEMORY_CA`. Do not touch any SATA/NVMe disk.

## Handoff binding and verification

Copy only the [signed public handoff](signed-release-handoff.md), source directory
`/data/tmp/memory-signed-release-20260922.0Ov3K6/handoff`. Its top-level inventory
SHA-256 must remain
`de6339d324bff0a2abd9ff4f1aa697692b7e10aed69d2417baad21b7662517fb`.
Destination folder: `memory-release-2.1.0-rc.1` on the newly identified volume.
Verify all 55 entries, compare the inventory itself, flush writes, and recheck
after remount before declaring the media ready. Keep private signing keys,
encrypted signing backups, live credentials and datasets off this transfer USB.

One-off identity-bound operation and evidence:
`/data/tmp/memory-ca-transfer-usb-20260922.GPa4zJ`.
Do not reuse its formatter after reconnection or against another device.

## Pre-format interruptions

The first administrative attempt stopped during read-only inspection: requesting
`PATH` without `NAME`/`--tree` made `lsblk` return a flat JSON list rather than a
tree. No unmount or write occurred. The corrected command explicitly requests
`--tree`; the original formatter and `format-result.json` are retained.

The second attempt passed identity checks and unmounted `TransferDisk`, but
`writable` was busy. `format-result-v2.json` confirms execution stopped before
`sfdisk` or `mkfs` writes. Files (`nautilus`, PID 1712105 at observation) holds the
volume; normal desktop `gio mount --unmount` also refused it. No process was
killed and no forced/lazy unmount was attempted. The operator was asked to close
only the Files window/tab using that USB volume. Both original ext4 UUIDs and all
three partition sizes still match; no files have been erased. A retry must retain
the same complete device checks while accepting already-unmounted approved
partitions. It must stop if the disk is reconnected or otherwise changes identity.
