from .server import main
from .http_server import serve_http

import argparse
import sys


def http_main() -> None:
    parser = argparse.ArgumentParser(prog="debug-trace-mcp-http")
    parser.add_argument("--host", default="127.0.0.1", help="Host to bind (default: 127.0.0.1)")
    parser.add_argument("--port", type=int, default=8766, help="Port to bind (default: 8766)")
    args = parser.parse_args()
    serve_http(args.host, args.port)


if __name__ == "__main__":
    if len(sys.argv) > 1 and sys.argv[1] == "http":
        sys.argv.pop(1)
        http_main()
    else:
        main()
