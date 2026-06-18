# Python

> Language guidelines for python packages. Soft recommendations
> unless gated by a policy in `policies.md` or a hard rule in
> `pyproject.toml` / `ruff.toml`. Compare with the per-package
> `CLAUDE.md` for package-specific conventions.

## Lint gate (binding)

`make lint` hard-fails on any ruff warning over the package
tree. Concretely:

    ruff check .
    ruff format --check .

If the project uses static types, `mypy` joins the gate:

    mypy src/

Waiver process:

- Repo-wide config in `pyproject.toml` under `[tool.ruff]` (or
  `ruff.toml` for standalone), with `select` / `ignore` lists.
  Each ignore carries a one-line reason comment.
- Inline waivers on a specific line:
  `# noqa: <rule>  # <reason>`. The reason is **mandatory**.
- Type-check escapes: `# type: ignore[<code>]  # <reason>`.

Do not introduce new ruff or mypy warnings. If a code change
would create one, either fix the underlying issue or add an
explicit waiver with reason; the lint target rejects
unreasoned disables.

## Build / test invocation

These are the canonical mappings from the generic `make`
targets in `.repo/project/skills/testing.md` to python tools:

    make check-unit  → pytest tests/unit -q
    make check-sit   → pytest tests/sit -q
    make compile     → python -m build      # or: pip install -e .
    make lint        → ruff check . && ruff format --check .
    make install     → pip install .        # or: pipx install .

`make check-pit` maps to `pytest tests/pit -q` when the
package opts into PIT.

## Project layout

Canonical src-layout (preferred over flat-layout — keeps
imports honest):

    pyproject.toml       # PEP 621 metadata; ruff/mypy/pytest config
    src/
        <pkg>/
            __init__.py
            <module>.py
    tests/
        unit/
            test_<module>.py
        sit/
            test_<scenario>.py
        conftest.py      # shared fixtures
    .python-version      # pin via pyenv / asdf

Build backend declared in `pyproject.toml` (`setuptools`,
`hatchling`, `poetry-core` — pick one and stick with it).
Lock files (`requirements.txt`, `poetry.lock`, `uv.lock`)
are checked in for applications, not for libraries.

## Idiom recommendations

- Type-annotate public APIs (function signatures, dataclass
  fields, module-level constants). Internal helpers may skip
  if the inference is obvious.
- `pathlib.Path` over string paths; `os.path` only when
  interop demands it.
- `pytest` over `unittest`. Plain `assert` reads better than
  `self.assertEqual`.
- `with` blocks for any resource (files, locks, sockets,
  subprocesses); never rely on `__del__`.
- `dataclasses` (or `pydantic` if validation is needed) over
  bare `dict` for structured data.
- f-strings over `%`-formatting and `.format()`.
- Catch specific exception types, not bare `except:`.
- Module-level `logging.getLogger(__name__)` over `print` for
  diagnostics; print is reserved for the command's stdout
  payload (mirrors the bash logging contract in
  `.repo/project/skills/logging.md`).

## Testing

`pytest` is the framework. Conventions:

- Files: `tests/unit/test_<module>.py`, mirroring `src/<pkg>/`.
- Functions: `def test_<behaviour>():` — no class wrapping
  unless the test needs shared per-test state.
- Fixtures: `conftest.py` at the appropriate level
  (`tests/conftest.py` for project-wide, `tests/unit/conftest.py`
  for unit-only). Mark expensive fixtures with
  `scope="session"`.
- Parametrise with `@pytest.mark.parametrize`; one assertion
  shape, many cases.
- `tmp_path` and `monkeypatch` are the built-in fixtures of
  first resort.

The cross-walk to `.repo/project/skills/testing.md`:
`tests/unit/` is the unit layer, `tests/sit/` is SIT (podman
fixtures live alongside in `tests/sit/podman/`), `tests/pit/`
is PIT.

## Reading

- *PEP 8* (style) — <https://peps.python.org/pep-0008/>
- *PEP 484* (type hints) — <https://peps.python.org/pep-0484/>
- *Ruff rules* — <https://docs.astral.sh/ruff/rules/>
- *pytest docs* — <https://docs.pytest.org/>
- Per-package `CLAUDE.md` and `pyproject.toml`'s `[tool.ruff]`
  section (the explicit known-debt list and idiom waivers,
  with reasons)
