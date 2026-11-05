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
Keep metadata, dependencies and tool configuration in pyproject.toml, set requires-python to versions that are still
supported, and test in CI on the lowest and highest version you claim. Do not add setup.py or scattered tool config
files to new projects.

#### Use isolated environments and a lock file
<!-- rule: id=PY-2 enforcement=rule -->
Install into a per-project virtual environment managed by uv or Poetry, commit the lock file (uv.lock or poetry.lock)
and install from it in CI and images, e.g. uv sync --locked. Never pip install into the system interpreter.

## Code

Type hints, a fast linter and explicit resource handling catch most Python defects before runtime.

#### Type-hint code and check it
<!-- rule: id=PY-3 enforcement=rule check=typecheck -->
Annotate every function signature and public attribute and run mypy --strict or pyright in strict mode in CI. Use
modern syntax (list[int], X | None) and allow Any only at untyped third-party boundaries.

#### Lint and format with ruff
<!-- rule: id=PY-4 enforcement=rule check=lint -->
Run ruff check with at least the pyflakes (F), pycodestyle (E), isort (I), bugbear (B) and pyupgrade (UP) rule sets,
plus ruff format, configured in pyproject.toml and enforced in CI; do not mix in other formatters.

#### Model structured data with dataclasses or pydantic
<!-- rule: id=PY-5 enforcement=rule -->
Use dataclasses (frozen=True and slots=True where suitable) for internal records and pydantic models for validated
external input and settings; do not pass loosely shaped dicts between functions.

#### Never use mutable default arguments
<!-- rule: id=PY-6 enforcement=rule check=lint -->
Do not use lists, dicts or sets as default argument values; they are created once and shared across calls. Default to
None and create the object in the body, or use field(default_factory=list) in dataclasses. Ruff's B006 reports it.

#### Manage resources with context managers
<!-- rule: id=PY-7 enforcement=rule -->
Open files, locks, connections, transactions and temporary directories with with or async with (or contextlib
helpers) so they are released on every path, including exceptions; never rely on garbage collection to close them.

#### Log with the logging module, not print
<!-- rule: id=PY-8 enforcement=rule -->
Use a module-level logger = logging.getLogger(__name__) and pass values as arguments, e.g. logger.info("saved %s",
order_id), so formatting is lazy; configure handlers once at the entry point. Do not use print() for diagnostics.

#### Never build SQL with f-strings
<!-- rule: id=PY-9 enforcement=rule -->
Pass values as query parameters (cursor.execute("... WHERE id = %s", (order_id,))), ORM filters or SQLAlchemy bound
parameters; never format SQL with f-strings, % or .format(). Allowlist dynamic identifiers separately (SEC-4).

#### Do not block the event loop in async code
<!-- rule: id=PY-10 enforcement=rule -->
Inside async def call only non-blocking libraries; run blocking I/O or CPU-bound work with asyncio.to_thread or in a
plain def endpoint, which FastAPI runs in a thread pool. A blocking call in async code stalls every request on that
worker.

## Testing

pytest fixtures keep tests short, isolated and independent of real services.

#### Test with pytest and fixtures
<!-- rule: id=PY-11 enforcement=rule check=test -->
Write tests with pytest, using fixtures (tmp_path, monkeypatch, factory fixtures) instead of setUp hierarchies and
pytest.mark.parametrize for case tables; keep tests independent of execution order and of real external services.
