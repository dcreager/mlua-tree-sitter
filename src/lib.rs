// -*- coding: utf-8 -*-
// ------------------------------------------------------------------------------------------------
// Copyright © 2023, Douglas Creager.
// Please see the LICENSE file in this distribution for license details.
// ------------------------------------------------------------------------------------------------

//! Provides access to the [`ltreesitter`][ltreesitter] Lua tree-sitter bindings in Lua
//! environments managed by the [`mlua`][mlua] crate.
//!
//! [ltreesitter]: https://github.com/euclidianAce/ltreesitter/
//! [mlua]: https://docs.rs/mlua/
//!
//! ## Parsing from Rust, consuming the parse tree from Lua
//!
//! A common use case is to parse source code in Rust using tree-sitter, and then invoke a Lua
//! script to consume that parse tree:
//!
//! ```
//! # fn main() -> Result<(), anyhow::Error> {
//! use mlua_tree_sitter::Module;
//! use mlua_tree_sitter::WithSource;
//!
//! // Parse some Python source code
//! let code = br#"
//!     def double(x):
//!         return x * 2
//! "#;
//! let mut parser = tree_sitter::Parser::new();
//! parser.set_language(tree_sitter_python::language())?;
//! let parsed = parser.parse(code, None).expect("Could not parse Python code");
//!
//! // Create a Lua interpreter and define a Lua function that will
//! // parse the syntax tree.
//! let lua = mlua::Lua::new();
//! lua.open_ltreesitter()?;
//! let chunk = r#"
//!     function process_tree(parsed)
//!       local root = parsed:root()
//!       assert(root:type() == "module", "expected module as root of tree")
//!     end
//! "#;
//! lua.load(chunk).set_name("Lua chunk").exec()?;
//! let process_tree: mlua::Function = lua.globals().get("process_tree")?;
//!
//! // Execute the Lua function
//! process_tree.call(parsed.with_source(code))?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Building
//!
//! This crate depends on the [`mlua`][mlua] crate, which supports multiple Lua versions, and can
//! either link against a system-installed copy of Lua, or build its own copy from vendored Lua
//! source.  These choices are all controlled via `mlua` features.
//!
//! When building and testing this crate, make sure to provide all necessary features on the
//! command line:
//!
//! ``` console
//! $ cargo test --features mlua/lua54,mlua/vendored
//! ```
//!
//! When building a crate that depends on this crate, add a dependency on `mlua` so that you can
//! set its feature flags:
//!
//! ``` toml
//! [dependencies]
//! mlua = { version="0.9", features=["lua54", "vendored"] }
//! mlua-tree-sitter = { version="0.1" }
//! ```

use std::ffi::c_char;
use std::ffi::c_void;
use std::ops::Deref;
use std::ops::DerefMut;

use mlua::Lua;
use tree_sitter::Tree;

/// An extension trait that lets you load the `ltreesitter` module into a Lua environment.
pub trait Module {
    /// Loads the `ltreesitter` module into a Lua environment.
    fn open_ltreesitter(&self) -> Result<(), mlua::Error>;
}

impl Module for Lua {
    fn open_ltreesitter(&self) -> Result<(), mlua::Error> {
        unsafe extern "C-unwind" fn load_ltreesitter(l: *mut mlua::lua_State) -> i32 {
            extern "C-unwind" {
                fn luaopen_ltreesitter(l: *mut mlua::lua_State) -> i32;
            }
            mlua::ffi::luaL_requiref(
                l,
                "ltreesitter".as_ptr() as *const _,
                luaopen_ltreesitter,
                false as i32,
            );
            1
        }
        let load = unsafe { self.create_c_function(load_ltreesitter) }?;
        load.call(())?;
        Ok(())
    }
}

/// An extension trait that lets you combine a [`tree_sitter::Tree`] with the source code that it
/// was parsed from.
pub trait WithSource {
    /// Combines a [`tree_sitter::Tree`] with the source code that it was parsed from.
    fn with_source<'a>(self, src: &'a [u8]) -> TreeWithSource<'a>;
}

