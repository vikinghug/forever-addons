# Rust instructions

These instructions are mandatory when writing Rust in this repository.

If these instructions conflict with repo-specific architecture in `CLAUDE.md`, follow `CLAUDE.md`.

## Baseline

Write modern, idiomatic Rust using the 2024 edition.

Optimize for:

* correctness
* simple ownership
* explicit domain types
* narrow visibility
* small modules
* readable control flow
* compiler-enforced invariants
* boring, maintainable code

Do not write Java, TypeScript, Go, or C++ patterns translated into Rust.

## Module layout

Use modern Rust module layout.

Prefer:

```text
src/
  lib.rs
  source.rs
  source/
    curseforge.rs
    wago.rs
    github.rs
  domain.rs
  domain/
    addon.rs
    version.rs
    folder.rs
```

Use declarations like:

```rust
mod source;
mod domain;
```

Inside `source.rs`:

```rust
pub mod curseforge;
pub mod wago;
pub mod github;
```

Do not create new `mod.rs` files.

Avoid:

```text
src/
  source/
    mod.rs
    curseforge.rs
    wago.rs
```

Existing `mod.rs` files may be touched when necessary, but do not expand that pattern. When a nearby migration is small and safe, prefer `module.rs` plus `module/child.rs`.

## Visibility

Default to private.

Use the narrowest useful visibility:

```rust
pub(crate)
pub(super)
pub
```

Do not make fields, modules, functions, or types public because it is convenient during implementation.

Prefer constructors, methods, and typed accessors over exposing mutable internals.

## Type design

Use domain types instead of primitive obsession.

Prefer:

```rust
pub struct AddonKey(String);
pub struct AddonFolder(String);
pub struct InterfaceVersion(u32);
```

Over passing raw `String`, `&str`, `u32`, or `usize` through domain logic.

Use enums for known variants:

```rust
pub enum Download {
    Direct,
    Brokered,
    Unsupported,
    External,
}
```

Use `Unknown` or `Unsupported` variants when parsing external data that may evolve:

```rust
pub enum ParsedDirective {
    Known(KnownDirective),
    Unknown {
        key: String,
        value: String,
    },
}
```

Do not erase domain uncertainty into `serde_json::Value` inside core logic.

## Ownership and borrowing

Prefer borrowed inputs when the function does not need ownership:

```rust
fn parse_toc(input: &str) -> Result<Toc>
```

Take ownership when storing or transforming:

```rust
fn insert_record(&mut self, record: InstalledRecord)
```

Avoid unnecessary `clone()`, but do not contort code to avoid a cheap clone that makes ownership clearer.

Use `Arc` only for shared ownership across long-lived structures or threads.

Use `Rc` only for single-threaded shared ownership when the ownership graph actually needs it.

Avoid interior mutability unless there is a specific reason.

Do not use `RefCell`, `Mutex`, `RwLock`, global state, or lazy singletons to avoid designing ownership.

## Function size and control flow

Prefer small, single-purpose functions.

A function should usually do one of these:

* coordinate a workflow
* transform one data shape into another
* validate one invariant
* parse one syntactic form
* apply one domain operation
* render or serialize one output shape

Avoid large procedural functions that parse, validate, mutate, log, normalize, and persist in one body.

As a guideline:

* under 40 lines is preferred
* 40-80 lines is acceptable when the function is still linear and readable
* over 80 lines requires a strong reason
* over 120 lines should usually be split before continuing
* hundreds of lines are not acceptable for hand-written code unless generated, table-driven, or deliberately centralized parser/matcher code

Use a top-level function as a table of contents:

```rust
pub fn refresh_catalog(input: RefreshInput) -> Result<Catalog> {
    let raw = fetch_source_pages(input.source)?;
    let parsed = parse_source_pages(raw)?;
    let normalized = normalize_summaries(parsed)?;
    validate_catalog(&normalized)?;

    Ok(normalized)
}
```

