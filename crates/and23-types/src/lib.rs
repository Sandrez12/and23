//! Verificador de tipos estáticos de And23 (M2). Se ejecuta entre el parser y el compilador:
//! un programa mal tipado se rechaza con su posición antes de generar bytecode.

use and23_syntax::{BinOp, Block, Diagnostic, Expr, FnDecl, Program, Span, Stmt, TypeRef, UnOp};
use std::collections::HashMap;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Type {
    Int,
    Float,
    Bool,
    Unit,
}

impl Type {
    fn from_name(name: &str) -> Option<Type> {
        match name {
            "Int" => Some(Type::Int),
            "Float" => Some(Type::Float),
            "Bool" => Some(Type::Bool),
            "Unit" => Some(Type::Unit),
            _ => None,
        }
    }

    fn is_numeric(self) -> bool {
        matches!(self, Type::Int | Type::Float)
    }
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Type::Int => "Int",
            Type::Float => "Float",
            Type::Bool => "Bool",
            Type::Unit => "Unit",
        })
    }
}

#[derive(Clone)]
struct Sig {
    params: Vec<Type>,
    /// `None` mientras no se infirió (función con cuerpo `= expr` sin `->`).
    ret: Option<Type>,
}

type Vars = HashMap<String, (Type, bool)>;

/// Verificador con estado: recuerda las funciones y globales de las entradas anteriores (REPL).
#[derive(Clone, Default)]
pub struct Checker {
    funcs: HashMap<String, Sig>,
    globals: Vars,
}

