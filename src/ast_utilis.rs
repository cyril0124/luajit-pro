//! # AST Utilities
//!
//! This module provides utility functions for manipulating full_moon Lua AST nodes.
//! These are primarily used by the transformer and optimizer modules.

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

// ============================================================================
// Token Manipulation
// ============================================================================

/// Creates an empty token (whitespace with no characters)
pub fn empty_token(token_ref: &TokenReference) -> TokenReference {
    token_ref.with_token(Token::new(TokenType::Whitespace {
        characters: ShortString::new(""),
    }))
}

/// Creates an empty contained span
pub fn empty_contained_span(span: &ContainedSpan) -> ContainedSpan {
    ContainedSpan::new(empty_token(span.tokens().0), empty_token(span.tokens().1))
}

/// Replaces a token's content with new text
pub fn replace_token(token_ref: &TokenReference, text: &str) -> TokenReference {
    token_ref.with_token(Token::new(TokenType::Whitespace {
        characters: ShortString::new(text),
    }))
}

// ============================================================================
// Text Insertion
// ============================================================================

/// Inserts text after a contained span's closing token
pub fn insert_after_contained_span(span: &ContainedSpan, text: &str) -> ContainedSpan {
    ContainedSpan::new(
        span.tokens().0.clone(),
        insert_after_token(span.tokens().1, text),
    )
}

/// Inserts text before a contained span's opening token
pub fn insert_before_contained_span(span: &ContainedSpan, text: &str) -> ContainedSpan {
    ContainedSpan::new(
        insert_before_token(span.tokens().0, text),
        span.tokens().1.clone(),
    )
}

/// Inserts text after a token (in trailing trivia)
pub fn insert_after_token(token_ref: &TokenReference, text: &str) -> TokenReference {
    let mut trailing: Vec<Token> = vec![Token::new(TokenType::Whitespace {
        characters: ShortString::new(text),
    })];
    trailing.extend(token_ref.trailing_trivia().cloned());

    TokenReference::new(
        token_ref.leading_trivia().cloned().collect(),
        token_ref.token().clone(),
        trailing,
    )
}

/// Inserts text before a token (in leading trivia)
pub fn insert_before_token(token_ref: &TokenReference, text: &str) -> TokenReference {
    let mut leading: Vec<Token> = token_ref.leading_trivia().cloned().collect();
    leading.push(Token::new(TokenType::Whitespace {
        characters: ShortString::new(text),
    }));

    TokenReference::new(
        leading,
        token_ref.token().clone(),
        token_ref.trailing_trivia().cloned().collect(),
    )
}

/// Surrounds a token with text on both sides
pub fn surround_token(token_ref: &TokenReference, left: &str, right: &str) -> TokenReference {
    let mut leading: Vec<Token> = token_ref.leading_trivia().cloned().collect();
    leading.push(Token::new(TokenType::Whitespace {
        characters: ShortString::new(left),
    }));

    let mut trailing = vec![Token::new(TokenType::Whitespace {
        characters: ShortString::new(right),
    })];
    trailing.extend(token_ref.trailing_trivia().cloned());

    TokenReference::new(leading, token_ref.token().clone(), trailing)
}

// ============================================================================
// Expression Manipulation
// ============================================================================

/// Inserts text after a variable expression's last suffix
pub fn insert_after_var_expr(var_expr: &VarExpression, text: &str) -> VarExpression {
    let suffixes: Vec<Suffix> = var_expr
        .suffixes()
        .map(|suffix| match suffix.clone() {
            Suffix::Index(index) => Suffix::Index(match index {
                Index::Brackets {
                    brackets,
                    expression,
                } => Index::Brackets {
                    brackets: ContainedSpan::new(
                        brackets.tokens().0.clone(),
                        insert_after_token(brackets.tokens().1, text),
                    ),
                    expression,
                },
                Index::Dot { dot, name } => Index::Dot {
                    dot,
                    name: insert_after_token(&name, text),
                },
                other => panic!("Unexpected index type: {:?}", other),
            }),
            Suffix::Call(_) => todo!("insert_after_var_expr for Call suffix"),
            other => panic!("Unexpected suffix type: {:?}", other),
        })
        .collect();

    var_expr.clone().with_suffixes(suffixes)
}

/// Inserts text before an expression
pub fn insert_before_expr(expr: &Expression, text: &str) -> Expression {
    match expr.clone() {
        Expression::Number(n) => Expression::Number(insert_before_token(&n, text)),
        Expression::BinaryOperator { lhs, binop, rhs } => Expression::BinaryOperator {
            lhs: Box::new(insert_before_expr(&lhs, text)),
            binop,
            rhs,
        },
        Expression::Parentheses {
            contained,
            expression,
        } => Expression::Parentheses {
            contained: ContainedSpan::new(
                insert_before_token(contained.tokens().0, text),
                contained.tokens().1.clone(),
            ),
            expression: Box::new(insert_after_expr(&expression, text)),
        },
        other => panic!("insert_before_expr not implemented for: {}", other),
    }
}

