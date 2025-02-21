use full_moon::{
    ast::{
        span::ContainedSpan, Call, FunctionArgs, FunctionCall, FunctionDeclaration, Index,
        LastStmt, Parameter, Prefix, Return, Suffix,
    },
    tokenizer::{Token, TokenReference, TokenType},
    visitors::VisitorMut,
    ShortString,
};

use crate::{ast_utilis, lang_utils, transform_lua_code};

trait StringLuaCommentRemove {
    fn remove_lua_comments(&self) -> String;
}

impl StringLuaCommentRemove for String {
    fn remove_lua_comments(&self) -> String {
        lang_utils::remove_lua_comments(self.as_str())
    }
}

pub struct LuaTransformer {
    pub file_path: Option<String>,
    pub input_param_list: Option<Vec<(String, String)>>,
}

struct LuaLastReturnRemover;

impl LuaLastReturnRemover {
    fn new() -> Self {
        LuaLastReturnRemover {}
    }
}

impl VisitorMut for LuaLastReturnRemover {
    fn visit_last_stmt(&mut self, node: LastStmt) -> LastStmt {
        let new_node = match &node {
            LastStmt::Return(ret) => {
                LastStmt::Return(Return::new().with_token(ast_utilis::empty_token(ret.token())))
            }
            _ => panic!("Cannot find `return`!"),
        };

        new_node
    }
}

impl LuaTransformer {
    pub fn new() -> LuaTransformer {
        LuaTransformer {
            file_path: None,
            input_param_list: None,
        }
    }
}

impl LuaTransformer {
    #[inline]
    fn load_param_list_into_lua_env(&self) {
        if let Some(input_param_list) = &self.input_param_list {
            let mut code = String::new();
            for (key, value) in input_param_list {
                if value == "true" || value == "1" {
                    code = code + "rawset(_G, \"" + key + "\", true)\n";
                } else if value == "false" || value == "0" {
                    code = code + "rawset(_G, \"" + key + "\", false)\n";
                } else {
                    panic!(
                        "[load_param_list_into_env] invalid value: {}, key: {}",
                        value, key
                    )
                }
            }
            code = code + "return \"\"";
            lang_utils::lua_dostring("[load_param_list_into_lua_env]", &code);
        }
    }

    #[inline]
    fn unload_param_list_from_lua_env(&self) {
        if let Some(input_param_list) = &self.input_param_list {
            let mut code = String::new();
            for (key, _) in input_param_list {
                code = code + "rawset(_G, \"" + key + "\", nil)\n";
            }
            code = code + "return \"\"";
            lang_utils::lua_dostring("[unload_param_list_from_lua_env]", &code);
        }
    }

