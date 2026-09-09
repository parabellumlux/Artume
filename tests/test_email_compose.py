"""Tests for email compose/send voice flows (artome_core.email_action)."""

from unittest.mock import MagicMock, patch

from artome_core import execute_action
from mail_engine import AudioMailClient


def _ctx(mail, phrases=None, ask=True):
    dialog = MagicMock()
    if phrases:
        dialog.capture_phrase.side_effect = list(phrases)
    dialog.ask.return_value = ask
    navigator = MagicMock()
    navigator.select_menu_option.return_value = (None, "")
    return {
        'tts': MagicMock(),
        'mail_client': mail,
        'confirm_dialog': dialog,
        'navigator': navigator,
    }


_FAKE_CREDS = {
    "imap_server": "imap.example.com",
    "smtp_server": "smtp.example.com",
    "smtp_port": "465",
    "username": "me@example.com",
    "password": "secret",
}


def _send(mail, ctx, speech="send email"):
    with patch("earcons.play_earcon"):
        return execute_action(
            {"action": "email_action", "speech": "Sending email", "target": "send_draft"},
            speech, "DESKTOP", ctx)


class TestCompose:
    def test_compose_guided_creates_draft(self):
        mail = AudioMailClient()
        ctx = _ctx(mail, phrases=["alice@example.com", "Project status", "Hi Alice, all done."])
        with patch("earcons.play_earcon"):
            mode = execute_action(
                {"action": "email_action", "speech": "Preparing to compose", "target": "compose"},
                "compose email", "DESKTOP", ctx)
        assert mode == "DESKTOP"
        assert mail.active_draft == {
            "to": "alice@example.com",
            "subject": "Project status",
            "body": "Hi Alice, all done.",
        }

    def test_compose_missing_recipient_aborts(self):
        mail = AudioMailClient()
        ctx = _ctx(mail, phrases=[None])
        with patch("earcons.play_earcon"):
            execute_action(
                {"action": "email_action", "speech": "Preparing to compose", "target": "compose"},
                "compose email", "DESKTOP", ctx)
        assert mail.active_draft is None


class TestSendDraft:
    def test_send_without_draft(self):
        mail = AudioMailClient()
        ctx = _ctx(mail)
        _send(mail, ctx)
        ctx['tts'].speak.assert_any_call("No draft to send. Say 'compose email' first.")

    def test_send_needs_credentials(self):
        mail = AudioMailClient()
        mail.active_draft = {"to": "b@x.com", "subject": "hi", "body": "yo"}
        ctx = _ctx(mail)
        mock_vault = MagicMock()
        mock_vault.get.return_value = None
        with patch("credential_vault.get_vault", return_value=mock_vault):
            _send(mail, ctx)
        ctx['tts'].speak.assert_any_call(
            "No email credentials stored. Say 'store credential' to set up.")

    def test_send_cancelled_by_user(self):
        mail = AudioMailClient()
        mail.active_draft = {"to": "b@x.com", "subject": "hi", "body": "yo"}
        ctx = _ctx(mail, ask=False)
        mock_vault = MagicMock()
        mock_vault.get.return_value = _FAKE_CREDS
        with patch("credential_vault.get_vault", return_value=mock_vault):
            _send(mail, ctx)
        ctx['tts'].speak.assert_any_call("Email send cancelled.")
        assert mail.active_draft is not None

    def test_send_confirmed(self):
        mail = AudioMailClient()
        mail.active_draft = {"to": "b@x.com", "subject": "hi", "body": "yo"}
        ctx = _ctx(mail, ask=True)
        mock_vault = MagicMock()
        mock_vault.get.return_value = _FAKE_CREDS
        with patch("credential_vault.get_vault", return_value=mock_vault), \
             patch("mail_engine.smtplib.SMTP_SSL") as smtp:
            _send(mail, ctx)
        smtp.assert_called_once()
        smtp.return_value.login.assert_called_once_with("me@example.com", "secret")
        smtp.return_value.send_message.assert_called_once()
        assert smtp.return_value.quit.called
        assert mail.active_draft is None
        spoken = [str(c) for c in ctx['tts'].speak.call_args_list]
        assert any("successfully sent" in s for s in spoken)


class TestCancelDraft:
    def test_cancel_clears_draft(self):
        mail = AudioMailClient()
        mail.active_draft = {"to": "b@x.com", "subject": "hi", "body": "yo"}
        ctx = _ctx(mail)
        with patch("earcons.play_earcon"):
            execute_action(
                {"action": "email_action", "speech": "Cancelling", "target": "cancel_draft"},
                "cancel draft", "DESKTOP", ctx)
        assert mail.active_draft is None