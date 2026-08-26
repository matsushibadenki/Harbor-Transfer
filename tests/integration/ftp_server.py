import os

from pyftpdlib.authorizers import DummyAuthorizer
from pyftpdlib.handlers import FTPHandler, TLS_FTPHandler
from pyftpdlib.servers import FTPServer

authorizer = DummyAuthorizer()
authorizer.add_user("harbor", "harbor", "/srv/ftp", perm="elradfmwMT")

if os.environ.get("FTP_TLS") == "1":
    handler = TLS_FTPHandler
    handler.certfile = "/certs/server.crt"
    handler.keyfile = "/certs/server.key"
    handler.tls_control_required = True
    handler.tls_data_required = True
else:
    handler = FTPHandler

handler.authorizer = authorizer
handler.encoding = "utf-8"
handler.masquerade_address = "127.0.0.1"
handler.passive_ports = range(
    int(os.environ.get("PASV_MIN_PORT", "30000")),
    int(os.environ.get("PASV_MAX_PORT", "30009")) + 1,
)

FTPServer(("0.0.0.0", 21), handler).serve_forever()
