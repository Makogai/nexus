---
nexus: 1
id: {{id}}
kind: rule
format: constitution
title: Python
status: active
applies: on-demand
prefix: PY
stacks: [python, django, fastapi, flask]
created: {{now}}
updated: {{now}}
source: import
---

Rules for Python applications and libraries, including Django, FastAPI and Flask. Apply to Python code; they refine
CQ, TST and BE.

## Project setup

Declared versions, isolated environments and locked dependencies make a Python project reproducible.

#### Declare the Python version and metadata in pyproject.toml
<!-- rule: id=PY-1 enforcement=rule -->
Keep metadata, dependencies and tool configuration in pyproject.toml with requires-python set to supported versions;
libraries test the lowest and highest version they claim in CI. Do not add setup.py or separate tool config files to
new projects.

#### Use isolated environments and a lock file
<!-- rule: id=PY-2 enforcement=rule -->
Install into a per-project virtual environment with the project's manager (uv for new projects, or Poetry), commit its
lock file and install from it in CI and images, e.g. uv sync --locked. Never pip install into the system interpreter.

## Code

Type hints, a fast linter and explicit resource handling catch most Python defects before runtime.

#### Type-hint code and check it
<!-- rule: id=PY-3 enforcement=rule check=typecheck -->
Annotate every function signature and public attribute with modern syntax (list[int], X | None) and run mypy or
pyright in CI at the project's strictness, strict mode for new projects. Allow Any only at untyped third-party
boundaries.

#### Lint and format with ruff
<!-- rule: id=PY-4 enforcement=rule check=lint -->
Use ruff check and ruff format configured in pyproject.toml and enforced in CI, with at least the F, E, I, B and UP
rule sets in new projects (B also catches mutable default arguments). Keep a project's existing linter and formatter
if it uses others; do not run two formatters.

#### Model structured data with dataclasses or pydantic
<!-- rule: id=PY-5 enforcement=rule -->
Use dataclasses (frozen=True and slots=True where suitable) for internal records and the project's validation library
(pydantic for new code) for external input and settings; do not pass loosely shaped dicts between functions.

#### Manage resources with context managers
<!-- rule: id=PY-6 enforcement=rule -->
Open files, locks, connections, transactions and temporary directories with with or async with (or contextlib
helpers) so they are released on every path, including exceptions; never rely on garbage collection to close them.

#### Log with the logging module, not print
<!-- rule: id=PY-7 enforcement=rule -->
Use a module-level logger = logging.getLogger(__name__) and pass values as arguments, e.g. logger.info("saved %s",
order_id), so formatting is lazy; configure handlers once at the entry point. Do not use print() for diagnostics.

#### Never build SQL with f-strings
<!-- rule: id=PY-8 enforcement=rule -->
Pass values as driver parameters (cursor.execute("... WHERE id = %s", (order_id,))), ORM filters or SQLAlchemy bound
parameters, also inside text(); never format SQL with f-strings, % or .format(). Allowlist dynamic identifiers
separately (SEC-4).

#### Do not block the event loop in async code
<!-- rule: id=PY-9 enforcement=rule -->
Inside async def call only non-blocking libraries; run blocking I/O or CPU-bound work with asyncio.to_thread or in a
plain def endpoint, which FastAPI runs in a thread pool. A blocking call in async code stalls every request on that
worker.

## Testing

pytest fixtures keep tests short, isolated and independent of real services.

#### Test with pytest and fixtures
<!-- rule: id=PY-10 enforcement=rule check=test -->
Write tests with pytest, using fixtures (tmp_path, monkeypatch, factory fixtures) instead of setUp hierarchies and
pytest.mark.parametrize for case tables; keep tests independent of execution order and of real external services.
