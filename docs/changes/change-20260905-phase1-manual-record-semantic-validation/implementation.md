---
change: change-20260905-phase1-manual-record-semantic-validation
role: implementation
---

<!-- lifecycle is owned by change.md -->

# Implementation

## Responsibility boundary

Keep procedure-specific meaning in `validate_semantics`. Add small predicates for
concrete string arrays, all-true boolean arrays, non-empty string maps, and closed
string-map domains. Derive the required application key set from the process
identity classification maps when validating loopback maps; do not duplicate
adapter identifiers in Rust.

Increment the typed-observation semantics version included in the procedure digest
because records accepted under the weaker validator are no longer current.
