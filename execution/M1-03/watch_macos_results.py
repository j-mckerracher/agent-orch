"""Watch origin for a newly pushed M1-03 macOS evidence branch."""

from __future__ import annotations

import argparse
import subprocess
import time
from pathlib import Path


FAILED_ATTEMPT_BRANCH = (
    "codex/m1-03-macos-evidence-"
    "20261108T010732Z-4f607ba8-f2ec-4014-a86b-e484ca255bfc"
)
EVIDENCE_PATTERN = "refs/heads/codex/m1-03-macos-evidence-*"
EVIDENCE_ROOT = "docs/evidence/m1-03-macos/"


def git(repo: Path, *args: str, timeout: int = 30) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["git", "-C", str(repo), *args],
        text=True,
        capture_output=True,
        timeout=timeout,
        check=False,
    )


def remote_evidence_refs(repo: Path) -> list[tuple[str, str]]:
    result = git(repo, "ls-remote", "--heads", "origin", EVIDENCE_PATTERN)
    if result.returncode != 0:
        detail = result.stderr.strip() or f"git exited {result.returncode}"
        raise RuntimeError(f"cannot query origin: {detail}")

    refs: list[tuple[str, str]] = []
    for line in result.stdout.splitlines():
        fields = line.split("\t", 1)
        if len(fields) != 2 or not fields[1].startswith("refs/heads/"):
            continue
        branch = fields[1][len("refs/heads/") :]
        if branch == FAILED_ATTEMPT_BRANCH:
            continue
        refs.append((fields[0], branch))
    return refs


def inspect_branch(repo: Path, advertised_sha: str, branch: str) -> bool:
    local_ref = f"refs/remotes/origin/{branch}"
    fetched = git(repo, "fetch", "origin", f"refs/heads/{branch}:{local_ref}")
    if fetched.returncode != 0:
        print(f"Fetch failed for {branch}: {fetched.stderr.strip()}", flush=True)
        return False

    current = git(repo, "rev-parse", local_ref)
    if current.returncode != 0:
        print(f"Could not resolve fetched branch {branch}", flush=True)
        return False
    fetched_sha = current.stdout.strip()

    files = git(repo, "ls-tree", "-r", "--name-only", local_ref, "--", EVIDENCE_ROOT)
    if files.returncode != 0:
        print(f"Could not list evidence files on {branch}", flush=True)
        return False
    result_files = [
        path for path in files.stdout.splitlines() if path.endswith("/result.md")
    ]
    if not result_files:
        print(
            f"Found {branch} at {fetched_sha}, but it has no result.md yet; continuing.",
            flush=True,
        )
        return False

    print(f"NEW_M1_03_MACOS_EVIDENCE_BRANCH={branch}", flush=True)
    print(f"EVIDENCE_COMMIT={fetched_sha} (advertised {advertised_sha})", flush=True)
    for path in result_files:
        print(f"--- {path} ---", flush=True)
        result = git(repo, "show", f"{local_ref}:{path}")
        if result.returncode == 0:
            print(result.stdout.rstrip(), flush=True)
        else:
            print(result.stderr.strip(), flush=True)
    return True


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--repo",
        type=Path,
        default=Path(__file__).resolve().parents[2],
        help="repository checkout containing origin (default: this workspace)",
    )
    parser.add_argument("--interval-seconds", type=int, default=15)
    parser.add_argument("--once", action="store_true", help="poll once and exit")
    args = parser.parse_args()

    repo = args.repo.resolve()
    if args.interval_seconds < 1:
        parser.error("--interval-seconds must be at least 1")

    print(f"Watching origin for M1-03 macOS evidence in {repo}", flush=True)
    print(f"Ignoring already-reviewed branch {FAILED_ATTEMPT_BRANCH}", flush=True)
    polls = 0
    while True:
        polls += 1
        try:
            refs = remote_evidence_refs(repo)
            for sha, branch in refs:
                if inspect_branch(repo, sha, branch):
                    return 0
        except (OSError, RuntimeError, subprocess.TimeoutExpired) as error:
            print(f"Poll {polls} failed: {error}", flush=True)

        if args.once:
            print("No new M1-03 macOS evidence branch yet.", flush=True)
            return 0
        if polls % 4 == 0:
            print(f"Still waiting; checked origin {polls} times.", flush=True)
        time.sleep(args.interval_seconds)


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except KeyboardInterrupt:
        print("Watcher stopped by user.", flush=True)
        raise SystemExit(130)
