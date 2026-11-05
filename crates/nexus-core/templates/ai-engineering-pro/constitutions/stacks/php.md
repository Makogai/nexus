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

Rules for modern PHP (8.3 and later; 8.4 features where the project's minimum version allows), independent of
framework. Apply to all PHP code; LRV adds Laravel specifics.

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
Use readonly properties or classes for value objects, backed enums instead of constant lists, match instead of
switch for value mapping, constructor promotion and `#[\Override]`. On PHP 8.4 prefer asymmetric visibility
(public private(set)) and property hooks to boilerplate getters and setters.

#### Throw exceptions and never suppress errors
<!-- rule: id=PHP-4 enforcement=rule -->
Signal failures with specific exception classes instead of returning false, null or error codes, and catch only where
you can handle or translate the error. Never use the @ operator; check the condition first or convert the warning
into an exception, so failures stay visible.

#### Never use eval or variable variables
<!-- rule: id=PHP-5 enforcement=rule -->
Do not use eval(), variable variables ($$name), extract() on input, or include/require paths built from input. They
defeat static analysis and can turn data into code; use maps, match or polymorphism instead.

## Data access

Database access is where untyped strings most often become vulnerabilities.

#### Use PDO prepared statements with native prepares
<!-- rule: id=PHP-6 enforcement=rule -->
Run raw SQL through prepared statements with bound parameters (SEC-4), and set PDO::ATTR_EMULATE_PREPARES to false
where the driver supports native prepares, so values stay typed and separate from the SQL text. PDO already throws
exceptions by default since PHP 8.0; do not switch that off.

## Tooling

Composer, a fixer and a static analyser catch most PHP defects before runtime.

#### Install from composer.lock
<!-- rule: id=PHP-7 enforcement=rule -->
Commit composer.lock for applications and run composer install, never composer update, in CI and deployments, with
--no-dev --optimize-autoloader for production builds.

#### Enforce the coding style with a fixer
<!-- rule: id=PHP-8 enforcement=rule check=lint -->
Format code to PER Coding Style with the project's fixer (PHP-CS-Fixer, or Laravel Pint in Laravel projects) and
check it in CI with --dry-run or --test; never hand-format against it.

#### Run static analysis at a high level
<!-- rule: id=PHP-9 enforcement=rule check=lint -->
Run PHPStan (Larastan in Laravel projects) or Psalm in CI at the level the project sets, level 8 or higher for new
projects, and never lower it. Keep a baseline only for legacy errors and never add new entries; static analysis
catches null and type errors PHP reports only at runtime.
