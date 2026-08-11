"""A loopback HTTP server for the viewer, and why it is shaped like this.

A page opened from disk cannot fetch anything: its origin is ``null`` and every
CORS check fails. That is why the standalone viewer inlines megabytes of
geometry and a copy of three.js into each file. Serving the same page over
``http://127.0.0.1`` removes the restriction, and with it the reason to embed
anything: the page becomes a few kilobytes, the library is fetched once and
cached by the browser, and geometry is requested per mesh instead of being
decided at generation time.

**Security is by construction, not by validation.** This is a listening socket
on a machine full of the user's files, so the design assumes someone will try
to make it serve something it should not:

* It binds ``127.0.0.1`` only, never ``0.0.0.0``, so nothing off the machine
  can reach it at all.
* It listens on an ephemeral port, chosen by the OS.
* Every request must carry a per-session token. Without it the answer is 404 --
  not 403, which would confirm the path exists.
* **It has no filesystem mapping.** Payloads are registered in memory and
  served by key. There is no code path from a URL to :func:`open`, so path
  traversal is not defended against, it is absent. This is the single most
  important property here and the reason
  :class:`http.server.SimpleHTTPRequestHandler` is not used: that class exists
  to serve a directory, which is exactly what must not happen.
"""

from __future__ import annotations

import secrets
import threading
from dataclasses import dataclass
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import TYPE_CHECKING
from urllib.parse import parse_qs, urlparse

from wraithguard.logging_setup import get_logger

if TYPE_CHECKING:
    from collections.abc import Mapping

LOG = get_logger(__name__)

#: Loopback only. Spelled out rather than left to a default, because the
#: difference between this and "" (all interfaces) is the whole security model.
_HOST = "127.0.0.1"

#: How long to wait for the server thread to stop before giving up on it.
_SHUTDOWN_TIMEOUT = 2.0


@dataclass(frozen=True, slots=True)
class Payload:
    """One thing the server will hand out.

    Attributes:
        body: The bytes to send.
        content_type: The MIME type to send them as.
    """

    body: bytes
    content_type: str = "application/octet-stream"


@dataclass(frozen=True, slots=True)
class PublishSession:
    """One view's private namespace on a server shared with every other view.

    Two windows publishing through the same bare server would collide on the
    names they both reach for by habit -- every view wants to call its own
    page ``"index.html"`` and count its own blobs from zero -- and the second
    publish would silently replace the first's, changing what an already-open
    tab serves out from under it. Prefixing every key in a session with a
    short random id removes that without any caller having to invent or
    coordinate its own scheme, so the guarantee holds for a view type that
    does not exist yet as much as for the ones that do.

    Attributes:
        server: The server payloads are published to.
        prefix: This session's key prefix, unique per call to
            :meth:`ViewerServer.publish_session`.
    """

    server: ViewerServer
    prefix: str

    def publish(self, key: str, payload: Payload) -> str:
        """Publish one payload under this session's namespace.

        Args:
            key: The payload's name within this session -- ``"index.html"``,
                ``"g0.bin"``, whatever the caller would have used against the
                bare server. Every session can reuse the same names freely;
                only the prefix has to be unique, and that is not this
                caller's problem.
            payload: The bytes and their type.

        Returns:
            The URL that serves it.
        """
        return self.server.publish(f"{self.prefix}/{key}", payload)


class ViewerServer:
    """Serves a small set of in-memory payloads on loopback.

    Not a general web server and deliberately incapable of becoming one: it can
    only return things that were handed to it, so the set of reachable bytes is
    exactly the set someone registered.
    """

    def __init__(self) -> None:
        """Create a server. Nothing listens until :meth:`start`."""
        self._payloads: dict[str, Payload] = {}
        self._token = secrets.token_urlsafe(24)
        self._server: ThreadingHTTPServer | None = None
        self._thread: threading.Thread | None = None
        self._lock = threading.Lock()

    @property
    def token(self) -> str:
        """The per-session token every request must present."""
        return self._token

    @property
    def port(self) -> int:
        """The bound port, or ``0`` when not started."""
        return self._server.server_port if self._server else 0

    @property
    def running(self) -> bool:
        """Whether the server is listening."""
        return self._server is not None

    def publish(self, key: str, payload: Payload) -> str:
        """Register one payload and return the URL that serves it.

        Args:
            key: A name for it. Used as a lookup key and nothing else -- it is
                never joined to a path, so its contents cannot escape anywhere.
            payload: The bytes and their type.

        Returns:
            An absolute URL including the session token.

        Raises:
            RuntimeError: If the server is not running, since a URL to a
                stopped server is worse than an error.
        """
        if self._server is None:
            raise RuntimeError("the viewer server is not running")
        with self._lock:
            self._payloads[key] = payload
        return f"http://{_HOST}:{self.port}/{key}?t={self._token}"

    def publish_session(self, kind: str) -> PublishSession:
        """Start a namespaced set of publishes for one view.

        Every kind of view -- the mesh viewer, the texture comparison,
        whatever reuses this server next -- gets its own collision-free slice
        of the same server and the same port, which is the point of sharing
        one server rather than standing up a second listener per feature.

        Args:
            kind: A short tag for the view type (``"mesh"``, ``"texture"``,
                ...). Carried into the key only so a URL reads sensibly in a
                log or a browser's network tab; it plays no part in collision
                safety, the random id appended to it does.

        Returns:
            A session whose :meth:`PublishSession.publish` namespaces every
            key automatically.
        """
        return PublishSession(self, f"{kind}-{secrets.token_hex(8)}")

    def fetch(self, key: str, token: str) -> Payload | None:
        """Look up a payload, checking the token first.

        Args:
            key: The registered name.
            token: The token from the request.

        Returns:
            The payload, or ``None`` when the token is wrong or the key is
            unknown. The two are deliberately indistinguishable to a caller:
            saying "wrong token" would confirm the key exists.
        """
        if not secrets.compare_digest(token, self._token):
            return None
        with self._lock:
            return self._payloads.get(key)

    def start(self) -> None:
        """Begin listening on an ephemeral loopback port.

        Raises:
            OSError: If no socket could be bound -- which happens on locked
                down machines, and is why the caller keeps the embedded path.
        """
        if self._server is not None:
            return
        handler = _make_handler(self)
        self._server = ThreadingHTTPServer((_HOST, 0), handler)
        self._thread = threading.Thread(
            target=self._server.serve_forever, name="mlox-viewer", daemon=True
        )
        self._thread.start()
        LOG.info("viewer server listening on %s:%d", _HOST, self.port)

    def stop(self) -> None:
        """Stop listening and drop every payload.

        Safe to call when not running, and safe to call twice: shutting down a
        viewer is exactly the sort of thing that gets wired to two events.
        """
        server, thread = self._server, self._thread
        self._server = self._thread = None
        if server is None:
            return
        server.shutdown()
        server.server_close()
        if thread is not None:
            thread.join(timeout=_SHUTDOWN_TIMEOUT)
        with self._lock:
            self._payloads.clear()
        LOG.info("viewer server stopped")