impl Checker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Verifica el programa. Si falla, el estado del verificador no cambia.
    pub fn check(&mut self, program: &Program) -> Result<(), Diagnostic> {
        let mut pass = Pass {
            funcs: self.funcs.clone(),
            globals: self.globals.clone(),
            decls: HashMap::new(),
            state: HashMap::new(),
        };
        pass.program(program)?;
        self.funcs = pass.funcs;
        self.globals = pass.globals;
        Ok(())
    }

    /// Olvida globales que no llegaron a definirse (error de ejecución en el REPL).
    pub fn forget_globals(&mut self, names: &[String]) {
        for name in names {
            self.globals.remove(name);
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum State {
    Unchecked,
    InProgress,
    Done,
}

enum Ret {
    /// Nivel superior del archivo.
    Script,
    Declared(Type),
    /// Función con cuerpo `= expr` sin `->`: el tipo es el de la expresión.
    Infer(Option<Type>),
}

struct Ctx {
    scopes: Vec<Vars>,
    ret: Ret,
}

struct Pass<'p> {
    funcs: HashMap<String, Sig>,
    globals: Vars,
    decls: HashMap<String, &'p FnDecl>,
    state: HashMap<String, State>,
}

impl<'p> Pass<'p> {
    fn program(&mut self, program: &'p Program) -> Result<(), Diagnostic> {
        // Las firmas se registran antes: las funciones pueden usarse antes de su definición.
        for stmt in &program.stmts {
            let Stmt::Fn(decl) = stmt else { continue };
            if self.funcs.contains_key(&decl.name) {
                return Err(Diagnostic::new(
                    decl.name_span,
                    format!("la función `{}` ya está definida", decl.name),
                ));
            }
            let params = decl
                .params
                .iter()
                .map(|p| resolve_type(&p.ty))
                .collect::<Result<Vec<_>, _>>()?;
            let ret = match (&decl.ret, decl.expr_body) {
                (Some(ty), _) => Some(resolve_type(ty)?),
                (None, false) => Some(Type::Unit),
                (None, true) => None,
            };
            self.funcs.insert(decl.name.clone(), Sig { params, ret });
            self.decls.insert(decl.name.clone(), decl);
            self.state.insert(decl.name.clone(), State::Unchecked);
        }

        let mut script = Ctx {
            scopes: Vec::new(),
            ret: Ret::Script,
        };
        for stmt in &program.stmts {
            if !matches!(stmt, Stmt::Fn(_)) {
                self.stmt(&mut script, stmt)?;
            }
        }
        for stmt in &program.stmts {
            if let Stmt::Fn(decl) = stmt {
                if self.state[&decl.name] == State::Unchecked {
                    self.check_fn(decl)?;
                }
            }
        }
        Ok(())
    }

    fn check_fn(&mut self, decl: &'p FnDecl) -> Result<(), Diagnostic> {
        self.state.insert(decl.name.clone(), State::InProgress);
        let sig = self.funcs[&decl.name].clone();
        let mut scope = Vars::new();
        for (param, ty) in decl.params.iter().zip(&sig.params) {
            scope.insert(param.name.clone(), (*ty, false));
        }
        let mut ctx = Ctx {
            scopes: vec![scope],
            ret: match sig.ret {
                Some(ty) => Ret::Declared(ty),
                None => Ret::Infer(None),
            },
        };
        self.block(&mut ctx, &decl.body)?;
        match ctx.ret {
            Ret::Infer(found) => {
                let ty = found.unwrap_or(Type::Unit);
                if let Some(sig) = self.funcs.get_mut(&decl.name) {
                    sig.ret = Some(ty);
                }
            }
            Ret::Declared(ty) if ty != Type::Unit && !block_returns(&decl.body) => {
                return Err(Diagnostic::new(
                    decl.name_span,
                    format!(
                        "la función `{}` puede terminar sin devolver un valor de tipo {ty}",
                        decl.name
                    ),
                ));
            }
            _ => {}
        }
        self.state.insert(decl.name.clone(), State::Done);
        Ok(())
    }

    fn block(&mut self, ctx: &mut Ctx, block: &Block) -> Result<(), Diagnostic> {
        ctx.scopes.push(Vars::new());
        let result = block.stmts.iter().try_for_each(|stmt| self.stmt(ctx, stmt));
        ctx.scopes.pop();
        result
    }

    fn lookup(&self, ctx: &Ctx, name: &str) -> Option<(Type, bool)> {
        ctx.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name))
            .or_else(|| self.globals.get(name))
            .copied()
    }

    fn declare(
        &mut self,
        ctx: &mut Ctx,
        name: &str,
        info: (Type, bool),
        span: Span,
    ) -> Result<(), Diagnostic> {
        let scope = match ctx.scopes.last_mut() {
            Some(scope) => scope,
            None => &mut self.globals,
        };
        if scope.contains_key(name) {
            return Err(Diagnostic::new(
                span,
                format!("`{name}` ya está declarada en este ámbito"),
            ));
        }
        scope.insert(name.to_string(), info);
        Ok(())
    }

    fn stmt(&mut self, ctx: &mut Ctx, stmt: &Stmt) -> Result<(), Diagnostic> {
        match stmt {
            Stmt::Show(expr, _) | Stmt::Expr(expr) => {
                self.expr(ctx, expr)?;
            }
            Stmt::Let {
                name,
                name_span,
                mutable,
                ty,
                init,
                ..
            } => {
                let found = self.expr(ctx, init)?;
                let declared = match ty {
                    Some(ty) => {
                        let expected = resolve_type(ty)?;
                        expect(expected, found, init.span())?;
                        expected
                    }
                    None => found,
                };
                if declared == Type::Unit {
                    return Err(Diagnostic::new(
                        init.span(),
                        format!("`{name}` no puede guardar un valor de tipo Unit"),
                    ));
                }
                self.declare(ctx, name, (declared, *mutable), *name_span)?;
            }
            Stmt::Assign {
                name,
                name_span,
                op,
                value,
                span,
            } => {
                let Some((ty, mutable)) = self.lookup(ctx, name) else {
                    return Err(undefined_variable(name, *name_span));
                };
                if !mutable {
                    return Err(Diagnostic::new(
                        *name_span,
                        format!("no se puede asignar a `{name}` porque es inmutable; decláralo con `mut`"),
                    ));
                }
                let found = self.expr(ctx, value)?;
                let result = match op {
                    Some(op) => binary_type(*op, ty, found, *span)?,
                    None => found,
                };
                expect(ty, result, value.span())?;
            }
            Stmt::If {
                cond,
                then,
                otherwise,
                ..
            } => {
                self.condition(ctx, cond)?;
                self.block(ctx, then)?;
                if let Some(otherwise) = otherwise {
                    self.block(ctx, otherwise)?;
                }
            }
            Stmt::While { cond, body, .. } => {
                self.condition(ctx, cond)?;
                self.block(ctx, body)?;
            }
            Stmt::Return(expr, span) => {
                let (found, at) = match expr {
                    Some(expr) => (self.expr(ctx, expr)?, expr.span()),
                    None => (Type::Unit, *span),
                };
                match &mut ctx.ret {
                    Ret::Script => {}
                    Ret::Infer(slot) => match slot {
                        Some(expected) => expect(*expected, found, at)?,
                        None => *slot = Some(found),
                    },
                    Ret::Declared(expected) => {
                        if *expected == Type::Unit && found != Type::Unit {
                            return Err(Diagnostic::new(
                                at,
                                format!(
                                    "la función no declara un tipo de retorno, pero devuelve {found}; añade `-> {found}`"
                                ),
                            ));
                        }
                        expect(*expected, found, at)?;
                    }
                }
            }
            Stmt::Break(_) | Stmt::Continue(_) | Stmt::Fn(_) => {}
        }
        Ok(())
    }

    fn condition(&mut self, ctx: &mut Ctx, cond: &Expr) -> Result<(), Diagnostic> {
        match self.expr(ctx, cond)? {
            Type::Bool => Ok(()),
            other => Err(Diagnostic::new(
                cond.span(),
                format!("la condición debe ser Bool, pero es {other}"),
            )),
        }
    }

    fn expr(&mut self, ctx: &mut Ctx, expr: &Expr) -> Result<Type, Diagnostic> {
        match expr {
            Expr::Int(..) => Ok(Type::Int),
            Expr::Float(..) => Ok(Type::Float),
            Expr::Bool(..) => Ok(Type::Bool),
            Expr::Var(name, span) => self
                .lookup(ctx, name)
                .map(|(ty, _)| ty)
                .ok_or_else(|| undefined_variable(name, *span)),
            Expr::Unary { op, expr, span } => {
                let ty = self.expr(ctx, expr)?;
                match (op, ty) {
                    (UnOp::Neg, ty) if ty.is_numeric() => Ok(ty),
                    (UnOp::Not, Type::Bool) => Ok(Type::Bool),
                    (op, ty) => Err(Diagnostic::new(
                        *span,
                        format!(
                            "el operador `{}` no admite {ty}",
                            if *op == UnOp::Neg { "-" } else { "!" }
                        ),
                    )),
                }
            }
            Expr::Binary { op, lhs, rhs, span } => {
                let a = self.expr(ctx, lhs)?;
                let b = self.expr(ctx, rhs)?;
                binary_type(*op, a, b, *span)
            }
            Expr::Call { name, args, span } => self.call(ctx, name, args, *span),
        }
    }

    fn call(
        &mut self,
        ctx: &mut Ctx,
        name: &str,
        args: &[Expr],
        span: Span,
    ) -> Result<Type, Diagnostic> {
        let Some(sig) = self.funcs.get(name) else {
            return Err(Diagnostic::new(
                span,
                format!("la función `{name}` no está definida"),
            ));
        };
        if sig.params.len() != args.len() {
            return Err(Diagnostic::new(
                span,
                format!(
                    "la función `{name}` espera {} argumento(s), pero recibió {}",
                    sig.params.len(),
                    args.len()
                ),
            ));
        }
        let params = sig.params.clone();
        for (arg, expected) in args.iter().zip(params) {
            let found = self.expr(ctx, arg)?;
            expect(expected, found, arg.span())?;
        }
        // Si el tipo de retorno aún no se infirió, se verifica ahora el cuerpo de esa función.
        if self.funcs[name].ret.is_none() {
            match self.state.get(name) {
                Some(State::InProgress) => {
                    return Err(Diagnostic::new(
                        span,
                        format!(
                            "`{name}` se llama a sí misma antes de conocer su tipo; añade `-> Tipo` a su declaración"
                        ),
                    ));
                }
                _ => {
                    if let Some(decl) = self.decls.get(name).copied() {
                        self.check_fn(decl)?;
                    }
                }
            }
        }
        Ok(self.funcs[name].ret.unwrap_or(Type::Unit))
    }
}

