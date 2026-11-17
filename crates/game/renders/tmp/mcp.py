"""Scratch helper: call a modeler MCP tool through the shared proxy.

Usage: python3 mcp.py TOOL '{"json": "args"}'   (args default {})
Passes --doc from $MODELER_DOC if set.
"""
import subprocess, json, os, sys

PROXY = ["python3", "/Users/doomy/Code/modeler/bin/modeler/mcp_driver.py", "--proxy"]


def call(name, args):
    p = subprocess.Popen(PROXY, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                         stderr=subprocess.DEVNULL, text=True)

    def send(obj):
        p.stdin.write(json.dumps(obj) + "\n")
        p.stdin.flush()

    def read_until(want_id):
        for line in p.stdout:
            line = line.strip()
            if not line:
                continue
            try:
                msg = json.loads(line)
            except Exception:
                continue
            if msg.get("id") == want_id:
                return msg
        return None

    send({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
        "protocolVersion": "2024-11-05", "capabilities": {},
        "clientInfo": {"name": "zcode", "version": "1"}}})
    read_until(1)
    send({"jsonrpc": "2.0", "method": "notifications/initialized"})
    send({"jsonrpc": "2.0", "id": 2, "method": "tools/call",
          "params": {"name": name, "arguments": args}})
    res = read_until(2)
    p.terminate()
    if res is None:
        sys.exit("no response from modeler")
    if res.get("result") is None:
        sys.exit("error: " + json.dumps(res)[:1000])
    out = "\n".join(c.get("text", "") for c in res["result"].get("content", []))
    for c in res["result"].get("content", []):
        if c.get("type") == "image":
            out += "\n[image %s]" % c.get("mimeType")
    return out


if __name__ == "__main__":
    name = sys.argv[1]
    args = json.loads(sys.argv[2]) if len(sys.argv) > 2 else {}
    doc = os.environ.get("MODELER_DOC")
    if doc and name not in ("scene",):
        args.setdefault("doc", doc)
    print(call(name, args))
