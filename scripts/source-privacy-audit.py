#!/usr/bin/env python3
"""Read-only inventory of privacy patterns in an immutable, tracked Git tree.

Only fixed rule IDs, blob IDs, categories and counts are reported. A match is
a candidate for review, not evidence of an actual secret. No file is edited.
"""
import argparse
import collections
import io
import json
import re
import subprocess
import sys
import tarfile
import zipfile
from pathlib import Path

MAX_GIT_BYTES = 256 * 1024 * 1024
MAX_MEMBER_BYTES = 64 * 1024 * 1024
MAX_EXPANDED_BYTES = 512 * 1024 * 1024
MAX_MEMBERS = 40000
MAX_DEPTH = 3


def git(root, *args, payload=None):
    result = subprocess.run(
        ["git", "-C", str(root), "--no-replace-objects", *args],
        input=payload, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
        check=True, timeout=180,
    )
    if len(result.stdout) > MAX_GIT_BYTES:
        raise ValueError("Git output budget exceeded")
    return result.stdout


def category(path):
    parts = path.lower().replace("\\", "/").split("/")
    name = parts[-1]
    if name.startswith(("license", "licence", "notice", "third_party", "copying")):
        return "license-notice"
    if any(p in ("test", "tests", "__tests__", "fixtures", "testdata", "snapshots") for p in parts):
        return "test-data"
    if parts[0] == "vendor":
        return "vendor"
    if name in ("package.json", "pnpm-lock.yaml", "package-lock.json"):
        return "package-metadata"
    if "docs" in parts or name.endswith((".md", ".rst")):
        return "documentation"
    if parts[0] in ("scripts", ".github"):
        return "build-ci"
    return "source-asset"


def load_rules(path):
    data = json.loads(path.read_text(encoding="utf-8"))
    if data.get("schema_version") != 3:
        raise ValueError("invalid policy")
    rules = {}
    for field, prefix in (("privacy_patterns", "privacy-"), ("secret_patterns", "")):
        for rule in data[field]:
            name = prefix + rule["name"]
            if not re.fullmatch(r"[a-z][a-z0-9-]{0,80}", name) or name in rules:
                raise ValueError("invalid rule ID")
            rules[name] = re.compile(rule["pattern"])
    if not rules:
        raise ValueError("empty policy")
    return rules


