---
nexus: 1
id: {{id}}
kind: rule
format: constitution
title: Angular
status: active
applies: on-demand
prefix: NG
stacks: [angular]
created: {{now}}
updated: {{now}}
source: import
---

Rules for current Angular (standalone components, signals, built-in control flow, zoneless change detection). Apply
to Angular code; they refine FE, TS and TST.

## Components and state

Standalone components with signals and OnPush give precise dependencies and predictable change detection.

#### Write standalone components without new NgModules
<!-- rule: id=NG-1 enforcement=rule -->
Create components, directives and pipes as standalone (the default in current Angular) and import their dependencies
directly; do not add NgModules in new code. Standalone makes the dependency graph explicit and lets each component be
lazy-loaded.

#### Use OnPush change detection and prefer zoneless
<!-- rule: id=NG-2 enforcement=rule -->
Set changeDetection: ChangeDetectionStrategy.OnPush on every component and drive updates through signals, inputs and
the async pipe; never mutate input objects in place. For new apps prefer zoneless change detection
(provideZonelessChangeDetection) on Angular versions where it is stable.

#### Hold state in signals
<!-- rule: id=NG-3 enforcement=rule -->
Model state with signal(), derive values with computed() or linkedSignal(), and use input(), output() and model()
instead of @Input/@Output in new code. Use effect() only to sync with non-Angular APIs (storage, logging, widgets),
never to copy one signal into another.

#### Inject dependencies with inject()
<!-- rule: id=NG-4 enforcement=rule -->
Use the inject() function in field initialisers instead of constructor parameters, and provide services with
providedIn: 'root' unless they need a narrower scope. inject() also works in functional guards, resolvers and
interceptors.

#### Use built-in control flow with a stable track expression
<!-- rule: id=NG-5 enforcement=rule -->
Write templates with @if, @for and @switch instead of the deprecated *ngIf, *ngFor and ngSwitch. Track @for items by
a stable identity (track item.id, not $index for lists that change), add @empty where a list can be empty, and use
@defer for heavy content below the fold.

## Forms and routing

Typed forms and lazy routes catch mistakes at compile time and keep the initial bundle small.

#### Use typed reactive forms
<!-- rule: id=NG-6 enforcement=rule -->
Build non-trivial forms as typed reactive forms (NonNullableFormBuilder, typed controls) unless the project has
settled on another approach; avoid UntypedFormGroup and template-driven forms beyond trivial cases. Keep validators
pure functions and show errors after touch or submit.

#### Lazy-load feature routes
<!-- rule: id=NG-7 enforcement=rule -->
Load feature areas with loadComponent or loadChildren pointing to a routes file, protect them with functional guards
(CanActivateFn), and bind route parameters to component inputs with withComponentInputBinding() instead of reading
ActivatedRoute by hand.

## RxJS

Leaked and nested subscriptions are the most common source of Angular memory leaks and race conditions.

#### Never leave subscriptions open or nest them
<!-- rule: id=NG-8 enforcement=rule -->
Prefer the async pipe or toSignal() to manual subscribe(). When you must subscribe, end it with takeUntilDestroyed();
never subscribe inside a subscribe, compose with switchMap, concatMap, mergeMap or exhaustMap chosen for the
cancellation semantics you need.

## Tooling and tests

The compiler and the CLI catch most Angular mistakes when they are configured strictly.

#### Enable strict template type checking
<!-- rule: id=NG-9 enforcement=rule check=build -->
Keep strict: true in tsconfig and strictTemplates, strictInjectionParameters and strictInputAccessModifiers in
angularCompilerOptions, so template bindings are type-checked and the build fails on template type errors.

#### Follow Angular CLI conventions and upgrade with ng update
<!-- rule: id=NG-10 enforcement=guideline -->
Generate code with ng generate so names and structure follow the project's style-guide settings, keep one component
per file with a consistent selector prefix, and propose framework upgrades through ng update, which runs the code
migrations, instead of bumping versions by hand.

#### Test with TestBed and component harnesses
<!-- rule: id=NG-11 enforcement=rule check=test -->
Test components through TestBed with their real templates in the project's runner (Karma is deprecated; new projects
use the CLI's Vitest support), interact via CDK component harnesses or Testing Library queries, and mock HTTP with
provideHttpClientTesting() and HttpTestingController.
