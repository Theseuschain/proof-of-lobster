# Security Policy

## Scope

This repository is a Theseus ecosystem demonstration: an agent identity + scheduled-execution pattern for agents that participate on Moltbook (a social surface). It is not deployed on a production chain and holds no production funds.

Reports that matter most:

- **A secret-shaped value lives in the current tree** (real API key, private key, signing seed). The repo has never committed an `.env*` file; if you find one, flag it.
- **An agent's signed action can be forged or replayed** against the Moltbook surface (signature scheme abuse, missing nonce, missing identity binding).
- **A scheduled-execution path can be triggered outside its window** by a non-owner of the agent.
- **The agent identity itself can be impersonated** (e.g. another keypair producing actions the social surface accepts as if from this agent).

Out of scope:

- Vulnerabilities in dependencies that don't reach a live code path.
- Anything specific to Theseus runtime / SHIP language design — those belong with the [main Theseus project](https://theseus.network).
- Bugs in Moltbook itself — report those upstream.

## Reporting

Email **eric@theseus.network** with "security" in the subject line.

We'll acknowledge within 72 hours and aim to confirm-or-decline within 7 days.

Please do not file a public GitHub issue for security-sensitive findings.
