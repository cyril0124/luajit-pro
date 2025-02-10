use std::ffi::{c_char, CStr, CString};
use std::path::Path;
use std::time::{Duration, Instant};
use std::{cell::RefCell, env, fs::File, io::Write, str, vec};

use darklua_core::generator::LuaGenerator;
use darklua_core::rules::Rule;
use full_moon::ast::{Call, FunctionArgs, Parameter};
use full_moon::{
    ast::{
        punctuated::{Pair, Punctuated},
        span::ContainedSpan,
        Expression, FunctionCall, FunctionDeclaration, FunctionName, Index, LastStmt, Prefix, Stmt,
        Suffix, Var, VarExpression,
    },
    tokenizer::{Token, TokenReference, TokenType},
    visitors::VisitorMut,
    ShortString,
};
use mlua::prelude::*;

use once_cell::sync::Lazy;

static LJP_KEEP_FILE: Lazy<String> =
    Lazy::new(|| env::var("LJP_KEEP_FILE").unwrap_or_else(|_| String::from("0")));

fn lua_dostring(code_name: &str, code: &str) -> String {
    thread_local! {
        static LUA: RefCell<Lua> = RefCell::new(unsafe {
            let lua = Lua::unsafe_new();
            lua.load(r#"
    _G.__code_name__ = "[Anonymous]"
    local purple = "\27[35m"
    local reset = "\27[0m"
    local old_print = print
    package.path = package.path .. ";?.lua"
    
    function print(...)
        old_print(purple .. "[comp_time] " .. _G.__code_name__ .. reset, ...)
    end
    
    function printf(...)
        io.write(purple .. "[comp_time] " .. _G.__code_name__ .. reset .. "\t" .. string.format(...))
    end
    
    env_vars = {}
    setmetatable(env_vars, {
        __index = function(table, key)
            local value = os.getenv(key)
            if value == nil then
                printf("[warn] env_vars[%s] is nil!\n", key)
            end
            return os.getenv(key)
        end
    })
    
    getmetatable('').__index.render = function(template, vars)
        assert(type(template) == "string", "template must be a string")
        assert(type(vars) == "table", "vars must be a table")
        return (template:gsub("{{(.-)}}", function(key)
            assert(vars[key], string.format("[render] key not found: %s\n\ttemplate_str is: %s\n", key, template))
            return tostring(vars[key] or "")
        end))
    end
    
    getmetatable('').__index.strip = function(str, suffix)
        assert(type(suffix) == "string", "suffix must be a string")
        if str:sub(-#suffix) == suffix then
            return str:sub(1, -#suffix - 1)
        else
            return str
        end
    end
                "#).exec().unwrap();
            lua
        });
    }
    LUA.with(|lua| {
        lua.borrow()
            .globals()
            .set("__code_name__", code_name)
            .expect("Failed to set __code_name__");
        lua.borrow()
            .load(code)
            .eval::<String>()
            .expect(format!("Failed to eval code => \n----------\n{}\n----------\n", code).as_str())
    })
}
fn empty_token(token_ref: &TokenReference) -> TokenReference {
    token_ref.with_token(Token::new(TokenType::Whitespace {
        characters: ShortString::new(""),
    }))
}

fn empty_contained_span(contained_span: &ContainedSpan) -> ContainedSpan {
    ContainedSpan::new(
        empty_token(contained_span.tokens().0),
        empty_token(contained_span.tokens().1),
    )
}

fn insert_after_token(token_ref: &TokenReference, text: &str) -> TokenReference {
    TokenReference::new(
        token_ref.leading_trivia().cloned().collect(),
        token_ref.token().clone(),
        vec![
            vec![Token::new(TokenType::Whitespace {
                characters: ShortString::new(text),
            })],
            token_ref.trailing_trivia().cloned().collect(),
        ]
        .concat(),
    )
}

fn insert_before_token(token_ref: &TokenReference, text: &str) -> TokenReference {
    TokenReference::new(
        vec![
            token_ref.leading_trivia().cloned().collect(),
            vec![Token::new(TokenType::Whitespace {
                characters: ShortString::new(text),
            })],
        ]
        .concat(),
        token_ref.token().clone(),
        token_ref.trailing_trivia().cloned().collect(),
    )
}

fn surround_token(token_ref: &TokenReference, text_left: &str, text_right: &str) -> TokenReference {
    TokenReference::new(
        vec![
            token_ref.leading_trivia().cloned().collect(),
            vec![Token::new(TokenType::Whitespace {
                characters: ShortString::new(text_left),
            })],
        ]
        .concat(),
        token_ref.token().clone(),
        vec![
            vec![Token::new(TokenType::Whitespace {
                characters: ShortString::new(text_right),
            })],
            token_ref.trailing_trivia().cloned().collect(),
        ]
        .concat(),
    )
}

fn insert_after_var_expr(var_expr: &VarExpression, text: &str) -> VarExpression {
    let mut new_suffixes: Vec<Suffix> = Vec::new();
    var_expr.suffixes().cloned().for_each(|suffix| {
        let new_suffix = match suffix.clone() {
            Suffix::Call(_call) => {
                todo!()
            }
            Suffix::Index(index) => Suffix::Index(match index {
                Index::Brackets {
                    brackets,
                    expression,
                } => Index::Brackets {
                    brackets: ContainedSpan::new(
                        brackets.tokens().0.clone(),
                        insert_after_token(brackets.tokens().1, text),
                    ),
                    expression: expression,
                },
                Index::Dot { dot, name } => Index::Dot {
                    dot: dot,
                    name: insert_after_token(&name, text),
                },
                _ => panic!(),
            }),
            _ => panic!(),
        };
        new_suffixes.push(new_suffix);
    });

    var_expr.clone().with_suffixes(new_suffixes)
}

fn insert_after_expr(expr: &Expression, text: &str) -> Expression {
    match expr.clone() {
        Expression::Number(number) => Expression::Number(insert_after_token(&number, text)),
        Expression::BinaryOperator { lhs, binop, rhs } => Expression::BinaryOperator {
            lhs: Box::new(*lhs.to_owned()),
            binop,
            rhs: Box::new(insert_after_expr(&*rhs.to_owned(), text)),
        },
        Expression::String(str) => Expression::String(insert_after_token(&str, text)),
        Expression::Var(var) => Expression::Var(match var {
            Var::Expression(var_expr) => {
                Var::Expression(Box::new(insert_after_var_expr(&var_expr, text)))
            }
            Var::Name(var_name) => Var::Name(insert_after_token(&var_name, text)),
            _ => panic!("{:?}", var),
        }),
        Expression::TableConstructor(table_constructor) => {
            let new_braces = ContainedSpan::new(
                table_constructor.braces().tokens().0.clone(),
                insert_after_token(table_constructor.braces().tokens().1, text),
            );
            Expression::TableConstructor(table_constructor.with_braces(new_braces))
        }
        Expression::Parentheses {
            contained,
            expression,
        } => Expression::Parentheses {
            contained: ContainedSpan::new(
                contained.tokens().0.clone(),
                insert_after_token(contained.tokens().1, text),
            ),
            expression: Box::new(*expression.to_owned()),
        },
        Expression::FunctionCall(func_call) => {
            let new_prefix = match func_call.prefix() {
                Prefix::Name(name) => Prefix::Name(name.clone()),
                Prefix::Expression(expr) => {
                    Prefix::Expression(Box::new(insert_after_expr(&*expr.to_owned(), text)))
                }
                _ => panic!("{}", func_call.prefix()),
            };
            // let new_suffix = func_call.suffixes();
            let mut suffix_vec = func_call.suffixes().cloned().collect::<Vec<Suffix>>();
            let size = suffix_vec.len();
            suffix_vec
                .clone()
                .iter()
                .enumerate()
                .for_each(|(idx, suffix)| {
                    if idx == (size - 1) {
                        suffix_vec[idx] = match suffix.clone() {
                            Suffix::Index(index) => Suffix::Index(match index {
                                Index::Dot { dot, name } => Index::Dot {
                                    dot: dot,
                                    name: insert_after_token(&name, text),
                                },
                                _ => panic!("Unexpected Index {:?}", index),
                            }),
                            Suffix::Call(call) => Suffix::Call(match call {
                                Call::MethodCall(method_call) => Call::MethodCall(
                                    method_call.clone().with_args(match method_call.args() {
                                        FunctionArgs::Parentheses {
                                            parentheses,
                                            arguments,
                                        } => FunctionArgs::Parentheses {
                                            parentheses: ContainedSpan::new(
                                                parentheses.tokens().0.clone(),
                                                insert_after_token(parentheses.tokens().1, text),
                                            ),
                                            arguments: arguments.clone(),
                                        },
                                        _ => panic!(
                                            "Unexpected Call {}",
                                            method_call.args().to_string()
                                        ),
                                    }),
                                ),
                                Call::AnonymousCall(anonymous_call) => {
                                    Call::AnonymousCall(match anonymous_call {
                                        FunctionArgs::Parentheses {
                                            parentheses,
                                            arguments,
                                        } => FunctionArgs::Parentheses {
                                            parentheses: ContainedSpan::new(
                                                parentheses.tokens().0.clone(),
                                                insert_after_token(parentheses.tokens().1, text),
                                            ),
                                            arguments: arguments.clone(),
                                        },
                                        FunctionArgs::String(str) => {
                                            FunctionArgs::String(insert_after_token(&str, text))
                                        }
                                        _ => {
                                            panic!("Unexpected Call {}", anonymous_call.to_string())
                                        }
                                    })
                                }
                                _ => panic!("Unexpected Call {:?}", suffix),
                            }),
                            _ => panic!("{:?}", suffix),
                        };
                    }
                });

            Expression::FunctionCall(func_call.with_prefix(new_prefix).with_suffixes(suffix_vec))
        }
        _ => panic!("{:?}", expr),
    }
}

fn insert_before_punc_var(var: &Punctuated<Var>, text: &str) -> Punctuated<Var> {
    let mut var_list = Punctuated::new();
    for (idx, pair) in var.pairs().enumerate() {
        if idx == 0 {
            let pair = pair.to_owned().map(|var| match var {
                Var::Name(token) => Var::Name(insert_before_token(&token, text)),
                _ => panic!("{:?}", var),
            });
            var_list.push(pair);
        } else {
            var_list.push(pair.to_owned());
        }
    }
    var_list
}

fn insert_after_punc_expr(
    expr_list: &Punctuated<Expression>,
    text: &str,
) -> Punctuated<Expression> {
    let mut punc: Punctuated<Expression> = Punctuated::new();
    let size = expr_list.len();
    for (idx, pair) in expr_list.pairs().enumerate() {
        if idx == (size - 1) {
            let pair = pair.to_owned().map(|expr| insert_after_expr(&expr, text));
            punc.push(pair);
        } else {
            println!("idx => {} {}", idx, pair.clone());
            punc.push(pair.to_owned());
        }
    }
    punc
}

fn create_punc(token_ref: &TokenReference) -> Punctuated<TokenReference> {
    let mut punc: Punctuated<TokenReference> = Punctuated::new();
    punc.push(Pair::new(token_ref.clone(), None));
    punc
}

fn create_empty_token_ref() -> TokenReference {
    TokenReference::new(
        vec![],
        Token::new(TokenType::Whitespace {
            characters: ShortString::new(""),
        }),
        vec![],
    )
}

fn convert_luau_to_lua(input: &str) -> String {
    let resources: darklua_core::Resources = darklua_core::Resources::from_memory();
    let context = darklua_core::rules::ContextBuilder::new(".", &resources, input).build();
    let mut block = darklua_core::Parser::default()
        .preserve_tokens()
        .parse(input)
        .unwrap_or_else(|error| {
            panic!(
                "[convert_luau_to_lua] darklua_core could not parse content: {:?}\ncontent:\n{}",
                error, input
            );
        });

    darklua_core::rules::RemoveCompoundAssignment::default()
        .process(&mut block, &context)
        .expect("Failed to remove compound assignment");
    darklua_core::rules::RemoveFloorDivision::default()
        .process(&mut block, &context)
        .expect("Failed to remove floor division");
    darklua_core::rules::RemoveTypes::default()
        .process(&mut block, &context)
        .expect("Failed to remove types");
    darklua_core::rules::RemoveIfExpression::default()
        .process(&mut block, &context)
        .expect("Failed to remove if expression");
    darklua_core::rules::RemoveContinue::default()
        .process(&mut block, &context)
        .expect("Failed to remove continue");
    darklua_core::rules::RemoveInterpolatedString::default()
        .process(&mut block, &context)
        .expect("Failed to remove interpolated string");
    darklua_core::rules::RemoveUnusedIfBranch::default()
        .process(&mut block, &context)
        .expect("Failed to remove unused if branches");

    let mut generator = darklua_core::generator::TokenBasedLuaGenerator::new(input);
    generator.write_block(&block);
    let lua_code = generator.into_string();

    lua_code
}

fn remove_lua_comments(input: &str) -> String {
    let resources = darklua_core::Resources::from_memory();
    let context = darklua_core::rules::ContextBuilder::new(".", &resources, input).build();
    let mut block = darklua_core::Parser::default()
        .preserve_tokens()
        .parse(input)
        .unwrap_or_else(|error| {
            panic!("could not parse content: {:?}\ncontent:\n{}\norigin_code:\n----------------------\n{input}\n----------------------", error, input);
        });

    darklua_core::rules::RemoveComments::default()
        .process(&mut block, &context)
        .expect("rule should suceed");
    let mut generator = darklua_core::generator::TokenBasedLuaGenerator::new(input);
    generator.write_block(&block);
    let lua_code = generator.into_string();

    lua_code
}

trait StringLuaCommentRemove {
    fn remove_lua_comments(&self) -> String;
}

impl StringLuaCommentRemove for String {
    fn remove_lua_comments(&self) -> String {
        remove_lua_comments(self.as_str())
    }
}

struct LuaTransformer {
    pub file_path: Option<String>,
}

impl LuaTransformer {
    fn new() -> LuaTransformer {
        LuaTransformer { file_path: None }
    }
}

impl LuaTransformer {
    fn resolve_comp_time(&self, node: FunctionDeclaration) -> FunctionDeclaration {
        assert!(
            node.body().block().last_stmt().is_some(),
            "Last statement(Return) should not be None"
        );

        let old_block = node.body().block().to_owned();
        let old_last_stmt = old_block.last_stmt().cloned().unwrap();
        let new_return = match old_last_stmt.clone() {
            LastStmt::Return(return_node) => return_node
                .clone()
                .with_token(insert_before_token(return_node.clone().token(), "--[==[ "))
                .with_returns(insert_after_punc_expr(
                    &return_node.returns().to_owned(),
                    " --]==] ",
                )),
            _ => panic!(),
        };

        let new_stmts: Vec<(Stmt, Option<TokenReference>)> = old_block
            .stmts()
            .map(|s| {
                let t = match s.clone() {
                    Stmt::LocalAssignment(local_assignment) => {
                        let expr_list =
                            insert_after_punc_expr(local_assignment.expressions(), " --]==]");
                        Stmt::LocalAssignment(
                            local_assignment
                                .clone()
                                .with_local_token(insert_before_token(
                                    local_assignment.local_token(),
                                    "--[==[ ",
                                ))
                                .with_expressions(expr_list),
                        )
                    }

                    Stmt::Assignment(assignment) => {
                        let var_list = insert_before_punc_var(&assignment.variables(), " --[==[ ");
                        let expr_list = insert_after_punc_expr(assignment.expressions(), " --]==]");
                        Stmt::Assignment(
                            assignment
                                .with_variables(var_list)
                                .with_expressions(expr_list),
                        )
                    }

                    Stmt::NumericFor(numeric_for) => {
                        let new_for_token = insert_before_token(numeric_for.for_token(), "--[==[ ");
                        let new_end_token = insert_after_token(numeric_for.end_token(), " --]==]");
                        Stmt::NumericFor(
                            numeric_for
                                .clone()
                                .with_for_token(new_for_token)
                                .with_end_token(new_end_token),
                        )
                    }

                    Stmt::GenericFor(generic_for) => {
                        let new_for_token = insert_before_token(generic_for.for_token(), "--[==[ ");
                        let new_end_token = insert_after_token(generic_for.end_token(), " --]==]");
                        Stmt::GenericFor(
                            generic_for
                                .clone()
                                .with_for_token(new_for_token)
                                .with_end_token(new_end_token),
                        )
                    }

                    Stmt::Do(do_stmt) => {
                        let new_do_token = insert_before_token(do_stmt.do_token(), "--[==[ ");
                        let new_end_token = insert_after_token(do_stmt.end_token(), " --]==]");
                        Stmt::Do(
                            do_stmt
                                .clone()
                                .with_do_token(new_do_token)
                                .with_end_token(new_end_token),
                        )
                    }

                    Stmt::FunctionCall(func_call) => {
                        let new_prefix = match func_call.prefix() {
                            Prefix::Name(token) => Prefix::Name(insert_before_token(token, "-- ")),
                            _ => panic!("{:?}", func_call.prefix()),
                        };
                        Stmt::FunctionCall(func_call.with_prefix(new_prefix))
                    }

                    Stmt::If(if_stmt) => Stmt::If(
                        if_stmt
                            .clone()
                            .with_if_token(insert_before_token(if_stmt.if_token(), "--[==[ "))
                            .with_end_token(insert_after_token(if_stmt.end_token(), " --]==]")),
                    ),

                    _ => panic!("{:?}", s),
                };
                (t, None)
            })
            .collect();
        let new_block = old_block
            .with_last_stmt(Some((LastStmt::Return(new_return), None)))
            .with_stmts(new_stmts);

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

        let new_func_body = if parameter_vec.len() == 0 {
            node.body()
                .clone()
                .with_end_token(empty_token(node.clone().body().end_token()))
                .with_parameters_parentheses(empty_contained_span(
                    node.body().parameters_parentheses(),
                ))
                .with_block(new_block)
        } else {
            let mut new_parameters: Punctuated<Parameter> = Punctuated::new();
            new_parameters.push(Pair::new(
                Parameter::Name(empty_token(&old_parameter_name_token.clone().unwrap())),
                None,
            ));
            node.body()
                .clone()
                .with_end_token(empty_token(node.clone().body().end_token()))
                .with_parameters_parentheses(empty_contained_span(
                    node.body().parameters_parentheses(),
                ))
                .with_parameters(new_parameters)
                .with_block(new_block)
        };

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

        #[cfg(not(test))]
        let comp_time_ret = lua_dostring(
            &(self.file_path.clone().unwrap_or_default() + " " + parameter_name.as_str()),
            node.body().block().to_string().as_str(),
        )
        .remove_lua_comments()
        .replace("\n", " "); // The generated code should not have any newlines and comments.

        #[cfg(test)]
        let comp_time_ret = lua_dostring(
            &(self.file_path.clone().unwrap_or_default() + " " + parameter_name.as_str()),
            node.body().block().to_string().as_str(),
        )
        .replace("\n", " ");

        let ret = node
            .clone()
            .with_function_token(insert_before_token(
                &empty_token(&node.clone().function_token()),
                comp_time_ret.as_str(),
            ))
            .with_name(FunctionName::new(create_punc(&create_empty_token_ref())))
            .with_body(new_func_body);

        ret
    }
}

fn get_func_call_name(func_call: &FunctionCall) -> (String, String) {
    let mut func_call_name = match func_call.prefix() {
        Prefix::Name(token) => token.token().to_string(),
        _ => panic!("Unexpected Prefix {:?}", func_call.prefix()),
    };
    let mut func_call_arg = String::from("");
    let suffix_vec = func_call.suffixes().cloned().collect::<Vec<Suffix>>();
    suffix_vec.iter().for_each(|suffix| match suffix.clone() {
        Suffix::Index(index) => {
            match index {
                Index::Dot { dot, name } => {
                    func_call_name = format!("{}.{}", func_call_name, name.token().to_string());
                }
                _ => panic!("Unexpected Index {:?}", index),
            };
        }
        Suffix::Call(call) => {
            match call {
                Call::MethodCall(method_call) => {
                    func_call_name = format!(
                        "{}:{}",
                        func_call_name,
                        method_call.name().token().to_string()
                    );
                    func_call_arg = match method_call.args() {
                        FunctionArgs::Parentheses {
                            parentheses,
                            arguments,
                        } => {
                            assert!(
                                arguments.len() == 1,
                                "More than 1 arguments, got {} => \"{}\"",
                                arguments.len(),
                                arguments.to_string()
                            );
                            arguments.to_string()
                        }
                        _ => panic!("Unexpected Call {}", method_call.args().to_string()),
                    };
                }
                _ => panic!("Unexpected Call {:?}", suffix),
            };
        }
        _ => panic!("{:?}", suffix),
    });

    (func_call_name, func_call_arg)
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
            (full_func_name, func_arg) = get_func_call_name(&node)
        } else if func_name.starts_with("_G") {
            if suffix_vec.len() > 0 {
                match suffix_vec[0].clone() {
                    Suffix::Index(index) => match index {
                        Index::Dot { dot, name } => {
                            if name.token().to_string().to_uppercase() == "__LJP" {
                                (full_func_name, func_arg) = get_func_call_name(&node);
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
            "__LJP:INCLUDE" | "_G.__LJP:INCLUDE"
        ) {
            #[cfg(test)]
            let new_prefix = {
                match node.prefix() {
                    Prefix::Name(token) => Prefix::Name(insert_before_token(
                        &insert_before_token(&token, "some code "),
                        "--[==[ ",
                    )),
                    _ => panic!("Unexpected Prefix {:?}", node.prefix()),
                }
            };

            #[cfg(not(test))]
            let new_prefix = {
                let include_file = lua_dostring(
                    "__LJP:INCLUDE",
                    &format!(
                        "return assert(package.searchpath({}, package.path))",
                        func_arg
                    ),
                );
                let include_code = std::fs::read_to_string(include_file.clone())
                    .expect(format!("Failed to read file => {}", include_file).as_str())
                    .remove_lua_comments()
                    .replace("\n", " ");

                match node.prefix() {
                    Prefix::Name(token) => Prefix::Name(insert_before_token(
                        &insert_before_token(&token, include_code.as_str()),
                        "--[==[ ",
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
                                        insert_after_token(parentheses.tokens().1, " --]==]"),
                                    ),
                                    arguments: arguments,
                                },
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

const OUTPUT_DIR: &str = ".luajit_pro";

thread_local! {
    static TRANSFORMER: RefCell<LuaTransformer> = RefCell::new(LuaTransformer::new());
}

#[no_mangle]
pub fn transform_lua(file_path: *const c_char) -> *const c_char {
    #[cfg(feature = "print-time")]
    let start = Instant::now();

    let c_str = unsafe { CStr::from_ptr(file_path) };
    let lua_file_path = c_str.to_str().unwrap();
    let content = std::fs::read_to_string(lua_file_path)
        .expect(format!("Failed to read file => {}", lua_file_path).as_str());
    let ast = full_moon::parse(&content).unwrap();

    TRANSFORMER.with(|transformer| {
        let mut transformer = transformer.borrow_mut();
        transformer.file_path = Some((lua_file_path.to_string()).to_string());
        let new_ast = transformer.visit_ast(ast);

        let mut new_content = new_ast.to_string();

        let first_line = content.lines().next().unwrap_or("");
        if first_line.contains("luau") {
            new_content = convert_luau_to_lua(&new_content);
        }

        if first_line.contains("format") {
            if first_line.contains("no-comment") {
                new_content = remove_lua_comments(&new_content);
            }

            let ast = full_moon::parse(&new_content).expect("Failed to parse generated AST");
            let ret_ast = stylua_lib::format_ast(
                ast,
                stylua_lib::Config::new(),
                None,
                stylua_lib::OutputVerification::None,
            )
            .unwrap();
            new_content = ret_ast.to_string();
        }

        if *LJP_KEEP_FILE == "1" {
            let filename = Path::new(lua_file_path)
                .file_name()
                .unwrap()
                .to_str()
                .unwrap();
            let out_file_path = format!("{}/{}.ljp_out", OUTPUT_DIR, filename);

            std::fs::create_dir_all(OUTPUT_DIR).expect("Failed to create directory");
            File::create(out_file_path.as_str())
                .expect(format!("Failed to create file => {}", out_file_path).as_str())
                .write_all(new_content.as_bytes())
                .expect("Failed to write to file");
        }

        let c_str = CString::new(new_content).unwrap();

        #[cfg(feature = "print-time")]
        {
            let duration = start.elapsed();
            println!(
                "[luajit_pro_heler] Time elapsed in transform_lua() is: {:?}, file: {}",
                duration, lua_file_path
            );
            std::io::stdout().flush().unwrap();
        }

        c_str.into_raw()
    })
}

#[test]
fn test() {
    let prj_root = {
        let metadata = cargo_metadata::MetadataCommand::new().exec().unwrap();
        metadata.workspace_root.to_string()
    };

    let file_path = format!("{}/tests/main.lua", prj_root);

    let ret_code = transform_lua(CString::new(file_path.as_str()).unwrap().as_ptr());
    let ret_code = unsafe {
        CStr::from_ptr(ret_code)
            .to_str()
            .unwrap_or("Not a valid UTF-8 string")
            .to_string()
    };

    println!("{}", ret_code);
}
