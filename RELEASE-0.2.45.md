# KAY POS 0.2.45

- Fix Invalid field/column order when enabling customer name or customer address on an existing receipt layout.
- Accept legacy receipt orders that predate the optional customer fields while retaining validation for duplicate, unknown or missing existing fields and invalid item columns.
- Save the completed receipt field order without resetting the existing field sequence or styling.

Validated with legacy-order upgrade and invalid-order regression tests, receipt layout tests and the desktop test suite.
