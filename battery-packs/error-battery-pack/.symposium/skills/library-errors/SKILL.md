---
name: library-errors
description: Modeling, exposing, and formatting errors in Rust libraries
---

# Library Errors

How to model errors inside a crate, make them opaque at public boundaries, and control how they render.

> **Prerequisites:** This skill references examples from the error battery pack.
> ```sh
> cargo install cargo-bp   # install the battery-pack CLI
> cargo bp add error       # add anyhow + thiserror to your project
> ```

## Internal Error Types

Within your crate boundary, use `thiserror` freely. No semver concerns since consumers never see these types.

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub(crate) enum StorageError {
    #[error("failed to read from disk")]
    Io(#[from] std::io::Error),

    #[error("corrupt data at offset {offset}")]
    Corrupt { offset: u64 },

    #[error("item not found: {0}")]
    NotFound(String),
}
```

Guidelines for internal errors:

- **One enum per module or subsystem.** Don't create a single god-error for the whole crate.
- **Use `#[from]` liberally.** It generates `From` impls so `?` works across internal boundaries.
- **Include diagnostic data in variants** (offsets, keys, paths). You control the match sites, so adding fields is free.
- **No `#[non_exhaustive]` needed.** You own all the match arms.

### Composing internal errors across modules

```rust
// in src/indexer.rs
#[derive(Error, Debug)]
pub(crate) enum IndexError {
    #[error("storage failure")]
    Storage(#[from] StorageError),

    #[error("index corrupted")]
    Corrupted,
}
```

## Public API Boundary

At the crate's public API, wrap internal errors in an opaque type. This decouples your internal structure from your consumers' code.

### The Error+ErrorKind Pattern

This is the same pattern used by `std::io::Error`. Private fields, `#[non_exhaustive]` kind enum, boxed cause:

```rust
use std::fmt::Display;

#[non_exhaustive]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum MyLibErrorKind {
    QueueFull,
    Storage,
    NotFound,
}

#[derive(Debug)]
pub struct MyLibError {
    kind: MyLibErrorKind,
    cause: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
}

impl MyLibError {
    pub fn kind(&self) -> MyLibErrorKind {
        self.kind
    }

    pub(crate) fn new(kind: MyLibErrorKind, cause: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self { kind, cause: Some(Box::new(cause)) }
    }

    pub(crate) fn from_kind(kind: MyLibErrorKind) -> Self {
        Self { kind, cause: None }
    }
}

impl Display for MyLibError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.kind {
            MyLibErrorKind::QueueFull => write!(f, "queue is full"),
            MyLibErrorKind::Storage => write!(f, "storage operation failed"),
            MyLibErrorKind::NotFound => write!(f, "not found"),
        }
    }
}

impl std::error::Error for MyLibError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.cause.as_deref()
    }
}
```

This gives semver freedom: private fields hide internals, `#[non_exhaustive]` allows adding variants, and the boxed cause hides dependency types. `Copy` on the kind lets `.kind()` return by value; if you later need variants carrying data, derive only `Clone` and return `&MyLibErrorKind` instead.

### Converting internal errors to the public type

```rust
impl From<StorageError> for MyLibError {
    fn from(e: StorageError) -> Self {
        let kind = match &e {
            StorageError::NotFound(_) => MyLibErrorKind::NotFound,
            _ => MyLibErrorKind::Storage,
        };
        MyLibError::new(kind, e)
    }
}
```

Map your rich internal taxonomy to the coarser public kinds. The internal error becomes the opaque `.source()`.

### Using thiserror at the boundary (middle ground)

`thiserror` with boxed sources gives derive convenience without exposing dependency types:

```rust
#[non_exhaustive]
#[derive(Error, Debug)]
pub enum MyLibError {
    #[error("queue is full")]
    QueueFull,

    #[error("storage operation failed")]
    Storage { #[source] source: Box<dyn std::error::Error + Send + Sync + 'static> },

    #[error("not found")]
    NotFound,
}
```

Use this only for crates with stable, well-understood error categories. For evolving APIs, prefer the struct pattern: any public enum has a larger semver surface than a struct (variant names, field arity, and tuple positions are all public API).

### How consumers use your error type

```rust
match client.upload(data) {
    Ok(()) => {}
    Err(e) => match e.kind() {
        MyLibErrorKind::QueueFull => retry_later(),
        MyLibErrorKind::NotFound => create_and_retry(),
        _ => return Err(e.into()),
    }
}
```

## Formatting Rules

### Display: only this error's message

`Display` must not include the source. Reporters walk `.source()` separately:

```rust
// Wrong: source appears twice in output
write!(f, "storage failed: {}", self.cause.as_ref().unwrap())

// Right
write!(f, "storage failed")
```

### Messages: lowercase, no trailing punctuation

```rust
// Good: composes well when reporters join with ": "
"connection refused"
"invalid header: expected utf-8"

// Bad
"Connection refused."
"Error: Invalid header"
```

## Re-exporting Dependency Types

If consumers need to downcast to your dependency's error type, re-export it:

```rust
pub use aws_sdk_s3::error::SdkError;
```

Prefer exposing information through your error kind or accessor methods over requiring consumers to downcast.

## Checklist

- [ ] Internal errors: `thiserror` enum per module, `#[from]` for conversions
- [ ] Public errors: `#[non_exhaustive]`, private fields, accessor methods
- [ ] `From` impls map internal errors to public error kinds
- [ ] `Box<dyn Error + Send + Sync + 'static>` for causes (not concrete types)
- [ ] `Display` prints only this error's message, never the source
- [ ] `source()` returns the underlying cause
- [ ] Messages lowercase, no trailing punctuation
- [ ] Re-export dependency types only if consumers need to downcast

## Related

See the **application-errors** skill for the consumer side: propagating errors with `anyhow`, formatting for logs and terminals, backtrace configuration.

```sh
cargo run --example custom-errors -p error-battery-pack
```
