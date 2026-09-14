#!/usr/bin/env python3
"""
Version Check - Compare installed vs latest package versions.

Usage:
    check_versions.py <package-manager> <package-name> [--all]
    check_versions.py npm resend
    check_versions.py pip openai
    check_versions.py npm --all          # Check all packages in package.json

Outputs a clear verdict: UP_TO_DATE, MINOR_BEHIND, MAJOR_BEHIND, or NOT_INSTALLED.
"""

import subprocess
import sys
import json
import re
from pathlib import Path
from typing import Dict, List, Optional, Tuple

# Valid package name pattern: npm scoped (@scope/name) or unscoped (name)
# Allows alphanumeric, hyphens, dots, underscores, and @scope/ prefix
VALID_PACKAGE_RE = re.compile(r'^(@[a-zA-Z0-9._-]+/)?[a-zA-Z0-9._-]+$')


def validate_package_name(name: str) -> str:
    """Validate and sanitize package name to prevent path traversal and injection."""
    name = name.strip()
    if not name:
        print("❌ Package name cannot be empty")
        sys.exit(1)
    if '..' in name or name.startswith('/') or name.startswith('\\'):
        print(f"❌ Invalid package name (path traversal detected): {name}")
        sys.exit(1)
    if not VALID_PACKAGE_RE.match(name):
        print(f"❌ Invalid package name: {name}")
        print("   Expected format: 'package-name' or '@scope/package-name'")
        sys.exit(1)
    return name


def run_cmd(cmd: List[str]) -> Optional[str]:
    """Run a command and return stdout, or None on failure."""
    try:
        result = subprocess.run(cmd, capture_output=True, text=True, timeout=15)
        return result.stdout.strip() if result.returncode == 0 else None
    except (subprocess.TimeoutExpired, FileNotFoundError):
        return None


def parse_semver(version: str) -> Optional[Tuple[int, int, int]]:
    """Parse a semver string into (major, minor, patch)."""
    match = re.match(r'(\d+)\.(\d+)\.(\d+)', version.strip().lstrip('v^~>=<'))
    if match:
        return int(match.group(1)), int(match.group(2)), int(match.group(3))
    return None


def check_npm(package: str) -> Dict[str, Optional[str]]:
    """Check npm package version."""
    package = validate_package_name(package)

    # Installed version — resolve path and verify it stays within node_modules
    installed = None
    pkg_json = Path("node_modules", package, "package.json").resolve()
    node_modules_root = Path("node_modules").resolve()
    if not str(pkg_json).startswith(str(node_modules_root)):
        print(f"❌ Path traversal blocked: {package}")
        sys.exit(1)
    if pkg_json.exists():
        try:
            data = json.loads(pkg_json.read_text())
            installed = data.get("version")
        except (json.JSONDecodeError, IOError):
            pass

    # Latest version
    latest = run_cmd(["npm", "info", package, "version"])

    return {"package": package, "manager": "npm", "installed": installed, "latest": latest}


def check_pip(package: str) -> Dict[str, Optional[str]]:
    """Check pip package version."""
    package = validate_package_name(package)
    # Installed version
    installed = None
    output = run_cmd(["pip", "show", package])
    if output:
        for line in output.split("\n"):
            if line.startswith("Version:"):
                installed = line.split(":", 1)[1].strip()
                break

    # Latest version
    latest = None
    output = run_cmd(["pip", "index", "versions", package])
    if output:
        match = re.search(r'\((.+?)\)', output)
        if match:
            latest = match.group(1)

    return {"package": package, "manager": "pip", "installed": installed, "latest": latest}


def check_all_npm() -> List[Dict[str, Optional[str]]]:
    """Check all packages in package.json."""
    pkg_json = Path("package.json")
    if not pkg_json.exists():
        print("❌ No package.json found in current directory")
        return []

    data = json.loads(pkg_json.read_text())
    deps = {}
    deps.update(data.get("dependencies", {}))
    deps.update(data.get("devDependencies", {}))

    results = []
    for package in sorted(deps.keys()):
        result = check_npm(package)
        results.append(result)

    return results


def get_verdict(installed: Optional[str], latest: Optional[str]) -> str:
    """Determine update verdict."""
    if not installed:
        return "NOT_INSTALLED"
    if not latest:
        return "UNKNOWN"

    inst = parse_semver(installed)
    lat = parse_semver(latest)

    if not inst or not lat:
        return "UNKNOWN"

    if inst[0] < lat[0]:
        return "MAJOR_BEHIND"
    elif inst[1] < lat[1]:
        return "MINOR_BEHIND"
    elif inst[2] < lat[2]:
        return "PATCH_BEHIND"
    else:
        return "UP_TO_DATE"


def format_result(result: Dict[str, Optional[str]]) -> str:
    """Format a single check result."""
    verdict = get_verdict(result["installed"], result["latest"])
    installed = result["installed"] or "not installed"
    latest = result["latest"] or "unknown"

    icons = {
        "UP_TO_DATE": "✅",
        "PATCH_BEHIND": "✅",
        "MINOR_BEHIND": "⚠️ ",
        "MAJOR_BEHIND": "🚨",
        "NOT_INSTALLED": "❌",
        "UNKNOWN": "❓",
    }

    icon = icons.get(verdict, "❓")
    line = f"{icon} {result['package']}: {installed} → {latest} [{verdict}]"

    if verdict == "MAJOR_BEHIND":
        line += "\n   ⚡ MAJOR version behind — check migration guide before updating!"
    
    return line


def main():
    if len(sys.argv) < 2:
        print(__doc__)
        sys.exit(1)

    manager = sys.argv[1]

    # --all flag: check everything in package.json
    if manager == "npm" and (len(sys.argv) < 3 or sys.argv[2] == "--all"):
        if len(sys.argv) >= 3 and sys.argv[2] == "--all":
            results = check_all_npm()
            if not results:
                sys.exit(1)
            
            major_behind = []
            for r in results:
                print(format_result(r))
                if get_verdict(r["installed"], r["latest"]) == "MAJOR_BEHIND":
                    major_behind.append(r["package"])
            
            print(f"\n📊 Checked {len(results)} packages")
            if major_behind:
                print(f"🚨 {len(major_behind)} packages are MAJOR versions behind: {', '.join(major_behind)}")
                print("   Run version check on each before updating!")
            sys.exit(0)
        else:
            print("Usage: check_versions.py npm <package> or check_versions.py npm --all")
            sys.exit(1)

    if len(sys.argv) < 3:
        print("Usage: check_versions.py <npm|pip> <package-name>")
        sys.exit(1)

    package = sys.argv[2]

    checkers = {"npm": check_npm, "pip": check_pip}
    checker = checkers.get(manager)

    if not checker:
        print(f"❌ Unknown package manager: {manager}. Supported: npm, pip")
        sys.exit(1)

    result = checker(package)
    print(format_result(result))

    verdict = get_verdict(result["installed"], result["latest"])
    sys.exit(1 if verdict in ("MAJOR_BEHIND", "NOT_INSTALLED") else 0)


if __name__ == "__main__":
    main()
