use crate::ast::{BinOp, Expr, Program, Stmt, UnOp};
use crate::error::Diagnostic;
use crate::lexer::{Token, TokenKind};

/// Poder de enlace del `-` prefijo: más fuerte que `*` y `/`.
const PREFIX_BP: u8 = 5;

pub fn parse_tokens(tokens: Vec<Token>) -> Result<Program, Diagnostic> {
    Parser { tokens, pos: 0 }.program()
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

fn describe(kind: &TokenKind) -> String {
    match kind {
        TokenKind::Int(v) => format!("el entero `{v}`"),
        TokenKind::Float(v) => format!("el decimal `{v}`"),
        TokenKind::Ident(name) => format!("el identificador `{name}`"),
        TokenKind::Plus => "`+`".into(),
        TokenKind::Minus => "`-`".into(),
        TokenKind::Star => "`*`".into(),
        TokenKind::Slash => "`/`".into(),
        TokenKind::Percent => "`%`".into(),
        TokenKind::LParen => "`(`".into(),
        TokenKind::RParen => "`)`".into(),
        TokenKind::Newline => "el fin de la línea".into(),
        TokenKind::Eof => "el fin del programa".into(),
    }
}

impl Parser {
    fn peek(&self) -> &Token {
        &self.tokens[self.pos]
    }

    fn peek_at(&self, offset: usize) -> &Token {
        &self.tokens[(self.pos + offset).min(self.tokens.len() - 1)]
    }

    /// Consume el token actual; el `Eof` final no se consume nunca.
    fn bump(&mut self) -> Token {
        let token = self.tokens[self.pos].clone();
        if self.pos < self.tokens.len() - 1 {
            self.pos += 1;
        }
        token
    }

    fn skip_newlines(&mut self) {
        while self.peek().kind == TokenKind::Newline {
            self.bump();
        }
    }

    fn expect(&mut self, kind: TokenKind, what: &str) -> Result<Token, Diagnostic> {
        if self.peek().kind == kind {
            Ok(self.bump())
        } else {
            let found = self.peek();
            Err(Diagnostic::new(
                found.span,
                format!(
                    "se esperaba {what}, pero se encontró {}",
                    describe(&found.kind)
                ),
            ))
        }
    }

    fn program(&mut self) -> Result<Program, Diagnostic> {
        let mut stmts = Vec::new();
        loop {
            self.skip_newlines();
            if self.peek().kind == TokenKind::Eof {
                return Ok(Program { stmts });
            }
            stmts.push(self.stmt()?);
            if !matches!(self.peek().kind, TokenKind::Newline | TokenKind::Eof) {
                let found = self.peek();
                return Err(Diagnostic::new(
                    found.span,
                    format!(
                        "se esperaba el fin de la sentencia, pero se encontró {}",
                        describe(&found.kind)
                    ),
                ));
            }
        }
    }

    fn stmt(&mut self) -> Result<Stmt, Diagnostic> {
        let is_show = matches!(&self.peek().kind, TokenKind::Ident(n) if n == "show")
            && self.peek_at(1).kind == TokenKind::LParen;
        if is_show {
            let open = self.bump().span;
            self.bump();
            let value = self.expr(0)?;
            let close = self.expect(TokenKind::RParen, "`)`")?;
            return Ok(Stmt::Show(value, open.to(close.span)));
        }
        Ok(Stmt::Expr(self.expr(0)?))
    }

    /// Pratt: `min_bp` es el poder de enlace mínimo que debe tener un operador para seguir.
    fn expr(&mut self, min_bp: u8) -> Result<Expr, Diagnostic> {
        let token = self.bump();
        let mut lhs = match token.kind {
            TokenKind::Int(v) => Expr::Int(v, token.span),
            TokenKind::Float(v) => Expr::Float(v, token.span),
            TokenKind::Minus => {
                let operand = self.expr(PREFIX_BP)?;
                let span = token.span.to(operand.span());
                Expr::Unary {
                    op: UnOp::Neg,
                    expr: Box::new(operand),
                    span,
                }
            }
            TokenKind::LParen => {
                let inner = self.expr(0)?;
                self.expect(TokenKind::RParen, "`)`")?;
                inner
            }
            TokenKind::Ident(name) => {
                return Err(Diagnostic::new(
                    token.span,
                    format!("el identificador `{name}` aún no está soportado (las variables llegan en M1)"),
                ));
            }
            other => {
                return Err(Diagnostic::new(
                    token.span,
                    format!(
                        "se esperaba una expresión, pero se encontró {}",
                        describe(&other)
                    ),
                ));
            }
        };

        loop {
            let (op, left_bp, right_bp) = match self.peek().kind {
                TokenKind::Plus => (BinOp::Add, 1, 2),
                TokenKind::Minus => (BinOp::Sub, 1, 2),
                TokenKind::Star => (BinOp::Mul, 3, 4),
                TokenKind::Slash => (BinOp::Div, 3, 4),
                TokenKind::Percent => (BinOp::Rem, 3, 4),
                _ => break,
            };
            if left_bp < min_bp {
                break;
            }
            self.bump();
            let rhs = self.expr(right_bp)?;
            let span = lhs.span().to(rhs.span());
            lhs = Expr::Binary {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
                span,
            };
        }
        Ok(lhs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;

    /// Forma S-expression del AST para comparar la estructura sin ruido de spans.
    fn sexp(expr: &Expr) -> String {
        match expr {
            Expr::Int(v, _) => v.to_string(),
            Expr::Float(v, _) => format!("{v:?}"),
            Expr::Unary { expr, .. } => format!("(neg {})", sexp(expr)),
            Expr::Binary { op, lhs, rhs, .. } => {
                let sym = match op {
                    BinOp::Add => "+",
                    BinOp::Sub => "-",
                    BinOp::Mul => "*",
                    BinOp::Div => "/",
                    BinOp::Rem => "%",
                };
                format!("({sym} {} {})", sexp(lhs), sexp(rhs))
            }
        }
    }

    fn shape(src: &str) -> String {
        match &parse(src).unwrap().stmts[0] {
            Stmt::Expr(e) | Stmt::Show(e, _) => sexp(e),
        }
    }

    #[test]
    fn precedencia_multiplicacion_sobre_suma() {
        assert_eq!(shape("1 + 2 * 3"), "(+ 1 (* 2 3))");
    }

    #[test]
    fn asociatividad_izquierda() {
        assert_eq!(shape("10 - 4 - 3"), "(- (- 10 4) 3)");
        assert_eq!(shape("8 / 4 / 2"), "(/ (/ 8 4) 2)");
    }

    #[test]
    fn parentesis_cambian_el_orden() {
        assert_eq!(shape("(1 + 2) * 3"), "(* (+ 1 2) 3)");
    }

    #[test]
    fn menos_unario_enlaza_mas_que_binarios() {
        assert_eq!(shape("-2 * 3"), "(* (neg 2) 3)");
        assert_eq!(shape("2 * -3"), "(* 2 (neg 3))");
        assert_eq!(shape("- -4"), "(neg (neg 4))");
    }

    #[test]
    fn show_es_una_sentencia() {
        let prog = parse("show(1 + 2)").unwrap();
        assert!(matches!(prog.stmts[0], Stmt::Show(..)));
    }

    #[test]
    fn varias_sentencias_separadas_por_linea_o_punto_y_coma() {
        assert_eq!(parse("1\n\n2; 3\n").unwrap().stmts.len(), 3);
    }

    #[test]
    fn error_parentesis_sin_cerrar() {
        let err = parse("(1 + 2").unwrap_err();
        assert!(err.message.contains("se esperaba `)`"), "{}", err.message);
    }

    #[test]
    fn error_expresion_faltante() {
        let err = parse("1 + ").unwrap_err();
        assert!(err.message.contains("se esperaba una expresión"));
    }

    #[test]
    fn error_dos_expresiones_en_la_misma_linea() {
        let err = parse("1 2").unwrap_err();
        assert!(err.message.contains("fin de la sentencia"));
    }

    #[test]
    fn identificadores_aun_no_soportados() {
        let err = parse("dueño + 1").unwrap_err();
        assert!(err.message.contains("`dueño`") && err.message.contains("M1"));
    }
}
