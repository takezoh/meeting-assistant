# Phase 1 signal-timeline fixtures

The five Phase 1 timelines are deterministic, redacted seed fixtures. They were authored on the
portable development host to exercise the committed JSONL, label-sidecar and decision-sidecar
contracts; they are not evidence that the Windows collectors observed the four target
applications.

The Windows-tier procedure `v-win1-process-identity-live-probe` must replace these seeds with
redacted output captured by `ma-diag record` for the three desktop scenarios and Meet with and
without the extension. The record retains the real-to-synthetic mapping outside this fixture
directory and confirms that `ma-diag replay --synthetic-tables` reproduces every committed
decision sidecar. Until that manual record passes, the repository treats real-fixture provenance
as deferred Windows-tier evidence.