/// Inserts text after an expression
pub fn insert_after_expr(expr: &Expression, text: &str) -> Expression {
    match expr.clone() {
        Expression::Number(n) => Expression::Number(insert_after_token(&n, text)),
        Expression::String(s) => Expression::String(insert_after_token(&s, text)),
        Expression::BinaryOperator { lhs, binop, rhs } => Expression::BinaryOperator {
            lhs,
            binop,
            rhs: Box::new(insert_after_expr(&rhs, text)),
        },
        Expression::Var(var) => Expression::Var(match var {
            Var::Expression(var_expr) => {
                Var::Expression(Box::new(insert_after_var_expr(&var_expr, text)))
            }
            Var::Name(name) => Var::Name(insert_after_token(&name, text)),
            other => panic!("Unexpected Var type: {:?}", other),
        }),
        Expression::TableConstructor(table) => {
            let new_braces = ContainedSpan::new(
                table.braces().tokens().0.clone(),
                insert_after_token(table.braces().tokens().1, text),
            );
            Expression::TableConstructor(table.with_braces(new_braces))
        }
        Expression::Parentheses {
            contained,
            expression,
        } => Expression::Parentheses {
            contained: ContainedSpan::new(
                contained.tokens().0.clone(),
                insert_after_token(contained.tokens().1, text),
            ),
            expression,
        },
        Expression::FunctionCall(func_call) => {
            let new_prefix = match func_call.prefix() {
                Prefix::Name(name) => Prefix::Name(name.clone()),
                Prefix::Expression(e) => Prefix::Expression(Box::new(insert_after_expr(e, text))),
                other => panic!("Unexpected prefix: {}", other),
            };

            let mut suffixes: Vec<Suffix> = func_call.suffixes().cloned().collect();
            let len = suffixes.len();

            if len > 0 {
                let last_idx = len - 1;
                suffixes[last_idx] = insert_text_after_suffix(&suffixes[last_idx], text);
            }

            Expression::FunctionCall(func_call.with_prefix(new_prefix).with_suffixes(suffixes))
        }
        other => panic!("insert_after_expr not implemented for: {:?}", other),
    }
}

/// Helper to insert text after a suffix
fn insert_text_after_suffix(suffix: &Suffix, text: &str) -> Suffix {
    match suffix.clone() {
        Suffix::Index(index) => Suffix::Index(match index {
            Index::Dot { dot, name } => Index::Dot {
                dot,
                name: insert_after_token(&name, text),
            },
            other => panic!("Unexpected Index: {:?}", other),
        }),
        Suffix::Call(call) => Suffix::Call(match call {
            Call::MethodCall(method) => {
                let new_args = match method.args() {
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
                    other => panic!("Unexpected MethodCall args: {}", other),
                };
                Call::MethodCall(method.with_args(new_args))
            }
            Call::AnonymousCall(args) => Call::AnonymousCall(match args {
                FunctionArgs::Parentheses {
                    parentheses,
                    arguments,
                } => FunctionArgs::Parentheses {
                    parentheses: ContainedSpan::new(
                        parentheses.tokens().0.clone(),
                        insert_after_token(parentheses.tokens().1, text),
                    ),
                    arguments,
                },
                FunctionArgs::String(s) => FunctionArgs::String(insert_after_token(&s, text)),
                other => panic!("Unexpected AnonymousCall args: {}", other),
            }),
            other => panic!("Unexpected Call: {:?}", other),
        }),
        other => panic!("Unexpected Suffix: {:?}", other),
    }
}

// ============================================================================
// Punctuated List Manipulation
// ============================================================================

/// Inserts text before the first variable in a punctuated list
pub fn insert_before_punc_var(vars: &Punctuated<Var>, text: &str) -> Punctuated<Var> {
    let mut result = Punctuated::new();

    for (idx, pair) in vars.pairs().enumerate() {
        if idx == 0 {
            let modified = pair.clone().map(|var| match var {
                Var::Name(token) => Var::Name(insert_before_token(&token, text)),
                Var::Expression(expr) => {
                    let new_prefix = match expr.prefix() {
                        Prefix::Name(name) => Prefix::Name(insert_before_token(name, text)),
                        Prefix::Expression(e) => {
                            Prefix::Expression(Box::new(insert_before_expr(e, text)))
                        }
                        other => panic!("Unexpected prefix: {}", other),
                    };
                    Var::Expression(Box::new(expr.with_prefix(new_prefix)))
                }
                other => panic!("Unexpected Var: {:?}", other),
            });
            result.push(modified);
        } else {
            result.push(pair.clone());
        }
    }

    result
}

