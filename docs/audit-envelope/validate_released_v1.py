#!/usr/bin/env python3
"""Check mcao.audit-envelope.v1 fixtures against a released AEC checkout.

Three fail-closed checks:
  siem/     released schemas/v1/event.schema.json plus the release's semantic_error().
  source/   native schema mcao.audit-envelope.v1.schema.json plus cross-field rules:
            user-actor identity equality and day_key derived from ts in timezone.
  invalid/  each record must be rejected by the source checks on its expected field.

Usage:
  validate_released_v1.py --release DIR --fixtures DIR [--native-schema FILE]
Exit status is 0 only when every check passes.
"""

import argparse
import importlib.util
import json
import re
import sys
from datetime import datetime
from pathlib import Path
from zoneinfo import ZoneInfo

from jsonschema import Draft202012Validator, FormatChecker

DEFAULT_NATIVE_SCHEMA = Path(__file__).with_name("mcao.audit-envelope.v1.schema.json")

# Field on which each invalid fixture must be rejected. A rejection on any other field fails.
EXPECTED_INVALID = {
    "actor-identity-mismatch.json": "actor.id",
    "bad-version.json": "schema",
    "impossible-date.json": "ts",
    "malformed-event-id.json": "event_id",
    "malformed-home-id.json": "home_id",
    "malformed-pr.json": "pr",
    "malformed-repo.json": "repo",
    "missing-identity.json": "user",
    "seq-zero-with-prev-hash.json": "prev_hash",
    "session-missing.json": "session_id",
}


def load_release_tests(release: Path):
    path = release / "conformance" / "test_contracts.py"
    spec = importlib.util.spec_from_file_location("aec_v1_contract_tests", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def check_siem(release, fixtures: Path) -> list[str]:
    validator = Draft202012Validator(
        release.load("schemas/v1/event.schema.json"), format_checker=FormatChecker()
    )
    failures = []
    for path in sorted((fixtures / "siem").glob("*.json")):
        event = json.loads(path.read_text())
        schema_errors = [error.message for error in validator.iter_errors(event)]
        semantic = None if schema_errors else release.semantic_error(event)
        if schema_errors or semantic is not None:
            failures.append(f"siem/{path.name}: schema={schema_errors[:1]} semantic={semantic}")
        else:
            source_path = fixtures / "source" / path.name
            if not source_path.is_file():
                failures.append(f"siem/{path.name}: paired source fixture is missing")
                continue
            source = json.loads(source_path.read_text())
            expected = {
                "schema": "mcao.siem.event/v1",
                "event_type": "audit.activity",
                "event_id": f"audit.activity:{source['event_id']}",
                "occurred_at_utc": source["ts"],
                "source_route": source["schema"],
                "data": {name: source.get(name) for name in event["data"]},
                "lineage": {name: source[name] for name in ("event_id", "seq", "prev_hash")},
            }
            if event != expected:
                failures.append(f"siem/{path.name}: paired source projection differs")
            else:
                print(f"PASS siem/{path.name} and paired source projection")
    return failures


def native_validator(schema_path: Path) -> Draft202012Validator:
    # FormatChecker silently skips date-time when rfc3339-validator is missing.
    if "date-time" not in FormatChecker().checkers:
        raise SystemExit("rfc3339-validator is required: the date-time format would go unchecked")
    schema = json.loads(schema_path.read_text())
    Draft202012Validator.check_schema(schema)
    return Draft202012Validator(schema, format_checker=FormatChecker())


def error_field(error) -> str:
    if error.validator == "required":
        return re.match(r"'([^']+)'", error.message).group(1)
    return ".".join(str(part) for part in error.path)


def source_problems(validator, record) -> list[tuple[str, str]]:
    schema_errors = [(error_field(error), error.message) for error in validator.iter_errors(record)]
    if schema_errors:
        return schema_errors
    problems = []
    if record["actor"]["type"] == "user" and record["actor"]["id"] != record["user"]["id"]:
        problems.append(("actor.id", "user actor id differs from the resolved user id"))
    try:
        instant = datetime.fromisoformat(record["ts"].replace("Z", "+00:00"))
        local_day = instant.astimezone(ZoneInfo(record["timezone"])).date().isoformat()
    except (KeyError, ValueError) as error:
        return problems + [("timezone", f"cannot derive local day: {error}")]
    if local_day != record["day_key"]:
        problems.append(("day_key", f"{record['day_key']} != local {local_day}"))
    return problems


def check_source(fixtures: Path, validator) -> list[str]:
    failures = []
    for path in sorted((fixtures / "source").glob("*.json")):
        problems = source_problems(validator, json.loads(path.read_text()))
        if problems:
            failures.append(f"source/{path.name}: " + "; ".join(f"{f}: {m}" for f, m in problems))
        else:
            print(f"PASS source/{path.name}")
    return failures


def check_invalid(fixtures: Path, validator) -> list[str]:
    failures = []
    present = {path.name for path in (fixtures / "invalid").glob("*.json")}
    if present != set(EXPECTED_INVALID):
        failures.append(
            f"invalid/: fixtures {sorted(present)} != expected {sorted(EXPECTED_INVALID)}"
        )
    for name, expected in sorted(EXPECTED_INVALID.items()):
        path = fixtures / "invalid" / name
        if not path.exists():
            continue
        fields = {field for field, _ in source_problems(validator, json.loads(path.read_text()))}
        if expected in fields:
            print(f"PASS invalid/{name} rejected on {expected}")
        else:
            failures.append(
                f"invalid/{name}: expected rejection on {expected}, got {sorted(fields) or 'none'}"
            )
    return failures


def main() -> int:
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    parser.add_argument("--release", required=True, type=Path)
    parser.add_argument("--fixtures", required=True, type=Path)
    parser.add_argument("--native-schema", type=Path, default=DEFAULT_NATIVE_SCHEMA)
    args = parser.parse_args()
    for directory in ("source", "siem", "invalid"):
        if not any((args.fixtures / directory).glob("*.json")):
            parser.error(f"{args.fixtures / directory}: no JSON fixtures found")
    release = load_release_tests(args.release)
    validator = native_validator(args.native_schema)
    failures = (
        check_siem(release, args.fixtures)
        + check_source(args.fixtures, validator)
        + check_invalid(args.fixtures, validator)
    )
    for failure in failures:
        print(f"FAIL {failure}", file=sys.stderr)
    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
