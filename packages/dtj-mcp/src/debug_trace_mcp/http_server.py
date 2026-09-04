"""HTTP server for debug-trace MCP — JSON-RPC over HTTP.

Provides HTTP/JSON-RPC endpoint for the debug-trace MCP server.
Can be run standalone or alongside the stdio server.
"""

from __future__ import annotations

import json
import sys
import traceback
from typing import Any
from http.server import HTTPServer, BaseHTTPRequestHandler
from urllib.parse import urlparse
import threading
import signal

from mcp.server.fastmcp import FastMCP

from .server import mcp


class JSONRPCRequestHandler(BaseHTTPRequestHandler):
    """HTTP request handler for JSON-RPC requests."""

    def do_POST(self) -> None:
        """Handle POST requests for JSON-RPC."""
        parsed_url = urlparse(self.path)

        if parsed_url.path != '/rpc':
            self.send_error(404, "Not Found")
            return

        content_length = int(self.headers.get('Content-Length', 0))
        if content_length == 0:
            self.send_error(400, "Bad Request: Empty body")
            return

        body = self.rfile.read(content_length).decode('utf-8')

        try:
            req = json.loads(body)
        except json.JSONDecodeError as exc:
            self._send_json_response(self._err(None, -32700, f"parse error: {exc}"))
            return

        if not isinstance(req, dict):
            self._send_json_response(self._err(None, -32600, "request must be an object"))
            return

        if isinstance(req, list):
            responses = []
            for item in req:
                if not isinstance(item, dict):
                    responses.append(self._err(None, -32600, "request must be an object"))
                    continue
                resp = self._handle_single_request(item)
                if resp is not None:
                    responses.append(resp)
            self._send_json_response(responses)
            return

        if "id" not in req:
            try:
                self._handle_single_request({**req, "id": None})
            except Exception:
                traceback.print_exc(file=sys.stderr)
            self.send_response(204)
            self.end_headers()
            return

        resp = self._handle_single_request(req)
        self._send_json_response(resp)

    def do_GET(self) -> None:
        """Handle GET requests for health check and info."""
        parsed_url = urlparse(self.path)

        if parsed_url.path == '/health':
            self._send_json_response({"status": "ok", "service": "debug-trace-mcp"})
            return

        if parsed_url.path == '/info':
            self._send_json_response({
                "service": "debug-trace-mcp",
                "version": "0.1.0",
                "transport": "http",
                "endpoints": {
                    "rpc": "/rpc",
                    "health": "/health",
                    "info": "/info"
                }
            })
            return

        self.send_error(404, "Not Found")

    def _handle_single_request(self, req: dict[str, Any]) -> dict[str, Any] | None:
        """Handle a single JSON-RPC request using FastMCP's internal handler."""
        method = req.get("method")
        params = req.get("params", {})
        req_id = req.get("id")

        if not method:
            return self._err(req_id, -32600, "method is required")

        try:
            # Use FastMCP's internal tool calling mechanism
            # FastMCP stores tools in mcp._tool_manager
            tool_manager = mcp._tool_manager
            if method not in tool_manager._tools:
                return self._err(req_id, -32601, f"method not found: {method}")

            tool = tool_manager._tools[method]
            result = tool.fn(**params)
            return self._ok(req_id, result)
        except Exception as exc:
            traceback.print_exc(file=sys.stderr)
            return self._err(req_id, -32603, f"internal error: {exc}")

    def _ok(self, id_: Any, result: Any) -> dict[str, Any]:
        return {"jsonrpc": "2.0", "id": id_, "result": result}

    def _err(self, id_: Any, code: int, message: str, data: Any = None) -> dict[str, Any]:
        err: dict[str, Any] = {"code": code, "message": message}
        if data is not None:
            err["data"] = data
        return {"jsonrpc": "2.0", "id": id_, "error": err}

    def _send_json_response(self, data: Any) -> None:
        self.send_response(200)
        self.send_header('Content-Type', 'application/json')
        self.send_header('Access-Control-Allow-Origin', '*')
        self.send_header('Access-Control-Allow-Methods', 'POST, GET, OPTIONS')
        self.send_header('Access-Control-Allow-Headers', 'Content-Type')
        self.end_headers()
        self.wfile.write(json.dumps(data, ensure_ascii=False).encode('utf-8'))

    def do_OPTIONS(self) -> None:
        self.send_response(200)
        self.send_header('Access-Control-Allow-Origin', '*')
        self.send_header('Access-Control-Allow-Methods', 'POST, GET, OPTIONS')
        self.send_header('Access-Control-Allow-Headers', 'Content-Type')
        self.end_headers()

    def log_message(self, format: str, *args) -> None:
        pass


def serve_http(host: str = "127.0.0.1", port: int = 8766) -> None:
    """Start HTTP JSON-RPC server."""
    server = HTTPServer((host, port), JSONRPCRequestHandler)

    def signal_handler(signum, frame):
        print(f"\nShutting down HTTP server on {host}:{port}...", file=sys.stderr)
        server.shutdown()
        sys.exit(0)

    signal.signal(signal.SIGINT, signal_handler)
    signal.signal(signal.SIGTERM, signal_handler)

    print(f"Starting debug-trace HTTP server on http://{host}:{port}", file=sys.stderr)
    print(f"  RPC endpoint: http://{host}:{port}/rpc", file=sys.stderr)
    print(f"  Health check: http://{host}:{port}/health", file=sys.stderr)
    print(f"  Info endpoint: http://{host}:{port}/info", file=sys.stderr)

    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        server.server_close()
        print("HTTP server stopped.", file=sys.stderr)


def serve_http_threaded(host: str = "127.0.0.1", port: int = 8766) -> threading.Thread:
    """Start HTTP server in a background thread."""
    server = HTTPServer((host, port), JSONRPCRequestHandler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    print(f"Started debug-trace HTTP server on http://{host}:{port} (threaded)", file=sys.stderr)
    return thread