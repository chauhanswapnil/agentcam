# Security

agentcam reads agent session logs, which often contain secrets. Security bugs matter here.

## Reporting

Please don't open a public issue. Use [GitHub private vulnerability reporting](https://github.com/chauhanswapnil/agentcam/security/advisories/new) instead.

Include what you found, how to reproduce it, and what an attacker could do with it. You'll get a reply within a week.

## In scope

- The server listening on anything other than `127.0.0.1`, or being reachable by other sites in your browser (CSRF, DNS rebinding, permissive CORS).
- Another site changing agentcam's guard rules (cross-site writes to the local API).
- `agentcam guard` blocking calls it shouldn't, or failing closed (stopping an agent) on bad input.
- agentcam writing to, moving or deleting agent log files.
- agentcam sending any data off the machine.
- Secrets from logs ending up somewhere new (its own logs, crash reports, exports) without the user asking.
