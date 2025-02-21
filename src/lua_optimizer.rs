use std::collections::HashMap;

use full_moon::{
    ast::{Expression, Field, Index, Prefix, Stmt, Suffix, Var},
    tokenizer::TokenType,
    visitors::VisitorMut,
};

use crate::ast_utilis;

pub struct LuaOptimizer {
    pub enum_map: Option<HashMap<String, HashMap<String, String>>>,
}

impl LuaOptimizer {
    pub fn new() -> LuaOptimizer {
        LuaOptimizer { enum_map: None }
    }
}

impl VisitorMut for LuaOptimizer {
    fn visit_stmt(&mut self, node: Stmt) -> Stmt {
        match node.clone() {
            Stmt::LocalAssignment(local_assignment) => {
                let local_tk = local_assignment.local_token();
                let mut has_annotation = false;
                local_tk.trailing_trivia().into_iter().for_each(|trivia| {
                    match trivia.token_type() {
                        TokenType::MultiLineComment { blocks, comment } => {
                            if blocks == &0 && comment.as_str() == "@comp_time_enum" {
                                has_annotation = true;
                            }
                        }
                        _ => {}
                    };
                });

                if has_annotation {
                    let name_vec: Vec<String> = local_assignment
                        .names()
                        .pairs()
                        .map(|n| {
                            let name_tk = n.clone().into_value();
                            name_tk.token().to_string()
                        })
                        .collect();
                    if name_vec.len() == 1 {
                        let enum_name = name_vec.first().unwrap();
                        assert!(
                            local_assignment.expressions().pairs().count() == 1,
                            "[@comp_time_enum] Only support one expression!"
                        );
                        let mut key_value_map = HashMap::new();
                        local_assignment.expressions().pairs().for_each(|e| {
                            let expr = e.clone().into_value();
                            match expr {
                                Expression::TableConstructor(tbl_constructor) => {
                                    tbl_constructor
                                        .fields()
                                        .iter()
                                        .for_each(|field| match field {
                                            Field::NameKey {
                                                key,
                                                equal: _,
                                                value,
                                            } => {
                                                let v = match value {
                                                    Expression::Number(number) => {
                                                        Some(number.token().to_string())
                                                    }
                                                    Expression::String(str) => {
                                                        Some(str.token().to_string())
                                                    }
                                                    _ => {
                                                        println!("[@comp_time_enum] [warn] Unexpected Expression: <{}>, enum_name: {}", value.to_string(), enum_name);
                                                        None
                                                    }
                                                };
                                                if v.is_some() {
                                                    key_value_map.insert(key.token().to_string(), v.unwrap());
                                                }
                                            }
                                            Field::ExpressionKey {
                                                brackets: _,
                                                key: _,
                                                equal: _,
                                                value: _,
                                            } => {
                                                todo!("{}", field.to_string())
                                            }
                                            _ => {}
                                        });
                                }
                                _ => panic!("[@comp_time_enum] Unexpected Expression: {:?}", expr),
                            }
                        });
                        if self.enum_map.is_none() {
                            self.enum_map = Some(HashMap::new());
                            println!("enum: {} => <{:?}>", enum_name, key_value_map);
                            self.enum_map
                                .as_mut()
                                .unwrap()
                                .insert(enum_name.clone(), key_value_map);
                        } else {
                            println!("enum: {} => <{:?}>", enum_name, key_value_map);
                            self.enum_map
                                .as_mut()
                                .unwrap()
                                .insert(enum_name.clone(), key_value_map);
                        }
                    } else {
                        panic!("[@comp_time_enum] Should only have one enum name!");
                    }
                }

                node
            }
            _ => node,
        }
    }

    fn visit_var(&mut self, node: Var) -> Var {
        match &node {
            Var::Expression(var_expr) => {
                // Replace enum expression with its value
                if self.enum_map.is_none() {
                    return node;
                }

                let name = match &var_expr.prefix() {
                    Prefix::Name(name) => Some(name.token().to_string()),
                    _ => None,
                };

                if name.is_none() {
                    return node;
                }

                let mut new_suffixes: Vec<Suffix> = Vec::new();
                let mut enum_value = String::new();
                let mut enum_expr = String::new();
                if let Some(enum_key) = name {
                    if let Some(enum_map) = self.enum_map.as_ref().unwrap().get(&enum_key) {
                        let suffixes: Vec<&Suffix> = var_expr.suffixes().into_iter().collect();
                        if suffixes.len() == 1 {
                            let suffix = suffixes.first().cloned().unwrap();
                            match suffix {
                                Suffix::Index(index) => match index {
                                    Index::Brackets {
                                        brackets: _,
                                        expression: _,
                                    } => {
                                        todo!()
                                    }
                                    Index::Dot { dot, name } => {
                                        if let Some(v) = enum_map.get(&name.token().to_string()) {
                                            enum_value = v.clone();
                                            enum_expr =
                                                enum_key + "." + name.token().to_string().as_str();
                                            new_suffixes.push(Suffix::Index(Index::Dot {
                                                dot: ast_utilis::empty_token(dot),
                                                name: ast_utilis::empty_token(name),
                                            }));
                                        }
                                    }
                                    _ => {}
                                },
                                _ => {}
                            }
                        }
                    } else {
                        return node;
                    }
                }
                let has_valid_enum_expr = new_suffixes.len() > 0;
                if has_valid_enum_expr {
                    let new_prefix: Prefix = if let Prefix::Name(name) = var_expr.prefix() {
                        Prefix::Name(ast_utilis::replace_token(
                            name,
                            format!("{enum_value} --[[{enum_expr}]]").as_str(),
                        ))
                    } else {
                        unreachable!()
                    };
                    Var::Expression(Box::new(
                        var_expr
                            .clone()
                            .with_suffixes(new_suffixes)
                            .with_prefix(new_prefix),
                    ))
                } else {
                    node
                }
            }
            _ => node,
        }
    }
}
