"""Tests for the EMAIL mode fallback (handlers/modes.py)."""

from unittest.mock import MagicMock, patch

from handlers.modes import handle


def _ctx(mail=None, vault_creds=None):
    mail_client = mail if mail is not None else MagicMock()
    mail_client.fetch_inbox.return_value = "3 unread emails from Alice, Bob, Carol."
    vault = MagicMock()
    vault.get.return_value = vault_creds
    return {
        'tts': MagicMock(),
        'navigator': MagicMock(),
        'ebook_reader': MagicMock(),
        'doc_writer': MagicMock(),
        'sys_settings': MagicMock(),
        'file_browser': MagicMock(),
        'browser': MagicMock(),
        'mail_client': mail_client,
        'state_mgr': MagicMock(),
        'vault': vault,
    }


def _call(ctx, speech, target, action=None, mode="EMAIL"):
    with patch("earcons.play_earcon"), \
         patch("credential_vault.get_vault", return_value=ctx['vault']):
        return handle(
            speech, target.lower(), target, speech, action, mode, ctx)


class TestEmailMode:
    def test_check_inbox_uses_vault_creds(self):
        creds = {"imap_server": "imap.x.com", "username": "u", "password": "p"}
        mail = MagicMock()
        mail.fetch_inbox.return_value = "3 unread emails."
        handled, mode = _call(_ctx(mail, creds), "check email", "check")
        assert handled is True and mode == "EMAIL"
        mail.fetch_inbox.assert_called_once_with("imap.x.com", "u", "p")

    def test_check_inbox_without_creds(self):
        ctx = _ctx(MagicMock(), None)
        handled, _ = _call(ctx, "check email", "check")
        assert handled is True
        ctx['tts'].speak.assert_any_call(
            "No email credentials stored. Say 'store credential' to set up.")

    def test_read_uses_requested_index(self):
        mail = MagicMock()
        mail.read_email_audio.return_value = "Email 3"
        handled, _ = _call(_ctx(mail), "read email 3", "read email 3")
        assert handled is True
        mail.read_email_audio.assert_called_once_with(3)

    def test_read_defaults_to_first(self):
        mail = MagicMock()
        mail.read_email_audio.return_value = "Email 1"
        handled, _ = _call(_ctx(mail), "read", "read")
        assert handled is True
        mail.read_email_audio.assert_called_once_with(1)