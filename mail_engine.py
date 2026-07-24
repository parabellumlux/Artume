#!/usr/bin/env python3
"""Artome Audio Email Client Engine - IMAP/SMTP reader and voice composer."""

import imaplib
import smtplib
import email
from email.header import decode_header
from email.mime.text import MIMEText
from email.mime.multipart import MIMEMultipart
from bs4 import BeautifulSoup
import html2text

class AudioMailClient:
    """IMAP/SMTP voice mail interface for Artome DE."""

    def __init__(self):
        self.inbox_cache = []
        self.active_draft = None
        self.h2t = html2text.HTML2Text()
        self.h2t.ignore_links = True
        self.h2t.ignore_images = True

    def _decode_str(self, header_val):
        if not header_val:
            return ""
        decoded, encoding = decode_header(header_val)[0]
        if isinstance(decoded, bytes):
            return decoded.decode(encoding or 'utf-8', errors='ignore')
        return str(decoded)

    def fetch_inbox(self, imap_server, username, password, limit=5):
        """Fetch unread emails from IMAP server and return audio summary."""
        try:
            mail = imaplib.IMAP4_SSL(imap_server)
            mail.login(username, password)
            mail.select("INBOX")

            status, response = mail.search(None, "UNSEEN")
            email_ids = response[0].split()

            if not email_ids:
                return "You have no unread emails."

            self.inbox_cache = []
            recent_ids = email_ids[-limit:]

            for e_id in reversed(recent_ids):
                status, data = mail.fetch(e_id, "(RFC822)")
                for response_part in data:
                    if isinstance(response_part, tuple):
                        msg = email.message_from_bytes(response_part[1])
                        subject = self._decode_str(msg.get("Subject"))
                        from_sender = self._decode_str(msg.get("From"))

                        body_text = ""
                        if msg.is_multipart():
                            for part in msg.walk():
                                content_type = part.get_content_type()
                                if content_type == "text/plain":
                                    body_text = part.get_payload(decode=True).decode('utf-8', errors='ignore')
                                    break
                                elif content_type == "text/html":
                                    html_body = part.get_payload(decode=True).decode('utf-8', errors='ignore')
                                    body_text = self.h2t.handle(html_body)
                                    break
                        else:
                            body_text = msg.get_payload(decode=True).decode('utf-8', errors='ignore')

                        self.inbox_cache.append({
                            "id": e_id,
                            "from": from_sender,
                            "subject": subject,
                            "body": body_text.strip()[:500]
                        })

            mail.logout()

            summary = f"You have {len(self.inbox_cache)} unread emails. "
            for idx, item in enumerate(self.inbox_cache, 1):
                clean_sender = item['from'].split('<')[0].strip()
                summary += f"Email {idx} from {clean_sender}: Subject: {item['subject']}. "

            return summary

        except Exception as e:
            return f"Failed to check inbox: {str(e)[:50]}"

    def read_email_audio(self, index=1):
        """Read email contents out loud by 1-based index."""
        if not self.inbox_cache:
            return "No emails loaded in inbox cache."
        if 1 <= index <= len(self.inbox_cache):
            item = self.inbox_cache[index - 1]
            clean_sender = item['from'].split('<')[0].strip()
            speech = f"Reading email {index} from {clean_sender}. Subject: {item['subject']}. Message reads: {item['body']}"
            return speech
        return f"Invalid email number {index}."

    def prepare_draft(self, recipient, subject, body):
        """Store pending email draft for voice confirmation."""
        self.active_draft = {
            "to": recipient,
            "subject": subject,
            "body": body
        }
        return f"Prepared email draft to {recipient} with subject '{subject}'. Message reads: {body}. Say 'send' to confirm, or 'cancel'."

    def send_draft(self, smtp_server, smtp_port, sender_email, password):
        """Send the active email draft via SMTP."""
        if not self.active_draft:
            return "No active draft to send."

        try:
            msg = MIMEMultipart()
            msg['From'] = sender_email
            msg['To'] = self.active_draft['to']
            msg['Subject'] = self.active_draft['subject']
            msg.attach(MIMEText(self.active_draft['body'], 'plain'))

            server = smtplib.SMTP_SSL(smtp_server, smtp_port)
            server.login(sender_email, password)
            server.send_message(msg)
            server.quit()

            to_user = self.active_draft['to']
            self.active_draft = None
            return f"Email successfully sent to {to_user}."
        except Exception as e:
            return f"Failed to send email: {str(e)[:50]}"

if __name__ == "__main__":
    mail = AudioMailClient()
    print("Testing Audio Mail draft confirmation loop...")
    print(mail.prepare_draft("alice@example.com", "Project Status", "Hi Alice, the Artome audio engine is working great."))
