#!/usr/bin/env python3
"""Funke plugin template (Python).

Speaks the same line-delimited JSON-RPC 2.0 protocol as the Rust template
(see ../../docs/PLUGINS.md): one JSON object per line over stdin/stdout, nothing
else on stdout. Dependency-free — the standard library is all it needs.

`tpy <text>` offers UPPERCASE / lowercase / reversed variants of what you typed;
Enter copies one (via Windows' built-in `clip`). Copy this folder to start your own.
"""

import json
import subprocess
import sys

PROTOCOL_VERSION = 1

# key -> (subtitle label, transform)
TRANSFORMS = {
    "upper": ("UPPERCASE", str.upper),
    "lower": ("lowercase", str.lower),
    "reverse": ("esreveR", lambda s: s[::-1]),
}


def query(text):
    items = []
    for rank, (key, (label, transform)) in enumerate(TRANSFORMS.items()):
        items.append(
            {
                # Encode the query into the row id so invoke() can recompute the value.
                "id": f"{key}:{text}",
                "title": transform(text),
                "subtitle": f"{label} — Enter copies",
                "score": 10 - rank,
                "actions": [{"label": "Copy", "confirm": False}],
            }
        )
    return items


def invoke(item_id, _action_index):
    kind, _, text = item_id.partition(":")
    entry = TRANSFORMS.get(kind)
    if entry is None:
        raise ValueError("unknown item")
    _, transform = entry
    # `clip` (built into Windows) reads stdin and sets the clipboard.
    subprocess.run("clip", input=transform(text), text=True, check=True)


def handle(request):
    method = request.get("method")
    rid = request.get("id")
    if method == "initialize":
        return _ok(rid, {"name": "Template (Python)", "version": "0.1.0", "protocol": PROTOCOL_VERSION})
    if method == "query":
        return _ok(rid, {"items": query(request.get("params", {}).get("text", ""))})
    if method == "invoke":
        params = request.get("params", {})
        try:
            invoke(params.get("item_id", ""), params.get("action_index", 0))
        except Exception as exc:  # surface the failure to the host, never crash
            return _err(rid, str(exc))
        return _ok(rid, {})
    return _err(rid, f"unknown method: {method}")


def _ok(rid, result):
    return {"jsonrpc": "2.0", "id": rid, "result": result}


def _err(rid, message):
    return {"jsonrpc": "2.0", "id": rid, "error": {"code": -32000, "message": message}}


def main():
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            request = json.loads(line)
        except json.JSONDecodeError:
            continue  # stray line — not ours to crash over
        if request.get("method") == "shutdown":
            return  # notification, no response
        sys.stdout.write(json.dumps(handle(request)) + "\n")
        sys.stdout.flush()


if __name__ == "__main__":
    main()
