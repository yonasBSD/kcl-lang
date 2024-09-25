use std::sync::Arc;

use crate::builtin::system_module::{get_system_module_members, UNITS, UNITS_NUMBER_MULTIPLIER};
use crate::builtin::{get_system_member_function_ty, STRING_MEMBER_FUNCTIONS};
use crate::resolver::Resolver;
use crate::ty::TypeKind::Schema;
use crate::ty::{
    DictType, ModuleKind, Parameter, Type, TypeKind, TypeRef, SCHEMA_MEMBER_FUNCTIONS,
};
use kclvm_ast::ast;
use kclvm_error::diagnostic::{dummy_range, Range};
use kclvm_error::*;

use super::node::ResolvedResult;

impl<'ctx> Resolver<'ctx> {
    pub fn check_attr_ty(&mut self, attr_ty: &Type, range: Range) {
        if !attr_ty.is_any() && !attr_ty.is_key() {
            self.handler.add_error(
                ErrorKind::IllegalAttributeError,
                &[Message {
                    range,
                    style: Style::LineAndColumn,
                    message: format!(
                        "A attribute must be string type, got '{}'",
                        attr_ty.ty_str()
                    ),
                    note: None,
                    suggested_replacement: None,
                }],
            );
        }
    }

    pub fn load_attr(&mut self, obj: TypeRef, attr: &str, range: Range) -> ResolvedResult {
        let (result, return_ty) = match &obj.kind {
            TypeKind::Any => (true, self.any_ty()),
            TypeKind::None
            | TypeKind::Bool
            | TypeKind::BoolLit(_)
            | TypeKind::Int
            | TypeKind::IntLit(_)
            | TypeKind::Float
            | TypeKind::FloatLit(_)
            | TypeKind::List(_)
            | TypeKind::NumberMultiplier(_)
            | TypeKind::Function(_)
            | TypeKind::Named(_)
            | TypeKind::Void => (false, self.any_ty()),
            TypeKind::Str | TypeKind::StrLit(_) => match STRING_MEMBER_FUNCTIONS.get(attr) {
                Some(ty) => (true, Arc::new(ty.clone())),
                None => (false, self.any_ty()),
            },
            TypeKind::Dict(DictType {
                key_ty: _,
                val_ty,
                attrs,
            }) => (
                true,
                attrs
                    .get(attr)
                    .map(|attr| attr.ty.clone())
                    .unwrap_or(Arc::new(val_ty.as_ref().clone())),
            ),
            // union type load attr based the type guard. e.g, a: str|int; if a is str: xxx; if a is int: xxx;
            // return sup([self.load_attr_type(t, attr, filename, line, column) for t in obj.types])
            TypeKind::Union(_) => (true, self.any_ty()),
            TypeKind::Schema(schema_ty) => {
                let (result, schema_attr_ty) = self.schema_load_attr(schema_ty, attr);
                if result {
                    (result, schema_attr_ty)
                } else if schema_ty.is_member_functions(attr) {
                    (
                        true,
                        Arc::new(Type::function(
                            Some(obj.clone()),
                            Type::list_ref(self.any_ty()),
                            &[Parameter {
                                name: "full_pkg".to_string(),
                                ty: Type::bool_ref(),
                                // Default value is False
                                has_default: true,
                                default_value: None,
                                range: dummy_range(),
                            }],
                            "",
                            false,
                            None,
                        )),
                    )
                } else {
                    (false, self.any_ty())
                }
            }
            TypeKind::Module(module_ty) => {
                match &module_ty.kind {
                    crate::ty::ModuleKind::User => match self.scope_map.get(&module_ty.pkgpath) {
                        Some(scope) => match scope.borrow().elems.get(attr) {
                            Some(v) => {
                                if v.borrow().ty.is_module() {
                                    self.handler
                                            .add_compile_error(&format!("can not import the attribute '{}' from the module '{}'", attr, module_ty.pkgpath), range.clone());
                                }
                                (true, v.borrow().ty.clone())
                            }
                            None => (false, self.any_ty()),
                        },
                        None => (false, self.any_ty()),
                    },
                    ModuleKind::System => {
                        if module_ty.pkgpath == UNITS && attr == UNITS_NUMBER_MULTIPLIER {
                            (true, Arc::new(Type::number_multiplier_non_lit_ty()))
                        } else {
                            let members = get_system_module_members(&module_ty.pkgpath);
                            (
                                members.contains(&attr),
                                get_system_member_function_ty(&module_ty.pkgpath, attr),
                            )
                        }
                    }
                    ModuleKind::Plugin => (true, self.any_ty()),
                }
            }
        };

        if !result {
            // The attr user input.
            let (attr, suggestion) = if attr.is_empty() {
                ("[missing name]", "".to_string())
            } else {
                let mut suggestion = String::new();
                // Calculate the closest miss attributes.
                if let Schema(schema_ty) = &obj.kind {
                    // Get all the attributes of the schema.
                    let attrs = if schema_ty.is_instance {
                        schema_ty.attrs.keys().cloned().collect::<Vec<String>>()
                    } else {
                        SCHEMA_MEMBER_FUNCTIONS
                            .iter()
                            .map(|s| s.to_string())
                            .collect::<Vec<String>>()
                    };
                    let suggs = suggestions::provide_suggestions(attr, &attrs);
                    if suggs.len() > 0 {
                        suggestion = format!(", did you mean '{:?}'?", suggs);
                    }
                }
                (attr, suggestion)
            };

            self.handler.add_type_error(
                &format!(
                    "attribute '{}' not found in '{}'{}",
                    attr,
                    obj.ty_str(),
                    suggestion
                ),
                range,
            );
        }
        return_ty
    }

