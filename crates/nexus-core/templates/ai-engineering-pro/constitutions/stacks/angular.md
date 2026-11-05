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

Rules for current Angular (standalone components, signals, built-in control flow). Apply to Angular code; they refine
FE, TS and TST.

## Components and state

Standalone components with signals and OnPush give precise dependencies and predictable change detection.

#### Write standalone components without new NgModules
<!-- rule: id=NG-1 enforcement=rule -->
Create components, directives and pipes as standalone (the default in current Angular) and import their dependencies
directly in imports; do not add new NgModules. Standalone gives explicit dependency graphs and lets each component be
lazy-loaded.

#### Use OnPush change detection
<!-- rule: id=NG-2 enforcement=rule -->
Set changeDetection: ChangeDetectionStrategy.OnPush on every component and drive updates through signals, inputs and
the async pipe; never mutate input objects in place. OnPush (and zoneless apps) re-render only when Angular is told
something changed.

#### Hold state in signals
<!-- rule: id=NG-3 enforcement=rule -->
Model state with signal(), derive values with computed() or linkedSignal(), and use input(), output() and model()
instead of @Input/@Output decorators. Use effect() only to sync with non-Angular APIs (storage, logging, widgets),
never to copy one signal into another.

#### Inject dependencies with inject()
<!-- rule: id=NG-4 enforcement=rule -->
Use the inject() function in field initialisers instead of constructor parameters, and provide services with
providedIn: 'root' unless they need a narrower scope. inject() also works in functional guards, resolvers and
interceptors.

#### Use built-in control flow with a stable track expression
<!-- rule: id=NG-5 enforcement=rule -->
Write templates with @if, @for and @switch instead of *ngIf, *ngFor and ngSwitch. Track @for items by a stable
identity (track item.id, not $index for lists that change), add @empty where a list can be empty, and use @defer for
heavy content below the fold.

## Forms and routing

Typed forms and lazy routes catch mistakes at compile time and keep the initial bundle small.

#### Use typed reactive forms
<!-- rule: id=NG-6 enforcement=rule -->
Build non-trivial forms as typed reactive forms (FormGroup with typed controls, NonNullableFormBuilder) so values are
type-checked; avoid UntypedFormGroup and template-driven forms beyond trivial cases. Keep validators pure functions
and show errors after touch or submit.

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

#### Follow Angular CLI and style-guide conventions
<!-- rule: id=NG-10 enforcement=guideline -->
Generate code with ng generate so file names, selectors and structure follow the official style guide, keep one
component per file with a consistent selector prefix, and upgrade with ng update, which runs the migrations, instead
of bumping versions by hand.

#### Test with TestBed and component harnesses
<!-- rule: id=NG-11 enforcement=rule check=test -->
Test components through TestBed with their real templates, interact via Angular CDK component harnesses or Testing
Library queries instead of touching internals, and mock HTTP with provideHttpClientTesting() and HttpTestingController.