/// The combination of a [`tree_sitter::Tree`] with the source code that it was parsed from.  This
/// type implements the [`mlua::IntoLua`] trait, so you can push it onto a Lua stack.
pub struct TreeWithSource<'a> {
    pub tree: Tree,
    pub src: &'a [u8],
}

impl WithSource for Tree {
    fn with_source<'a>(self, src: &'a [u8]) -> TreeWithSource<'a> {
        TreeWithSource {
            tree: self,
            src: src.as_ref(),
        }
    }
}

// We can implement this for any lifetime because Lua takes ownership of the tree, and will free it
// when the Lua wrapper is garbage-collected; and ltreesitter makes a copy of the source code.
impl mlua::IntoLua<'_> for TreeWithSource<'_> {
    fn into_lua(self, l: &Lua) -> Result<mlua::Value, mlua::Error> {
        unsafe extern "C-unwind" fn load_tree(l: *mut mlua::lua_State) -> i32 {
            extern "C-unwind" {
                fn ltreesitter_push_tree(
                    l: *mut mlua::lua_State,
                    t: *mut c_void,
                    src_len: usize,
                    src: *const c_char,
                );
            }
            let tree = mlua::ffi::lua_touserdata(l, 1);
            let src_len = mlua::ffi::lua_tointeger(l, 2);
            let src = mlua::ffi::lua_touserdata(l, 3);
            ltreesitter_push_tree(l, tree, src_len as usize, src as *const _);
            1
        }

        let tree =
            mlua::Value::LightUserData(mlua::LightUserData(self.tree.into_raw() as *mut c_void));
        let src_len = self.src.len();
        let src = mlua::Value::LightUserData(mlua::LightUserData(self.src.as_ptr() as *mut _));
        let load = unsafe { l.create_c_function(load_tree) }?;
        load.call((tree, src_len, src))
    }
}

// We can only implement this for the 'lua lifetime, to express that the returned Rust value is
// only valid while the Lua interpreter is live.
impl<'lua> mlua::FromLua<'lua> for TreeWithSource<'lua> {
    fn from_lua(value: mlua::Value<'lua>, lua: &'lua Lua) -> Result<Self, mlua::Error> {
        // Use some trickery to use ltreesitter's C accessor to get at the tree-sitter
        // Tree.  Return it back up to the "safe" mlua code as a light userdata.
        unsafe extern "C-unwind" fn get_tree(l: *mut mlua::lua_State) -> i32 {
            extern "C-unwind" {
                fn ltreesitter_check_tree_arg(l: *mut mlua::lua_State, index: u32) -> *mut c_void;
            }
            let ltreesitter_tree = ltreesitter_check_tree_arg(l, 1);
            mlua::ffi::lua_pushlightuserdata(l, ltreesitter_tree);
            1
        }

        #[repr(C)]
        struct LTreeSitterSourceText {
            length: usize,
            text: u8, // this is a VLA down in C
        }

        #[repr(C)]
        struct LTreeSitterTree {
            tree: *mut tree_sitter::ffi::TSTree,
            source: *const LTreeSitterSourceText,
        }

        let get_tree = unsafe { lua.create_c_function(get_tree) }?;
        let mlua::LightUserData(ltreesitter_tree) = get_tree.call(value)?;
        let ltreesitter_tree = ltreesitter_tree as *mut LTreeSitterTree;
        unsafe {
            let ltreesitter_source = (*ltreesitter_tree).source;
            let src = std::slice::from_raw_parts(
                &(*ltreesitter_source).text,
                (*ltreesitter_source).length,
            );
            let tree = (*ltreesitter_tree).tree;
            // The Rust tree-sitter bindings want to take ownership of the tree, so we need to make
            // a copy first.
            let tree = tree_sitter::ffi::ts_tree_copy(tree);
            let tree = tree_sitter::Tree::from_raw(tree);
            Ok(TreeWithSource { tree, src })
        }
    }
}