fn resolve_type(ty: &TypeRef) -> Result<Type, Diagnostic> {
    Type::from_name(&ty.name).ok_or_else(|| {
        Diagnostic::new(
            ty.span,
            format!(
                "el tipo `{}` no existe (hay Int, Float, Bool y Unit)",
                ty.name
            ),
        )
    })
}

fn expect(expected: Type, found: Type, span: Span) -> Result<(), Diagnostic> {
    if expected == found {
        return Ok(());
    }
    let hint = if expected == Type::Float && found == Type::Int {
        "; no hay conversión implícita de Int a Float (escribe `1.0` en vez de `1`)"
    } else {
        ""
    };
    Err(Diagnostic::new(
        span,
        format!("se esperaba {expected}, pero es {found}{hint}"),
    ))
}

fn undefined_variable(name: &str, span: Span) -> Diagnostic {
    Diagnostic::new(span, format!("la variable `{name}` no está definida"))
}

/// Tipo del resultado de un operador binario. Mezclar Int y Float en aritmética y
/// comparaciones es válido y da Float; en asignaciones y argumentos no hay conversión.
fn binary_type(op: BinOp, a: Type, b: Type, span: Span) -> Result<Type, Diagnostic> {
    let invalid = || {
        Diagnostic::new(
            span,
            format!("la operación `{}` no admite {a} y {b}", op.symbol()),
        )
    };
    match op {
        BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Rem => match (a, b) {
            (Type::Int, Type::Int) => Ok(Type::Int),
            _ if a.is_numeric() && b.is_numeric() => Ok(Type::Float),
            _ => Err(invalid()),
        },
        BinOp::Eq | BinOp::Ne => {
            if a == b || (a.is_numeric() && b.is_numeric()) {
                Ok(Type::Bool)
            } else {
                Err(invalid())
            }
        }
        BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
            if a.is_numeric() && b.is_numeric() {
                Ok(Type::Bool)
            } else {
                Err(invalid())
            }
        }
        BinOp::And | BinOp::Or => match [a, b].into_iter().find(|t| *t != Type::Bool) {
            None => Ok(Type::Bool),
            Some(other) => Err(Diagnostic::new(
                span,
                format!(
                    "el operador `{}` requiere Bool, pero recibió {other}",
                    op.symbol()
                ),
            )),
        },
    }
}

