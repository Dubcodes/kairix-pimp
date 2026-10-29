# Codex task template

Copy this template for either a focused fix or a larger feature. Remove irrelevant acceptance items; do not expand the architecture without evidence.

```text
# Kairix PIMP — <task title>

Repository: J:\projects\kairix-pimp

## Required preparation

Work only in the existing repository. Do not use `git worktree add`, create an external worktree/working copy, modify another Kairix project, force-push, or rewrite published history.

Read AGENTS.md, README.md, and the relevant files in docs/, especially PROJECT_BRIEF.md, ARCHITECTURE.md, DOMAIN_RULES.md, DECISIONS.md, and TESTING.md. Inspect current code/tests and verify Git branch, status, remotes, and worktrees before editing. Preserve unrelated user changes.

## Objective

<Describe the user outcome and why it matters.>

## Scope

In scope:
- <item>

Out of scope:
- <item>

## Acceptance criteria

- <observable criterion>
- <failure/safety criterion>
- <accessibility or platform criterion where relevant>

## Boundaries and rules

Affected domain rules: <DR IDs and how they are preserved>.
Affected architecture/data flows: <components and adapters>.
Security/destructive-operation review: <risks and controls>.

For an external service or third-party client, identify the documented supported interface or written authorization, authentication/credential-storage boundary, independent verification method, Windows/runtime support, dependency and license compatibility, retry/interruption semantics, and prohibited private/undocumented mechanisms. Upload acknowledgement, listings and browser-driven transfer are not backup verification.

Use the smallest maintainable change. Keep portable logic out of UI/OS adapters, preserve stable IDs and relative paths, and do not redesign unrelated areas. If a requirement conflicts with a domain rule or accepted decision, stop and document the decision before implementing.

## Implementation and verification

Add or update targeted tests, including failure/regression behavior. Run:

npm test
npm run lint
npm run format
npm run build
npm audit --audit-level=high

Run native/external checks only when their prerequisites are genuinely available. Never claim browser tests prove Tauri, camera, Syncthing, Premiere, or archive behavior.

For remote storage, do not claim verification until PIMP independently reads every stored object through the supported provider interface and compares it with the immutable manifest. Record blocked or manual-only exercises as inconclusive rather than passed.

Update the relevant README/docs/roadmap/decision log alongside substantive changes. Inspect the final diff and repository-local worktree state. Commit and push through the existing safe workflow only when the task explicitly requests it.

## Final report

Report the outcome, files and documentation changed, rules preserved, exact checks and results, security/regression review, screenshots where useful, limitations, Git/CI status, and a focused next milestone. Clearly label implemented, tested, scaffolded, and planned behavior; do not make unverified completion claims.
```