/// Inserts text after the last expression in a punctuated list
pub fn insert_after_punc_expr(
    exprs: &Punctuated<Expression>,
    text: &str,
) -> Punctuated<Expression> {
    let mut result = Punctuated::new();
    let len = exprs.len();

    for (idx, pair) in exprs.pairs().enumerate() {
        if idx == len - 1 {
            result.push(pair.clone().map(|e| insert_after_expr(&e, text)));
        } else {
            result.push(pair.clone());
        }
    }

    result
}

// ============================================================================
// Utility Functions
// ============================================================================

/// Creates a single-element punctuated list
pub fn create_punc(token_ref: &TokenReference) -> Punctuated<TokenReference> {
    let mut punc = Punctuated::new();
    punc.push(Pair::new(token_ref.clone(), None));
    punc
}

/// Creates an empty token reference
pub fn create_empty_token_ref() -> TokenReference {
    TokenReference::new(
        vec![],
        Token::new(TokenType::Whitespace {
            characters: ShortString::new(""),
        }),
        vec![],
    )
}

/// Extracts the function name and argument from a function call
///
/// For example, `__LJP:include("module")` returns `("__LJP:include", "\"module\"")`
pub fn get_func_call_name(func_call: &FunctionCall) -> (String, String) {
    let mut name = match func_call.prefix() {
        Prefix::Name(token) => token.token().to_string(),
        other => panic!("Unexpected prefix: {:?}", other),
    };

    let mut arg = String::new();

    for suffix in func_call.suffixes() {
        match suffix {
            Suffix::Index(Index::Dot { name: field, .. }) => {
                name = format!("{}.{}", name, field.token());
            }
            Suffix::Call(Call::MethodCall(method)) => {
                name = format!("{}:{}", name, method.name().token());

                arg = match method.args() {
                    FunctionArgs::Parentheses { arguments, .. } => {
                        assert!(
                            arguments.len() == 1,
                            "Expected 1 argument, got {}: {}",
                            arguments.len(),
                            arguments
                        );
                        arguments.to_string()
                    }
                    FunctionArgs::String(s) => s.to_string(),
                    other => panic!("Unexpected function args: {}", other),
                };
            }
            other => panic!("Unexpected suffix: {:?}", other),
        }
    }

    (name, arg)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use full_moon::parse;

    /// Tests creating an empty token from an existing token reference.
    ///
    /// This is useful when you need to effectively "delete" a token
    /// while maintaining the AST structure.
    #[test]
    fn test_empty_token() {
        let ast = parse("local x = 1").unwrap();
        let code = ast.to_string();
        assert!(code.contains("local"));

        // Get a token reference from the AST
        let stmts: Vec<_> = ast.nodes().stmts().collect();
        if let Some(full_moon::ast::Stmt::LocalAssignment(local)) = stmts.first() {
            let empty = empty_token(local.local_token());
            // Empty token should produce empty string
            assert_eq!(empty.token().to_string(), "");
        }
    }

    /// Tests replacing token content with new text.
    ///
    /// # Example
    /// Replacing `x` with `y` in `local x = 1`
    #[test]
    fn test_replace_token() {
        let ast = parse("local x = 1").unwrap();

        let stmts: Vec<_> = ast.nodes().stmts().collect();
        if let Some(full_moon::ast::Stmt::LocalAssignment(local)) = stmts.first() {
            if let Some(pair) = local.names().pairs().next() {
                let replaced = replace_token(pair.value(), "replaced_name");
                assert_eq!(replaced.token().to_string(), "replaced_name");
            }
        }
    }

    /// Tests inserting text after a token.
    ///
    /// This is commonly used to add comments or additional code
    /// after specific tokens.
    ///
    /// # Example
    /// Adding `-- comment` after the `=` in `local x = 1`
    #[test]
    fn test_insert_after_token() {
        let ast = parse("local x = 1").unwrap();

        let stmts: Vec<_> = ast.nodes().stmts().collect();
        if let Some(full_moon::ast::Stmt::LocalAssignment(local)) = stmts.first() {
            let eq_token = local.equal_token().unwrap();
            let modified = insert_after_token(eq_token, " --[[inserted]]");
            let full_str = format!(
                "{}{}{}",
                modified
                    .leading_trivia()
                    .map(|t| t.to_string())
                    .collect::<String>(),
                modified.token(),
                modified
                    .trailing_trivia()
                    .map(|t| t.to_string())
                    .collect::<String>()
            );
            assert!(full_str.contains("--[[inserted]]"));
        }
    }

    /// Tests inserting text before a token.
    ///
    /// This is useful for prepending code or comments before
    /// specific tokens.
    #[test]
    fn test_insert_before_token() {
        let ast = parse("local x = 1").unwrap();

        let stmts: Vec<_> = ast.nodes().stmts().collect();
        if let Some(full_moon::ast::Stmt::LocalAssignment(local)) = stmts.first() {
            let modified = insert_before_token(local.local_token(), "--[[before]] ");
            let full_str = format!(
                "{}{}{}",
                modified
                    .leading_trivia()
                    .map(|t| t.to_string())
                    .collect::<String>(),
                modified.token(),
                modified
                    .trailing_trivia()
                    .map(|t| t.to_string())
                    .collect::<String>()
            );
            assert!(full_str.contains("--[[before]]"));
        }
    }

    /// Tests surrounding a token with text on both sides.
    ///
    /// This is useful for wrapping tokens in comments or markers.
    #[test]
    fn test_surround_token() {
        let ast = parse("local x = 1").unwrap();

        let stmts: Vec<_> = ast.nodes().stmts().collect();
        if let Some(full_moon::ast::Stmt::LocalAssignment(local)) = stmts.first() {
            let modified = surround_token(local.local_token(), "--[[L]] ", " --[[R]]");
            let full_str = format!(
                "{}{}{}",
                modified
                    .leading_trivia()
                    .map(|t| t.to_string())
                    .collect::<String>(),
                modified.token(),
                modified
                    .trailing_trivia()
                    .map(|t| t.to_string())
                    .collect::<String>()
            );
            assert!(full_str.contains("--[[L]]"));
            assert!(full_str.contains("--[[R]]"));
        }
    }

    /// Tests inserting text after an expression.
    ///
    /// This handles various expression types including:
    /// - Number literals
    /// - String literals
    /// - Binary operations
    /// - Variable references
    /// - Table constructors
    #[test]
    fn test_insert_after_expr_number() {
        let ast = parse("local x = 42").unwrap();

        let stmts: Vec<_> = ast.nodes().stmts().collect();
        if let Some(full_moon::ast::Stmt::LocalAssignment(local)) = stmts.first() {
            if let Some(pair) = local.expressions().pairs().next() {
                let modified = insert_after_expr(pair.value(), " --[[after]]");
                let result = modified.to_string();
                assert!(result.contains("--[[after]]"));
                assert!(result.contains("42"));
            }
        }
    }

    /// Tests inserting text after a table constructor expression.
    #[test]
    fn test_insert_after_expr_table() {
        let ast = parse("local t = {1, 2, 3}").unwrap();

        let stmts: Vec<_> = ast.nodes().stmts().collect();
        if let Some(full_moon::ast::Stmt::LocalAssignment(local)) = stmts.first() {
            if let Some(pair) = local.expressions().pairs().next() {
                let modified = insert_after_expr(pair.value(), " --[[end of table]]");
                let result = modified.to_string();
                assert!(result.contains("--[[end of table]]"));
            }
        }
    }

    /// Tests creating a single-element punctuated list.
    #[test]
    fn test_create_punc() {
        let ast = parse("local x = 1").unwrap();

        let stmts: Vec<_> = ast.nodes().stmts().collect();
        if let Some(full_moon::ast::Stmt::LocalAssignment(local)) = stmts.first() {
            let punc = create_punc(local.local_token());
            assert_eq!(punc.len(), 1);
        }
    }

    /// Tests creating an empty token reference.
    #[test]
    fn test_create_empty_token_ref() {
        let token = create_empty_token_ref();
        assert_eq!(token.token().to_string(), "");
    }

    /// Tests extracting function name and argument from a method call.
    ///
    /// # Example
    /// ```lua
    /// __LJP:include("module_name")
    /// ```
    /// Returns: `("__LJP:include", "\"module_name\"")`
    #[test]
    fn test_get_func_call_name_method() {
        let ast = parse("__LJP:include(\"test\")").unwrap();

        let stmts: Vec<_> = ast.nodes().stmts().collect();
        if let Some(full_moon::ast::Stmt::FunctionCall(call)) = stmts.first() {
            let (name, arg) = get_func_call_name(call);
            assert_eq!(name, "__LJP:include");
            assert!(arg.contains("test"));
        }
    }

    /// Tests extracting function name from a dot-notation call.
    ///
    /// # Example
    /// ```lua
    /// _G.__LJP:include("module")
    /// ```
    #[test]
    fn test_get_func_call_name_with_dot() {
        let ast = parse("module.submodule:method(\"arg\")").unwrap();

        let stmts: Vec<_> = ast.nodes().stmts().collect();
        if let Some(full_moon::ast::Stmt::FunctionCall(call)) = stmts.first() {
            let (name, arg) = get_func_call_name(call);
            assert_eq!(name, "module.submodule:method");
            assert!(arg.contains("arg"));
        }
    }
}
