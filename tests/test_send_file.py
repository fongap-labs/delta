"""Foundation send_file contract: provider dispatch plus local path containment."""

from pathlib import Path

import pytest

from core.roots import RootDir
from integrations.connectors.base import SendResult
from integrations.connectors.messaging_providers import (
    MessagingProvider,
    clear_messaging_providers,
    register_messaging_provider,
)
from integrations.connectors.tools import build_send_file
from packages.credential_store import CredentialStore as SecretStore


@pytest.fixture(autouse=True)
def _clean_provider_registry():
    clear_messaging_providers()
    yield
    clear_messaging_providers()


def _store(tmp_path) -> SecretStore:
    return SecretStore(tmp_path / "secrets.json")


def _register_file_provider(record: list, *, platform: str = "fake") -> None:
    def send(_secrets, _chat_id, _text, _thread_id):
        return SendResult(True, message_id="M1")

    def send_file(
        _secrets, chat_id, thread_id, filename, data, title, comment
    ):
        record.append(
            {
                "chat_id": chat_id,
                "thread_id": thread_id,
                "filename": filename,
                "data": data,
                "title": title,
                "comment": comment,
            }
        )
        return SendResult(True, message_id="F123")

    register_messaging_provider(
        MessagingProvider(platform=platform, send=send, send_file=send_file)
    )


def test_send_file_success_within_workspace(tmp_path):
    ws = tmp_path / "ws"
    ws.mkdir()
    (ws / "report.pdf").write_bytes(b"%PDF-fake")
    record: list = []
    _register_file_provider(record)
    tool = build_send_file(_store(tmp_path), workspace=ws)

    out = tool("fake:C9:1700.1", "report.pdf", comment="here you go")
    assert out == {
        "ok": True,
        "file_id": "F123",
        "target": "fake:C9:1700.1",
        "filename": "report.pdf",
    }
    sent = record[0]
    assert sent["chat_id"] == "C9" and sent["thread_id"] == "1700.1"
    assert sent["data"] == b"%PDF-fake" and sent["comment"] == "here you go"


def test_send_file_rejects_paths_outside_roots(tmp_path):
    ws = tmp_path / "ws"
    ws.mkdir()
    outside = tmp_path / "elsewhere.txt"
    outside.write_text("secret")
    _register_file_provider([])
    tool = build_send_file(_store(tmp_path), workspace=ws)

    assert "error" in tool("fake:C9", str(outside))
    assert "error" in tool("fake:C9", "../elsewhere.txt")


def test_send_file_roots_extend_the_reachable_set(tmp_path):
    ws = tmp_path / "ws"
    ws.mkdir()
    shared = tmp_path / "shared"
    shared.mkdir()
    (shared / "data.csv").write_text("a,b\n1,2\n")
    record: list = []
    _register_file_provider(record)
    tool = build_send_file(
        _store(tmp_path), workspace=ws, roots=[RootDir(path=shared)]
    )
    out = tool("fake:C9", str(shared / "data.csv"))
    assert out["ok"] and record[0]["filename"] == "data.csv"


def test_send_file_requires_provider_file_capability(tmp_path):
    ws = tmp_path / "ws"
    ws.mkdir()
    (ws / "a.txt").write_text("x")

    def send(_secrets, _chat_id, _text, _thread_id):
        return SendResult(True, message_id="M1")

    register_messaging_provider(MessagingProvider(platform="textonly", send=send))
    tool = build_send_file(_store(tmp_path), workspace=ws)
    assert "not supported" in tool("textonly:C9", "a.txt")["error"]
    assert "unknown messaging platform" in tool("missing:C9", "a.txt")["error"]


def test_send_file_screenshot_is_html_only_and_renames_to_png(tmp_path):
    ws = tmp_path / "ws"
    ws.mkdir()
    (ws / "dash.html").write_text("<h1>hi</h1>")
    (ws / "notes.md").write_text("# hi")
    record: list = []
    _register_file_provider(record)
    tool = build_send_file(
        _store(tmp_path),
        workspace=ws,
        render_html=lambda p: b"PNG-bytes-for-" + Path(p).name.encode(),
    )

    assert (
        "only applies to .html"
        in tool("fake:C9", "notes.md", should_render_image=True)["error"]
    )
    out = tool("fake:C9", "dash.html", should_render_image=True)
    assert out["ok"] and out["filename"] == "dash.png"
    assert record[-1]["data"] == b"PNG-bytes-for-dash.html"
