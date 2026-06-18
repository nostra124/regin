---
id: FEAT-007
type: feature
priority: medium
complexity: M
estimate_tokens: 40k-80k
estimate_time: 45-90min
phase: done
status: done
---

# Skill (task) creation flow

## Description
**As an** operator
**I want** a first-class way to create a new skill from the CLI, optionally
agent-assisted
**So that** I don't have to hand-author `skill.md` files and directory layout by
hand.

Today skills are markdown files an operator must place manually in
`~/.config/regin/skills/<name>/skill.md`. There is no creation verb.

## Implementation
- Add a `task create <name>` verb (sibling of `list/show/exec`):
  - `--from-prompt "<goal>"` — agent-assisted: regind asks the LLM to draft a
    `skill.md` (first line = description, then instructions) for the stated goal,
    writes it to the user skills dir, and shows the result.
  - Without `--from-prompt` — scaffold a template `skill.md` + dir to edit.
  - `--edit` — open `$EDITOR` on the new `skill.md`.
- Refuse to overwrite an existing user skill unless `--force`; surface that a
  user skill shadows a system skill of the same name.
- Reuse `config::user_skills_dir()` and the existing skills resolver so the new
  skill is immediately visible to `task list/show/exec`.

## Acceptance Criteria
1. `task create disk-trend` scaffolds `~/.config/regin/skills/disk-trend/skill.md`
   and it shows up in `task list` (source: user).
2. `task create … --from-prompt "alert when /var over 80%"` produces a coherent
   `skill.md` whose first line is a description.
3. Creating over an existing user skill is refused without `--force`.
4. Created skills run via `task exec` with no extra steps.
5. Unit test covers scaffold path + overwrite guard (LLM path exercised manually).
