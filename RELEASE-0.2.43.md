# KAY POS 0.2.43

- Fix checkout defaults overwriting the received amount after selecting Credit.
- Apply checkout payment defaults once after loading, outside the resource reactive scope.
- Preserve zero received and partial payments across subsequent checkout refreshes.
- Verify unpaid credit checkout appears in customer balances and outstanding invoices.

Validated with checkout received-amount regression tests, credit collection/refund/retry tests, core/standalone tests and desktop tests.
