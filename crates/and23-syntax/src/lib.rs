//! Frontend de And23: lexer, AST, parser y diagnósticos.

pub mod ast;
pub mod error;
pub mod format;
pub mod lexer;
pub mod modules;
pub mod parser;
pub mod span;

pub use ast::{
    BinOp, Block, CtorParam, Expr, FieldDecl, FnDecl, InterpPart, KindDecl, KindMethod,
    LambdaParam, Param, Program, Rescue, Stmt, SuperRef, TestDecl, ThingDecl, TypeParam, TypeRef,
    UnOp, UseDecl,
};
pub use error::Diagnostic;
pub use span::Span;

/// Código que toda sesión ejecuta antes del programa del usuario: la clase base de las excepciones
/// (`fail` solo lanza objetos de clases que heredan de `Fallo`) y la biblioteca base `Lista<T>` y
/// `Mapa<K, V>`, escrita en el propio lenguaje (`prelude.jb`).
pub const PRELUDE: &str = concat!("thing Fallo(fix msg: Str)\n", include_str!("../prelude.jb"));

/// Analiza un programa completo: lexer y parser.
pub fn parse(src: &str) -> Result<Program, Diagnostic> {
    parse_at(src, 0)
}

/// Como `parse`, pero las posiciones de los diagnósticos y del AST empiezan en `base` (varios
/// archivos comparten un mismo espacio de posiciones; ver `modules`).
pub fn parse_at(src: &str, base: usize) -> Result<Program, Diagnostic> {
    parser::parse_tokens(lexer::lex_at(src, base)?)
}
