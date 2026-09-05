---
change: change-20260905-phase1-manual-record-semantic-validation
role: requirements
---

<!-- lifecycle is owned by change.md -->

# Requirements

## Validation requirements

- MRV-001: Application and capture identifier arrays must contain concrete,
  non-empty strings; their required count cannot be satisfied by nulls or empty
  strings.
- MRV-002: Every recorded fixture replay result must be boolean `true`.
- MRV-003: The redaction mapping must contain only non-empty source and redacted
  identifiers.
- MRV-004: Every loopback activation map must cover the same target applications,
  and every value must belong to the procedure's closed domain.
- MRV-005: Microphone endpoint selection history must contain at least two
  concrete, distinct endpoint identifiers.
- MRV-006: Tightening typed semantics invalidates records made against the older
  validator digest.

## Exclusions

This change does not collect Windows evidence, add target applications, or change
the manual procedure and pass-criterion text.