    fn resolve_comp_time(&self, node: FunctionDeclaration) -> FunctionDeclaration {
        assert!(
            node.body().block().last_stmt().is_some(),
            "Last statement(Return) should not be None"
        );

        // let old_block = node.body().block().to_owned();
        // let old_last_stmt = old_block.last_stmt().cloned().unwrap();
        // let new_return = match old_last_stmt.clone() {
        //     LastStmt::Return(return_node) => return_node
        //         .clone()
        //         .with_token(insert_before_token(return_node.clone().token(), "--[==[ "))
        //         .with_returns(insert_after_punc_expr(
        //             &return_node.returns().to_owned(),
        //             " --]==] ",
        //         )),
        //     _ => panic!(),
        // };

        // let new_stmts: Vec<(Stmt, Option<TokenReference>)> = old_block
        //     .stmts()
        //     .map(|s| {
        //         let t = match s.clone() {
        //             Stmt::LocalAssignment(local_assignment) => {
        //                 let expr_list =
        //                     insert_after_punc_expr(local_assignment.expressions(), " --]==]");
        //                 Stmt::LocalAssignment(
        //                     local_assignment
        //                         .clone()
        //                         .with_local_token(insert_before_token(
        //                             local_assignment.local_token(),
        //                             "--[==[ ",
        //                         ))
        //                         .with_expressions(expr_list),
        //                 )
        //             }

        //             Stmt::Assignment(assignment) => {
        //                 let var_list = insert_before_punc_var(&assignment.variables(), " --[==[ ");
        //                 let expr_list = insert_after_punc_expr(assignment.expressions(), " --]==]");
        //                 Stmt::Assignment(
        //                     assignment
        //                         .with_variables(var_list)
        //                         .with_expressions(expr_list),
        //                 )
        //             }

        //             Stmt::NumericFor(numeric_for) => {
        //                 let new_for_token = insert_before_token(numeric_for.for_token(), "--[==[ ");
        //                 let new_end_token = insert_after_token(numeric_for.end_token(), " --]==]");
        //                 Stmt::NumericFor(
        //                     numeric_for
        //                         .clone()
        //                         .with_for_token(new_for_token)
        //                         .with_end_token(new_end_token),
        //                 )
        //             }

        //             Stmt::GenericFor(generic_for) => {
        //                 let new_for_token = insert_before_token(generic_for.for_token(), "--[==[ ");
        //                 let new_end_token = insert_after_token(generic_for.end_token(), " --]==]");
        //                 Stmt::GenericFor(
        //                     generic_for
        //                         .clone()
        //                         .with_for_token(new_for_token)
        //                         .with_end_token(new_end_token),
        //                 )
        //             }

        //             Stmt::Do(do_stmt) => {
        //                 let new_do_token = insert_before_token(do_stmt.do_token(), "--[==[ ");
        //                 let new_end_token = insert_after_token(do_stmt.end_token(), " --]==]");
        //                 Stmt::Do(
        //                     do_stmt
        //                         .clone()
        //                         .with_do_token(new_do_token)
        //                         .with_end_token(new_end_token),
        //                 )
        //             }

        //             Stmt::FunctionCall(func_call) => {
        //                 let new_prefix = match func_call.prefix() {
        //                     Prefix::Name(token) => Prefix::Name(insert_before_token(token, "-- ")),
        //                     Prefix::Expression(expr) => Prefix::Expression(Box::new(
        //                         insert_before_expr(&expr.to_owned(), " --"),
        //                     )),
        //                     _ => panic!("{:?}", func_call.prefix()),
        //                 };
        //                 Stmt::FunctionCall(func_call.with_prefix(new_prefix))
        //             }

        //             Stmt::If(if_stmt) => Stmt::If(
        //                 if_stmt
        //                     .clone()
        //                     .with_if_token(insert_before_token(if_stmt.if_token(), "--[==[ "))
        //                     .with_end_token(insert_after_token(if_stmt.end_token(), " --]==]")),
        //             ),

        //             _ => panic!("{:?}", s),
        //         };
        //         (t, None)
        //     })
        //     .collect();
        // let new_block = old_block
        //     .with_last_stmt(Some((LastStmt::Return(new_return), None)))
        //     .with_stmts(new_stmts);

        // Remove parameters
        let mut parameter_vec: Vec<String> = Vec::new();
        let mut old_parameter_name_token: Option<TokenReference> = None;
        node.body().parameters().pairs().for_each(|param| {
            let param = param.value();
            match param.clone() {
                Parameter::Ellipsis(ellipsis) => {
                    panic!("Unexpected Ellipsis {}", ellipsis);
                }
                Parameter::Name(name) => {
                    old_parameter_name_token = Some(name.clone());
                    parameter_vec.push(name.to_string());
                }
                _ => panic!("{:?}", param),
            }
        });
        assert!(
            parameter_vec.len() <= 1,
            "More than 1 parameters, got {} => \"{}\"",
            parameter_vec.len(),
            parameter_vec.join(", ")
        );

        // let new_func_body = if parameter_vec.len() == 0 {
        //     node.body()
        //         .clone()
        //         .with_end_token(empty_token(node.clone().body().end_token()))
        //         .with_parameters_parentheses(empty_contained_span(
        //             node.body().parameters_parentheses(),
        //         ))
        //         .with_block(new_block)
        // } else {
        //     let mut new_parameters: Punctuated<Parameter> = Punctuated::new();
        //     new_parameters.push(Pair::new(
        //         Parameter::Name(empty_token(&old_parameter_name_token.clone().unwrap())),
        //         None,
        //     ));
        //     node.body()
        //         .clone()
        //         .with_end_token(empty_token(node.clone().body().end_token()))
        //         .with_parameters_parentheses(empty_contained_span(
        //             node.body().parameters_parentheses(),
        //         ))
        //         .with_parameters(new_parameters)
        //         .with_block(new_block)
        // };

        let new_func_body = node
            .body()
            .clone()
            .with_end_token(ast_utilis::insert_after_token(
                node.body().end_token(),
                " --]=====]",
            ));

        let empty_token_ref = TokenReference::new(
            vec![],
            Token::new(TokenType::Whitespace {
                characters: ShortString::new("[Anonymous]"),
            }),
            vec![],
        );
        let parameter_name = old_parameter_name_token
            .unwrap_or(empty_token_ref)
            .to_string();

        let comp_time_ret = {
            // Make parameter list available to Lua at the compile time context.
            self.load_param_list_into_lua_env();

            let ret = lang_utils::lua_dostring(
                &(self.file_path.clone().unwrap_or_default() + " " + parameter_name.as_str()),
                node.body().block().to_string().as_str(),
            )
            .remove_lua_comments()
            .replace("\n", " "); // The generated code should not have any newlines and comments.

            self.unload_param_list_from_lua_env();

            ret
        };

        // let ret = node
        //     .clone()
        //     .with_function_token(insert_before_token(
        //         &empty_token(&node.clone().function_token()),
        //         comp_time_ret.as_str(),
        //     ))
        //     .with_name(FunctionName::new(create_punc(&create_empty_token_ref())))
        //     .with_body(new_func_body);

        let ret = node
            .clone()
            .with_function_token(ast_utilis::insert_before_token(
                &ast_utilis::insert_before_token(
                    &node.clone().function_token(),
                    comp_time_ret.as_str(),
                ),
                " --[=====[ ",
            ))
            .with_body(new_func_body);

        ret
    }
}

