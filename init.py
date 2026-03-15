#!/usr/bin/env python3
from typing import Optional

from pathlib import Path

GREEN = "\033[92m"
RED = "\033[91m"
ENDC = "\033[0m"


def info(text):
    print(f"{GREEN}[INFO]{ENDC} {text}")


def error(text):
    print(f"{RED}[ERROR]{ENDC} {text}")


def init():
    info("Initialising pre-push hook")
    hook_path = Path(".git/hooks/pre-push")
    if hook_path.exists():
        info("Pre-push hook already exists, skipping")
        return
    hook_content: Optional[str] = None
    with open("./etc/pre-push.sh", "r") as f:
        hook_content = f.read()
    if hook_content is None:
        error("Failed to read pre-push hook content")
        exit(1)
    with open(hook_path, "w") as f:
        f.write(hook_content)
    hook_path.chmod(0o755)
    info("Pre-push hook created successfully")


if __name__ == "__main__":
    init()
