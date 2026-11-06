use crate::ast::{BinOp, Block, Expr, FnDecl, Param, Program, Stmt, TypeRef, UnOp};
use crate::error::Diagnostic;
use crate::lexer::{Token, TokenKind};
use crate::span::Span;

/// Poder de enlace de los prefijos `-` y `!`: más fuerte que cualquier binario.
const PREFIX_BP: u8 = 13;

pub fn parse_tokens(tokens: Vec<Token>) -> Result<Program, Diagnostic> {
    Parser {
        tokens,
        pos: 0,
        fn_depth: 0,
        loop_depth: 0,
    }
    .program()
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    /// Cuántas funciones envuelven a la sentencia actual (0 o 1).
    fn_depth: usize,
    /// Cuántos `while` envuelven a la sentencia actual.
    loop_depth: usize,
}

/// Operador binario, su enlace izquierdo y su enlace derecho.
fn binary_op(kind: &TokenKind) -> Option<(BinOp, u8, u8)> {
    use TokenKind as T;
    Some(match kind {
        T::OrOr => (BinOp::Or, 1, 2),
        T::AndAnd => (BinOp::And, 3, 4),
        T::EqEq => (BinOp::Eq, 5, 6),
        T::NotEq => (BinOp::Ne, 5, 6),
        T::Lt => (BinOp::Lt, 7, 8),
        T::LtEq => (BinOp::Le, 7, 8),
        T::Gt => (BinOp::Gt, 7, 8),
        T::GtEq => (BinOp::Ge, 7, 8),
        T::Plus => (BinOp::Add, 9, 10),
        T::Minus => (BinOp::Sub, 9, 10),
        T::Star => (BinOp::Mul, 11, 12),
        T::Slash => (BinOp::Div, 11, 12),
        T::Percent => (BinOp::Rem, 11, 12),
        _ => return None,
    })
}