// A wrapper around a [`tree_sitter::Node`].  This only exists to get around Rust's orphan rules,
// so that we can implement the [`mlua::FromLua`] trait.
pub struct TSNode<'n>(pub tree_sitter::Node<'n>);

impl<'n> Deref for TSNode<'n> {
    type Target = tree_sitter::Node<'n>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'n> DerefMut for TSNode<'n> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

// We can only implement this for the 'lua lifetime, to express that the returned Rust value is
// only valid while the Lua interpreter is live.
impl<'lua> mlua::FromLua<'lua> for TSNode<'lua> {
    fn from_lua(value: mlua::Value<'lua>, lua: &'lua Lua) -> Result<Self, mlua::Error> {
        // Use some trickery to use ltreesitter's C accessor to get at the tree-sitter
        // Node.  Return it back up to the "safe" mlua code as a light userdata.
        unsafe extern "C-unwind" fn get_node(l: *mut mlua::lua_State) -> i32 {
            extern "C-unwind" {
                fn ltreesitter_check_node(l: *mut mlua::lua_State, index: u32) -> *mut c_void;
            }
            let ltreesitter_node = ltreesitter_check_node(l, 1);
            mlua::ffi::lua_pushlightuserdata(l, ltreesitter_node);
            1
        }

        #[repr(C)]
        struct LTreeSitterNode {
            node: tree_sitter::ffi::TSNode,
        }

        let get_node = unsafe { lua.create_c_function(get_node) }?;
        let mlua::LightUserData(ltreesitter_node) = get_node.call(value)?;
        let ltreesitter_node = ltreesitter_node as *mut LTreeSitterNode;
        Ok(TSNode(unsafe {
            tree_sitter::Node::from_raw((*ltreesitter_node).node)
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    trait CheckLua {
        fn call<'lua, R: mlua::FromLuaMulti<'lua>>(&'lua self, chunk: &str) -> R;
        fn check(&self, chunk: &str);
    }

    impl CheckLua for Lua {
        fn call<'lua, R: mlua::FromLuaMulti<'lua>>(&'lua self, chunk: &str) -> R {
            self.load(chunk).set_name("test chunk").call(()).unwrap()
        }

        fn check(&self, chunk: &str) {
            self.call(chunk)
        }
    }

    #[test]
    fn can_consume_parse_tree_from_lua() {
        let code = br#"
          def double(x):
              return x * 2
        "#;
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(tree_sitter_python::language()).unwrap();
        let parsed = parser.parse(code, None).unwrap();
        let l = Lua::new();
        l.open_ltreesitter().unwrap();
        l.globals().set("parsed", parsed.with_source(code)).unwrap();
        l.check(
            r#"
              local root = parsed:root()
              assert(root:type() == "module", "expected module as root of tree")
            "#,
        );
    }

    #[test]
    fn can_return_trees_back_to_rust() {
        let code = br#"
          def double(x):
              return x * 2
        "#;
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(tree_sitter_python::language()).unwrap();
        let parsed = parser.parse(code, None).unwrap();
        let l = Lua::new();
        l.open_ltreesitter().unwrap();
        l.globals().set("parsed", parsed.with_source(code)).unwrap();
        let tws: TreeWithSource = l.call(r#" return parsed "#);
        assert_eq!(code, tws.src);
        assert_eq!("module", tws.tree.root_node().kind());
    }

    #[test]
    fn can_return_nodes_back_to_rust() {
        let code = br#"
          def double(x):
              return x * 2
        "#;
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(tree_sitter_python::language()).unwrap();
        let parsed = parser.parse(code, None).unwrap();
        let l = Lua::new();
        l.open_ltreesitter().unwrap();
        l.globals().set("parsed", parsed.with_source(code)).unwrap();
        let root: TSNode = l.call(r#" return parsed:root() "#);
        assert_eq!("module", root.kind());
    }
}
