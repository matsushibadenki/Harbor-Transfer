import base64
import email.utils
import html
import os
import shutil
import ssl
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import quote, unquote, urlparse

ROOT = Path("/srv/webdav").resolve()
EXPECTED_AUTH = "Basic " + base64.b64encode(b"harbor:harbor").decode("ascii")


class WebDavHandler(BaseHTTPRequestHandler):
    server_version = "HarborWebDAVTest/1.0"

    def authenticated(self):
        if self.headers.get("Authorization") == EXPECTED_AUTH:
            return True
        self.send_response(401)
        self.send_header("WWW-Authenticate", 'Basic realm="Harbor Transfer Integration"')
        self.send_header("Content-Length", "0")
        self.end_headers()
        return False

    def target(self, raw_path=None):
        request_path = unquote(urlparse(raw_path or self.path).path)
        target = (ROOT / request_path.lstrip("/")).resolve()
        if target != ROOT and ROOT not in target.parents:
            raise ValueError("path escaped WebDAV root")
        return request_path, target

    def empty(self, status, headers=None):
        self.send_response(status)
        for name, value in (headers or {}).items():
            self.send_header(name, value)
        self.send_header("Content-Length", "0")
        self.end_headers()

    def do_OPTIONS(self):
        if self.authenticated():
            self.empty(200, {"DAV": "1", "Allow": "OPTIONS, PROPFIND, GET, PUT, MKCOL, MOVE, DELETE"})

    def do_PROPFIND(self):
        if not self.authenticated():
            return
        request_path, target = self.target()
        if not target.exists():
            self.empty(404)
            return
        resources = [(request_path, target)]
        if self.headers.get("Depth", "infinity") == "1" and target.is_dir():
            resources.extend((request_path.rstrip("/") + "/" + child.name, child) for child in target.iterdir())
        responses = []
        for resource_path, resource in resources:
            href = quote(resource_path or "/", safe="/")
            if resource.is_dir() and not href.endswith("/"):
                href += "/"
            stat = resource.stat()
            resource_type = "<d:collection/>" if resource.is_dir() else ""
            size = 0 if resource.is_dir() else stat.st_size
            modified = email.utils.formatdate(stat.st_mtime, usegmt=True)
            responses.append(
                f"<d:response><d:href>{html.escape(href)}</d:href><d:propstat><d:prop>"
                f"<d:resourcetype>{resource_type}</d:resourcetype>"
                f"<d:getcontentlength>{size}</d:getcontentlength>"
                f"<d:getlastmodified>{modified}</d:getlastmodified>"
                "</d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response>"
            )
        body = ('<?xml version="1.0" encoding="utf-8"?><d:multistatus xmlns:d="DAV:">' + "".join(responses) + "</d:multistatus>").encode("utf-8")
        self.send_response(207)
        self.send_header("Content-Type", "application/xml; charset=utf-8")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        if not self.authenticated():
            return
        _, target = self.target()
        if not target.is_file():
            self.empty(404)
            return
        size = target.stat().st_size
        self.send_response(200)
        self.send_header("Content-Length", str(size))
        self.end_headers()
        with target.open("rb") as source:
            shutil.copyfileobj(source, self.wfile, 64 * 1024)

    def do_PUT(self):
        if not self.authenticated():
            return
        _, target = self.target()
        target.parent.mkdir(parents=True, exist_ok=True)
        remaining = int(self.headers.get("Content-Length", "0"))
        with target.open("wb") as output:
            while remaining:
                chunk = self.rfile.read(min(remaining, 64 * 1024))
                if not chunk:
                    break
                output.write(chunk)
                remaining -= len(chunk)
        self.empty(201)

    def do_MKCOL(self):
        if not self.authenticated():
            return
        _, target = self.target()
        if target.exists():
            self.empty(405)
            return
        target.mkdir(parents=False)
        self.empty(201)

    def do_MOVE(self):
        if not self.authenticated():
            return
        _, source = self.target()
        destination_header = self.headers.get("Destination")
        if not destination_header or not source.exists():
            self.empty(400 if not destination_header else 404)
            return
        _, destination = self.target(destination_header)
        if destination.exists() and self.headers.get("Overwrite", "T") == "F":
            self.empty(412)
            return
        destination.parent.mkdir(parents=True, exist_ok=True)
        source.rename(destination)
        self.empty(201)

    def do_DELETE(self):
        if not self.authenticated():
            return
        _, target = self.target()
        if not target.exists():
            self.empty(404)
            return
        if target.is_dir():
            shutil.rmtree(target)
        else:
            target.unlink()
        self.empty(204)

    def log_message(self, message, *args):
        print(f"webdav: {message % args}", flush=True)


ROOT.mkdir(parents=True, exist_ok=True)
server = ThreadingHTTPServer(("0.0.0.0", 8443), WebDavHandler)
context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
context.load_cert_chain("/certs/server.crt", "/certs/server.key")
server.socket = context.wrap_socket(server.socket, server_side=True)
server.serve_forever()
