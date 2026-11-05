//! Frontend de And23: lexer, AST, parser y diagnósticos.

pub mod ast;
pub mod error;
pub mod lexer;
pub mod parser;
pub mod span;

pub use ast::{BinOp, Expr, Program, Stmt, UnOp};
pub use error::Diagnostic;
pub use span::Span;

/// Analiza un programa completo: lexer y parser.
pub fn parse(src: &str) -> Result<Program, Diagnostic> {
    parser::parse_tokens(lexer::lex(src)?)
}
