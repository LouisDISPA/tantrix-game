---
name: documentation
description: How to maintain AGENTS.md, domain docs and skills. Use when creating or updating project documentation, rules, or skills.
allowed-tools: Read, Grep, Glob
---

# Documentation — Maintaining Rules & Skills

One tracked source drives both assistants. Codex reads `AGENTS.md` and
`.codex/skills/`; Claude reads `CLAUDE.md` (a single `@AGENTS.md` import) and
`.claude/skills/` (a symlink to `.codex/skills/`). Edit only the shared files;
never add a Claude-only copy or a `.claude/rules/` folder.

## Where Things Go

| Content Type | Location | Why |
|---|---|---|
| Project overview, workspace/app maps, commands, domain model | `AGENTS.md` | Always loaded by both assistants |
| Required conventions (hard rules, one line each) | `AGENTS.md` → **Required conventions** | Must be in context at all times |
| Skills table + triggers | `AGENTS.md` → **Skills** | Assistants must know skills exist to read them |
| Component tiers, boundaries, render verification | `doc/components.md` | Linked from `AGENTS.md`; read for UI work |
| Reactive ownership, store/signal rules, turn flow | `doc/signals.md` | Linked from `AGENTS.md` |
| Engine, AI, multiplayer, styling, releases | `doc/*.md` | Linked from `AGENTS.md` → **Domain docs** |
| Detailed how-to with code examples and checklists | `.codex/skills/{name}/SKILL.md` | Loaded on demand — keeps context budget low |

**Key principle:** `AGENTS.md` is **always loaded** — keep it concise (rules,
not tutorials). Domain docs describe how this codebase works. Skills are
**loaded on demand** and can hold full examples and checklists.

## Decision Tree

| Question | Answer |
|----------|--------|
| Is it a hard rule that must always be in context? | `AGENTS.md` → **Required conventions** |
| Is it a structural convention (tiers, folders, imports)? | `doc/components.md` |
| Is it a state or reactivity pattern? | `doc/signals.md` |
| Is it a detailed pattern with code examples? | `.codex/skills/{name}/SKILL.md` |
| Is it project metadata (crates, routes, commands)? | `AGENTS.md` |
| Can a tool check it? | Add the check too (see below) |

## Mechanical Checks

Prefer a check over prose when a rule can be detected:

| Rule | Check |
|------|-------|
| Randomness is seeded | `clippy.toml` → `disallowed-methods` (`rand::rng`, `rand::random*`); intentional sites use `#[expect(clippy::disallowed_methods, reason = "…")]` |
| `components/` never touches session state | `tantrix-app/src/components/tier_tests.rs` |
| `pub` only for other crates; library crates documented | root `Cargo.toml` → `[workspace.lints]` (`unreachable_pub`); `#![warn(missing_docs)]` in game, proto, room |

Name the check in the rule's line so a reader knows it is enforced.

## When Updating Rules

1. **Update over append** — search existing content first; replace or refine the relevant section instead of adding a duplicate
2. **Match the code** — check the rule against the code before writing it; a rule the code breaks is worse than none
3. **Keep it generic** — abstract from the specific mistake to a reusable rule; never reference the incident that prompted the change
4. **Keep it concise** — 1-2 lines per rule, add examples only when the convention is non-obvious
5. **Don't teach what AI knows** — skip standard language/framework conventions (e.g. "use const", "prefer async/await")
6. **Right file** — use the decision tree above; if it doesn't fit any file, ask the user before creating a new one
7. **Relative links** — link from the file's own folder (`doc/…` from `AGENTS.md`, `../../../doc/…` from a skill)

## When Creating a New Skill

### Structure

```
.codex/skills/{skill-name}/SKILL.md
```

`.claude/skills` is a symlink to `.codex/skills`, so the new skill reaches both assistants.

### Required Frontmatter

```yaml
---
name: {skill-name}
description: {one-line description}. Use when {trigger condition}.
allowed-tools: Read, Grep, Glob
---
```

### Recommended Sections

1. **Quick Reference** — decision table for common cases
2. **Patterns** — code examples with explanations
3. **Common Mistakes** — table of mistake → fix
4. **Checklist** — verification steps
5. **File Locations** — where to put things

### After Creating a Skill

1. Add a row to the **Skills** table in `AGENTS.md` with its trigger
2. If there's a hard rule, add one line to **Required conventions** in `AGENTS.md`
3. If there's a structural or state rule, add 1-3 lines to `doc/components.md` or `doc/signals.md`

## Skill vs Rule — Size Guide

| Type | Target size | Detail level |
|------|-------------|--------------|
| `AGENTS.md` entry | 1-2 lines | Name + when to use, or the rule |
| `doc/*.md` rule | 1-5 lines | Rule + minimal example if needed |
| `skills/*/SKILL.md` | 100-300 lines | Full patterns, examples, checklists |
