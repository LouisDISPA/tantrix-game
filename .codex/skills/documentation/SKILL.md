---
name: documentation
description: How to maintain CLAUDE.md, rules, and skills. Use when creating or updating project documentation, rules, or skills.
allowed-tools: Read, Grep, Glob
---

# Documentation — Maintaining Rules & Skills

## Where Things Go

| Content Type | Location | Why |
|---|---|---|
| Project overview, domain model, glossary, integrations, commands, git workflow | `CLAUDE.md` | Always loaded — gives Claude essential project context |
| Critical rules (hard violations, "stop immediately") | `CLAUDE.md` | Must be in context at all times |
| Available skills table + invocation rules | `CLAUDE.md` | Claude must know skills exist to invoke them |
| Concise architectural rules (directory structure, layers) | `.claude/rules/architecture.md` | Always loaded, supplements CLAUDE.md |
| Concise code style rules (naming, types, imports) | `.claude/rules/code-style.md` | Always loaded, supplements CLAUDE.md |
| Concise code patterns (server components, actions, dialogs) | `.claude/rules/patterns.md` | Always loaded, supplements CLAUDE.md |
| Detailed how-to with code examples and checklists | `.claude/skills/{name}/SKILL.md` | Loaded on-demand — keeps context budget low |

**Key principle:** `CLAUDE.md` and `rules/` are **always loaded** — keep them concise (rules, not tutorials). Skills are **loaded on-demand** — they can be detailed with full examples and checklists.

## Decision Tree

| Question | Answer |
|----------|--------|
| Is it a hard rule that must always be in context? | `CLAUDE.md` or `rules/` |
| Is it a detailed pattern with code examples? | `skills/{name}/SKILL.md` |
| Does it apply to a specific feature workflow? | `skills/{name}/SKILL.md` |
| Is it project metadata (domain, glossary, integrations)? | `CLAUDE.md` |
| Is it a structural convention (directories, layers)? | `rules/architecture.md` |
| Is it a style convention (naming, types, imports)? | `rules/code-style.md` |
| Is it a code pattern (components, actions, hooks)? | `rules/patterns.md` |

## Rules Files Map

| File | Scope |
|------|-------|
| `architecture.md` | Where code goes — directories, layers, decision tree |
| `code-style.md` | How code looks — naming, formatting, types, imports |
| `patterns.md` | How to write code — component patterns, server actions, dialogs |

## When Updating Rules

1. **Update over append** — search existing content first; replace or refine the relevant section instead of adding a duplicate
2. **Keep it generic** — abstract from the specific mistake to a reusable rule; never reference the incident that prompted the change
3. **Keep it concise** — 1-2 lines per rule, add examples only when the convention is non-obvious
4. **Don't teach what AI knows** — skip standard language/framework conventions (e.g. "use const", "prefer async/await")
5. **Right file** — use the decision tree above to place the rule; if it doesn't fit any file, ask the user before creating a new one

## When Creating a New Skill

### Structure

```
.claude/skills/{skill-name}/SKILL.md
```

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

1. Add to the **Available Skills** table in `CLAUDE.md`
2. Add a trigger line to the **"When to invoke skills"** section in `CLAUDE.md`
3. If there's a "Stop Immediately" anti-pattern, add it to `CLAUDE.md`
4. If there's a brief rule for `rules/patterns.md`, add it there (1-3 lines, no full examples)

## Skill vs Rule — Size Guide

| Type | Target size | Detail level |
|------|-------------|--------------|
| `CLAUDE.md` entry | 1-2 lines | Name + when to use |
| `rules/*.md` entry | 1-5 lines | Rule + minimal example if needed |
| `skills/*/SKILL.md` | 100-300 lines | Full patterns, examples, checklists |
