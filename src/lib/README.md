# Frontend architecture

The frontend is split into layers with a one-way dependency rule. The goal is
the same property Zed gets from its crate graph: a fast, predictable core that
UI code composes, instead of UI code owning I/O and business rules.

```
             components/  (feature UI)
                  |
                 app/     (composition: commands, dialogs, layout)
                  |
               domain/    (git state + business rules)
                  |
              platform/   (Tauri IPC, types, keys, updater)
                  |
        design/  (tokens, motion, highlighting)
                  |
          config/ + util/  (leaf helpers)
```

A layer may import **only itself and layers below it**. `src/App.svelte` and
`src/main.ts` are the composition root and may import anything.

## Layers

| Layer | Owns | Examples |
| --- | --- | --- |
| `config/` | App configuration and feature flags | `featureFlags.svelte.ts` |
| `util/` | Generic helpers with no internal layer dependencies | `fuzzy.ts`, `toast.ts` |
| `design/` | Visual language and presentation helpers | `styles/*.css`, `motion.ts`, `highlight.ts` |
| `platform/` | External boundaries and shared contracts | `api.ts`, `types.ts`, `keys.ts`, `updater.ts` |
| `domain/` | Git/business state and rules | `stores/*`, `clone.ts`, `diff-split.ts`, `reorder.ts` |
| `app/` | App-wide orchestration | `commands.ts`, `menu.ts`, `ui.ts`, `layout.ts`, `resizer.ts` |
| `components/` | Feature UI | `Sidebar/`, `Pane/`, `Diff/`, `Viewer/`, `common/` |

## Rules

1. **No upward imports.** `domain/` never imports `app/` or `components/`;
   `design/` never imports `domain/`; `platform/` is the only layer that talks
   to `@tauri-apps/*` at runtime (type-only imports are allowed where a
   component needs the package's types). `npm run lint:layers` enforces this.
2. **Platform owns the IPC contract.** All Tauri commands and DTOs live in
   `platform/api.ts` and `platform/types.ts`.
3. **Domain owns mutation.** Components should call domain actions
   (`domain/stores/*`, `domain/*.ts`) rather than invoking the platform directly.
   New feature work should follow this; existing direct calls are being migrated.
4. **Design is the only source of visual values.** Components consume tokens
   from `design/styles/tokens.css`. New hardcoded colors, `font-size: Npx`,
   `transition: all`, and `!important` are rejected by the ratchet in
   `scripts/design-guard.mjs` (see below).
5. **Small, single-purpose state owners.** Prefer per-domain stores and derived
   selectors over one global object. Avoid replacing an entire map to update one
   entry when a selector can narrow the invalidation.
6. **Async work is cancellable.** Use `platform/request.ts` for latest-wins
   loads so switching repos/files cannot apply a stale result.
7. **Keep optional UI off the startup path.** Panels that only some users open
   (the terminal, heavy viewers) should be dynamically imported so the window
   paints from the smallest possible bundle.

## Design ratchet

```bash
npm run lint:design                  # fail if a file adds new violations
node scripts/design-guard.mjs --update  # lower the budget after a cleanup
```

The baseline in `scripts/design-baseline.json` is a budget, not an allowlist to
grow. When you touch a file, prefer replacing literals with tokens and then
lowering its budget.

## Layer guard

```bash
npm run lint:layers
```

Fails on any import from a higher layer to a lower one and on runtime
`@tauri-apps/*` imports outside `platform/`. Both guarantees are also checked
in CI.

## Adding code

- New Git/business logic: `domain/`, tested from `tests/`.
- New Tauri command or DTO: `platform/api.ts` + `platform/types.ts` (+ Rust).
- New reusable visual behaviour: `design/`.
- New app-wide command/shortcut/dialog orchestration: `app/`.
- New screen or widget: `components/<Feature>/`.