/// ¿Todos los caminos del bloque terminan en `return`?
fn block_returns(block: &Block) -> bool {
    block.stmts.iter().any(|stmt| match stmt {
        Stmt::Return(..) => true,
        Stmt::If {
            then,
            otherwise: Some(otherwise),
            ..
        } => block_returns(then) && block_returns(otherwise),
        _ => false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use and23_syntax::parse;

    fn check(src: &str) -> Result<(), String> {
        Checker::new()
            .check(&parse(src).map_err(|d| d.message)?)
            .map_err(|d| d.message)
    }

    fn err(src: &str) -> String {
        check(src).expect_err("debía rechazarse")
    }

    #[test]
    fn programas_bien_tipados() {
        for src in [
            "fix x = 1\nmut y: Float = 2.5\ny += 1\nshow(x + y)",
            "fn f(n: Int) -> Int { if n < 2 { return n }\n return f(n - 1) + f(n - 2) }\nshow(f(5))",
            "fn doble(x: Int) = x * 2\nshow(doble(4))",
            "fn saluda() { show(1) }\nsaluda()",
            "fn g() -> Bool { if true { return true } else { return false } }\nshow(g())",
            "fn f() = g()\nfn g() = 3\nshow(f())",
            "fn f() = g\nfix g = 5\nshow(f())",
            "show(1 == 1.0)\nshow(2 < 3.5)\nshow(1 + 2.0)",
            "fn f(x: Int) { fix x = x + 1\n show(x) }\nf(1)",
        ] {
            assert_eq!(check(src), Ok(()), "{src}");
        }
    }

    #[test]
    fn los_ejemplos_del_repo_se_aceptan() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_some_and(|e| e == "jb") {
                let src = std::fs::read_to_string(&path).unwrap();
                assert_eq!(check(&src), Ok(()), "{path:?}");
            }
        }
    }

    #[test]
    fn let_con_anotacion_incorrecta() {
        assert_eq!(err("fix x: Int = true"), "se esperaba Int, pero es Bool");
        assert!(err("fix x: Float = 1").contains("no hay conversión implícita"));
    }

    #[test]
    fn tipo_desconocido() {
        assert!(err("fix x: Texto = 1").contains("el tipo `Texto` no existe"));
        assert!(err("fn f(a: Cosa) { }").contains("`Cosa`"));
    }

    #[test]
    fn argumento_de_tipo_incorrecto() {
        assert_eq!(
            err("fn f(a: Int) = a\nshow(f(1.5))"),
            "se esperaba Int, pero es Float"
        );
    }

    #[test]
    fn retorno_incorrecto() {
        assert_eq!(
            err("fn f() -> Int { return true }"),
            "se esperaba Int, pero es Bool"
        );
        assert!(err("fn f() { return 1 }").contains("añade `-> Int`"));
        assert!(err("fn f() -> Int { return }").contains("se esperaba Int, pero es Unit"));
    }

    #[test]
    fn funcion_que_puede_no_devolver() {
        assert!(err("fn f(a: Bool) -> Int { if a { return 1 } }")
            .contains("puede terminar sin devolver un valor de tipo Int"));
        assert!(err("fn f() -> Int { }").contains("puede terminar sin devolver"));
    }

    #[test]
    fn condiciones_y_operadores() {
        assert_eq!(err("if 1 { }"), "la condición debe ser Bool, pero es Int");
        assert_eq!(
            err("while 1.5 { }"),
            "la condición debe ser Bool, pero es Float"
        );
        assert_eq!(
            err("show(1 + true)"),
            "la operación `+` no admite Int y Bool"
        );
        assert_eq!(
            err("show(true < false)"),
            "la operación `<` no admite Bool y Bool"
        );
        assert_eq!(
            err("show(true == 1)"),
            "la operación `==` no admite Bool y Int"
        );
        assert_eq!(err("show(!1)"), "el operador `!` no admite Int");
        assert_eq!(err("show(-true)"), "el operador `-` no admite Bool");
        assert_eq!(
            err("show(true && 1)"),
            "el operador `&&` requiere Bool, pero recibió Int"
        );
    }

    #[test]
    fn asignaciones() {
        assert_eq!(err("mut x = 1\nx = true"), "se esperaba Int, pero es Bool");
        assert_eq!(err("mut x = 1\nx += 1.5"), "se esperaba Int, pero es Float");
        assert!(err("fix x = 1\nx = 2").contains("inmutable"));
        assert_eq!(err("y = 1"), "la variable `y` no está definida");
    }

    #[test]
    fn no_se_guarda_unit() {
        assert!(err("fn f() { }\nfix x = f()").contains("Unit"));
    }

    #[test]
    fn recursion_sin_tipo_de_retorno_pide_anotacion() {
        assert!(err("fn f(n: Int) = f(n + 1)").contains("añade `-> Tipo`"));
        assert_eq!(check("fn f(n: Int) -> Int = f(n + 1)"), Ok(()));
    }

    #[test]
    fn uso_de_global_declarada_mas_abajo_desde_el_nivel_superior() {
        assert_eq!(
            err("show(g)\nfix g = 1"),
            "la variable `g` no está definida"
        );
    }

    #[test]
    fn el_verificador_conserva_estado_entre_entradas() {
        let mut c = Checker::new();
        let run = |c: &mut Checker, src: &str| c.check(&parse(src).unwrap()).map_err(|d| d.message);
        assert_eq!(
            run(&mut c, "mut total = 1\nfn doble(x: Int) = x * 2"),
            Ok(())
        );
        assert_eq!(run(&mut c, "total = doble(total) + 1"), Ok(()));
        assert!(run(&mut c, "total = true").is_err());
        // un error no deja nada a medias
        assert!(run(&mut c, "fix a = 1\nfix b = zzz").is_err());
        assert_eq!(run(&mut c, "fix a = 2"), Ok(()));
        c.forget_globals(&["a".to_string()]);
        assert_eq!(run(&mut c, "fix a = 3.5"), Ok(()));
    }
}
