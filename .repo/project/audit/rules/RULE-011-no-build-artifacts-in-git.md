# RULE-011 — No build artifacts or runtime state committed to git

scope: full
severity: block

## Rule

The git repository must contain only source files, configuration, and
documentation. Build artifacts, generated files, editor state, IDE
configuration, runtime logs, and secrets must never be committed.

## Categories and patterns

### Build artifacts (always block)

| Pattern | Source |
|---|---|
| `target/`, `debug/` | Cargo build output |
| `*.o`, `*.a`, `*.so`, `*.dylib`, `*.exe` | C/C++ or linker output |
| `Makefile`, `Makefile.in` | Autotools generated (not hand-written) |
| `aclocal.m4`, `autom4te.cache/` | Autotools generated |
| `configure`, `config.log`, `config.status` | Autotools generated |
| `config.h`, `config.h.in`, `stamp-h1` | Autotools generated |
| `install-sh`, `missing`, `depcomp`, `compile` | Autotools helpers |
| `*.rs.bk` | rustfmt backups |
| `*.pdb` | MSVC debug info |
| `__pycache__/`, `*.py[cod]`, `.pytest_cache/` | Python runtime |
| `node_modules/` | Node.js dependencies |

### Runtime state (always block)

| Pattern | Source |
|---|---|
| `.repo/dvalin/logs/` | dvalin dwarf session logs |
| `.repo/dvalin/workshop.log` | dvalin workshop duration log |
| `.repo/dvalin/*.jsonl`, `.repo/dvalin/*.log` | dvalin runtime state |
| `*.sessions.jsonl` outside `issues/` | Effort tracking logs |

### Editor and IDE artifacts (always block)

| Pattern | Source |
|---|---|
| `.idea/` | JetBrains IDEs |
| `.vscode/` | VS Code workspace settings |
| `*.swp`, `*.swo`, `*~` | Vim/Emacs temporaries |
| `.DS_Store`, `Thumbs.db` | macOS / Windows OS metadata |
| `.claude/settings.local.json` | Claude Code local overrides |

### Secrets and environment (always block)

| Pattern | Source |
|---|---|
| `.env`, `.env.*` (except `.env.example`) | Environment variable files |
| `*.key`, `*.pem`, `*.p12`, `*.pfx` | Private keys / certificates |
| `*_rsa`, `*_ed25519`, `id_*` | SSH private keys |
| `credentials.json`, `service-account*.json` | Cloud credentials |

## Pass criteria

- `.gitignore` covers all pattern categories above.
- `git ls-files` returns no matches for any blocked pattern.
- No file in the repository contains a plaintext API key or token
  (scan for `sk-`, `ghp_`, `ANTHROPIC_API_KEY=`, `AWS_SECRET_ACCESS_KEY=`).

## Fail criteria

- Any blocked file or directory appears in `git ls-files` output.
- `.gitignore` is absent or missing a category.
- A `.env` file with real values (not example placeholders) is tracked.
- Any private key file is tracked.

## Audit instruction

1. Run `git ls-files` and grep for each blocked pattern. Any match is a
   violation.
2. Check that `.gitignore` exists and contains entries for every category
   in the table above.
3. Scan tracked files for secret patterns:
   `git grep -l 'sk-\|ghp_\|ANTHROPIC_API_KEY=.*[a-zA-Z0-9]\|AWS_SECRET'`
   Any match with a real value (not a placeholder or test fixture) is a
   block violation.
4. If violations are found: remove the file, add the pattern to `.gitignore`,
   and if a secret was committed — rotate the secret immediately before
   anything else.

## Remediation

```bash
# Remove a tracked artifact without deleting the file
git rm --cached <file-or-dir>

# Add pattern and commit
echo '<pattern>' >> .gitignore
git add .gitignore
git commit -m "chore: exclude <pattern> from git tracking"
```

If a secret was committed, assume it is compromised regardless of how
recently it was pushed. Rotate first, then remove from history.
