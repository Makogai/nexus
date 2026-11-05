---
nexus: 1
id: {{id}}
kind: rule
format: constitution
title: Laravel
status: active
applies: on-demand
prefix: LRV
stacks: [laravel]
created: {{now}}
updated: {{now}}
source: import
---

Rules for Laravel applications (Laravel 11 and later conventions). Apply to Laravel code; they refine BE, DB and PHP,
which also covers Pint and Larastan.

## HTTP layer

Laravel has a dedicated place for validation, authorisation and response shaping; use it instead of controller code.

#### Validate with Form Requests
<!-- rule: id=LRV-1 enforcement=rule -->
Validate input in a Form Request class (rules() and authorize()) rather than in controllers, and read only validated
data with $request->validated() or safe(). Never pass $request->all() to models or services.

#### Authorise with Policies and Gates
<!-- rule: id=LRV-2 enforcement=rule -->
Put authorisation for model actions in Policies and check it in every action with Gate::authorize(), the can
middleware or $user->can(). Hiding a button in Blade or the frontend is not authorisation (SEC-7).

#### Shape API responses with API Resources
<!-- rule: id=LRV-3 enforcement=rule -->
Return API data through JsonResource and resource collections that list exposed fields explicitly, so new internal
columns never leak, and include relations with whenLoaded() so serialisation cannot trigger lazy loading.

## Eloquent and database

Eloquent's convenience hides query counts and mass assignment; make both explicit.

#### Eager-load relations and forbid lazy loading outside production
<!-- rule: id=LRV-4 enforcement=rule -->
Load relations with with() or load() for every list, and call Model::preventLazyLoading(! app()->isProduction()) or
Model::shouldBeStrict() in a service provider, so N+1 queries throw in development and tests instead of reaching
production.

#### Protect models against mass assignment
<!-- rule: id=LRV-5 enforcement=rule -->
Declare $fillable on every model (never an empty $guarded) and fill models only from validated data; mass-assigning
unchecked input lets clients set columns such as is_admin or user_id.

#### Write reversible migrations
<!-- rule: id=LRV-6 enforcement=rule -->
Implement down() for every reversible migration and run migrate:rollback locally before merging; for irreversible
changes, make down() throw and follow DB-3.

#### Never interpolate values into raw queries
<!-- rule: id=LRV-7 enforcement=rule -->
Prefer Eloquent and the query builder. In raw fragments (DB::select, DB::raw, whereRaw, orderByRaw, selectRaw) pass
values as bindings, e.g. whereRaw('price > ?', [$min]), and allowlist column names; interpolating input there is SQL
injection (SEC-4).

## Configuration and background work

Cached configuration and queues behave differently from local development unless used the Laravel way.

#### Call env() only inside config files
<!-- rule: id=LRV-8 enforcement=rule -->
Read environment variables with env() only in config/*.php and use config('services.x.key') everywhere else. After
php artisan config:cache, env() calls outside config files return null.

#### Queue slow work and dispatch it after commit
<!-- rule: id=LRV-9 enforcement=rule -->
Dispatch mail, notifications, webhooks and exports as idempotent queued jobs (ShouldQueue) with $tries, backoff() and
a failed() handler. Dispatch jobs raised inside a database transaction with afterCommit() so workers never see
uncommitted data, and run workers under a supervisor or Horizon.

## Quality

Laravel's testing helpers make feature tests cheap; use them for every route.

#### Write feature tests with Pest or PHPUnit
<!-- rule: id=LRV-10 enforcement=rule check=test -->
Cover each route with feature tests in the project's framework (Pest for new projects) using RefreshDatabase,
factories and actingAs(), asserting status, JSON structure and authorisation failures. Fake side effects with
Queue::fake(), Mail::fake() and Http::fake() instead of calling real services.