/// Operador de asignación: `None` es `=`, `Some(op)` es `op=`.
fn assign_op(kind: &TokenKind) -> Option<Option<BinOp>> {
    use TokenKind as T;
    Some(match kind {
        T::Assign => None,
        T::PlusEq => Some(BinOp::Add),
        T::MinusEq => Some(BinOp::Sub),
        T::StarEq => Some(BinOp::Mul),
        T::SlashEq => Some(BinOp::Div),
        T::PercentEq => Some(BinOp::Rem),
        _ => return None,
    })
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

    fn error_here(&self, message: String) -> Diagnostic {
        Diagnostic::new(self.peek().span, message)
    }

    fn expect(&mut self, kind: TokenKind, what: &str) -> Result<Token, Diagnostic> {
        if self.peek().kind == kind {
            Ok(self.bump())
        } else {
            Err(self.error_here(format!(
                "se esperaba {what}, pero se encontró {}",
                self.peek().kind.describe()
            )))
        }
    }

    fn expect_ident(&mut self, what: &str) -> Result<(String, Span), Diagnostic> {
        match self.peek().kind.clone() {
            TokenKind::Ident(name) => {
                let span = self.bump().span;
                Ok((name, span))
            }
            other => Err(self.error_here(format!(
                "se esperaba {what}, pero se encontró {}",
                other.describe()
            ))),
        }
    }

    /// Tras una sentencia debe venir un salto de línea, o el cierre del bloque / fin del archivo.
    fn end_of_stmt(&self, in_block: bool) -> Result<(), Diagnostic> {
        match (&self.peek().kind, in_block) {
            (TokenKind::Newline, _) | (TokenKind::Eof, false) | (TokenKind::RBrace, true) => Ok(()),
            (TokenKind::Eof, true) => {
                Err(self.error_here("se esperaba `}` para cerrar el bloque".into()))
            }
            (other, _) => Err(self.error_here(format!(
                "se esperaba el fin de la sentencia, pero se encontró {}",
                other.describe()
            ))),
        }
    }

    fn program(&mut self) -> Result<Program, Diagnostic> {
        let mut stmts = Vec::new();
        loop {
            self.skip_newlines();
            if self.peek().kind == TokenKind::Eof {
                return Ok(Program { stmts });
            }
            let stmt = if self.peek().kind == TokenKind::Fn {
                Stmt::Fn(Box::new(self.fn_decl()?))
            } else {
                self.stmt()?
            };
            stmts.push(stmt);
            self.end_of_stmt(false)?;
        }
    }

    fn block(&mut self) -> Result<Block, Diagnostic> {
        let open = self.expect(TokenKind::LBrace, "`{`")?.span;
        let unclosed = || Diagnostic::new(open, "este bloque no se cierra: falta `}`");
        let mut stmts = Vec::new();
        loop {
            self.skip_newlines();
            match self.peek().kind {
                TokenKind::RBrace => {
                    let close = self.bump().span;
                    return Ok(Block {
                        stmts,
                        span: open.to(close),
                    });
                }
                TokenKind::Eof => return Err(unclosed()),
                _ => {}
            }
            stmts.push(self.stmt()?);
            if self.peek().kind == TokenKind::Eof {
                return Err(unclosed());
            }
            self.end_of_stmt(true)?;
        }
    }

    fn type_ref(&mut self) -> Result<TypeRef, Diagnostic> {
        let (name, span) = self.expect_ident("un nombre de tipo")?;
        Ok(TypeRef { name, span })
    }

    fn stmt(&mut self) -> Result<Stmt, Diagnostic> {
        match self.peek().kind.clone() {
            TokenKind::Fix | TokenKind::Mut => self.let_stmt(),
            TokenKind::If => self.if_stmt(),
            TokenKind::While => self.while_stmt(),
            TokenKind::Return => self.return_stmt(),
            TokenKind::Show => self.show_stmt(),
            TokenKind::Break | TokenKind::Continue => self.jump_stmt(),
            TokenKind::Fn => Err(self.error_here(
                "las funciones solo se pueden declarar al nivel superior del archivo".into(),
            )),
            TokenKind::Else => {
                Err(self.error_here("`else` debe ir en la misma línea que la `}` del `if`".into()))
            }
            TokenKind::Ident(_) if assign_op(&self.peek_at(1).kind).is_some() => self.assign_stmt(),
            _ => Ok(Stmt::Expr(self.expr(0)?)),
        }
    }

    fn let_stmt(&mut self) -> Result<Stmt, Diagnostic> {
        let keyword = self.bump();
        let mutable = keyword.kind == TokenKind::Mut;
        let (name, name_span) = self.expect_ident("el nombre de la variable")?;
        let ty = if self.peek().kind == TokenKind::Colon {
            self.bump();
            Some(self.type_ref()?)
        } else {
            None
        };
        self.expect(TokenKind::Assign, "`=` y el valor inicial")?;
        let init = self.expr(0)?;
        let span = keyword.span.to(init.span());
        Ok(Stmt::Let {
            name,
            name_span,
            mutable,
            ty,
            init,
            span,
        })
    }

    fn assign_stmt(&mut self) -> Result<Stmt, Diagnostic> {
        let (name, name_span) = self.expect_ident("el nombre de la variable")?;
        let op = assign_op(&self.bump().kind).expect("el llamador verificó el operador");
        let value = self.expr(0)?;
        let span = name_span.to(value.span());
        Ok(Stmt::Assign {
            name,
            name_span,
            op,
            value,
            span,
        })
    }

    fn if_stmt(&mut self) -> Result<Stmt, Diagnostic> {
        let start = self.bump().span;
        let cond = self.expr(0)?;
        let then = self.block()?;
        let mut end = then.span;
        let otherwise = if self.peek().kind == TokenKind::Else {
            self.bump();
            if self.peek().kind == TokenKind::If {
                let nested = self.if_stmt()?;
                end = nested.span();
                Some(Block {
                    span: end,
                    stmts: vec![nested],
                })
            } else {
                let block = self.block()?;
                end = block.span;
                Some(block)
            }
        } else {
            None
        };
        Ok(Stmt::If {
            cond,
            then,
            otherwise,
            span: start.to(end),
        })
    }

    fn while_stmt(&mut self) -> Result<Stmt, Diagnostic> {
        let start = self.bump().span;
        let cond = self.expr(0)?;
        self.loop_depth += 1;
        let body = self.block();
        self.loop_depth -= 1;
        let body = body?;
        let span = start.to(body.span);
        Ok(Stmt::While { cond, body, span })
    }

    fn return_stmt(&mut self) -> Result<Stmt, Diagnostic> {
        let keyword = self.bump();
        if self.fn_depth == 0 {
            return Err(Diagnostic::new(
                keyword.span,
                "`return` solo se puede usar dentro de una función",
            ));
        }
        if matches!(
            self.peek().kind,
            TokenKind::Newline | TokenKind::RBrace | TokenKind::Eof
        ) {
            return Ok(Stmt::Return(None, keyword.span));
        }
        let value = self.expr(0)?;
        let span = keyword.span.to(value.span());
        Ok(Stmt::Return(Some(value), span))
    }

    fn jump_stmt(&mut self) -> Result<Stmt, Diagnostic> {
        let keyword = self.bump();
        if self.loop_depth == 0 {
            return Err(Diagnostic::new(
                keyword.span,
                format!(
                    "`{}` solo se puede usar dentro de un `while`",
                    keyword.kind.describe().trim_matches('`')
                ),
            ));
        }
        Ok(if keyword.kind == TokenKind::Break {
            Stmt::Break(keyword.span)
        } else {
            Stmt::Continue(keyword.span)
        })
    }

    fn show_stmt(&mut self) -> Result<Stmt, Diagnostic> {
        let start = self.bump().span;
        self.expect(TokenKind::LParen, "`(` después de `show`")?;
        let value = self.expr(0)?;
        let close = self.expect(TokenKind::RParen, "`)`")?;
        Ok(Stmt::Show(value, start.to(close.span)))
    }

    fn fn_decl(&mut self) -> Result<FnDecl, Diagnostic> {
        let start = self.bump().span;
        let (name, name_span) = self.expect_ident("el nombre de la función")?;
        self.expect(TokenKind::LParen, "`(`")?;
        let mut params: Vec<Param> = Vec::new();
        if self.peek().kind != TokenKind::RParen {
            loop {
                let (pname, pspan) = self.expect_ident("el nombre del parámetro")?;
                if params.iter().any(|p| p.name == pname) {
                    return Err(Diagnostic::new(
                        pspan,
                        format!("el parámetro `{pname}` está repetido"),
                    ));
                }
                self.expect(TokenKind::Colon, "`:` y el tipo del parámetro")?;
                let ty = self.type_ref()?;
                let span = pspan.to(ty.span);
                params.push(Param {
                    name: pname,
                    ty,
                    span,
                });
                if self.peek().kind == TokenKind::Comma {
                    self.bump();
                } else {
                    break;
                }
            }
        }
        self.expect(TokenKind::RParen, "`,` o `)`")?;
        let ret = if self.peek().kind == TokenKind::Arrow {
            self.bump();
            Some(self.type_ref()?)
        } else {
            None
        };

        self.fn_depth += 1;
        let body = match self.peek().kind {
            TokenKind::LBrace => self.block(),
            TokenKind::Assign => {
                self.bump();
                self.expr(0).map(|value| {
                    let span = value.span();
                    Block {
                        stmts: vec![Stmt::Return(Some(value), span)],
                        span,
                    }
                })
            }
            _ => Err(self.error_here(format!(
                "se esperaba `{{` o `=` para el cuerpo de la función, pero se encontró {}",
                self.peek().kind.describe()
            ))),
        };
        self.fn_depth -= 1;
        let body = body?;
        let span = start.to(body.span);
        Ok(FnDecl {
            name,
            name_span,
            params,
            ret,
            body,
            span,
        })
    }

    /// Pratt: `min_bp` es el poder de enlace mínimo que debe tener un operador para seguir.
    fn expr(&mut self, min_bp: u8) -> Result<Expr, Diagnostic> {
        let token = self.bump();
        let mut lhs = match token.kind {
            TokenKind::Int(v) => Expr::Int(v, token.span),
            TokenKind::Float(v) => Expr::Float(v, token.span),
            TokenKind::True => Expr::Bool(true, token.span),
            TokenKind::False => Expr::Bool(false, token.span),
            TokenKind::Minus | TokenKind::Bang => {
                let op = if token.kind == TokenKind::Minus {
                    UnOp::Neg
                } else {
                    UnOp::Not
                };
                let operand = self.expr(PREFIX_BP)?;
                let span = token.span.to(operand.span());
                Expr::Unary {
                    op,
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
                if self.peek().kind == TokenKind::LParen {
                    self.call(name, token.span)?
                } else {
                    Expr::Var(name, token.span)
                }
            }
            other => {
                return Err(Diagnostic::new(
                    token.span,
                    format!(
                        "se esperaba una expresión, pero se encontró {}",
                        other.describe()
                    ),
                ));
            }
        };

        while let Some((op, left_bp, right_bp)) = binary_op(&self.peek().kind) {
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

    /// Llamada `nombre(args)`; el nombre ya se consumió y el siguiente token es `(`.
    fn call(&mut self, name: String, name_span: Span) -> Result<Expr, Diagnostic> {
        self.bump();
        let mut args = Vec::new();
        if self.peek().kind != TokenKind::RParen {
            loop {
                args.push(self.expr(0)?);
                if self.peek().kind == TokenKind::Comma {
                    self.bump();
                } else {
                    break;
                }
            }
        }
        let close = self.expect(TokenKind::RParen, "`,` o `)`")?;
        Ok(Expr::Call {
            name,
            args,
            span: name_span.to(close.span),
        })
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
            Expr::Bool(v, _) => v.to_string(),
            Expr::Var(name, _) => name.clone(),
            Expr::Unary { op, expr, .. } => {
                let sym = if *op == UnOp::Neg { "neg" } else { "not" };
                format!("({sym} {})", sexp(expr))
            }
            Expr::Binary { op, lhs, rhs, .. } => {
                format!("({} {} {})", op.symbol(), sexp(lhs), sexp(rhs))
            }
            Expr::Call { name, args, .. } => {
                let args: Vec<String> = args.iter().map(sexp).collect();
                format!("(call {name} {})", args.join(" ")).replace(" )", ")")
            }
        }
    }

    fn shape(src: &str) -> String {
        match &parse(src).unwrap().stmts[0] {
            Stmt::Expr(e) | Stmt::Show(e, _) => sexp(e),
            other => panic!("no es una expresión: {other:?}"),
        }
    }

    fn err(src: &str) -> String {
        parse(src).unwrap_err().message
    }

    #[test]
    fn precedencia_aritmetica() {
        assert_eq!(shape("1 + 2 * 3"), "(+ 1 (* 2 3))");
        assert_eq!(shape("(1 + 2) * 3"), "(* (+ 1 2) 3)");
        assert_eq!(shape("10 - 4 - 3"), "(- (- 10 4) 3)");
        assert_eq!(shape("8 / 4 / 2"), "(/ (/ 8 4) 2)");
    }

    #[test]
    fn precedencia_de_comparaciones_y_logicos() {
        assert_eq!(shape("1 + 2 < 4"), "(< (+ 1 2) 4)");
        assert_eq!(shape("a < b == c < d"), "(== (< a b) (< c d))");
        assert_eq!(shape("a || b && c == d"), "(|| a (&& b (== c d)))");
    }

    #[test]
    fn prefijos() {
        assert_eq!(shape("-2 * 3"), "(* (neg 2) 3)");
        assert_eq!(shape("2 * -3"), "(* 2 (neg 3))");
        assert_eq!(shape("- -4"), "(neg (neg 4))");
        assert_eq!(shape("!a && b"), "(&& (not a) b)");
        assert_eq!(shape("!(a || b)"), "(not (|| a b))");
    }

    #[test]
    fn llamadas() {
        assert_eq!(shape("f()"), "(call f)");
        assert_eq!(shape("f(1, g(2) + 3)"), "(call f 1 (+ (call g 2) 3))");
    }

    #[test]
    fn variables_y_literales_booleanos() {
        assert_eq!(shape("dueño + true"), "(+ dueño true)");
    }

    #[test]
    fn declaraciones_y_asignaciones() {
        let prog = parse("fix a = 1\nmut b: Int = 2\nb += 3\nb = a").unwrap();
        assert!(matches!(
            &prog.stmts[0],
            Stmt::Let {
                mutable: false,
                ty: None,
                ..
            }
        ));
        assert!(
            matches!(&prog.stmts[1], Stmt::Let { mutable: true, ty: Some(t), .. } if t.name == "Int")
        );
        assert!(matches!(
            &prog.stmts[2],
            Stmt::Assign {
                op: Some(BinOp::Add),
                ..
            }
        ));
        assert!(matches!(&prog.stmts[3], Stmt::Assign { op: None, .. }));
    }

    #[test]
    fn if_else_if_else_se_anida() {
        let prog = parse("if a { 1 } else if b { 2 } else { 3 }").unwrap();
        let Stmt::If {
            otherwise: Some(els),
            ..
        } = &prog.stmts[0]
        else {
            panic!("se esperaba un if con else");
        };
        assert!(matches!(
            &els.stmts[0],
            Stmt::If {
                otherwise: Some(_),
                ..
            }
        ));
    }

    #[test]
    fn funcion_con_cuerpo_de_bloque_y_de_expresion() {
        let prog =
            parse("fn f(a: Int, b: Float) -> Int { return a }\nfn g(x: Int) = x * 2").unwrap();
        let Stmt::Fn(f) = &prog.stmts[0] else {
            panic!()
        };
        assert_eq!(f.params.len(), 2);
        assert_eq!(f.ret.as_ref().unwrap().name, "Int");
        let Stmt::Fn(g) = &prog.stmts[1] else {
            panic!()
        };
        assert!(g.ret.is_none());
        assert!(matches!(&g.body.stmts[0], Stmt::Return(Some(_), _)));
    }

    #[test]
    fn bloques_en_una_linea_con_punto_y_coma() {
        let prog = parse("while a { x += 1; y += 2 }").unwrap();
        let Stmt::While { body, .. } = &prog.stmts[0] else {
            panic!()
        };
        assert_eq!(body.stmts.len(), 2);
    }

    #[test]
    fn varias_sentencias_separadas_por_linea_o_punto_y_coma() {
        assert_eq!(parse("1\n\n2; 3\n").unwrap().stmts.len(), 3);
    }

    #[test]
    fn return_fuera_de_funcion() {
        assert!(err("return 1").contains("dentro de una función"));
    }

    #[test]
    fn break_y_continue_fuera_de_bucle() {
        assert!(err("break").contains("`break` solo se puede usar dentro de un `while`"));
        assert!(err("fn f() { continue }").contains("`continue` solo"));
    }

    #[test]
    fn break_dentro_de_while_es_valido() {
        assert!(parse("while true { break }").is_ok());
        assert!(parse("fn f() { while true { if true { return } } }").is_ok());
    }

    #[test]
    fn else_en_linea_aparte() {
        assert!(err("if a {\n1\n}\nelse {\n2\n}").contains("misma línea"));
    }

    #[test]
    fn funcion_anidada_no_permitida() {
        assert!(err("fn f() {\nfn g() {}\n}").contains("nivel superior"));
    }

    #[test]
    fn parametro_sin_tipo_o_repetido() {
        assert!(err("fn f(a) {}").contains("`:` y el tipo del parámetro"));
        assert!(err("fn f(a: Int, a: Int) {}").contains("repetido"));
    }

    #[test]
    fn cuerpo_de_funcion_ausente() {
        assert!(err("fn f()").contains("cuerpo de la función"));
    }

    #[test]
    fn bloque_sin_cerrar_senala_la_llave_de_apertura() {
        for src in [
            "while true {\nx += 1\n",
            "while true {\n",
            "fn f() { x += 1",
            "fn f() {",
        ] {
            let e = parse(src).unwrap_err();
            assert!(e.message.contains("falta `}`"), "{src:?}: {}", e.message);
            assert_eq!(e.span.start, src.find('{').unwrap(), "{src:?}");
        }
    }

    #[test]
    fn error_parentesis_sin_cerrar() {
        assert!(err("(1 + 2").contains("se esperaba `)`"));
    }

    #[test]
    fn error_expresion_faltante() {
        assert!(err("1 + ").contains("se esperaba una expresión"));
    }

    #[test]
    fn error_dos_expresiones_en_la_misma_linea() {
        assert!(err("1 2").contains("fin de la sentencia"));
    }

    #[test]
    fn palabra_clave_no_es_nombre_de_variable() {
        assert!(err("fix if = 1").contains("el nombre de la variable"));
    }
}
