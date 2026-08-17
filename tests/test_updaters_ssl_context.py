"""fetch_url_bytes' SSL trust store: _verify_context and the fact that
verification stays genuinely active.

Every existing test in test_updaters.py runs the local server over plain
HTTP, so none of them ever touch certificate verification at all -- which is
exactly how a frozen Linux build could fail every HTTPS request with
CERTIFICATE_VERIFY_FAILED (unable to get local issuer certificate) without
the suite ever noticing. That failure mode -- OpenSSL never finding *any*
trust store to check against -- is why both update_plugin_order_yml and
update_rule_files broke identically on Steam Deck: they share this one
choke point.

The fix pins an explicit certifi CA bundle instead of relying on OpenSSL's
compiled-in default paths. The important thing to prove here isn't just that
requests succeed again -- a context that skipped verification entirely would
also make requests succeed -- it's that an *untrusted* certificate is still
correctly rejected. TestVerificationStaysActive is that proof, over a real
local HTTPS server with a self-signed cert certifi's bundle doesn't trust.
"""

from __future__ import annotations

import http.server
import ssl
import sys
import threading
from pathlib import Path

import pytest

from wraithguard.net.updaters import _verify_context, fetch_url_bytes


class TestVerifyContext:
    def test_returns_a_real_ssl_context_when_certifi_is_installed(self) -> None:
        pytest.importorskip("certifi")

        context = _verify_context()

        assert isinstance(context, ssl.SSLContext)
        assert context.verify_mode == ssl.CERT_REQUIRED

    def test_the_context_is_pinned_to_certifis_own_bundle(self) -> None:
        certifi = pytest.importorskip("certifi")

        context = _verify_context()

        # cert_store_stats() at least confirms *something* was loaded from a
        # real bundle, not an empty/default-constructed context.
        assert context is not None
        assert context.cert_store_stats()["x509_ca"] > 0
        assert Path(certifi.where()).exists()

    def test_falls_back_to_none_when_certifi_is_not_installed(
        self, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        # Setting a name to None in sys.modules makes the next `import` of it
        # raise ImportError immediately -- the standard way to simulate an
        # absent optional dependency without actually uninstalling it.
        monkeypatch.setitem(sys.modules, "certifi", None)

        assert _verify_context() is None

    def test_fetch_url_bytes_passes_the_context_through_to_urlopen(
        self, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        captured: dict[str, object] = {}

        class _FakeResponse:
            def __enter__(self):
                return self

            def __exit__(self, *exc):
                return False

            def read(self, _n):
                return b"ok"

        def fake_urlopen(url, timeout=None, context=None):
            captured["context"] = context
            return _FakeResponse()

        import urllib.request

        monkeypatch.setattr(urllib.request, "urlopen", fake_urlopen)

        fetch_url_bytes("https://example.invalid/x")

        assert captured["context"] is not None
        assert isinstance(captured["context"], ssl.SSLContext)


@pytest.fixture(scope="module")
def self_signed_https_server(tmp_path_factory: pytest.TempPathFactory):
    """A real local HTTPS server backed by a self-signed cert certifi
    doesn't trust -- proof that verification is still genuinely enforced.

    Built with the ``cryptography`` package rather than shelling out to the
    ``openssl`` CLI: the latter isn't guaranteed to be on PATH on a Windows
    dev machine (it isn't on every install), where this suite actually runs
    day to day. Skips gracefully -- not an error -- if ``cryptography``
    itself isn't installed either, the same way the rest of this file treats
    ``certifi`` as optional.
    """
    pytest.importorskip("cryptography")
    import datetime

    from cryptography import x509
    from cryptography.hazmat.primitives import hashes, serialization
    from cryptography.hazmat.primitives.asymmetric import rsa
    from cryptography.x509.oid import NameOID

    cert_dir = tmp_path_factory.mktemp("https-cert")
    key_path = cert_dir / "key.pem"
    cert_path = cert_dir / "cert.pem"

    key = rsa.generate_private_key(public_exponent=65537, key_size=2048)
    name = x509.Name([x509.NameAttribute(NameOID.COMMON_NAME, "127.0.0.1")])
    now = datetime.datetime.now(datetime.timezone.utc)
    cert = (
        x509.CertificateBuilder()
        .subject_name(name)
        .issuer_name(name)
        .public_key(key.public_key())
        .serial_number(x509.random_serial_number())
        .not_valid_before(now - datetime.timedelta(minutes=5))
        .not_valid_after(now + datetime.timedelta(days=1))
        .sign(key, hashes.SHA256())
    )
    key_path.write_bytes(
        key.private_bytes(
            encoding=serialization.Encoding.PEM,
            format=serialization.PrivateFormat.TraditionalOpenSSL,
            encryption_algorithm=serialization.NoEncryption(),
        )
    )
    cert_path.write_bytes(cert.public_bytes(serialization.Encoding.PEM))

    routes: dict[str, bytes] = {"/ok": b"hello"}

    class Handler(http.server.BaseHTTPRequestHandler):
        def do_GET(self):
            body = routes.get(self.path, b"not found")
            self.send_response(200 if self.path in routes else 404)
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)

        def log_message(self, *args):  # silence per-request stderr noise
            pass

    server = http.server.HTTPServer(("127.0.0.1", 0), Handler)
    context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    context.load_cert_chain(str(cert_path), str(key_path))
    server.socket = context.wrap_socket(server.socket, server_side=True)

    threading.Thread(target=server.serve_forever, daemon=True).start()
    yield f"https://127.0.0.1:{server.server_address[1]}"
    server.shutdown()


class TestVerificationStaysActive:
    """The fix must not have quietly traded 'can't find a trust store' for
    'don't check at all' -- those look identical from the happy path.
    """

    def test_a_self_signed_cert_is_still_rejected(self, self_signed_https_server) -> None:
        with pytest.raises(OSError, match=r"CERTIFICATE_VERIFY_FAILED|certificate verify failed"):
            fetch_url_bytes(f"{self_signed_https_server}/ok", timeout=5)
