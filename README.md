# `mlua-tree-sitter`

This crate provides access to the [`ltreesitter`][ltreesitter] Lua tree-sitter
bindings in Lua environments managed by the [`mlua`][mlua] crate.

[ltreesitter]: https://github.com/euclidianAce/ltreesitter/
[mlua]: https://docs.rs/mlua/

Please see the [crate documentation][docs] for example usage.

[docs]: https://docs.rs/mlua-tree-sitter/

## Building

This crate depends on the [`mlua`][mlua] crate, which supports multiple Lua
versions, and can either link against a system-installed copy of Lua, or build
its own copy from vendored Lua source.  These choices are all controlled via
`mlua` features.

When building and testing this crate, make sure to provide all necessary
features on the command line:

``` console
$ cargo test --features mlua/lua54,mlua/vendored
```

When building a crate that depends on this crate, add a dependency on `mlua` so
that you can set its feature flags:

``` toml
[dependencies]
mlua = { version="0.9", features=["lua54", "vendored"] }
mlua-tree-sitter = { version="0.1" }
```

## Licensed

Licensed under the MIT license.
