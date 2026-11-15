#!/usr/bin/env python3
"""Manage just our Thunar action, preserving other actions and backing up edits."""
import argparse
import os
import re
from pathlib import Path
import shlex
import shutil
import tempfile
import time
import xml.etree.ElementTree as ET

ACTION_ID = "linux-scripts-dock-pin-launcher"

def update(path: Path, executable: Path | None):
    path = path.resolve()  # Keep dotfile-manager symlinks intact.
    if path.exists():
        parser = ET.XMLParser(target=ET.TreeBuilder(insert_comments=True))
        # Thunar configurations can contain encoding before version. GLib
        # accepts this header, but ElementTree requires XML declaration order.
        # Normalize only this known UTF-8/1.0 form; keep rejecting other errors.
        content = path.read_bytes()
        content = re.sub(
            br"\A<\?xml\s+encoding=([\"'])UTF-8\1\s+version=([\"'])1\.0\2\s*\?>",
            b'<?xml version="1.0" encoding="UTF-8"?>', content, count=1,
        )
        tree = ET.ElementTree(ET.fromstring(content, parser=parser))
        if tree.getroot().tag != "actions":
            raise ValueError("Thunar config must have an <actions> root")
    elif executable is None:
        return False
    else:
        tree = ET.ElementTree(ET.Element("actions"))
    root = tree.getroot()
    before = ET.tostring(root)
    owned = [a for a in root.findall("action") if a.findtext("unique-id") == ACTION_ID]
    if executable is not None:
        values = {
            "icon": "list-add", "name": "Add to Linux Scripts Dock",
            "unique-id": ACTION_ID,
            "command": shlex.quote(str(executable).replace("%", "%%")) + " --pin-desktop %F",
            "description": "Pin the selected application launchers without launching them",
            "patterns": "*.desktop", "other-files": "", "text-files": "",
        }
        if len(owned) == 1 and {e.tag: e.text or "" for e in owned[0]} == values:
            return False
    for action in owned:
        root.remove(action)
    if executable is not None:
        action = ET.SubElement(root, "action")
        for key, value in values.items():
            ET.SubElement(action, key).text = value
    if ET.tostring(root) == before:
        return False
    path.parent.mkdir(parents=True, exist_ok=True)
    if path.exists():
        shutil.copy2(path, path.with_name(path.name + f".dock-backup-{time.time_ns()}"))
    fd, temporary = tempfile.mkstemp(prefix=".uca-dock-", dir=path.parent)
    try:
        with os.fdopen(fd, "wb") as stream:
            tree.write(stream, encoding="utf-8", xml_declaration=True)
            stream.flush()
            os.fsync(stream.fileno())
        if path.exists():
            os.chmod(temporary, path.stat().st_mode & 0o777)
        os.replace(temporary, path)
    finally:
        if os.path.exists(temporary):
            os.unlink(temporary)
    return True

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=["install", "remove"])
    parser.add_argument("executable", nargs="?", type=Path)
    args = parser.parse_args()
    if args.action == "install" and (args.executable is None or not args.executable.is_absolute()):
        parser.error("install requires an absolute executable path")
    config = Path(os.environ.get("XDG_CONFIG_HOME", Path.home() / ".config")) / "Thunar" / "uca.xml"
    try:
        changed = update(config, args.executable if args.action == "install" else None)
    except (OSError, ET.ParseError, ValueError) as error:
        parser.exit(1, f"Thunar integration unchanged: {error}\n")
    print(f"Thunar action {'updated' if changed else 'unchanged'}: {config}")

if __name__ == "__main__":
    main()