def publish_html_file(server: ViewerServer | None, path: str | Path) -> str | None:
    """Serve a self-contained HTML view from a running loopback server.

    Some platforms refuse a ``file://`` page -- the Steam Deck's browser and
    various sandboxed webviews among them -- so a generated view is served over
    loopback by default and only opened as the written file when no port can be
    bound. Every view this tool writes embeds its data and scripts (no external
    assets), so publishing the one document is the whole page; there are no
    sidecars to map, which keeps the server's no-filesystem guarantee intact.

    Args:
        server: A running :class:`ViewerServer`, or ``None``.
        path: The written HTML file.

    Returns:
        A ``http://127.0.0.1`` URL, or ``None`` when the server is not running
        or the file cannot be read -- in which case the caller keeps the file
        path as the fallback.
    """
    if server is None or not server.running:
        return None
    try:
        body = Path(path).read_bytes()
    except OSError:
        return None
    try:
        session = server.publish_session("view")
        return session.publish("index.html", Payload(body, "text/html; charset=utf-8"))
    except RuntimeError:
        # Raced with a stop(): treat as "no loopback", fall back to the file.
        return None


def _make_handler(owner: ViewerServer) -> type[BaseHTTPRequestHandler]:
    """Build a request handler bound to one server instance.

    A closure rather than a class attribute so two servers cannot end up
    sharing state, which is the sort of bug that only appears once someone
    opens a second window.

    Args:
        owner: The server whose payloads this handler serves.

    Returns:
        A handler class.
    """

    class Handler(BaseHTTPRequestHandler):
        """Serves registered payloads and nothing else."""

        protocol_version = "HTTP/1.1"

        def do_GET(self) -> None:
            """Answer a GET, or 404."""
            parsed = urlparse(self.path)
            key = parsed.path.lstrip("/")
            token = (parse_qs(parsed.query).get("t") or [""])[0]
            payload = owner.fetch(key, token)
            if payload is None:
                self.send_error(404)
                return
            self.send_response(200)
            self.send_header("Content-Type", payload.content_type)
            self.send_header("Content-Length", str(len(payload.body)))
            # Nothing here should ever be framed by another page, and the
            # content types are fixed at registration, so sniffing can only
            # produce a surprise.
            self.send_header("X-Content-Type-Options", "nosniff")
            self.send_header("X-Frame-Options", "DENY")
            self.send_header("Cache-Control", "no-store")
            self.end_headers()
            self.wfile.write(payload.body)

        def log_message(self, format: str, *args: object) -> None:  # noqa: A002
            """Send request logs to the application log, not to stderr.

            Args:
                format: A printf-style format.
                args: Its arguments.
            """
            LOG.debug("viewer server: " + format, *args)

    return Handler


def payloads_for(page: str, library: str, geometry: Mapping[str, bytes]) -> dict[str, Payload]:
    """Bundle the pieces of one viewer session into registrable payloads.

    Args:
        page: The HTML document.
        library: The three.js source.
        geometry: Packed geometry blobs, by key.

    Returns:
        Payloads ready for :meth:`ViewerServer.publish`.
    """
    bundle = {
        "index.html": Payload(page.encode("utf-8"), "text/html; charset=utf-8"),
        "three.js": Payload(library.encode("utf-8"), "text/javascript; charset=utf-8"),
    }
    for key, blob in geometry.items():
        bundle[key] = Payload(blob, "application/octet-stream")
    return bundle
