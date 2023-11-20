// -*- coding: utf-8 -*-
// ------------------------------------------------------------------------------------------------
// Copyright © 2023, Douglas Creager.
// Licensed under the MIT license.
// Please see the LICENSE file in this distribution for license details.
// ------------------------------------------------------------------------------------------------

use std::path::Path;

fn main() {
    let package_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let include = package_dir.join("deps/ltreesitter/include");
    let csrc = package_dir.join("deps/ltreesitter/csrc");
    let mut config = cc::Build::new();
    if let Some(include) = std::env::var_os("DEP_LUA_INCLUDE") {
        config.include(include);
    }
    if let Some(include) = std::env::var_os("DEP_TREE_SITTER_INCLUDE") {
        config.include(include);
    }
    println!("cargo:include={}", include.display());
    config
        .warnings(true)
        .opt_level(2)
        .cargo_metadata(true)
        .include(&include)
        .file(csrc.join("dynamiclib.c"))
        .file(csrc.join("ltreesitter.c"))
        .file(csrc.join("luautils.c"))
        .file(csrc.join("node.c"))
        .file(csrc.join("object.c"))
        .file(csrc.join("parser.c"))
        .file(csrc.join("query.c"))
        .file(csrc.join("query_cursor.c"))
        .file(csrc.join("tree.c"))
        .file(csrc.join("tree_cursor.c"))
        .file(csrc.join("types.c"))
        .compile("ltreesitter");
}