impl VisitorMut for LuaTransformer {
    fn visit_function_declaration(&mut self, node: FunctionDeclaration) -> FunctionDeclaration {
        match node.name().to_string().to_uppercase().as_str() {
            "__LJP:COMP_TIME" | "_G.__LJP:COMP_TIME" => self.resolve_comp_time(node),
            _ => {
                if node.name().to_string().contains("__LJP:COMP_TIME")
                    || node.name().to_string().contains("_G.__LJP:COMP_TIME")
                {
                    panic!("Function name for the `__LJP:COMP_TIME` should be the same line as the `function` token.")
                }
                node
            }
        }
    }

    fn visit_function_call(&mut self, node: FunctionCall) -> FunctionCall {
        let func_name = match node.prefix() {
            Prefix::Name(name) => Some(name.token().to_string().to_uppercase()),
            _ => None,
        };

        if func_name.is_none() {
            return node;
        }

        let func_name = func_name.unwrap();
        let mut full_func_name = String::from("");
        let mut func_arg = String::from("");

        let suffix_vec: Vec<Suffix> = node.suffixes().cloned().collect();

        if func_name.starts_with("__LJP") {
            (full_func_name, func_arg) = ast_utilis::get_func_call_name(&node)
        } else if func_name.starts_with("_G") {
            if suffix_vec.len() > 0 {
                match suffix_vec[0].clone() {
                    Suffix::Index(index) => match index {
                        Index::Dot { dot: _, name } => {
                            if name.token().to_string().to_uppercase() == "__LJP" {
                                (full_func_name, func_arg) = ast_utilis::get_func_call_name(&node);
                            }
                        }
                        _ => panic!(),
                    },
                    _ => panic!(),
                }
            }
        };

        if full_func_name == "" || func_arg == "" {
            return node;
        } else if matches!(
            full_func_name.to_uppercase().as_str(),
            "__LJP:INCLUDE"
                | "_G.__LJP:INCLUDE"
                | "__LJP:INCLUDE_NO_RETURN"
                | "_G.__LJP:INCLUDE_NO_RETURN"
        ) {
            let new_prefix = {
                let include_file = lang_utils::lua_dostring(
                    "__LJP:INCLUDE",
                    &format!(
                        "return assert(package.searchpath({}, package.path))",
                        func_arg
                    ),
                );
                let mut include_code = std::fs::read_to_string(include_file.clone())
                    .expect(format!("Failed to read file => {}", include_file).as_str());
                if let Some(first_line) = include_code.lines().next() {
                    if first_line.contains("luajit-pro") {
                        // Recursively transform the included code
                        include_code = transform_lua_code(&include_code, &include_file, None);
                    }
                }

                if full_func_name.to_uppercase().contains("NO_RETURN") {
                    let ast = full_moon::parse(&include_code).unwrap();
                    let mut return_remover = LuaLastReturnRemover::new();
                    include_code = return_remover.visit_ast(ast).to_string();
                }

                include_code = include_code.remove_lua_comments().replace("\n", " ");

                match node.prefix() {
                    Prefix::Name(token) => Prefix::Name(ast_utilis::insert_before_token(
                        &ast_utilis::insert_before_token(&token, include_code.as_str()),
                        " --[=====[ ",
                    )),
                    _ => panic!("Unexpected Prefix {:?}", node.prefix()),
                }
            };

            let new_suffix = suffix_vec
                .iter()
                .map(|suffix| match suffix.clone() {
                    Suffix::Index(index) => Suffix::Index(match index {
                        Index::Dot { dot, name } => Index::Dot {
                            dot: dot,
                            name: name,
                        },
                        _ => panic!("Unexpected Index {:?}", index),
                    }),
                    Suffix::Call(call) => Suffix::Call(match call {
                        Call::MethodCall(method_call) => {
                            let new_args = match method_call.args().clone() {
                                FunctionArgs::Parentheses {
                                    parentheses,
                                    arguments,
                                } => FunctionArgs::Parentheses {
                                    parentheses: ContainedSpan::new(
                                        parentheses.tokens().0.clone(),
                                        ast_utilis::insert_after_token(
                                            parentheses.tokens().1,
                                            " --]=====]",
                                        ),
                                    ),
                                    arguments: arguments,
                                },
                                FunctionArgs::String(str) => FunctionArgs::String(
                                    ast_utilis::insert_after_token(&str, " --]=====]"),
                                ),
                                _ => panic!(
                                    "Unexpected Call {}",
                                    method_call.args().clone().to_string()
                                ),
                            };
                            Call::MethodCall(method_call.with_args(new_args))
                        }
                        _ => panic!("{:?}", call),
                    }),
                    _ => panic!(),
                })
                .collect();

            return node.with_prefix(new_prefix).with_suffixes(new_suffix);
        } else {
            return node;
        }
    }
}
