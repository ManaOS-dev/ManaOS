# ManaOS Templates

Templates are copy-start files for contributors and agents. They are not
generated output; after copying one, replace every placeholder before committing.

## Template Catalog

| Template | Use |
| --- | --- |
| `driver.rs.template` | New driver module facade or narrow driver API starting point. |
| `module_mod.rs.template` | New `mod.rs` ownership block and thin module facade. |
| `documentation.md.template` | New English design or validation document. |
| `documentation.ja.md.template` | Japanese companion for a contributor-facing document. |
| `commit-message.template` | Commit message skeleton for non-trivial changes. |

## Usage Rules

- Read `AGENTS.md` and `CONTRIBUTING.md` before using any template.
- Keep Rust comments and Rust doc comments in English.
- Keep `mod.rs` files thin: ownership docs, module declarations, re-exports,
  and small forwarding APIs only.
- Use `process_*` for main-loop processors and `push_*` for interrupt-side
  event ingestion.
- Keep all statics private and expose state through functions.
- Search copied target files for placeholders before committing:

```powershell
rg -n "Replace with|<[^>]+>" src docs
```

## Validation

For template-only or Markdown-only edits, run:

```powershell
git diff --check
```

If a copied template becomes Rust code, run the checks required by
`CONTRIBUTING.md` for the touched subsystem.
