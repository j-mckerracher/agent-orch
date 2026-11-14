#!/usr/bin/env python3
"""Run omp headless in one-shot text mode or persistent RPC UI mode.

Examples:
  python utilities/omp_headless.py --provider purdue --model qwen3.8:27b --thinking high "Hello"
  Get-Content prompt.txt -Raw | python utilities/omp_headless.py --provider openai-codex --model gpt-6-luna
  python utilities/omp_headless.py --provider openai-codex --model gpt-6-luna --rpc

In RPC mode, stdin and stdout carry OMP's newline-delimited JSON protocol.
The caller must handle extension_ui_request frames and send responses. See
https://omp.sh/docs/rpc for the protocol, including questions and approvals.

Install omp and configure the selected provider's credentials first. See
https://omp.sh/docs/providers and https://omp.sh/docs/modes.
"""

import argparse
import subprocess
import sys


AGENT_INSTRUCTIONS = """The orchestrating agent is exempt from worker model, thinking,
oh-my-pstack, and Ponytail requirements. It may coordinate from any host using its
available model and tools. It must verify these requirements for delegated workers
as specified in orch-impl-w.md; its own host or model must not block worker dispatch.

The code itself is the primary documentation for agents working on this project.
Document important information alongside the code whenever you create or change it.
Document API contracts, inputs and outputs, errors, side effects, invariants, ownership,
concurrency and security assumptions, and non-obvious design decisions where applicable.
Use clear names and types, and use docstrings or comments where the code alone cannot
communicate the contract or rationale. Keep documentation accurate when behavior changes;
avoid repeating obvious implementation details. Whenever you create or change documentation,
include the date and time of the change in the documentation, including the time zone or UTC
offset. Preserve existing conventions and user
changes. Verify changes with appropriate checks, and report any unverified behavior or
limitations. Every delegated worker adding code MUST use Ponytail and follow the rules supplied by
the installed OMP Ponytail extension. Ensure Ponytail is active before adding code; if
it is unavailable, stop and report the blocker rather than silently proceeding.

Every delegated worker MUST use the installed oh-my-pstack skills.
At the start of work, load poteto-mode through the host's skill mechanism and follow
its applicable workflow, using pstack-pi for host and delegation mechanics. Follow
the applicable instructions for the agent's assigned role and scope. If the required
skills are unavailable, stop and report the blocker. In every delegation brief, require
the child to load and follow these pstack skills and pass this requirement onward, as
well as the Ponytail and documentation requirements for any agent adding code. The
coordinator remains responsible for reviewing and independently verifying results."""


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--provider", required=True, help="OMP provider ID")
    parser.add_argument("--model", required=True, help="Model ID for the provider")
    parser.add_argument("--thinking", choices=("off", "minimal", "low", "medium", "high", "xhigh", "max", "auto"),
                        help="OMP thinking level; omit to use its configured default")
    parser.add_argument("--rpc", action="store_true", help="Pass through persistent RPC UI protocol on stdin/stdout")
    parser.add_argument("prompt", nargs="?", help="Prompt text; if omitted, read from stdin")
    args = parser.parse_args()
    command = ["omp", "--model", f"{args.provider}/{args.model}",
               "--append-system-prompt", AGENT_INSTRUCTIONS]
    if args.thinking is not None:
        command += ["--thinking", args.thinking]

    if args.rpc:
        if args.prompt is not None:
            parser.error("send prompts through the RPC protocol when using --rpc")
        try:
            return subprocess.call(command + ["--mode", "rpc-ui"])
        except FileNotFoundError:
            print("omp was not found on PATH; install it from https://omp.sh/", file=sys.stderr)
            return 127

    if args.prompt is None and sys.stdin.isatty():
        parser.error("provide a prompt argument or pipe a prompt through stdin")
    prompt = args.prompt if args.prompt is not None else sys.stdin.read()
    if not prompt.strip():
        parser.error("prompt must not be empty")

    try:
        result = subprocess.run(
            command + ["--mode", "text"],
            input=prompt.encode("utf-8"),
        )
    except FileNotFoundError:
        print("omp was not found on PATH; install it from https://omp.sh/", file=sys.stderr)
        return 127
    return result.returncode


if __name__ == "__main__":
    sys.exit(main())
