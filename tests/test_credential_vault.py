"""Tests for credential_vault.py — encrypted credential storage."""

import json
import os
from unittest.mock import patch

import pytest

from credential_vault import CredentialVault


@pytest.fixture
def vault(tmp_path):
    path = str(tmp_path / "vault.json")
    return CredentialVault(vault_path=path)


class TestCreateVault:
    def test_create_vault_sets_unlocked(self, vault):
        result = vault.create_vault("1234")
        assert vault.is_unlocked
        assert "created" in result.lower()

    def test_create_vault_persists_file(self, vault):
        vault.create_vault("1234")
        assert os.path.exists(vault.vault_path)

    def test_create_vault_returns_pin_warning(self, vault):
        result = vault.create_vault("1234")
        assert "PIN" in result


class TestUnlock:
    def test_unlock_with_correct_pin(self, vault):
        vault.create_vault("1234")
        vault.lock()
        result = vault.unlock("1234")
        assert vault.is_unlocked
        assert "unlocked" in result.lower()

    def test_unlock_with_wrong_pin(self, vault):
        vault.create_vault("1234")
        vault.lock()
        result = vault.unlock("9999")
        assert not vault.is_unlocked
        assert "wrong" in result.lower()

    def test_unlock_no_vault(self, tmp_path):
        path = str(tmp_path / "nonexistent.json")
        v = CredentialVault(vault_path=path)
        result = v.unlock("1234")
        assert "no vault" in result.lower()


class TestLock:
    def test_lock_clears_state(self, vault):
        vault.create_vault("1234")
        result = vault.lock()
        assert not vault.is_unlocked
        assert vault._key is None
        assert vault._cipher is None
        assert "locked" in result.lower()


class TestStoreAndGet:
    def test_store_and_retrieve(self, vault):
        vault.create_vault("1234")
        vault.store("email", {"user": "test@example.com", "pass": "secret"})
        creds = vault.get("email")
        assert creds == {"user": "test@example.com", "pass": "secret"}

    def test_store_when_locked(self, vault):
        result = vault.store("email", {"user": "x"})
        assert "locked" in result.lower()

    def test_get_when_locked(self, vault):
        assert vault.get("email") is None

    def test_get_nonexistent_service(self, vault):
        vault.create_vault("1234")
        assert vault.get("nonexistent") is None


class TestListAndDelete:
    def test_list_services(self, vault):
        vault.create_vault("1234")
        vault.store("email", {"u": "a"})
        vault.store("wifi", {"ssid": "home"})
        result = vault.list_services()
        assert "email" in result
        assert "wifi" in result

    def test_list_empty(self, vault):
        vault.create_vault("1234")
        result = vault.list_services()
        assert "no credentials" in result.lower()

    def test_delete_service(self, vault):
        vault.create_vault("1234")
        vault.store("email", {"u": "a"})
        result = vault.delete("email")
        assert "deleted" in result.lower()
        assert vault.get("email") is None

    def test_delete_nonexistent(self, vault):
        vault.create_vault("1234")
        result = vault.delete("nonexistent")
        assert "no credentials" in result.lower()


class TestChangePin:
    def test_change_pin(self, vault):
        vault.create_vault("old")
        vault.store("email", {"u": "a"})
        result = vault.change_pin("old", "new")
        assert "changed" in result.lower()
        vault.lock()
        assert vault.unlock("new") or vault.is_unlocked
        assert vault.get("email") == {"u": "a"}

    def test_change_pin_wrong_old(self, vault):
        vault.create_vault("old")
        result = vault.change_pin("wrong", "new")
        assert "wrong" in result.lower()