Prefer extracting named helpers over nesting control flow.

Prefer early returns, `let else`, and small matches:

```rust
let Some(record) = records.get(id) else {
    return Err(ImportError::MissingRecord { id: id.clone() });
};

if !record.enabled {
    return Ok(None);
}

let normalized = normalize_record(record)?;
```

Avoid this shape:

```rust
fn import_everything(...) -> Result<()> {
    if condition {
        for item in items {
            if another_condition {
                match item {
                    // hundreds of lines
                }
            }
        }
    }

    Ok(())
}
```

When a function grows, split by responsibility, not by arbitrary line count.

Good extraction boundaries:

* `parse_*`
* `normalize_*`
* `validate_*`
* `resolve_*`
* `classify_*`
* `apply_*`
* `build_*`
* `write_*`

Avoid vague helper names:

* `handle_*`
* `process_*`
* `do_*`
* `run_inner`
* `helper`

Exceptions are allowed for:

* generated code
* dense lookup tables
* exhaustive domain `match` statements where splitting would reduce clarity
* test fixtures
* small command entrypoints that only sequence obvious steps

Even in exceptions, avoid deep nesting.

## Errors

Use `Result` for recoverable failures.

Do not use `unwrap`, `expect`, or `panic` in production or library code unless the invariant is local, obvious, and documented.

Good:

```rust
let addon = catalog
    .get(addon_id)
    .ok_or_else(|| AppError::UnknownAddon { addon_id })?;
```

Bad:

```rust
let addon = catalog.get(addon_id).unwrap();
```

Use precise error types in core/library crates.

Use broad error wrappers only at application boundaries, CLIs, import tools, tests, and prototypes.

Error messages should include useful debugging context:

* source file
* source id/key
* field name
* constructor name
* expected shape
* actual shape

Avoid vague errors like `invalid data` or `parse failed`.

## Traits

Do not introduce a trait just to make code look abstract.

Create a trait only when at least one is true:

* there are multiple real implementations
* tests need a meaningful seam
* the trait represents a stable domain capability
* dynamic dispatch is intentionally required

Prefer concrete types until abstraction is earned.

Avoid `Box<dyn Trait>` unless runtime polymorphism is actually needed.

Avoid async traits unless the boundary is truly async.

## Generics

Use generics when they reduce duplication without harming readability.

Do not over-generalize early.

Prefer simple concrete APIs in domain code. Generalize after two or more real call sites prove the shape.

## Lifetimes

Prefer owned domain structs unless borrowing materially improves performance or correctness.

Do not spread lifetime parameters through the domain model unless there is a clear reason.

Parsing code may borrow temporarily. Stored normalized models should usually own their data.

## Iterators and control flow

Use iterators when they are clearer:

```rust
let ids = addons.iter().map(|addon| addon.id).collect::<Vec<_>>();
```

Use loops when branching, mutation, or early exits make them clearer:

```rust
for entry in entries {
    if !entry.is_addon_file() {
        continue;
    }

    result.push(entry.normalize()?);
}
```

Do not force everything into iterator chains.

## Pattern matching

Prefer exhaustive `match` for domain enums.

Do not use wildcard arms when handling domain-critical enums unless unknown cases are intentionally grouped.

Good:

```rust
match summary.download {
    Download::Direct { .. } => install_direct(summary),
    Download::Brokered => resolve_then_install(summary),
    Download::Unsupported { .. } => refuse_with_page_link(summary),
    Download::External { .. } => refuse_with_page_link(summary),
}
```

Bad:

```rust
match summary.download {
    Download::Direct { .. } => install_direct(summary),
    _ => refuse(summary),
}
```

Use `_` only for truly irrelevant cases.

## Option and Result

Use combinators for simple transformations.

Use `match`, `if let`, or `let else` when it improves readability.

Good:

```rust
let latest = summary.version.as_ref().map(|version| version.as_str());
```

