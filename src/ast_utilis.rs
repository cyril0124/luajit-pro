#![allow(dead_code)]

use full_moon::{
    ast::{
        punctuated::{Pair, Punctuated},
        span::ContainedSpan,
        Call, Expression, FunctionArgs, FunctionCall, Index, Prefix, Suffix, Var, VarExpression,
    },
    tokenizer::{Token, TokenReference, TokenType},
    ShortString,
};

pub fn empty_token(token_ref: &TokenReference) -> TokenReference {
    token_ref.with_token(Token::new(TokenType::Whitespace {
        characters: ShortString::new(""),
    }))
}

pub fn empty_contained_span(contained_span: &ContainedSpan) -> ContainedSpan {
    ContainedSpan::new(
        empty_token(contained_span.tokens().0),
        empty_token(contained_span.tokens().1),
    )
}

pub fn replace_token(token_ref: &TokenReference, text: &str) -> TokenReference {
    token_ref.with_token(Token::new(TokenType::Whitespace {
        characters: ShortString::new(text),
    }))
}

pub fn insert_after_contained_span(contained_span: &ContainedSpan, text: &str) -> ContainedSpan {
    ContainedSpan::new(
        contained_span.tokens().0.clone(),
        insert_after_token(contained_span.tokens().1, text),
    )
}

pub fn insert_before_contained_span(contained_span: &ContainedSpan, text: &str) -> ContainedSpan {
    ContainedSpan::new(
        insert_before_token(contained_span.tokens().0, text),
        contained_span.tokens().1.clone(),
    )
}

pub fn insert_after_token(token_ref: &TokenReference, text: &str) -> TokenReference {
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

pub fn insert_before_token(token_ref: &TokenReference, text: &str) -> TokenReference {
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

pub fn surround_token(
    token_ref: &TokenReference,
    text_left: &str,
    text_right: &str,
) -> TokenReference {
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

pub fn insert_after_var_expr(var_expr: &VarExpression, text: &str) -> VarExpression {
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

pub fn insert_before_expr(expr: &Expression, text: &str) -> Expression {
    match expr.clone() {
        Expression::Number(number) => Expression::Number(insert_before_token(&number, text)),
        Expression::BinaryOperator { lhs, binop, rhs } => Expression::BinaryOperator {
            lhs: Box::new(insert_before_expr(&*lhs.to_owned(), text)),
            binop,
            rhs: Box::new(*rhs.to_owned()),
        },
        Expression::Parentheses {
            contained,
            expression,
        } => Expression::Parentheses {
            contained: ContainedSpan::new(
                insert_before_token(contained.tokens().0, text),
                contained.tokens().1.clone(),
            ),
            expression: Box::new(insert_after_expr(&*expression.to_owned(), text)),
        },
        _ => panic!("{}", expr),
    }
}

pub fn insert_after_expr(expr: &Expression, text: &str) -> Expression {
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

pub fn insert_before_punc_var(var: &Punctuated<Var>, text: &str) -> Punctuated<Var> {
    let mut var_list = Punctuated::new();
    for (idx, pair) in var.pairs().enumerate() {
        if idx == 0 {
            let pair = pair.to_owned().map(|var| match var {
                Var::Name(token) => Var::Name(insert_before_token(&token, text)),
                Var::Expression(expr) => Var::Expression(Box::new({
                    let new_prefix = match expr.to_owned().prefix().clone() {
                        Prefix::Name(name) => Prefix::Name(insert_before_token(&name, text)),
                        Prefix::Expression(expr) => Prefix::Expression(Box::new(
                            insert_before_expr(&*expr.to_owned(), text),
                        )),
                        _ => panic!("{}", expr.to_owned().prefix()),
                    };
                    expr.to_owned().with_prefix(new_prefix)
                })),
                _ => panic!("{:?}", var),
            });
            var_list.push(pair);
        } else {
            var_list.push(pair.to_owned());
        }
    }
    var_list
}

pub fn insert_after_punc_expr(
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

pub fn create_punc(token_ref: &TokenReference) -> Punctuated<TokenReference> {
    let mut punc: Punctuated<TokenReference> = Punctuated::new();
    punc.push(Pair::new(token_ref.clone(), None));
    punc
}

pub fn create_empty_token_ref() -> TokenReference {
    TokenReference::new(
        vec![],
        Token::new(TokenType::Whitespace {
            characters: ShortString::new(""),
        }),
        vec![],
    )
}

pub fn get_func_call_name(func_call: &FunctionCall) -> (String, String) {
    let mut func_call_name = match func_call.prefix() {
        Prefix::Name(token) => token.token().to_string(),
        _ => panic!("Unexpected Prefix {:?}", func_call.prefix()),
    };
    let mut func_call_arg = String::from("");
    let suffix_vec = func_call.suffixes().cloned().collect::<Vec<Suffix>>();
    suffix_vec.iter().for_each(|suffix| match suffix.clone() {
        Suffix::Index(index) => {
            match index {
                Index::Dot { dot: _, name } => {
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
                            parentheses: _,
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
                        FunctionArgs::String(str) => str.to_string(),
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
