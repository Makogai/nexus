---
nexus: 1
id: {{id}}
kind: rule
format: constitution
title: PHP
status: active
applies: on-demand
prefix: PHP
stacks: [php, laravel]
created: {{now}}
updated: {{now}}
source: import
---

Rules for modern PHP (8.3 and later), independent of framework. Apply to all PHP code; LRV adds Laravel specifics.

## Language

Modern PHP is a strictly typed language when you let it be; these rules turn that on.

#### Declare strict types in every file
<!-- rule: id=PHP-1 enforcement=rule check=lint -->
Start every PHP file with declare(strict_types=1); so scalar arguments and return values are not silently coerced
(the string "5 apples" passed as int). Enforce it with the fixer's declare_strict_types rule.

#### Type every property, parameter and return value
<!-- rule: id=PHP-2 enforcement=rule -->
Declare native types for all properties, parameters, return values and class constants, using union, nullable, void
and never types; add PHPDoc generics such as `array<int, User>` only where native types cannot express the shape.

#### Use modern language features
<!-- rule: id=PHP-3 enforcement=rule -->
Use readonly properties or readonly classes for value objects, backed enums instead of constant lists, match instead
of switch for value mapping, constructor property promotion, named arguments for unclear booleans, and `#[\Override]`
on overriding methods.

#### Throw exceptions instead of returning error codes
<!-- rule: id=PHP-4 enforcement=rule -->
Signal failures with specific exception classes (extending a project base exception) instead of returning false, null
or error codes; catch only where you can handle or translate the error, and never swallow it in an empty catch.

#### Never suppress errors with @
<!-- rule: id=PHP-5 enforcement=rule -->
Do not use the @ error-control operator; check the condition first or convert the warning into an exception, so
failures stay visible in logs and to static analysis.

#### Never use eval or variable variables
<!-- rule: id=PHP-6 enforcement=rule -->
Do not use eval(), variable variables ($$name), extract() on input, or include/require paths built from input. They
defeat static analysis and can turn data into code; use maps, match or polymorphism instead.

## Data access

Database access is where untyped strings most often become vulnerabilities.

#### Use PDO prepared statements
<!-- rule: id=PHP-7 enforcement=rule -->
Run SQL through prepared statements with bound parameters, with PDO::ATTR_ERRMODE set to ERRMODE_EXCEPTION and
PDO::ATTR_EMULATE_PREPARES false where the driver supports native prepares; never concatenate values into SQL (SEC-4).

## Tooling

Composer, a fixer and a static analyser catch most PHP defects before runtime.

#### Commit composer.lock and install from it
<!-- rule: id=PHP-8 enforcement=rule -->
Commit composer.lock for applications and run composer install (never composer update) in CI and deployments, with
--no-dev --optimize-autoloader for production, and run composer audit in CI.

#### Enforce the coding style with a fixer
<!-- rule: id=PHP-9 enforcement=rule check=lint -->
Format code to PER Coding Style (the successor of PSR-12) with PHP-CS-Fixer or Pint and check it in CI with --dry-run
or --test; do not hand-format code or debate style in review.

#### Run static analysis at a high level
<!-- rule: id=PHP-10 enforcement=rule check=lint -->
Run PHPStan at level 8 or higher, or Psalm at errorLevel 2 or stricter, in CI on all code. Keep a baseline only for
legacy errors and never add new entries to it; static analysis catches null and type errors PHP reports only at
runtime.
