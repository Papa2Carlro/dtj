"""Debug Trace MCP — Unified CLI + MCP server entry point."""

import argparse
import sys
from .server import main as mcp_main
from .dtj_read import read_session_dtj
from .dtj_search import search_session_dtj
from .dtj_catalog import event_catalog_dtj
from .dtj_core import ADAPTER_NAME, ADAPTER_VERSION


def main() -> None:
    """Main entry point for `dtj` CLI."""
    if len(sys.argv) < 2:
        print_usage()
        sys.exit(1)

    cmd = sys.argv[1]

    if cmd == "read-session":
        cmd_read_session()
    elif cmd == "search":
        cmd_search()
    elif cmd == "list":
        cmd_list()
    elif cmd == "mcp":
        # Run MCP server
        sys.argv = [sys.argv[0]] + sys.argv[2:]
        mcp_main()
    elif cmd in ("--version", "-V"):
        print(f"dtj-mcp {ADAPTER_NAME}/{ADAPTER_VERSION}")
    elif cmd in ("--help", "-h"):
        print_usage()
    else:
        print(f"unknown command: {cmd}", file=sys.stderr)
        print_usage()
        sys.exit(1)


def print_usage() -> None:
    print(f"""dtj — Debug Trace Journal tools (unified CLI + MCP)
Adapter: {ADAPTER_NAME} v{ADAPTER_VERSION}
Uses Rust dtj-core when available, falls back to pure Python.

Usage: dtj <command> [options]

Commands:
  read-session <path>    Read and print a .dtj session as JSON
  search <dir> <query>   Search sessions in directory
  list <dir>             List sessions in directory
  mcp                    Run MCP server (default when using as library)
  --version, -V         Print version
  --help, -h            Show this help

Examples:
  dtj read-session session-123.dtj
  dtj search ./traces "connection error"
  dtj list ./traces
  dtj mcp --help

The same dtj-core implementation is used by both CLI and MCP,
ensuring identical parsing results for humans and agents.""")


def cmd_read_session() -> None:
    if len(sys.argv) < 3:
        print("usage: dtj read-session <session_path>", file=sys.stderr)
        sys.exit(1)
    path = sys.argv[2]
    json_output = read_session_dtj(path)
    print(json_output)


def cmd_search() -> None:
    if len(sys.argv) < 4:
        print("usage: dtj search <dir> <query>", file=sys.stderr)
        sys.exit(1)
    dir_path = sys.argv[2]
    query = sys.argv[3]
    results = search_session_dtj(dir_path, query)
    for r in results:
        print(r)


def cmd_list() -> None:
    if len(sys.argv) < 3:
        print("usage: dtj list <dir>", file=sys.stderr)
        sys.exit(1)
    dir_path = sys.argv[2]
    sessions = event_catalog_dtj(dir_path)
    for s in sessions:
        print(s)


if __name__ == "__main__":
    main()
