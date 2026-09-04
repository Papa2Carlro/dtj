"""Debug Trace MCP — DocHub plugin (experimental foundation)."""

from .http_server import serve_http, serve_http_threaded

__version__ = "0.1.0"
__all__ = ["serve_http", "serve_http_threaded"]
