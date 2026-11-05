# Fixtures

Synthetic agent session logs used by tests.

Rules:

- **Never** copy real session logs here, not even trimmed or "scrubbed". Write records by hand to match the shape you need.
- Use fake paths (`/Users/dev/projects/demo`) and obviously fake secrets (`sk-test-000000000000`).
- One folder per agent: `claude-code/`, `codex/`. Name files after the case they cover, e.g. `tool-call-with-error.jsonl`.
