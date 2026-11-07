use crate::ast::{
    BinOp, Block, CtorParam, Expr, FieldDecl, FnDecl, InterpPart, KindDecl, KindMethod,
    LambdaParam, Param, Program, Rescue, Stmt, SuperRef, ThingDecl, TypeParam, TypeRef, UnOp,
    UseDecl,
};
use crate::error::Diagnostic;
use crate::lexer::{StrPart, Token, TokenKind};
use crate::span::Span;

/// Poder de enlace de los prefijos `-` y `!`: más fuerte que cualquier binario.
const PREFIX_BP: u8 = 13;

pub fn parse_tokens(tokens: Vec<Token>) -> Result<Program, Diagnostic> {
    Parser {
        tokens,
        pos: 0,
        fn_depth: 0,
        loop_depth: 0,
        no_trailing: false,
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
    /// Dentro de la condición de un `if`/`while` una `{` abre el cuerpo, no una lambda final.
    no_trailing: bool,
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

    /// Nombre de tipo o de clase, posiblemente cualificado por el alias de un `use`: `util.Persona`.
    fn qualified_ident(&mut self, what: &str) -> Result<(String, Span), Diagnostic> {
        let (mut name, mut span) = self.expect_ident(what)?;
        if self.peek().kind == TokenKind::Dot {
            if let TokenKind::Ident(member) = self.peek_at(1).kind.clone() {
                self.bump();
                span = span.to(self.bump().span);
                name = format!("{name}.{member}");
            }
        }
        Ok((name, span))
    }

    /// `use "ruta"` o `use "ruta" como alias`; la palabra `use` ya está en el token actual.
    fn use_decl(&mut self) -> Result<UseDecl, Diagnostic> {
        let start = self.bump().span;
        let TokenKind::Str(parts) = self.peek().kind.clone() else {
            return Err(
                self.error_here("se esperaba la ruta entre comillas después de `use`".into())
            );
        };
        let path_span = self.bump().span;
        let path = match parts.as_slice() {
            [StrPart::Text(text)] if !text.is_empty() => text.clone(),
            _ => {
                return Err(Diagnostic::new(
                    path_span,
                    "la ruta de `use` es un texto fijo, sin `${}` y no vacío",
                ))
            }
        };
        let mut span = start.to(path_span);
        let mut alias = None;
        if matches!(&self.peek().kind, TokenKind::Ident(w) if w == "como") {
            self.bump();
            let (name, name_span) = self.expect_ident("el alias después de `como`")?;
            span = span.to(name_span);
            alias = Some(name);
        }
        Ok(UseDecl { path, alias, span })
    }

    fn program(&mut self) -> Result<Program, Diagnostic> {
        let mut stmts = Vec::new();
        let mut uses = Vec::new();
        let mut exports = Vec::new();
        loop {
            self.skip_newlines();
            if self.peek().kind == TokenKind::Eof {
                return Ok(Program {
                    stmts,
                    uses,
                    exports,
                });
            }
            if matches!(&self.peek().kind, TokenKind::Ident(w) if w == "use")
                && matches!(self.peek_at(1).kind, TokenKind::Str(_))
            {
                uses.push(self.use_decl()?);
                self.end_of_stmt(false)?;
                continue;
            }
            let open = matches!(&self.peek().kind, TokenKind::Ident(w) if w == "open")
                && matches!(
                    self.peek_at(1).kind,
                    TokenKind::Fn
                        | TokenKind::Thing
                        | TokenKind::Kind
                        | TokenKind::Fix
                        | TokenKind::Mut
                );
            if open {
                self.bump();
                if self.peek().kind == TokenKind::Mut {
                    return Err(self.error_here(
                        "una variable `mut` no se puede abrir: otro archivo no podría cambiarla; usa `open fix` o una función".into(),
                    ));
                }
            }
            let stmt = if self.peek().kind == TokenKind::Fn {
                Stmt::Fn(Box::new(self.fn_decl()?))
            } else if self.peek().kind == TokenKind::Thing {
                Stmt::Thing(Box::new(self.thing_decl()?))
            } else if self.peek().kind == TokenKind::Kind {
                Stmt::Kind(Box::new(self.kind_decl()?))
            } else {
                self.stmt()?
            };
            if open {
                match &stmt {
                    Stmt::Fn(d) => exports.push(d.name.clone()),
                    Stmt::Thing(d) => exports.push(d.name.clone()),
                    Stmt::Kind(d) => exports.push(d.name.clone()),
                    Stmt::Let { name, .. } => exports.push(name.clone()),
                    _ => {}
                }
            }
            stmts.push(stmt);
            self.end_of_stmt(false)?;
        }
    }

    fn block(&mut self) -> Result<Block, Diagnostic> {
        let open = self.expect(TokenKind::LBrace, "`{`")?.span;
        self.block_rest(open)
    }

    /// El resto de un bloque cuyo `{` (en `open`) ya se consumió.
    fn block_rest(&mut self, open: Span) -> Result<Block, Diagnostic> {
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

    /// Parámetros de tipo `<T, U: Cota>` tras el nombre de una declaración (opcionales).
    fn type_params(&mut self) -> Result<Vec<TypeParam>, Diagnostic> {
        let mut params: Vec<TypeParam> = Vec::new();
        if self.peek().kind != TokenKind::Lt {
            return Ok(params);
        }
        self.bump();
        loop {
            let (name, name_span) = self.expect_ident("el nombre del parámetro de tipo")?;
            if params.iter().any(|p| p.name == name) {
                return Err(Diagnostic::new(
                    name_span,
                    format!("el parámetro de tipo `{name}` está repetido"),
                ));
            }
            let bound = if self.peek().kind == TokenKind::Colon {
                self.bump();
                Some(self.type_ref()?)
            } else {
                None
            };
            let span = bound.as_ref().map_or(name_span, |b| name_span.to(b.span));
            params.push(TypeParam {
                name,
                name_span,
                bound,
                span,
            });
            if self.peek().kind == TokenKind::Comma {
                self.bump();
            } else {
                break;
            }
        }
        self.expect(TokenKind::Gt, "`>` para cerrar los parámetros de tipo")?;
        Ok(params)
    }

    /// `<A, B>` seguido de `(`, o nada: así `a < b` no se confunde con argumentos de tipo.
    fn try_type_args(&mut self) -> Option<Vec<TypeRef>> {
        if self.peek().kind != TokenKind::Lt {
            return None;
        }
        self.bump();
        let mut args = Vec::new();
        loop {
            args.push(self.type_ref().ok()?);
            match self.peek().kind {
                TokenKind::Comma => {
                    self.bump();
                }
                TokenKind::Gt => {
                    self.bump();
                    break;
                }
                _ => return None,
            }
        }
        (self.peek().kind == TokenKind::LParen || self.can_trail()).then_some(args)
    }

    fn type_ref(&mut self) -> Result<TypeRef, Diagnostic> {
        let mut ty = self.base_type_ref()?;
        // `T?`; con varios (`T??`) el verificador explica que no tiene sentido.
        while self.peek().kind == TokenKind::Question {
            let q = self.bump().span;
            ty = TypeRef {
                name: "Nullable".to_string(),
                span: ty.span.to(q),
                args: vec![ty],
            };
        }
        Ok(ty)
    }

    fn base_type_ref(&mut self) -> Result<TypeRef, Diagnostic> {
        // `(Int, Str) -> Bool` (tipo de función) o `(T)` (agrupación, para `((Int) -> Int)?`).
        if self.peek().kind == TokenKind::LParen {
            let open = self.bump().span;
            let mut items = Vec::new();
            if self.peek().kind != TokenKind::RParen {
                loop {
                    items.push(self.type_ref()?);
                    if self.peek().kind == TokenKind::Comma {
                        self.bump();
                    } else {
                        break;
                    }
                }
            }
            let close = self.expect(TokenKind::RParen, "`,` o `)` en el tipo")?.span;
            if self.peek().kind == TokenKind::Arrow {
                self.bump();
                let ret = self.type_ref()?;
                let span = open.to(ret.span);
                items.push(ret);
                return Ok(TypeRef {
                    name: "Fn".to_string(),
                    args: items,
                    span,
                });
            }
            if items.len() == 1 {
                return Ok(items.pop().expect("un elemento"));
            }
            return Err(Diagnostic::new(
                open.to(close),
                "un tipo de función necesita `->` y el tipo que devuelve, por ejemplo `(Int) -> Int`",
            ));
        }
        if self.peek().kind == TokenKind::LBracket {
            let open = self.bump().span;
            let elem = self.type_ref()?;
            let close = self.expect(TokenKind::RBracket, "`]` para cerrar el tipo")?;
            return Ok(TypeRef {
                name: "Array".to_string(),
                args: vec![elem],
                span: open.to(close.span),
            });
        }
        let (name, span) = self.qualified_ident("un nombre de tipo")?;
        // `Lista<Int>`, `Mapa<Str, [Int]>`
        if self.peek().kind == TokenKind::Lt {
            self.bump();
            let mut args = Vec::new();
            loop {
                args.push(self.type_ref()?);
                if self.peek().kind == TokenKind::Comma {
                    self.bump();
                } else {
                    break;
                }
            }
            let close = self.expect(TokenKind::Gt, "`>` para cerrar los argumentos de tipo")?;
            return Ok(TypeRef {
                name,
                args,
                span: span.to(close.span),
            });
        }
        Ok(TypeRef {
            name,
            args: Vec::new(),
            span,
        })
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
            TokenKind::Attempt => self.attempt_stmt(),
            TokenKind::Fail => {
                let start = self.bump().span;
                let value = self.expr(0)?;
                let span = start.to(value.span());
                Ok(Stmt::Fail(value, span))
            }
            TokenKind::Rescue => Err(self.error_here(
                "`rescue` solo puede ir después de un bloque `attempt { ... }`".into(),
            )),
            TokenKind::Thing => Err(self.error_here(
                "las clases solo se pueden declarar al nivel superior del archivo".into(),
            )),
            TokenKind::Kind => Err(self.error_here(
                "las interfaces solo se pueden declarar al nivel superior del archivo".into(),
            )),
            TokenKind::Else => {
                Err(self.error_here("`else` debe ir en la misma línea que la `}` del `if`".into()))
            }
            TokenKind::Ident(_) if assign_op(&self.peek_at(1).kind).is_some() => self.assign_stmt(),
            _ => {
                let expr = self.expr(0)?;
                // `a[i] = v` y `a[i] += v`
                if let (Expr::Index { .. }, Some(op)) = (&expr, assign_op(&self.peek().kind)) {
                    self.bump();
                    let value = self.expr(0)?;
                    let Expr::Index {
                        target,
                        index,
                        span,
                    } = expr
                    else {
                        unreachable!("verificado arriba")
                    };
                    let span = span.to(value.span());
                    return Ok(Stmt::IndexAssign {
                        target: *target,
                        index: *index,
                        op,
                        value,
                        span,
                    });
                }
                // `obj.campo = v` y `obj.campo += v`
                if let (Expr::Field { .. }, Some(op)) = (&expr, assign_op(&self.peek().kind)) {
                    self.bump();
                    let value = self.expr(0)?;
                    let Expr::Field {
                        target,
                        name,
                        name_span,
                        span,
                    } = expr
                    else {
                        unreachable!("verificado arriba")
                    };
                    let span = span.to(value.span());
                    return Ok(Stmt::FieldAssign {
                        target: *target,
                        name,
                        name_span,
                        op,
                        value,
                        span,
                    });
                }
                Ok(Stmt::Expr(expr))
            }
        }
    }

    /// `thing Nombre(params) { campos y métodos }`; el cuerpo es opcional.
    fn thing_decl(&mut self) -> Result<ThingDecl, Diagnostic> {
        let start = self.bump().span;
        let (name, name_span) = self.expect_ident("el nombre de la clase")?;
        let type_params = self.type_params()?;
        self.expect(TokenKind::LParen, "`(` con los parámetros del constructor")?;
        let mut params: Vec<CtorParam> = Vec::new();
        if self.peek().kind != TokenKind::RParen {
            loop {
                let mutable = match self.peek().kind {
                    TokenKind::Fix => {
                        self.bump();
                        Some(false)
                    }
                    TokenKind::Mut => {
                        self.bump();
                        Some(true)
                    }
                    _ => None,
                };
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
                params.push(CtorParam {
                    name: pname,
                    name_span: pspan,
                    ty,
                    mutable,
                    span,
                });
                if self.peek().kind == TokenKind::Comma {
                    self.bump();
                } else {
                    break;
                }
            }
        }
        let mut end = self.expect(TokenKind::RParen, "`,` o `)`")?.span;
        let mut supers = Vec::new();
        if self.peek().kind == TokenKind::Colon {
            self.bump();
            loop {
                let (sname, sspan) = self.qualified_ident("el nombre de una clase o interfaz")?;
                let mut type_args = Vec::new();
                let mut head = sspan;
                if self.peek().kind == TokenKind::Lt {
                    self.bump();
                    loop {
                        type_args.push(self.type_ref()?);
                        if self.peek().kind == TokenKind::Comma {
                            self.bump();
                        } else {
                            break;
                        }
                    }
                    head = sspan.to(self
                        .expect(TokenKind::Gt, "`>` para cerrar los argumentos de tipo")?
                        .span);
                }
                let (args, span) = if self.peek().kind == TokenKind::LParen {
                    let (args, close) = self.call_args()?;
                    (Some(args), sspan.to(close))
                } else {
                    (None, head)
                };
                end = span;
                supers.push(SuperRef {
                    name: sname,
                    name_span: sspan,
                    type_args,
                    args,
                    span,
                });
                if self.peek().kind == TokenKind::Comma {
                    self.bump();
                } else {
                    break;
                }
            }
        }
        let mut fields = Vec::new();
        let mut methods = Vec::new();
        if self.peek().kind == TokenKind::LBrace {
            let open = self.bump().span;
            loop {
                self.skip_newlines();
                match self.peek().kind.clone() {
                    TokenKind::RBrace => {
                        end = self.bump().span;
                        break;
                    }
                    TokenKind::Eof => {
                        return Err(Diagnostic::new(open, "esta clase no se cierra: falta `}`"));
                    }
                    TokenKind::Fix | TokenKind::Mut => {
                        let Stmt::Let {
                            name,
                            name_span,
                            mutable,
                            ty,
                            init,
                            span,
                        } = self.let_stmt()?
                        else {
                            unreachable!("let_stmt devuelve Let")
                        };
                        fields.push(FieldDecl {
                            name,
                            name_span,
                            mutable,
                            ty,
                            init,
                            span,
                        });
                    }
                    TokenKind::Fn => {
                        methods.push(self.fn_decl()?);
                    }
                    other => {
                        return Err(self.error_here(format!(
                            "dentro de una clase se esperaba un campo (`fix`/`mut`) o un método (`fn`), pero se encontró {}",
                            other.describe()
                        )));
                    }
                }
                if self.peek().kind == TokenKind::Eof {
                    return Err(Diagnostic::new(open, "esta clase no se cierra: falta `}`"));
                }
                self.end_of_stmt(true)?;
            }
        }
        Ok(ThingDecl {
            name,
            name_span,
            type_params,
            params,
            supers,
            fields,
            methods,
            span: start.to(end),
        })
    }

    /// `kind Nombre { fn m(params) -> T ... }`: solo firmas.
    fn kind_decl(&mut self) -> Result<KindDecl, Diagnostic> {
        let start = self.bump().span;
        let (name, name_span) = self.expect_ident("el nombre de la interfaz")?;
        let type_params = self.type_params()?;
        let open = self.expect(TokenKind::LBrace, "`{`")?.span;
        let mut methods = Vec::new();
        let end;
        loop {
            self.skip_newlines();
            match self.peek().kind.clone() {
                TokenKind::RBrace => {
                    end = self.bump().span;
                    break;
                }
                TokenKind::Eof => {
                    return Err(Diagnostic::new(
                        open,
                        "esta interfaz no se cierra: falta `}`",
                    ));
                }
                TokenKind::Fn => {
                    let fstart = self.bump().span;
                    let (mname, mspan) = self.expect_ident("el nombre del método")?;
                    let params = self.params()?;
                    let ret = if self.peek().kind == TokenKind::Arrow {
                        self.bump();
                        Some(self.type_ref()?)
                    } else {
                        None
                    };
                    if matches!(self.peek().kind, TokenKind::LBrace | TokenKind::Assign) {
                        return Err(self.error_here(
                            "los métodos de una interfaz no llevan cuerpo; lo implementan las clases".into(),
                        ));
                    }
                    let span = fstart.to(ret.as_ref().map_or(mspan, |t| t.span));
                    methods.push(KindMethod {
                        name: mname,
                        name_span: mspan,
                        params,
                        ret,
                        span,
                    });
                }
                other => {
                    return Err(self.error_here(format!(
                        "dentro de una interfaz se esperaba un método (`fn`), pero se encontró {}",
                        other.describe()
                    )));
                }
            }
            if self.peek().kind == TokenKind::Eof {
                return Err(Diagnostic::new(
                    open,
                    "esta interfaz no se cierra: falta `}`",
                ));
            }
            self.end_of_stmt(true)?;
        }
        Ok(KindDecl {
            name,
            name_span,
            type_params,
            methods,
            span: start.to(end),
        })
    }

    /// Lista de parámetros `(a: T, b: U)`; el siguiente token es `(`.
    fn params(&mut self) -> Result<Vec<Param>, Diagnostic> {
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
        Ok(params)
    }

    /// `attempt { ... } rescue e: Tipo { ... } rescue ...`; los `rescue` pueden ir en la línea siguiente.
    fn attempt_stmt(&mut self) -> Result<Stmt, Diagnostic> {
        let start = self.bump().span;
        let body = self.block()?;
        let mut end = body.span;
        let mut rescues = Vec::new();
        loop {
            let saved = self.pos;
            self.skip_newlines();
            if self.peek().kind != TokenKind::Rescue {
                self.pos = saved;
                break;
            }
            let keyword = self.bump().span;
            let (name, name_span) = self.expect_ident("el nombre de la variable del error")?;
            self.expect(TokenKind::Colon, "`:` y el tipo del error")?;
            let ty = self.type_ref()?;
            let block = self.block()?;
            end = block.span;
            rescues.push(Rescue {
                name,
                name_span,
                ty,
                span: keyword.to(block.span),
                body: block,
            });
        }
        if rescues.is_empty() {
            return Err(Diagnostic::new(
                start,
                "un `attempt` necesita al menos un `rescue` después de su bloque",
            ));
        }
        Ok(Stmt::Attempt {
            body,
            rescues,
            span: start.to(end),
        })
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
        let cond = self.condition_expr()?;
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

    /// Condición de `if`/`while`: una `{` tras ella abre el cuerpo, no es una lambda final.
    fn condition_expr(&mut self) -> Result<Expr, Diagnostic> {
        let saved = std::mem::replace(&mut self.no_trailing, true);
        let cond = self.expr(0);
        self.no_trailing = saved;
        cond
    }

    /// Analiza `f` con las lambdas finales permitidas (dentro de paréntesis y corchetes).
    fn nested<T>(
        &mut self,
        f: impl FnOnce(&mut Self) -> Result<T, Diagnostic>,
    ) -> Result<T, Diagnostic> {
        let saved = std::mem::replace(&mut self.no_trailing, false);
        let result = f(self);
        self.no_trailing = saved;
        result
    }

    /// Lambda final (`lista.map { x -> x }`) si el siguiente token es `{` y está permitida.
    fn trailing_lambda(&mut self) -> Result<Option<Expr>, Diagnostic> {
        if !self.can_trail() {
            return Ok(None);
        }
        let open = self.bump().span;
        self.lambda(open).map(Some)
    }

    /// Lambda `{ a, b: Int -> cuerpo }`; el `{` ya se consumió. Sin `->` no tiene parámetros.
    fn lambda(&mut self, open: Span) -> Result<Expr, Diagnostic> {
        let saved = self.pos;
        let params = match self.lambda_params() {
            Some(params) => params,
            None => {
                self.pos = saved;
                Vec::new()
            }
        };
        let (fn_depth, loop_depth) = (self.fn_depth, self.loop_depth);
        self.fn_depth = 1;
        self.loop_depth = 0;
        let body = self.nested(|p| p.block_rest(open));
        self.fn_depth = fn_depth;
        self.loop_depth = loop_depth;
        let body = body?;
        let span = body.span;
        Ok(Expr::Lambda { params, body, span })
    }

    /// `a, b: Int ->`; `None` si lo que sigue no es una lista de parámetros.
    fn lambda_params(&mut self) -> Option<Vec<LambdaParam>> {
        let mut params: Vec<LambdaParam> = Vec::new();
        if self.peek().kind == TokenKind::Arrow {
            self.bump();
            return Some(params);
        }
        loop {
            let TokenKind::Ident(name) = self.peek().kind.clone() else {
                return None;
            };
            let name_span = self.bump().span;
            if params.iter().any(|p| p.name == name) {
                return None;
            }
            let ty = if self.peek().kind == TokenKind::Colon {
                self.bump();
                Some(self.type_ref().ok()?)
            } else {
                None
            };
            params.push(LambdaParam {
                name,
                name_span,
                ty,
            });
            match self.peek().kind {
                TokenKind::Comma => {
                    self.bump();
                }
                TokenKind::Arrow => {
                    self.bump();
                    return Some(params);
                }
                _ => return None,
            }
        }
    }

    fn while_stmt(&mut self) -> Result<Stmt, Diagnostic> {
        let start = self.bump().span;
        let cond = self.condition_expr()?;
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
        let type_params = self.type_params()?;
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

        let expr_body = self.peek().kind == TokenKind::Assign;
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
            type_params,
            params,
            ret,
            expr_body,
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
            TokenKind::Str(parts) => self.string_expr(parts, token.span)?,
            TokenKind::LBracket => self.array_literal(token.span)?,
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
                let inner = self.nested(|p| p.expr(0))?;
                self.expect(TokenKind::RParen, "`)`")?;
                inner
            }
            TokenKind::LBrace => self.lambda(token.span)?,
            TokenKind::This => Expr::This(token.span),
            TokenKind::Nil => Expr::Nil(token.span),
            TokenKind::Super => {
                self.expect(TokenKind::Dot, "`.` después de `super`")?;
                let (name, name_span) =
                    self.expect_ident("el nombre de un método de la clase padre")?;
                if self.peek().kind != TokenKind::LParen {
                    return Err(self.error_here(
                        "`super` solo se usa para llamar métodos: `super.metodo(...)`".into(),
                    ));
                }
                let (args, close) = self.call_args()?;
                Expr::SuperCall {
                    name,
                    name_span,
                    args,
                    span: token.span.to(close),
                }
            }
            TokenKind::Ident(name) => {
                if self.peek().kind == TokenKind::LParen {
                    self.call(name, Vec::new(), token.span)?
                } else if let Some(lambda) = self.trailing_lambda()? {
                    // `repite { ... }`: llamada cuyo único argumento es la lambda.
                    let span = token.span.to(lambda.span());
                    Expr::Call {
                        name,
                        type_args: Vec::new(),
                        args: vec![lambda],
                        span,
                    }
                } else if self.peek().kind == TokenKind::Lt {
                    // `f<Int>(x)` o `Lista<Int>()`; si no encaja, era una comparación.
                    let saved = self.pos;
                    match self.try_type_args() {
                        Some(type_args) => self.call(name, type_args, token.span)?,
                        None => {
                            self.pos = saved;
                            Expr::Var(name, token.span)
                        }
                    }
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

        // Postfijos `a[i]`, `a.campo` y `a.metodo(args)`, los más fuertes de todos.
        loop {
            match self.peek().kind {
                TokenKind::LBracket => {
                    self.bump();
                    let index = self.nested(|p| p.expr(0))?;
                    let close = self.expect(TokenKind::RBracket, "`]`")?;
                    let span = lhs.span().to(close.span);
                    lhs = Expr::Index {
                        target: Box::new(lhs),
                        index: Box::new(index),
                        span,
                    };
                }
                TokenKind::LParen => {
                    // `f(1)(2)`, `obj.campo(1)(2)`: llama al valor de tipo función de la izquierda.
                    let (args, close) = self.call_args_trailing()?;
                    let span = lhs.span().to(close);
                    lhs = Expr::CallValue {
                        callee: Box::new(lhs),
                        args,
                        span,
                    };
                }
                TokenKind::QDot => {
                    self.bump();
                    let (name, name_span) =
                        self.expect_ident("el nombre de un campo o método después de `?.`")?;
                    if self.peek().kind == TokenKind::LParen || self.can_trail() {
                        let (args, close) = self.call_args_trailing()?;
                        let span = lhs.span().to(close);
                        lhs = Expr::SafeCall {
                            target: Box::new(lhs),
                            name,
                            name_span,
                            args,
                            span,
                        };
                    } else {
                        let span = lhs.span().to(name_span);
                        lhs = Expr::SafeField {
                            target: Box::new(lhs),
                            name,
                            name_span,
                            span,
                        };
                    }
                }
                TokenKind::Dot => {
                    self.bump();
                    let (name, name_span) =
                        self.expect_ident("el nombre de un campo o método después de `.`")?;
                    // `obj.metodo<Int>(x)`; si no encaja, `<` era una comparación.
                    let mut type_args = Vec::new();
                    if self.peek().kind == TokenKind::Lt {
                        let saved = self.pos;
                        match self.try_type_args() {
                            Some(found) => type_args = found,
                            None => self.pos = saved,
                        }
                    }
                    if self.peek().kind == TokenKind::LParen || self.can_trail() {
                        let (args, close) = self.call_args_trailing()?;
                        let span = lhs.span().to(close);
                        lhs = Expr::MethodCall {
                            target: Box::new(lhs),
                            name,
                            name_span,
                            type_args,
                            args,
                            span,
                        };
                    } else {
                        let span = lhs.span().to(name_span);
                        lhs = Expr::Field {
                            target: Box::new(lhs),
                            name,
                            name_span,
                            span,
                        };
                    }
                }
                _ => break,
            }
        }

        loop {
            // `x is T` (al nivel de las comparaciones) y `x as T` (más fuerte que la aritmética).
            if matches!(self.peek().kind, TokenKind::Is | TokenKind::As) {
                let is = self.peek().kind == TokenKind::Is;
                if (if is { 7 } else { 14 }) < min_bp {
                    break;
                }
                self.bump();
                let ty = self.type_ref()?;
                let span = lhs.span().to(ty.span);
                lhs = if is {
                    Expr::Is {
                        expr: Box::new(lhs),
                        ty,
                        span,
                    }
                } else {
                    Expr::Cast {
                        expr: Box::new(lhs),
                        ty,
                        span,
                    }
                };
                continue;
            }
            // `a ?: b`, el operador de menor precedencia.
            if self.peek().kind == TokenKind::Elvis {
                if 0 < min_bp {
                    break;
                }
                self.bump();
                let rhs = self.expr(1)?;
                let span = lhs.span().to(rhs.span());
                lhs = Expr::Elvis {
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                    span,
                };
                continue;
            }
            let Some((op, left_bp, right_bp)) = binary_op(&self.peek().kind) else {
                break;
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

    /// Cadena literal: sin interpolaciones es `Str`; con ellas, `Interp`.
    fn string_expr(&mut self, parts: Vec<StrPart>, span: Span) -> Result<Expr, Diagnostic> {
        let mut out = Vec::new();
        for part in parts {
            match part {
                StrPart::Text(text) => out.push(InterpPart::Text(text)),
                StrPart::Code(tokens) => {
                    let mut inner = Parser {
                        tokens,
                        pos: 0,
                        fn_depth: self.fn_depth,
                        loop_depth: self.loop_depth,
                        no_trailing: false,
                    };
                    let expr = inner.expr(0)?;
                    if inner.peek().kind != TokenKind::RBrace {
                        return Err(inner.error_here(format!(
                            "la interpolación `${{...}}` admite una sola expresión, pero se encontró {}",
                            inner.peek().kind.describe()
                        )));
                    }
                    out.push(InterpPart::Expr(expr));
                }
            }
        }
        Ok(match out.as_slice() {
            [InterpPart::Text(text)] => Expr::Str(text.clone(), span),
            _ => Expr::Interp { parts: out, span },
        })
    }

    /// Literal `[a, b, c]`; el `[` ya se consumió. Admite saltos de línea entre elementos.
    fn array_literal(&mut self, open: Span) -> Result<Expr, Diagnostic> {
        let mut elems = Vec::new();
        loop {
            self.skip_newlines();
            if self.peek().kind == TokenKind::RBracket {
                break;
            }
            elems.push(self.nested(|p| p.expr(0))?);
            self.skip_newlines();
            if self.peek().kind == TokenKind::Comma {
                self.bump();
            } else {
                break;
            }
        }
        self.skip_newlines();
        let close = self.expect(TokenKind::RBracket, "`,` o `]`")?;
        Ok(Expr::Array {
            elems,
            span: open.to(close.span),
        })
    }

    /// Llamada `nombre(args)`; el nombre ya se consumió y el siguiente token es `(`.
    fn call(
        &mut self,
        name: String,
        type_args: Vec<TypeRef>,
        name_span: Span,
    ) -> Result<Expr, Diagnostic> {
        let (args, close) = self.call_args_trailing()?;
        Ok(Expr::Call {
            name,
            type_args,
            args,
            span: name_span.to(close),
        })
    }

    /// ¿Sigue una lambda final sin paréntesis (`lista.map { ... }`)?
    fn can_trail(&self) -> bool {
        !self.no_trailing && self.peek().kind == TokenKind::LBrace
    }

    /// Argumentos `(a, b)` y/o una lambda final: `f(a) { ... }`, `x.m { ... }`. Devuelve también la
    /// posición del último token.
    fn call_args_trailing(&mut self) -> Result<(Vec<Expr>, Span), Diagnostic> {
        let (mut args, mut end) = if self.peek().kind == TokenKind::LParen {
            self.call_args()?
        } else {
            (Vec::new(), self.peek().span)
        };
        if let Some(lambda) = self.trailing_lambda()? {
            end = lambda.span();
            args.push(lambda);
        }
        Ok((args, end))
    }

    /// Argumentos `(a, b)`; el siguiente token es `(`. Devuelve también la posición del `)`.
    fn call_args(&mut self) -> Result<(Vec<Expr>, Span), Diagnostic> {
        self.bump();
        let mut args = Vec::new();
        if self.peek().kind != TokenKind::RParen {
            loop {
                args.push(self.nested(|p| p.expr(0))?);
                if self.peek().kind == TokenKind::Comma {
                    self.bump();
                } else {
                    break;
                }
            }
        }
        let close = self.expect(TokenKind::RParen, "`,` o `)`")?;
        Ok((args, close.span))
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
            Expr::Str(text, _) => format!("{text:?}"),
            Expr::Interp { parts, .. } => {
                let parts: Vec<String> = parts
                    .iter()
                    .map(|part| match part {
                        InterpPart::Text(text) => format!("{text:?}"),
                        InterpPart::Expr(e) => sexp(e),
                    })
                    .collect();
                format!("(interp {})", parts.join(" "))
            }
            Expr::Array { elems, .. } => {
                let elems: Vec<String> = elems.iter().map(sexp).collect();
                format!("[{}]", elems.join(" "))
            }
            Expr::Index { target, index, .. } => {
                format!("(index {} {})", sexp(target), sexp(index))
            }
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
            Expr::This(_) => "this".to_string(),
            Expr::Nil(_) => "nil".to_string(),
            Expr::SafeField { target, name, .. } => format!("(?. {} {name})", sexp(target)),
            Expr::SafeCall {
                target, name, args, ..
            } => {
                let args: Vec<String> = args.iter().map(sexp).collect();
                format!("(?. {} {name} {})", sexp(target), args.join(" ")).replace(" )", ")")
            }
            Expr::Elvis { lhs, rhs, .. } => format!("(?: {} {})", sexp(lhs), sexp(rhs)),
            Expr::SuperCall { name, args, .. } => {
                let args: Vec<String> = args.iter().map(sexp).collect();
                format!("(super {name} {})", args.join(" ")).replace(" )", ")")
            }
            Expr::Is { expr, ty, .. } => format!("(is {} {})", sexp(expr), ty.name),
            Expr::Cast { expr, ty, .. } => format!("(as {} {})", sexp(expr), ty.name),
            Expr::Field { target, name, .. } => format!("(. {} {name})", sexp(target)),
            Expr::MethodCall {
                target, name, args, ..
            } => {
                let args: Vec<String> = args.iter().map(sexp).collect();
                format!("(. {} {name} {})", sexp(target), args.join(" ")).replace(" )", ")")
            }
            Expr::CallValue { callee, args, .. } => {
                let args: Vec<String> = args.iter().map(sexp).collect();
                format!("(callv {} {})", sexp(callee), args.join(" ")).replace(" )", ")")
            }
            Expr::Lambda { params, body, .. } => {
                let names: Vec<String> = params
                    .iter()
                    .map(|p| match &p.ty {
                        Some(ty) => format!("{}:{}", p.name, ty.name),
                        None => p.name.clone(),
                    })
                    .collect();
                let body: Vec<String> = body
                    .stmts
                    .iter()
                    .map(|s| match s {
                        Stmt::Expr(e) => sexp(e),
                        _ => "stmt".to_string(),
                    })
                    .collect();
                format!("(lambda [{}] {})", names.join(" "), body.join(" "))
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

    // --- M3: cadenas, arreglos e indexación ---

    #[test]
    fn cadena_simple_y_con_escapes() {
        assert_eq!(shape(r#""hola""#), r#""hola""#);
        assert_eq!(shape(r#""a\nb\t\"c\" \$ \u{f1}""#), r#""a\nb\t\"c\" $ ñ""#);
    }

    #[test]
    fn interpolacion() {
        assert_eq!(
            shape(r#""x = ${1 + 2}!""#),
            r#"(interp "x = " (+ 1 2) "!")"#
        );
        assert_eq!(shape(r#""${a}${b}""#), "(interp a b)");
        assert_eq!(
            shape(r#""${f("in ${x}")}""#),
            r#"(interp (call f (interp "in " x)))"#
        );
        assert_eq!(shape(r#""$5 y {llaves}""#), r#""$5 y {llaves}""#);
    }

    #[test]
    fn arreglos_e_indices() {
        assert_eq!(shape("[1, 2, 3]"), "[1 2 3]");
        assert_eq!(shape("[]"), "[]");
        assert_eq!(shape("[\n 1,\n 2,\n]"), "[1 2]");
        assert_eq!(shape("a[0][1]"), "(index (index a 0) 1)");
        assert_eq!(shape("-a[0] * 2"), "(* (neg (index a 0)) 2)");
        assert_eq!(shape("f(1)[2]"), "(index (call f 1) 2)");
    }

    #[test]
    fn asignacion_a_indice() {
        let program = parse("a[i + 1] += 2").unwrap();
        assert!(matches!(
            &program.stmts[0],
            Stmt::IndexAssign {
                op: Some(BinOp::Add),
                ..
            }
        ));
        let program = parse("a[0] = 1").unwrap();
        assert!(matches!(
            &program.stmts[0],
            Stmt::IndexAssign { op: None, .. }
        ));
    }

    #[test]
    fn tipo_arreglo() {
        match &parse("fix a: [[Int]] = []").unwrap().stmts[0] {
            Stmt::Let { ty: Some(ty), .. } => {
                assert_eq!(ty.name, "Array");
                assert_eq!(ty.args[0].args[0].name, "Int");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn errores_de_cadenas() {
        assert_eq!(err("show(\"abc"), "esta cadena no se cierra: falta `\"`");
        assert!(err(r#"show("a\qb")"#).contains("escape no válido"));
        assert!(err(r#"show("${1 + }")"#).contains("se esperaba una expresión"));
        assert_eq!(
            err(r#"show("${1"#),
            "esta interpolación no se cierra: falta `}`"
        );
        assert!(err(r#"show("${1 2}")"#).contains("una sola expresión"));
    }

    #[test]
    fn la_posicion_de_un_error_dentro_de_una_interpolacion_es_absoluta() {
        let src = "show(\"ab ${1 + } cd\")";
        let d = parse(src).unwrap_err();
        assert_eq!(&src[d.span.start..d.span.end], "}");
    }

    // --- M4: clases, campos y métodos ---

    #[test]
    fn acceso_a_campos_y_llamadas_a_metodos() {
        assert_eq!(shape("a.b"), "(. a b)");
        assert_eq!(shape("a.b.c"), "(. (. a b) c)");
        assert_eq!(shape("a.f(1, 2)"), "(. a f 1 2)");
        assert_eq!(shape("a.f().g"), "(. (. a f) g)");
        assert_eq!(shape("a[0].x"), "(. (index a 0) x)");
        assert_eq!(shape("this.x + 1"), "(+ (. this x) 1)");
        assert_eq!(shape("-a.x"), "(neg (. a x))");
        assert_eq!(shape("f(1).x"), "(. (call f 1) x)");
    }

    #[test]
    fn asignacion_a_campos() {
        let program = parse("a.b.c += 2").unwrap();
        assert!(matches!(
            &program.stmts[0],
            Stmt::FieldAssign {
                op: Some(BinOp::Add),
                ..
            }
        ));
        let program = parse("this.x = 1").unwrap();
        assert!(matches!(
            &program.stmts[0],
            Stmt::FieldAssign { op: None, .. }
        ));
    }

    #[test]
    fn declaracion_de_clase() {
        let program = parse(
            "thing P(fix x: Int, mut y: Int, z: Str) {\n  mut n = 0\n  fix t: Str = \"a\"\n  fn f() -> Int = x\n  fn g(a: Int) { n += a }\n}",
        )
        .unwrap();
        let Stmt::Thing(t) = &program.stmts[0] else {
            panic!("no es una clase")
        };
        assert_eq!(t.name, "P");
        let kinds: Vec<Option<bool>> = t.params.iter().map(|p| p.mutable).collect();
        assert_eq!(kinds, vec![Some(false), Some(true), None]);
        assert_eq!(t.fields.len(), 2);
        assert!(t.fields[0].mutable && !t.fields[1].mutable);
        assert!(t.fields[1].ty.is_some());
        assert_eq!(t.methods.len(), 2);
        assert!(t.methods[0].expr_body);
    }

    #[test]
    fn clase_sin_cuerpo_y_con_cuerpo_vacio() {
        assert!(matches!(
            parse("thing A()").unwrap().stmts[0],
            Stmt::Thing(_)
        ));
        assert!(matches!(
            parse("thing A() { }").unwrap().stmts[0],
            Stmt::Thing(_)
        ));
        assert_eq!(parse("thing A()\nthing B()").unwrap().stmts.len(), 2);
    }

    #[test]
    fn errores_de_clases() {
        assert!(err("thing").contains("el nombre de la clase"));
        assert!(err("thing A").contains("`(`"));
        assert!(err("thing A(x)").contains("`:` y el tipo"));
        assert!(err("thing A(x: Int, x: Int)").contains("repetido"));
        assert!(err("thing A() {\n show(1)\n}").contains("se esperaba un campo"));
        assert!(err("thing A() {\n fix x = 1").contains("no se cierra"));
        assert!(
            err("fn f() {\n thing A()\n}").contains("solo se pueden declarar al nivel superior")
        );
        assert!(err("a.").contains("después de `.`"));
        assert!(err("a.1").contains("después de `.`"));
        assert!(err("thing A() { fix x }").contains("`=`"));
        assert!(err("thing A() { fix x = 1 fix y = 2 }").contains("fin de la sentencia"));
    }

    // --- M5: kind, herencia, is/as/super ---

    #[test]
    fn is_y_as_con_su_precedencia() {
        assert_eq!(shape("a is B"), "(is a B)");
        assert_eq!(shape("a as B"), "(as a B)");
        assert_eq!(shape("a is B && c"), "(&& (is a B) c)");
        assert_eq!(shape("a.b is B"), "(is (. a b) B)");
        assert_eq!(shape("(a as B).x"), "(. (as a B) x)");
        assert_eq!(shape("a as B == c"), "(== (as a B) c)");
        assert_eq!(shape("1 + 2 is B"), "(is (+ 1 2) B)");
        assert_eq!(shape("a is B == true"), "(== (is a B) true)");
        assert_eq!(shape("a as [B]"), "(as a Array)");
    }

    #[test]
    fn llamada_a_super() {
        assert_eq!(shape("super.f(1, 2)"), "(super f 1 2)");
        assert_eq!(shape("super.f()"), "(super f)");
        assert_eq!(shape("super.f().g"), "(. (super f) g)");
        assert!(err("super").contains("`.` después de `super`"));
        assert!(err("super.x").contains("solo se usa para llamar métodos"));
    }

    #[test]
    fn cabecera_con_padre_e_interfaces() {
        let program = parse("thing P(n: Str) : Animal(n, 4), Habla, Corre { }").unwrap();
        let Stmt::Thing(t) = &program.stmts[0] else {
            panic!("no es una clase")
        };
        assert_eq!(t.supers.len(), 3);
        assert_eq!(t.supers[0].name, "Animal");
        assert_eq!(t.supers[0].args.as_ref().map(Vec::len), Some(2));
        assert!(t.supers[1].args.is_none() && t.supers[2].args.is_none());
        let program = parse("thing P() : Q()").unwrap();
        let Stmt::Thing(t) = &program.stmts[0] else {
            panic!("no es una clase")
        };
        assert_eq!(t.supers.len(), 1);
    }

    #[test]
    fn declaracion_de_interfaz() {
        let program =
            parse("kind Figura {\n  fn area() -> Float\n  fn mueve(dx: Int, dy: Int)\n}").unwrap();
        let Stmt::Kind(k) = &program.stmts[0] else {
            panic!("no es una interfaz")
        };
        assert_eq!(k.name, "Figura");
        assert_eq!(k.methods.len(), 2);
        assert!(k.methods[0].ret.is_some() && k.methods[1].ret.is_none());
        assert_eq!(k.methods[1].params.len(), 2);
        assert!(parse("kind K { }").is_ok());
    }

    #[test]
    fn errores_de_interfaces_y_herencia() {
        assert!(err("kind").contains("el nombre de la interfaz"));
        assert!(err("kind K { fn f() { } }").contains("no llevan cuerpo"));
        assert!(err("kind K { fn f() = 1 }").contains("no llevan cuerpo"));
        assert!(err("kind K { fix x = 1 }").contains("se esperaba un método"));
        assert!(err("kind K {\n fn f()").contains("no se cierra"));
        assert!(
            err("fn f() {\n kind K { }\n}").contains("solo se pueden declarar al nivel superior")
        );
        assert!(err("thing A() :").contains("una clase o interfaz"));
        assert!(err("thing A() : B(").contains("una expresión"));
        assert!(err("fix is = 1").contains("el nombre de la variable"));
    }

    // --- M6: excepciones y opcionales ---

    #[test]
    fn nil_acceso_seguro_y_elvis() {
        assert_eq!(shape("nil"), "nil");
        assert_eq!(shape("a?.b"), "(?. a b)");
        assert_eq!(shape("a?.f(1)"), "(?. a f 1)");
        assert_eq!(shape("a?.b?.c"), "(?. (?. a b) c)");
        assert_eq!(shape("a?.b.c"), "(. (?. a b) c)");
        assert_eq!(shape("a ?: b"), "(?: a b)");
        assert_eq!(shape("a ?: b ?: c"), "(?: (?: a b) c)");
        assert_eq!(shape("a ?: b + 1"), "(?: a (+ b 1))");
        assert_eq!(shape("a ?: b || c"), "(?: a (|| b c))");
        assert_eq!(shape("a + b ?: c"), "(?: (+ a b) c)");
        assert_eq!(shape("x == nil"), "(== x nil)");
        assert_eq!(shape("a?.b ?: 0"), "(?: (?. a b) 0)");
    }

    #[test]
    fn tipos_opcionales() {
        let ty = |src: &str| match &parse(src).unwrap().stmts[0] {
            Stmt::Let { ty: Some(ty), .. } => ty.clone(),
            other => panic!("{other:?}"),
        };
        let t = ty("fix a: Perro? = nil");
        assert_eq!(
            (t.name.as_str(), t.args[0].name.as_str()),
            ("Nullable", "Perro")
        );
        let t = ty("fix a: [Int?] = []");
        assert_eq!(t.name, "Array");
        assert_eq!(t.args[0].name, "Nullable");
        let t = ty("fix a: [Int]? = nil");
        assert_eq!(
            (t.name.as_str(), t.args[0].name.as_str()),
            ("Nullable", "Array")
        );
        assert!(parse("fn f(a: Int?, b: Str?) -> Int? = nil").is_ok());
        assert!(parse("thing P(fix a: P?, b: Int?) : Q(nil)").is_ok());
    }

    #[test]
    fn attempt_rescue_y_fail() {
        let program =
            parse("attempt { show(1) }\nrescue e: A { show(2) }\nrescue f: B { show(3) }").unwrap();
        let Stmt::Attempt { rescues, .. } = &program.stmts[0] else {
            panic!("no es un attempt")
        };
        assert_eq!(rescues.len(), 2);
        assert_eq!(rescues[0].name, "e");
        assert_eq!(rescues[1].ty.name, "B");
        // `rescue` en la misma línea
        assert!(parse("attempt { } rescue e: A { }").is_ok());
        // la sentencia siguiente no se confunde con un rescue
        assert_eq!(
            parse("attempt { }\nrescue e: A { }\n\nshow(1)")
                .unwrap()
                .stmts
                .len(),
            2
        );
        let program = parse("fail Error(\"x\", 1)").unwrap();
        assert!(matches!(
            &program.stmts[0],
            Stmt::Fail(Expr::Call { .. }, _)
        ));
        // dentro de funciones y bucles
        assert!(parse(
            "fn f() {\n while true {\n attempt { break }\n rescue e: A { continue }\n }\n}"
        )
        .is_ok());
    }

    #[test]
    fn errores_de_excepciones_y_opcionales() {
        assert!(err("attempt { }").contains("necesita al menos un `rescue`"));
        assert!(err("attempt { }\nshow(1)").contains("necesita al menos un `rescue`"));
        assert!(err("rescue e: A { }").contains("solo puede ir después de un bloque `attempt"));
        assert!(err("attempt { }\nrescue { }").contains("el nombre de la variable del error"));
        assert!(err("attempt { }\nrescue e { }").contains("`:` y el tipo del error"));
        assert!(err("attempt { }\nrescue e: { }").contains("un nombre de tipo"));
        assert!(err("attempt show(1)").contains("`{`"));
        assert!(err("fail").contains("una expresión"));
        assert!(err("a?.").contains("después de `?.`"));
        assert!(err("a ?:").contains("una expresión"));
        assert!(err("fix a: ? = 1").contains("un nombre de tipo"));
    }

    // --- M7: genéricos ---

    #[test]
    fn tipos_con_argumentos() {
        let ty = |src: &str| match &parse(src).unwrap().stmts[0] {
            Stmt::Let { ty: Some(ty), .. } => ty.clone(),
            other => panic!("{other:?}"),
        };
        let t = ty("fix a: Lista<Int> = x");
        assert_eq!((t.name.as_str(), t.args.len()), ("Lista", 1));
        let t = ty("fix a: Mapa<Str, [Int]> = x");
        assert_eq!(t.args.len(), 2);
        assert_eq!(t.args[1].name, "Array");
        let t = ty("fix a: Lista<Lista<Int>> = x");
        assert_eq!(t.args[0].args[0].name, "Int");
        let t = ty("fix a: Lista<Int>? = x");
        assert_eq!(t.name, "Nullable");
        assert_eq!(t.args[0].args.len(), 1);
        let t = ty("fix a: Lista<Int?> = x");
        assert_eq!(t.args[0].name, "Nullable");
        assert!(parse("fn f(a: Mapa<Str, Int>) -> Lista<Int> = x").is_ok());
    }

    #[test]
    fn llamadas_con_argumentos_de_tipo() {
        let call = |src: &str| match &parse(src).unwrap().stmts[0] {
            Stmt::Expr(Expr::Call {
                name,
                type_args,
                args,
                ..
            }) => (name.clone(), type_args.len(), args.len()),
            other => panic!("{other:?}"),
        };
        assert_eq!(call("Lista<Int>()"), ("Lista".to_string(), 1, 0));
        assert_eq!(call("Mapa<Str, Int>()"), ("Mapa".to_string(), 2, 0));
        assert_eq!(call("f<Int>(1, 2)"), ("f".to_string(), 1, 2));
        assert_eq!(call("f(1)"), ("f".to_string(), 0, 1));
        assert_eq!(call("Lista<Lista<Int>>()"), ("Lista".to_string(), 1, 0));
    }

    #[test]
    fn las_comparaciones_no_se_confunden_con_argumentos_de_tipo() {
        assert_eq!(shape("a < b"), "(< a b)");
        assert_eq!(shape("a < b && c > d"), "(&& (< a b) (> c d))");
        assert_eq!(shape("f(a < b, c > d)"), "(call f (< a b) (> c d))");
        assert_eq!(shape("a < b > c"), "(> (< a b) c)");
        assert_eq!(shape("x < f(1)"), "(< x (call f 1))");
        assert_eq!(shape("a < (b)"), "(< a b)");
    }

    #[test]
    fn declaraciones_genericas() {
        let program = parse("thing P<A, B: Ord<A>>(x: A) : Q<A>(x), K<B> { }").unwrap();
        let Stmt::Thing(t) = &program.stmts[0] else {
            panic!("no es una clase")
        };
        assert_eq!(t.type_params.len(), 2);
        assert!(t.type_params[0].bound.is_none());
        assert_eq!(t.type_params[1].bound.as_ref().unwrap().name, "Ord");
        assert_eq!(t.supers[0].type_args.len(), 1);
        assert!(t.supers[0].args.is_some());
        assert_eq!(t.supers[1].type_args.len(), 1);
        assert!(t.supers[1].args.is_none());

        let program = parse("kind K<T> { fn f(x: T) -> T }").unwrap();
        let Stmt::Kind(k) = &program.stmts[0] else {
            panic!("no es una interfaz")
        };
        assert_eq!(k.type_params.len(), 1);

        let program = parse("fn f<T, U: K>(a: T) = a").unwrap();
        let Stmt::Fn(f) = &program.stmts[0] else {
            panic!("no es una función")
        };
        assert_eq!(f.type_params.len(), 2);
        assert!(parse("thing P<T>()").is_ok());
        assert!(parse("thing P()").is_ok());
    }

    #[test]
    fn errores_de_genericos() {
        assert!(err("thing P<>()").contains("el nombre del parámetro de tipo"));
        assert!(err("thing P<T()").contains("`>` para cerrar los parámetros de tipo"));
        assert!(err("thing P<T, T>()").contains("está repetido"));
        assert!(err("fn f<T, T>() { }").contains("está repetido"));
        assert!(parse("thing A() { fn m<T>(x: T) { } }").is_ok());
        assert!(err("fix a: Lista<Int = x").contains("`>` para cerrar los argumentos de tipo"));
        assert!(err("fix a: Lista<> = x").contains("un nombre de tipo"));
        assert!(err("thing A() : B<Int(1)").contains("`>` para cerrar los argumentos de tipo"));
    }

    // --- M8: lambdas y tipos de función ---
    #[test]
    fn lambdas() {
        assert_eq!(shape("{ x -> x + 1 }"), "(lambda [x] (+ x 1))");
        assert_eq!(shape("{ a, b: Int -> a }"), "(lambda [a b:Int] a)");
        assert_eq!(shape("{ -> 1 }"), "(lambda [] 1)");
        assert_eq!(shape("{ 7 }"), "(lambda [] 7)");
        assert_eq!(shape("{ x }"), "(lambda [] x)");
        assert_eq!(shape("f({ x -> x })"), "(call f (lambda [x] x))");
    }

    #[test]
    fn lambdas_finales() {
        assert_eq!(
            shape("lista.map { x -> x }"),
            "(. lista map (lambda [x] x))"
        );
        assert_eq!(shape("f(1) { x -> x }"), "(call f 1 (lambda [x] x))");
        assert_eq!(shape("f { 1 }"), "(call f (lambda [] 1))");
        assert_eq!(
            shape("a.b(1) { x -> x }.c()"),
            "(. (. a b 1 (lambda [x] x)) c)"
        );
        // En la condición de un `if`/`while` la `{` abre el cuerpo.
        assert!(parse("if a.b { x = 1 }").is_ok());
        assert!(parse("while f(x) { x += 1 }").is_ok());
        // Dentro de paréntesis sí es una lambda.
        assert!(parse("if f({ 1 }) == 1 { x = 1 }").is_ok());
    }

    #[test]
    fn tipos_de_funcion() {
        let program =
            parse("fn f(a: (Int, Str) -> Bool, b: () -> Unit) -> (Int) -> Int = a").unwrap();
        let Stmt::Fn(f) = &program.stmts[0] else {
            panic!()
        };
        assert_eq!(f.params[0].ty.name, "Fn");
        assert_eq!(f.params[0].ty.args.len(), 3);
        assert_eq!(f.params[1].ty.args.len(), 1);
        assert_eq!(f.ret.as_ref().unwrap().name, "Fn");
        assert!(parse("fix g: ((Int) -> Int)? = nil").is_ok());
        assert!(parse("fix g: [(Int) -> Int] = []").is_ok());
        assert!(err("fix g: (Int, Int) = nil").contains("necesita `->`"));
    }

    #[test]
    fn return_y_break_en_lambdas() {
        assert!(parse("fix f = { x -> return x }").is_ok());
        assert!(
            err("while true { f({ break }) }").contains("solo se puede usar dentro de un `while`")
        );
    }

    #[test]
    fn argumentos_de_tipo_en_metodos_y_llamadas_a_valores() {
        let program = parse("c.mapea<Float> { x -> x }").unwrap();
        let Stmt::Expr(Expr::MethodCall {
            type_args, args, ..
        }) = &program.stmts[0]
        else {
            panic!()
        };
        assert_eq!(type_args.len(), 1);
        assert_eq!(args.len(), 1);
        assert_eq!(shape("f(1)(2)"), "(callv (call f 1) 2)");
        assert_eq!(shape("a.b(1)(2)"), "(callv (. a b 1) 2)");
        // Una comparación no se confunde con argumentos de tipo.
        assert_eq!(shape("a.b < c"), "(< (. a b) c)");
    }
}