    pub fn subscript_index(
        &mut self,
        value_ty: TypeRef,
        index: &'ctx ast::NodeRef<ast::Expr>,
        range: Range,
    ) -> ResolvedResult {
        if value_ty.is_any() {
            value_ty
        } else {
            match &value_ty.kind {
                TypeKind::Str | TypeKind::StrLit(_) | TypeKind::List(_) => {
                    self.must_be_type(index, self.int_ty());
                    if value_ty.is_list() {
                        value_ty.list_item_ty()
                    } else {
                        self.str_ty()
                    }
                }
                TypeKind::Dict(DictType {
                    key_ty: _, val_ty, ..
                }) => {
                    let index_key_ty = self.expr(index);
                    if index_key_ty.is_none_or_any() {
                        val_ty.clone()
                    } else if !index_key_ty.is_key() {
                        self.handler.add_compile_error(
                            &format!("invalid dict/schema key type: '{}'", index_key_ty.ty_str()),
                            range,
                        );
                        self.any_ty()
                    } else if let TypeKind::StrLit(lit_value) = &index_key_ty.kind {
                        self.load_attr(value_ty, lit_value, range)
                    } else {
                        val_ty.clone()
                    }
                }
                TypeKind::Schema(schema_ty) => {
                    let index_key_ty = self.expr(index);
                    if index_key_ty.is_none_or_any() {
                        schema_ty.val_ty()
                    } else if !index_key_ty.is_key() {
                        self.handler.add_compile_error(
                            &format!("invalid dict/schema key type: '{}'", index_key_ty.ty_str()),
                            range,
                        );
                        self.any_ty()
                    } else if let TypeKind::StrLit(lit_value) = &index_key_ty.kind {
                        self.load_attr(value_ty, lit_value, range)
                    } else {
                        schema_ty.val_ty()
                    }
                }
                _ => {
                    self.handler.add_compile_error(
                        &format!("'{}' object is not subscriptable", value_ty.ty_str()),
                        range,
                    );
                    self.any_ty()
                }
            }
        }
    }

    pub fn subscript(
        &mut self,
        value_ty: TypeRef,
        index: &'ctx Option<ast::NodeRef<ast::Expr>>,
        lower: &'ctx Option<ast::NodeRef<ast::Expr>>,
        upper: &'ctx Option<ast::NodeRef<ast::Expr>>,
        step: &'ctx Option<ast::NodeRef<ast::Expr>>,
        range: Range,
    ) -> ResolvedResult {
        if value_ty.is_any() {
            value_ty
        } else {
            match &value_ty.kind {
                TypeKind::Str | TypeKind::StrLit(_) | TypeKind::List(_) => {
                    if let Some(index) = &index {
                        self.subscript_index(value_ty, index, range)
                    } else {
                        for expr in [lower, upper, step].iter().copied().flatten() {
                            self.must_be_type(expr, self.int_ty());
                        }
                        if value_ty.is_list() {
                            value_ty
                        } else {
                            self.str_ty()
                        }
                    }
                }
                TypeKind::Dict(DictType { key_ty: _, .. }) => {
                    if let Some(index) = &index {
                        self.subscript_index(value_ty, index, range)
                    } else {
                        self.handler
                            .add_compile_error("unhashable type: 'slice'", range);
                        self.any_ty()
                    }
                }
                TypeKind::Schema(_) => {
                    if let Some(index) = &index {
                        self.subscript_index(value_ty, index, range)
                    } else {
                        self.handler
                            .add_compile_error("unhashable type: 'slice'", range);
                        self.any_ty()
                    }
                }
                _ => {
                    self.handler.add_compile_error(
                        &format!("'{}' object is not subscriptable", value_ty.ty_str()),
                        range,
                    );
                    self.any_ty()
                }
            }
        }
    }
}