Also good:

```rust
let Some(version) = summary.version.as_ref() else {
    return Ok(());
};
```

Do not stack combinators into unreadable pipelines.

## Async

Do not make code async unless it performs async I/O or must integrate with async boundaries.

Keep pure domain logic synchronous.

Do not leak async runtimes into core crates.

## Unsafe

Do not write unsafe Rust unless explicitly requested.

If unsafe is required:

* isolate it in the smallest possible module
* document the safety invariant
* add tests around the safe wrapper
* do not expose unsafe requirements through ordinary safe APIs

## Dependencies

Before adding a crate, justify why the standard library is not enough.

Prefer:

* small crates
* widely used crates
* crates with stable APIs
* crates that belong at this architectural layer

Do not add framework-scale dependencies for small tasks.

Do not add a database, async runtime, global registry, or plugin system casually.

## Serialization

Keep wire/import formats separate from domain models when practical.

Use DTO/input structs for raw imported data.

Normalize into domain structs before core logic.

Do not let serde attributes, optional raw fields, or external naming conventions contaminate the domain model unless there is a deliberate reason.

## Data imports

When importing external addon-source data (API payloads, TOC files, archives):

* preserve raw source data when useful for debugging
* normalize into typed Rust structures before core use
* track provenance when possible
* do not discard unknown fields silently
* represent unsupported shapes explicitly
* prefer deterministic IDs and stable ordering for repeatable tests

Unknown or unsupported directive/download forms should become explicit `Unknown` or `Unsupported` values, not silent drops.

Strict parsing may fail, but permissive import paths should preserve unknown data.

## Tests

Prefer tests that encode behavior and invariants.

Add tests for:

* parser edge cases
* normalization
* unknown or unsupported external input
* archive layout planning
* update-status checks
* regression cases from real data

Test names should describe behavior:

```rust
#[test]
fn preserves_unknown_toc_directive_as_unknown()
```

Avoid vague names:

```rust
#[test]
fn test_parser()
```

Use table tests for dense parser cases.

Use fixtures for real external data shapes.

## Lints

Assume code must pass:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

Do not silence lints broadly.

If a lint must be silenced, prefer the narrowest local `#[expect(...)]` with a reason.

Avoid `#[allow(...)]` unless there is no better option.

## Review checklist

Before finalizing Rust changes, check:

* Are module files using `foo.rs` plus `foo/child.rs`, not new `foo/mod.rs`?
* Are public items intentionally public?
* Are domain concepts represented as types instead of strings, bools, or maps?
* Are functions small and single-purpose?
* Is control flow shallow and readable?
* Are errors specific and diagnostic?
* Are `unwrap`, `expect`, and `panic` absent from production/library code?
* Are traits justified by real polymorphism or a real seam?
* Is async kept out of pure domain logic?
* Are parser DTOs separate from normalized domain models?
* Are unknown external data shapes preserved explicitly?
* Are tests named by behavior?
* Does the code pass fmt, clippy, and tests?

## Anti-patterns

Avoid:

```rust
pub struct Thing {
    pub kind: String,
    pub data: serde_json::Value,
}
```

Prefer:

```rust
pub enum Thing {
    Known(KnownThing),
    Unsupported(UnsupportedThing),
}
```

Avoid:

```rust
fn install(path: &str, force: bool)
```

Prefer:

```rust
fn install(folder: AddonFolder, mode: InstallMode)
```

Avoid:

```rust
pub trait Service {
    fn execute(&self);
}

pub struct DefaultService;
```

When there is only one implementation, prefer:

```rust
pub struct Service;

impl Service {
    pub fn execute(&self) {
        // ...
    }
}
```

Avoid:

```rust
let value = maybe_value.unwrap();
```

Prefer:

```rust
let value = maybe_value.ok_or(Error::MissingValue)?;
```

Avoid clever Rust.

Prefer boring Rust that makes invalid states hard to express.
