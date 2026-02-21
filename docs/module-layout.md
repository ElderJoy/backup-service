# Module Layout Convention

This project uses **`module_name.rs` + `module_name/`** instead of **`module_name/mod.rs`** for multi-file modules.

## Rule

- **Single-file module:** one file `module_name.rs` (e.g. `db.rs`, `config.rs`).
- **Multi-file module:** one file `module_name.rs` that declares submodules, plus a directory `module_name/` containing the submodule files (e.g. `handlers.rs` + `handlers/auth.rs`, `handlers/backups.rs`).

We do **not** use `module_name/mod.rs` as the module entry point.

## Current Layout

**backup-service:**

- `handlers.rs` → `pub mod auth; pub mod backups; pub mod health;`  
  Submodules live in `handlers/auth.rs`, `handlers/backups.rs`, `handlers/health.rs`.
- `middleware.rs` → `pub mod auth; pub mod rate_limit;`  
  Submodules in `middleware/auth.rs`, `middleware/rate_limit.rs`.
- `db.rs` → single file (all DB logic; no `db/` directory).

**backup-common:**

- `ffi.rs` → all FFI Rust code.  
  C sources stay in `ffi/c_src/entropy.c` (directory kept for non-Rust files; no `ffi/mod.rs`).

## Pros

| # | Benefit |
|---|--------|
| 1 | **Consistent entry point** — The module’s main file is always `module_name.rs`, so you know where to look. |
| 2 | **No special `mod.rs`** — Avoids the “magic” name `mod.rs` and keeps the same name as the module in the filesystem. |
| 3 | **Easier discovery** — In a flat list, `handlers.rs` and `middleware.rs` are next to sibling modules; the folder holds only submodules. |
| 4 | **Same as single-file modules** — Single-file modules are already `name.rs`; multi-file modules use `name.rs` + `name/`, so the pattern is uniform. |

## Cons

| # | Drawback |
|---|----------|
| 1 | **Two places for one module** — A multi-file module is split between `module_name.rs` (declarations) and `module_name/*.rs` (code). Some prefer “everything under one directory” with `mod.rs`. |
| 2 | **Rust default** — The book and many projects use `mod.rs`; this convention is less common. |
| 3 | **Refactor cost** — Switching from `mod.rs` to `module_name.rs` requires moving/creating files and updating `include_str!` paths if any (e.g. in `db.rs`). |

## Does it work?

Yes. Rust allows both styles:

- `mod foo;` loads either `foo.rs` or `foo/mod.rs`.
- With `foo.rs` and `pub mod bar;` inside it, Rust looks for `foo/bar.rs` (or `foo/bar/mod.rs`).

No change to `use` paths or public API; only file layout and the module root file name change.
