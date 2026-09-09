"""Tests for handlers/desktop.py — credential vault voice flows."""

import os
from unittest.mock import MagicMock, patch

from handlers.desktop import handle

from credential_vault import CredentialVault


def _new_vault(tmp_path):
    return CredentialVault(vault_path=str(tmp_path / "vault.json"))


def _make_ctx(vault, phrases=None):
    dialog = MagicMock()
    if phrases:
        dialog.capture_phrase.side_effect = list(phrases)
    dialog.ask.return_value = False
    return {
        'tts': MagicMock(),
        'notification_center': MagicMock(),
        'clipboard': MagicMock(),
        'window_mgr': MagicMock(),
        'app_launcher': MagicMock(),
        'vault': vault,
        'confirm_dialog': dialog,
    }


def _handle(speech, ctx):
    with patch("handlers.desktop.play_earcon"):
        return handle(speech, ctx)


class TestUnlockVault:
    def test_unlock_inline_pin(self, tmp_path):
        v = _new_vault(tmp_path)
        v.create_vault("1234")
        v.lock()
        ctx = _make_ctx(v)
        assert _handle("unlock vault 1234", ctx)
        assert v.is_unlocked
        ctx['confirm_dialog'].capture_phrase.assert_not_called()

    def test_unlock_captured_pin(self, tmp_path):
        v = _new_vault(tmp_path)
        v.create_vault("4321")
        v.lock()
        ctx = _make_ctx(v, phrases=["4321"])
        assert _handle("unlock vault", ctx)
        assert v.is_unlocked

    def test_unlock_wrong_pin(self, tmp_path):
        v = _new_vault(tmp_path)
        v.create_vault("1234")
        v.lock()
        ctx = _make_ctx(v, phrases=["9999"])
        assert _handle("unlock vault", ctx)
        assert not v.is_unlocked

    def test_unlock_no_pin_heard(self, tmp_path):
        v = _new_vault(tmp_path)
        v.create_vault("1234")
        v.lock()
        ctx = _make_ctx(v, phrases=[None])
        assert _handle("unlock vault", ctx)
        assert not v.is_unlocked


class TestCreateVault:
    def test_create_guided(self, tmp_path):
        v = _new_vault(tmp_path)
        assert _handle("create vault", _make_ctx(v, phrases=["1234"]))
        assert v.is_unlocked
        assert os.path.exists(v.vault_path)

    def test_create_refuses_to_overwrite(self, tmp_path):
        v = _new_vault(tmp_path)
        v.create_vault("1111")
        v.store("email", {"username": "a", "password": "b"})
        v.lock()
        ctx = _make_ctx(v, phrases=["2222"])
        ctx['confirm_dialog'].ask.return_value = False
        assert _handle("create vault", ctx)
        assert not v.is_unlocked
        assert v.unlock("1111").startswith("Vault unlocked")

    def test_create_replaces_after_confirm(self, tmp_path):
        v = _new_vault(tmp_path)
        v.create_vault("1111")
        v.store("email", {"username": "a", "password": "b"})
        v.lock()
        ctx = _make_ctx(v, phrases=["2222"])
        ctx['confirm_dialog'].ask.return_value = True
        assert _handle("create vault", ctx)
        assert v.is_unlocked
        assert v.get("email") is None


class TestStoreCredential:
    def test_store_requires_unlock(self, tmp_path):
        v = _new_vault(tmp_path)
        assert _handle("store credential email user x@y.z password s", _make_ctx(v))
        assert v.get("email") is None

    def test_store_inline(self, tmp_path):
        v = _new_vault(tmp_path)
        v.create_vault("1234")
        ctx = _make_ctx(v)
        assert _handle("store credential email user me@x.com password secret", ctx)
        assert v.get("email") == {"username": "me@x.com", "password": "secret"}
        ctx['confirm_dialog'].capture_phrase.assert_not_called()

    def test_store_guided(self, tmp_path):
        v = _new_vault(tmp_path)
        v.create_vault("1234")
        ctx = _make_ctx(v, phrases=["email", "me@x.com", "s3cret"])
        assert _handle("store credential", ctx)
        assert v.get("email") == {"username": "me@x.com", "password": "s3cret"}

    def test_store_skips_after_missing_input(self, tmp_path):
        v = _new_vault(tmp_path)
        v.create_vault("1234")
        ctx = _make_ctx(v, phrases=[None])
        assert _handle("store credential", ctx)
        assert v.get("email") is None