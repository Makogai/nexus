---
nexus: 1
id: {{id}}
kind: rule
format: constitution
title: Flutter
status: active
applies: on-demand
prefix: FLT
stacks: [flutter]
created: {{now}}
updated: {{now}}
source: import
---

Rules for Flutter apps (Flutter 3, Dart 3): widgets, state, layout, accessibility, platform integration and tests.
Apply to Flutter code; they refine DART, which also covers analysis and formatting.

## Widgets

Small const widgets with narrowly scoped rebuilds keep Flutter UIs fast and testable.

#### Compose small widget classes and use const
<!-- rule: id=FLT-1 enforcement=rule -->
Split screens into small widget classes rather than helper methods that return widgets, and use const constructors
and instances wherever possible, so Flutter can skip rebuilding unchanged subtrees and each widget can be tested on its
own.

#### Keep business logic out of widgets
<!-- rule: id=FLT-2 enforcement=rule -->
Widgets render state and forward user intents only; put validation, data loading and business rules in notifiers,
blocs, controllers or plain Dart classes that can be unit-tested without a widget tree.

#### Avoid rebuilding large subtrees
<!-- rule: id=FLT-3 enforcement=rule -->
Listen to state as low in the tree as possible (Consumer, select, BlocSelector, ValueListenableBuilder), pass static
parts as the builder's child, and use ListView.builder for long lists. Never do expensive work or create controllers
in build().

#### Dispose what you create
<!-- rule: id=FLT-4 enforcement=rule -->
Create TextEditingController, AnimationController, ScrollController, FocusNode and stream subscriptions in initState
and release them in dispose(). After every await, check context.mounted before using the BuildContext again.

## State

One deliberate approach to state is easier to reason about than several competing ones.

#### Use one state-management approach per app
<!-- rule: id=FLT-5 enforcement=rule -->
Follow the app's established approach; for a new app choose one (e.g. Riverpod, Bloc, or ChangeNotifier with
provider), document it and use it consistently, without mixing libraries or falling back to global singletons. Use
setState only for ephemeral UI state local to one widget.

## Layout and accessibility

Flutter apps run on phones, tablets, desktop and web, and must work with assistive technologies on each.

#### Build responsive layouts
<!-- rule: id=FLT-6 enforcement=rule -->
Adapt layouts with LayoutBuilder and breakpoints from MediaQuery.sizeOf(context) instead of fixed sizes, respect safe
areas, and check small screens and large text scaling, so the UI works across phones, tablets, desktop and web.

#### Provide semantics, tap targets and contrast
<!-- rule: id=FLT-7 enforcement=rule -->
Give icon buttons and meaningful images a tooltip or semanticLabel, wrap custom controls in Semantics with label and
actions, keep tap targets at least 48x48 dp and text contrast at least 4.5:1, and check them in widget tests with
meetsGuideline (androidTapTargetGuideline, textContrastGuideline) and with each platform's screen reader.

## Platform integration

Platform code is the least portable and hardest to test part of an app; keep it behind a seam.

#### Isolate platform channels behind interfaces
<!-- rule: id=FLT-8 enforcement=rule -->
Wrap platform channels and plugins in an app-owned Dart interface with a fake for tests, generate type-safe channel
code with Pigeon instead of hand-written method-name strings, and handle MissingPluginException and unsupported
platforms explicitly.

## Testing

Widget tests are cheap and catch most Flutter regressions.

#### Write widget and golden tests
<!-- rule: id=FLT-9 enforcement=rule check=test -->
Test widgets with testWidgets and WidgetTester (pump, tap, enterText, finders by text, key or semantics label),
protect key screens with golden tests (matchesGoldenFile) generated on one pinned platform, and cover critical flows
with integration tests.