class Audit:
    def __init__(self, rules):
        self.rules = rules
        self.expanded = 0
        self.members = 0
        self.archives = 0
        self.unverified = 0
        self.binary_blobs = 0

    def text_rules(self, text):
        return {name for name, pattern in self.rules.items() if pattern.search(text)}

    def inspect(self, data, depth=0):
        findings = self.text_rules(data.decode("utf-8", errors="replace"))
        if data.startswith((b"\xff\xfe", b"\xfe\xff")):
            findings.update(self.text_rules(data.decode("utf-16", errors="replace")))
        if depth == 0 and b"\x00" in data:
            self.binary_blobs += 1
        stream = io.BytesIO(data)
        is_zip = data.startswith((b"PK\x03\x04", b"PK\x05\x06", b"PK\x07\x08"))
        is_tar = data.startswith((b"\x1f\x8b", b"BZh", b"\xfd7zXZ\x00")) or data[257:262] == b"ustar"
        if not (is_zip or is_tar):
            return findings
        if depth >= MAX_DEPTH:
            self.unverified += 1
            return findings
        try:
            if is_zip:
                with zipfile.ZipFile(stream) as archive:
                    self.archives += 1
                    for entry in archive.infolist():
                        findings.update(self.text_rules(entry.filename))
                        if not entry.is_dir():
                            findings.update(self.member(entry.file_size, lambda: archive.open(entry), depth))
            else:
                with tarfile.open(fileobj=stream, mode="r:*") as archive:
                    self.archives += 1
                    for entry in archive:
                        findings.update(self.text_rules(entry.name))
                        if entry.isfile():
                            findings.update(self.member(entry.size, lambda: archive.extractfile(entry), depth))
                        elif entry.islnk() or entry.issym():
                            findings.update(self.text_rules(entry.linkname))
        except (OSError, ValueError, EOFError, RuntimeError, zipfile.BadZipFile, tarfile.TarError):
            self.unverified += 1
        return findings

    def member(self, size, opener, depth):
        if (size < 0 or size > MAX_MEMBER_BYTES or self.members >= MAX_MEMBERS
                or self.expanded + size > MAX_EXPANDED_BYTES):
            self.unverified += 1
            return set()
        self.members += 1
        self.expanded += size
        with opener() as stream:
            data = stream.read(size + 1)
        if len(data) != size:
            self.unverified += 1
            return set()
        return self.inspect(data, depth + 1)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("root", type=Path)
    parser.add_argument("revision")
    parser.add_argument("policy", type=Path)
    args = parser.parse_args()
    if not re.fullmatch(r"[0-9a-f]{40}", args.revision):
        raise ValueError("revision must be immutable")
    revision = git(args.root, "rev-parse", "--verify", args.revision + "^{commit}").decode().strip()
    if revision != args.revision:
        raise ValueError("revision mismatch")
    entries = []
    submodules = 0
    for record in git(args.root, "ls-tree", "-r", "-z", "--full-tree", revision).split(b"\x00"):
        if not record:
            continue
        header, path = record.split(b"\t", 1)
        mode, kind, sha = header.decode("ascii").split(" ")
        if kind != "blob":
            submodules += 1
            continue
        if not re.fullmatch(r"[0-9a-f]{40}", sha):
            raise ValueError("invalid object ID")
        entries.append((sha, path.decode("utf-8", errors="replace")))
    grouped = collections.defaultdict(list)
    for sha, path in entries:
        grouped[sha].append(path)
    audit = Audit(load_rules(args.policy))
    report = collections.Counter()
    flagged = []
    bytes_scanned = 0
    # Batch reads avoid a separate Git process for each file and preserve the
    # committed bytes, including binary and symlink blobs.
    shas = list(grouped)
    for start in range(0, len(shas), 128):
        batch = shas[start:start + 128]
        raw = git(args.root, "cat-file", "--batch", payload=("\n".join(batch) + "\n").encode())
        cursor = 0
        for sha in batch:
            end = raw.index(b"\n", cursor)
            object_id, kind, size = raw[cursor:end].decode("ascii").split(" ")
            size = int(size)
            cursor = end + 1
            if object_id != sha or kind != "blob" or size < 0 or raw[cursor + size:cursor + size + 1] != b"\n":
                raise ValueError("invalid Git batch framing")
            data = raw[cursor:cursor + size]
            cursor += size + 1
            bytes_scanned += size
            content_rules = audit.inspect(data)
            for path in grouped[sha]:
                rules = content_rules | audit.text_rules(path)
                area = category(path)
                for rule in sorted(rules):
                    report[(area, rule)] += 1
                if rules:
                    flagged.append((sha, area, ",".join(sorted(rules))))
        if cursor != len(raw):
            raise ValueError("unexpected Git batch data")
    print("central-ci: source-privacy-summary " + json.dumps({
        "revision": revision, "tracked_files": len(entries), "unique_blobs": len(grouped),
        "bytes_scanned": bytes_scanned, "binary_blobs": audit.binary_blobs,
        "archives_opened": audit.archives, "archive_members_read": audit.members,
        "unverified_archive_entries": audit.unverified, "submodules_not_scanned": submodules,
        "candidate_files": len(flagged), "reported_files": min(len(flagged), 4000),
    }, sort_keys=True))
    for (area, rule), count in sorted(report.items()):
        print(f"central-ci: source-privacy-count {area} {rule} {count}")
    for sha, area, rules in flagged[:4000]:
        print(f"central-ci: source-privacy-file {sha} {area} {rules}")


if __name__ == "__main__":
    try:
        main()
    except Exception:
        print("source privacy audit incomplete; sensitive details suppressed", file=sys.stderr)
        sys.exit(1)
