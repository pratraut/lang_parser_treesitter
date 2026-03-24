"""Example Python module for call-chain analysis."""

import os
import json
from pathlib import Path


class TokenLedger:
    """Simple in-memory token ledger."""

    def __init__(self, name: str, symbol: str, initial_supply: int):
        self.name = name
        self.symbol = symbol
        self._balances: dict[str, int] = {}
        self._allowances: dict[str, dict[str, int]] = {}
        self._total_supply = 0
        self._mint("genesis", initial_supply)

    def transfer(self, sender: str, recipient: str, amount: int) -> bool:
        self._require_nonzero(recipient)
        self._require_sufficient(sender, amount)
        self._transfer(sender, recipient, amount)
        return True

    def approve(self, owner: str, spender: str, amount: int) -> bool:
        self._require_nonzero(spender)
        self._approve(owner, spender, amount)
        return True

    def transfer_from(self, caller: str, sender: str, recipient: str, amount: int) -> bool:
        self._require_nonzero(recipient)
        self._require_sufficient(sender, amount)
        current = self._allowances.get(sender, {}).get(caller, 0)
        if current < amount:
            raise ValueError("insufficient allowance")
        self._approve(sender, caller, current - amount)
        self._transfer(sender, recipient, amount)
        return True

    def balance_of(self, account: str) -> int:
        return self._balances.get(account, 0)

    def total_supply(self) -> int:
        return self._total_supply

    # ── Internal helpers ──────────────────────────────────────────────────────

    def _transfer(self, sender: str, recipient: str, amount: int) -> None:
        self._balances[sender] = self.balance_of(sender) - amount
        self._balances[recipient] = self.balance_of(recipient) + amount

    def _mint(self, account: str, amount: int) -> None:
        self._total_supply += amount
        self._balances[account] = self.balance_of(account) + amount

    def _burn(self, account: str, amount: int) -> None:
        self._require_sufficient(account, amount)
        self._total_supply -= amount
        self._balances[account] = self.balance_of(account) - amount

    def _approve(self, owner: str, spender: str, amount: int) -> None:
        if owner not in self._allowances:
            self._allowances[owner] = {}
        self._allowances[owner][spender] = amount

    def _require_nonzero(self, addr: str) -> None:
        if not addr:
            raise ValueError("zero address")

    def _require_sufficient(self, addr: str, amount: int) -> None:
        if self.balance_of(addr) < amount:
            raise ValueError("insufficient balance")


def load_ledger(path: str) -> TokenLedger:
    data = _read_json(path)
    ledger = TokenLedger(data["name"], data["symbol"], data["supply"])
    return ledger


def save_ledger(ledger: TokenLedger, path: str) -> None:
    data = {
        "name":     ledger.name,
        "symbol":   ledger.symbol,
        "supply":   ledger.total_supply(),
        "balances": ledger._balances,
    }
    _write_json(path, data)


def _read_json(path: str) -> dict:
    with open(path) as f:
        return json.load(f)


def _write_json(path: str, data: dict) -> None:
    with open(path, "w") as f:
        json.dump(data, f, indent=2)
