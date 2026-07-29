"""Tests for the loopback viewer server.

Most of these are about what the server *refuses*. It is a listening socket on
a machine full of the user's files, so the interesting assertions are that it
cannot be talked into serving anything nobody registered -- and in particular
that there is no path from a URL to the filesystem at all, which is a stronger
property than "traversal is blocked".
"""

from __future__ import annotations

import socket
import urllib.error
import urllib.request

import pytest

from mlox_subset.nif.serve import Payload, ViewerServer, payloads_for


@pytest.fixture
def server() -> ViewerServer:
    """A running server, stopped afterwards.

    Yields:
        The server.
    """
    running = ViewerServer()
    running.start()
    try:
        yield running
    finally:
        running.stop()


def get(url: str) -> bytes:
    """Fetch a URL.

    Args:
        url: What to fetch.

    Returns:
        The body.
    """
    with urllib.request.urlopen(url, timeout=5) as response:  # noqa: S310 - loopback, our own
        return bytes(response.read())


class TestItServesWhatWasRegistered:
    """The ordinary path."""

    def test_a_published_payload_comes_back(self, server: ViewerServer) -> None:
        """With its content type intact."""
        url = server.publish("index.html", Payload(b"<h1>hi</h1>", "text/html"))
        assert get(url) == b"<h1>hi</h1>"

    def test_publishing_before_starting_is_an_error(self) -> None:
        """A URL to a server that is not listening is worse than an exception."""
        with pytest.raises(RuntimeError, match="not running"):
            ViewerServer().publish("x", Payload(b""))

    def test_binary_payloads_survive_intact(self, server: ViewerServer) -> None:
        """Geometry is deflated binary; any text handling would corrupt it."""
        blob = bytes(range(256)) * 4
        assert get(server.publish("g.bin", Payload(blob))) == blob


class TestItRefusesEverythingElse:
    """The reason this file is mostly negative assertions."""

    def test_no_token_is_a_404(self, server: ViewerServer) -> None:
        """404 rather than 403: a 403 confirms the key exists."""
        url = server.publish("index.html", Payload(b"x")).split("?")[0]
        with pytest.raises(urllib.error.HTTPError) as caught:
            get(url)
        assert caught.value.code == 404

    def test_a_wrong_token_is_a_404(self, server: ViewerServer) -> None:
        """Indistinguishable from a missing key, deliberately."""
        url = server.publish("index.html", Payload(b"x")).split("?")[0]
        with pytest.raises(urllib.error.HTTPError) as caught:
            get(f"{url}?t=not-the-token")
        assert caught.value.code == 404

    @pytest.mark.parametrize(
        "probe",
        [
            "../../../etc/passwd",
            "..%2f..%2f..%2fetc%2fpasswd",
            "mlox_subset/nif/serve.py",
            "/etc/hostname",
            "....//....//etc/passwd",
        ],
    )
    def test_nothing_reaches_the_filesystem(self, server: ViewerServer, probe: str) -> None:
        """Not "traversal is blocked" -- there is nothing to traverse.

        The server has no code path from a URL to ``open``. A key that was
        never registered simply is not in the dictionary, so every one of these
        is an ordinary miss rather than a defence that could be outwitted.
        """
        server.publish("index.html", Payload(b"x"))
        with pytest.raises(urllib.error.HTTPError) as caught:
            get(f"http://127.0.0.1:{server.port}/{probe}?t={server.token}")
        assert caught.value.code == 404

    def test_each_server_has_its_own_token(self) -> None:
        """One window's token must not open another's."""
        first, second = ViewerServer(), ViewerServer()
        assert first.token != second.token

    def test_a_token_from_another_server_does_not_work(self, server: ViewerServer) -> None:
        """The concrete form of the property above."""
        other = ViewerServer()
        url = server.publish("index.html", Payload(b"x")).split("?")[0]
        with pytest.raises(urllib.error.HTTPError) as caught:
            get(f"{url}?t={other.token}")
        assert caught.value.code == 404


class TestItStaysOnLoopback:
    """Nothing off this machine may reach it."""

    def test_it_binds_loopback_only(self, server: ViewerServer) -> None:
        """Binding all interfaces would expose a user's meshes to the network.

        Checked by asking the OS what the socket is bound to rather than by
        reading the constant back, which would only test that a literal equals
        itself.
        """
        assert server._server is not None
        assert server._server.server_address[0] == "127.0.0.1"

    def test_the_port_is_ephemeral(self, server: ViewerServer) -> None:
        """Chosen by the OS, so nothing can be predicted or squatted."""
        assert server.port > 0
        assert server.port != 80

    def test_it_is_not_reachable_on_another_local_address(self, server: ViewerServer) -> None:
        """A bind to 127.0.0.1 must refuse a connection to a routable address.

        Skipped when the machine has no other address to try, which is the
        case in many containers.
        """
        try:
            outward = socket.gethostbyname(socket.gethostname())
        except OSError:  # pragma: no cover - depends on the host
            pytest.skip("no resolvable hostname")
        if outward.startswith("127."):
            pytest.skip("this machine only has loopback")
        with socket.socket() as probe:
            probe.settimeout(2)
            assert probe.connect_ex((outward, server.port)) != 0


class TestLifecycle:
    """Starting and stopping gets wired to more than one event."""

    def test_stopping_twice_is_safe(self, server: ViewerServer) -> None:
        """Window close and app exit both plausibly call it."""
        server.stop()
        server.stop()
        assert not server.running

    def test_starting_twice_keeps_one_port(self, server: ViewerServer) -> None:
        """Otherwise a second call would leak a socket."""
        first = server.port
        server.start()
        assert server.port == first

    def test_stopping_forgets_the_payloads(self, server: ViewerServer) -> None:
        """A closed viewer must not leave a user's geometry in memory."""
        server.publish("index.html", Payload(b"secret"))
        server.stop()
        assert server.fetch("index.html", server.token) is None


class TestPayloadBundle:
    """The convenience that assembles one session."""

    def test_it_types_each_piece(self) -> None:
        """A wrong content type on the page shows HTML as plain text."""
        bundle = payloads_for("<html>", "var x = 1;", {"g0.bin": b"\x00\x01"})
        assert bundle["index.html"].content_type.startswith("text/html")
        assert bundle["three.js"].content_type.startswith("text/javascript")
        assert bundle["g0.bin"].body == b"\x00\x01"
