"""Artume Credential Vault — Encrypted credential storage.

Stores IMAP/SMTP/WiFi credentials encrypted at rest.
Unlocks with a voice PIN. Uses Fernet symmetric encryption.
"""

import json
import os
from typing import Optional, Dict, Any
from cryptography.fernet import Fernet
from cryptography.hazmat.primitives import hashes
from cryptography.hazmat.primitives.kdf.pbkdf2 import PBKDF2HMAC
import base64


class CredentialVault:
    """Encrypted credential store for Artume OS."""

    def __init__(self, vault_path: Optional[str] = None):
        self.vault_path = vault_path or os.path.expanduser("~/.config/artume/vault.json")
        self._key: Optional[bytes] = None
        self._cipher: Optional[Fernet] = None
        self._unlocked = False
        self._data: Dict[str, Any] = {}
        os.makedirs(os.path.dirname(self.vault_path), exist_ok=True)

    def _derive_key(self, pin: str, salt: bytes) -> bytes:
        kdf = PBKDF2HMAC(
            algorithm=hashes.SHA256(),
            length=32,
            salt=salt,
            iterations=480000,
        )
        return base64.urlsafe_b64encode(kdf.derive(pin.encode()))

    def create_vault(self, pin: str) -> str:
        """Create a new encrypted vault with the given PIN."""
        salt = os.urandom(16)
        self._key = self._derive_key(pin, salt)
        self._cipher = Fernet(self._key)
        self._data = {"_salt": base64.urlsafe_b64encode(salt).decode(), "credentials": {}}
        self._unlocked = True
        self._save()
        return "Credential vault created. Remember your PIN — it cannot be recovered."

    def unlock(self, pin: str) -> str:
        """Unlock the vault with a voice PIN."""
        if not os.path.exists(self.vault_path):
            return "No vault found. Say 'create vault' to set one up."

        try:
            with open(self.vault_path, "r") as f:
                stored = json.load(f)
            salt = base64.urlsafe_b64decode(stored["_salt"])
            self._key = self._derive_key(pin, salt)
            self._cipher = Fernet(self._key)
            # Verify by decrypting
            encrypted_data = stored["data"]
            decrypted = self._cipher.decrypt(encrypted_data.encode())
            self._data = json.loads(decrypted.decode())
            self._unlocked = True
            cred_count = len(self._data.get("credentials", {}))
            return f"Vault unlocked. {cred_count} credentials stored."
        except Exception:
            return "Wrong PIN. Try again."

    def lock(self) -> str:
        """Lock the vault."""
        self._key = None
        self._cipher = None
        self._unlocked = False
        self._data = {}
        return "Vault locked."

    @property
    def is_unlocked(self) -> bool:
        return self._unlocked

    def _save(self):
        if not self._cipher:
            return
        encrypted = self._cipher.encrypt(json.dumps(self._data).encode())
        stored = {
            "_salt": self._data["_salt"],
            "data": encrypted.decode(),
        }
        with open(self.vault_path, "w") as f:
            json.dump(stored, f)

    def store(self, service: str, credentials: Dict[str, str]) -> str:
        """Store credentials for a service (e.g. email, wifi)."""
        if not self._unlocked:
            return "Vault is locked."
        if "credentials" not in self._data:
            self._data["credentials"] = {}
        self._data["credentials"][service] = credentials
        self._save()
        return f"Stored credentials for {service}."

    def get(self, service: str) -> Optional[Dict[str, str]]:
        """Retrieve credentials for a service."""
        if not self._unlocked:
            return None
        return self._data.get("credentials", {}).get(service)

    def list_services(self) -> str:
        """List all stored credential services."""
        if not self._unlocked:
            return "Vault is locked."
        services = list(self._data.get("credentials", {}).keys())
        if not services:
            return "No credentials stored."
        return f"Stored credentials for: {', '.join(services)}."

    def delete(self, service: str) -> str:
        """Delete credentials for a service."""
        if not self._unlocked:
            return "Vault is locked."
        if service in self._data.get("credentials", {}):
            del self._data["credentials"][service]
            self._save()
            return f"Deleted credentials for {service}."
        return f"No credentials found for {service}."

    def change_pin(self, old_pin: str, new_pin: str) -> str:
        """Change the vault PIN."""
        result = self.unlock(old_pin)
        if "Wrong" in result:
            return result
        # Re-encrypt with new PIN
        salt = os.urandom(16)
        self._key = self._derive_key(new_pin, salt)
        self._cipher = Fernet(self._key)
        self._data["_salt"] = base64.urlsafe_b64encode(salt).decode()
        self._save()
        return "PIN changed successfully."


# Global singleton
_vault: Optional[CredentialVault] = None


def get_vault() -> CredentialVault:
    global _vault
    if _vault is None:
        _vault = CredentialVault()
    return _vault
