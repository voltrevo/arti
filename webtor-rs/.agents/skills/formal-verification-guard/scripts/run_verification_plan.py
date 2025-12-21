#!/usr/bin/env python3
"""
run_verification_plan.py

Run a series of shell commands defined in a simple plan file
and record their outputs and exit codes.

This is a lightweight way to keep a deterministic log of
verification steps for a claim.

Plan file format (simple text):

    # verify_plan.txt
    # Each non-empty, non-comment line is a shell command to run.
    pytest tests/test_algorithm.py
    python scripts/check_numeric_claim.py --test-sum --samples 1000

Usage:
    python scripts/run_verification_plan.py --plan verify_plan.txt --out verify_log.md
"""

import argparse
import subprocess
import sys
from datetime import datetime
from pathlib import Path
from typing import List, Tuple


def parse_plan(plan_path: Path) -> List[str]:
    """Parse a plan file into a list of commands."""
    lines = plan_path.read_text(encoding="utf-8").splitlines()
    commands = []
    for line in lines:
        stripped = line.strip()
        if not stripped or stripped.startswith("#"):
            continue
        commands.append(stripped)
    return commands


def run_command(cmd: str, timeout: int = 300) -> Tuple[int, str, str]:
    """Run a shell command and return (exit_code, stdout, stderr)."""
    try:
        result = subprocess.run(
            cmd,
            shell=True,
            capture_output=True,
            text=True,
            timeout=timeout,
        )
        return result.returncode, result.stdout, result.stderr
    except subprocess.TimeoutExpired:
        return -1, "", f"Command timed out after {timeout}s"
    except Exception as e:
        return -1, "", f"Error running command: {e}"


def run_plan(plan_path: Path, out_path: Path, timeout: int = 300) -> int:
    """Run all commands in a plan and write results to output file."""
    commands = parse_plan(plan_path)
    
    if not commands:
        print(f"[WARN] No commands found in {plan_path}")
        return 0
    
    print(f"Found {len(commands)} commands in plan")
    
    out_lines = []
    out_lines.append(f"# Verification Log\n")
    out_lines.append(f"- **Plan file**: `{plan_path}`\n")
    out_lines.append(f"- **Run at**: {datetime.utcnow().isoformat()}Z\n")
    out_lines.append(f"- **Commands**: {len(commands)}\n\n")
    
    total_failures = 0
    
    for i, cmd in enumerate(commands, 1):
        print(f"[{i}/{len(commands)}] Running: {cmd}")
        
        out_lines.append(f"## Command {i}\n\n")
        out_lines.append(f"```bash\n{cmd}\n```\n\n")
        
        exit_code, stdout, stderr = run_command(cmd, timeout)
        
        status = "[OK]" if exit_code == 0 else "[FAIL]"
        print(f"  {status} Exit code: {exit_code}")
        
        if exit_code != 0:
            total_failures += 1
        
        out_lines.append(f"**Exit code**: {exit_code} {status}\n\n")
        
        if stdout.strip():
            out_lines.append("**STDOUT**:\n")
            out_lines.append(f"```\n{stdout.strip()}\n```\n\n")
        
        if stderr.strip():
            out_lines.append("**STDERR**:\n")
            out_lines.append(f"```\n{stderr.strip()}\n```\n\n")
        
        out_lines.append("---\n\n")
    
    out_lines.append(f"## Summary\n\n")
    out_lines.append(f"- **Total commands**: {len(commands)}\n")
    out_lines.append(f"- **Passed**: {len(commands) - total_failures}\n")
    out_lines.append(f"- **Failed**: {total_failures}\n")
    
    if total_failures == 0:
        out_lines.append(f"\n**Overall**: All checks passed.\n")
    else:
        out_lines.append(f"\n**Overall**: {total_failures} check(s) failed.\n")
    
    out_path.write_text("".join(out_lines), encoding="utf-8")
    print(f"\nWrote verification log to {out_path}")
    
    return total_failures


def main():
    parser = argparse.ArgumentParser(
        description="Run a simple verification plan and log results."
    )
    parser.add_argument(
        "--plan", type=str, required=True,
        help="Path to plan file (one command per line)"
    )
    parser.add_argument(
        "--out", type=str, default="verify_log.md",
        help="Output log file path (default: verify_log.md)"
    )
    parser.add_argument(
        "--timeout", type=int, default=300,
        help="Timeout per command in seconds (default: 300)"
    )
    
    args = parser.parse_args()
    
    plan_path = Path(args.plan)
    out_path = Path(args.out)
    
    if not plan_path.exists():
        print(f"[ERROR] Plan file not found: {plan_path}")
        sys.exit(1)
    
    failures = run_plan(plan_path, out_path, args.timeout)
    
    if failures > 0:
        sys.exit(1)


if __name__ == "__main__":
    main()
