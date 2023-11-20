// -*- coding: utf-8 -*-
// ------------------------------------------------------------------------------------------------
// Copyright © 2023, Douglas Creager.
// Please see the LICENSE file in this distribution for license details.
// ------------------------------------------------------------------------------------------------

use std::ffi::c_char;
use std::ffi::c_void;

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

// Replace this with a call to Tree::into_raw once a >0.28.8 release is cut.
fn tree_into_raw(tree: Tree) -> *mut c_void {
    // The Lua wrapper will take ownership of the tree.
    let tree = std::mem::ManuallyDrop::new(tree);
    // Pull some shenanigans to access the tree's TSTree pointer.
    type RawTree = std::ptr::NonNull<c_void>;
    let raw_tree: RawTree = unsafe { std::mem::transmute(tree) };
    raw_tree.as_ptr()
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
    pub tree: tree_sitter::Tree,
    pub src: &'a [u8],
}

impl WithSource for tree_sitter::Tree {
    fn with_source<'a>(self, src: &'a [u8]) -> TreeWithSource<'a> {
        TreeWithSource {
            tree: self,
            src: src.as_ref(),
        }
    }
}

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
            mlua::Value::LightUserData(mlua::LightUserData(tree_into_raw(self.tree.clone())));
        let src_len = self.src.len();
        let src = mlua::Value::LightUserData(mlua::LightUserData(self.src.as_ptr() as *mut _));
        let load = unsafe { l.create_c_function(load_tree) }?;
        load.call((tree, src_len, src))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    trait CheckLua {
        fn check(&self, chunk: &str);
    }

    impl CheckLua for Lua {
        fn check(&self, chunk: &str) {
            self.load(chunk).set_name("test chunk").exec().unwrap()
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
}
