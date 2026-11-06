//! Intérprete de árbol de And23 (M0/M1a). Ya no es el camino de ejecución: se conserva como
//! referencia para comparar con la VM. No soporta cadenas ni arreglos (M3).

use and23_syntax::{BinOp, Block, Diagnostic, Expr, FnDecl, Program, Span, Stmt, UnOp};
use std::collections::HashMap;
use std::fmt;
use std::io::Write;
use std::rc::Rc;

/// Llamadas anidadas permitidas antes de abortar con un error en vez de agotar la pila.
pub const DEFAULT_MAX_DEPTH: usize = 10_000;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Value {
    Int(i64),
    Float(f64),
    Bool(bool),
    /// Resultado de una función sin valor de retorno.
    Unit,
}

impl Value {
    pub fn type_name(self) -> &'static str {
        match self {
            Value::Int(_) => "Int",
            Value::Float(_) => "Float",
            Value::Bool(_) => "Bool",
            Value::Unit => "Unit",
        }
    }

    fn as_f64(self) -> Option<f64> {
        match self {
            Value::Int(v) => Some(v as f64),
            Value::Float(v) => Some(v),
            _ => None,
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Int(v) => write!(f, "{v}"),
            // `{:?}` conserva el `.0` en decimales enteros (3.0 y no 3).
            Value::Float(v) => write!(f, "{v:?}"),
            Value::Bool(v) => write!(f, "{v}"),
            Value::Unit => f.write_str("()"),
        }
    }
}

struct Var {
    value: Value,
    mutable: bool,
}

type Scope = HashMap<String, Var>;

/// Ámbitos locales de un marco de ejecución. Vacío significa nivel superior del archivo,
/// donde las declaraciones van a las variables globales.
#[derive(Default)]
struct Env {
    scopes: Vec<Scope>,
}

enum Flow {
    Normal,
    Break,
    Continue,
    Return(Value),
}

/// Intérprete con estado: las variables y funciones sobreviven entre llamadas a `run` (REPL).
pub struct Interpreter<W: Write> {
    pub out: W,
    globals: Scope,
    funcs: HashMap<String, Rc<FnDecl>>,
    depth: usize,
    max_depth: usize,
}

impl<W: Write> Interpreter<W> {
    pub fn new(out: W) -> Self {
        Interpreter {
            out,
            globals: Scope::new(),
            funcs: HashMap::new(),
            depth: 0,
            max_depth: DEFAULT_MAX_DEPTH,
        }
    }

    pub fn with_max_depth(mut self, max_depth: usize) -> Self {
        self.max_depth = max_depth;
        self
    }

    /// Ejecuta el programa. `echo` imprime también el valor de las expresiones sueltas del
    /// nivel superior (modo REPL).
    pub fn run(&mut self, program: &Program, echo: bool) -> Result<(), Diagnostic> {
        // Las funciones se registran antes de ejecutar: pueden usarse antes de su definición.
        for stmt in &program.stmts {
            if let Stmt::Fn(decl) = stmt {
                if self.funcs.contains_key(&decl.name) {
                    return Err(Diagnostic::new(
                        decl.name_span,
                        format!("la función `{}` ya está definida", decl.name),
                    ));
                }
                self.funcs
                    .insert(decl.name.clone(), Rc::new(FnDecl::clone(decl)));
            }
        }

        let mut env = Env::default();
        for stmt in &program.stmts {
            match stmt {
                Stmt::Fn(_) => {}
                Stmt::Expr(expr) if echo => {
                    let value = self.eval(&mut env, expr)?;
                    if value != Value::Unit {
                        self.emit(value, expr.span())?;
                    }
                }
                _ => {
                    self.exec_stmt(&mut env, stmt)?;
                }
            }
        }
        Ok(())
    }

    /// Llama a `main()` si el programa la definió.
    pub fn run_main(&mut self) -> Result<(), Diagnostic> {
        let Some(main) = self.funcs.get("main").cloned() else {
            return Ok(());
        };
        if !main.params.is_empty() {
            return Err(Diagnostic::new(
                main.name_span,
                "`main` no puede tener parámetros",
            ));
        }
        self.call(&main, Vec::new(), main.name_span)?;
        Ok(())
    }

    fn emit(&mut self, value: Value, span: Span) -> Result<(), Diagnostic> {
        writeln!(self.out, "{value}")
            .map_err(|e| Diagnostic::new(span, format!("no se pudo escribir la salida: {e}")))
    }

    fn find<'a>(&'a self, env: &'a Env, name: &str) -> Option<&'a Var> {
        env.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name))
            .or_else(|| self.globals.get(name))
    }

    fn set(&mut self, env: &mut Env, name: &str, value: Value) {
        for scope in env.scopes.iter_mut().rev() {
            if let Some(var) = scope.get_mut(name) {
                var.value = value;
                return;
            }
        }
        if let Some(var) = self.globals.get_mut(name) {
            var.value = value;
        }
    }

    fn declare(
        &mut self,
        env: &mut Env,
        name: &str,
        var: Var,
        span: Span,
    ) -> Result<(), Diagnostic> {
        let scope = match env.scopes.last_mut() {
            Some(scope) => scope,
            None => &mut self.globals,
        };
        if scope.contains_key(name) {
            return Err(Diagnostic::new(
                span,
                format!("`{name}` ya está declarada en este ámbito"),
            ));
        }
        scope.insert(name.to_string(), var);
        Ok(())
    }

    fn exec_block(&mut self, env: &mut Env, block: &Block) -> Result<Flow, Diagnostic> {
        env.scopes.push(Scope::new());
        let result = self.exec_stmts(env, &block.stmts);
        env.scopes.pop();
        result
    }

    fn exec_stmts(&mut self, env: &mut Env, stmts: &[Stmt]) -> Result<Flow, Diagnostic> {
        for stmt in stmts {
            match self.exec_stmt(env, stmt)? {
                Flow::Normal => {}
                other => return Ok(other),
            }
        }
        Ok(Flow::Normal)
    }

    fn exec_stmt(&mut self, env: &mut Env, stmt: &Stmt) -> Result<Flow, Diagnostic> {
        match stmt {
            Stmt::Show(expr, span) => {
                let value = self.eval(env, expr)?;
                self.emit(value, *span)?;
            }
            Stmt::Expr(expr) => {
                self.eval(env, expr)?;
            }
            Stmt::Let {
                name,
                name_span,
                mutable,
                init,
                ..
            } => {
                let value = self.eval(env, init)?;
                let var = Var {
                    value,
                    mutable: *mutable,
                };
                self.declare(env, name, var, *name_span)?;
            }
            Stmt::Assign {
                name,
                name_span,
                op,
                value,
                span,
            } => {
                let Some(var) = self.find(env, name) else {
                    return Err(undefined_variable(name, *name_span));
                };
                let (current, mutable) = (var.value, var.mutable);
                if !mutable {
                    return Err(Diagnostic::new(
                        *name_span,
                        format!("no se puede asignar a `{name}` porque es inmutable; decláralo con `mut`"),
                    ));
                }
                let rhs = self.eval(env, value)?;
                let new = match op {
                    Some(op) => binary(*op, current, rhs, *span)?,
                    None => rhs,
                };
                self.set(env, name, new);
            }
            Stmt::If {
                cond,
                then,
                otherwise,
                ..
            } => {
                if self.eval_condition(env, cond)? {
                    return self.exec_block(env, then);
                } else if let Some(otherwise) = otherwise {
                    return self.exec_block(env, otherwise);
                }
            }
            Stmt::While { cond, body, .. } => {
                while self.eval_condition(env, cond)? {
                    match self.exec_block(env, body)? {
                        Flow::Break => break,
                        Flow::Return(value) => return Ok(Flow::Return(value)),
                        Flow::Continue | Flow::Normal => {}
                    }
                }
            }
            Stmt::Return(expr, _) => {
                let value = match expr {
                    Some(expr) => self.eval(env, expr)?,
                    None => Value::Unit,
                };
                return Ok(Flow::Return(value));
            }
            Stmt::Break(_) => return Ok(Flow::Break),
            Stmt::Continue(_) => return Ok(Flow::Continue),
            Stmt::IndexAssign { span, .. } => return Err(unsupported(*span)),
            // El parser solo admite funciones al nivel superior; `run` ya las registró.
            Stmt::Fn(_) => {}
        }
        Ok(Flow::Normal)
    }

    fn eval_condition(&mut self, env: &mut Env, cond: &Expr) -> Result<bool, Diagnostic> {
        match self.eval(env, cond)? {
            Value::Bool(b) => Ok(b),
            other => Err(Diagnostic::new(
                cond.span(),
                format!("la condición debe ser Bool, pero es {}", other.type_name()),
            )),
        }
    }

    fn eval(&mut self, env: &mut Env, expr: &Expr) -> Result<Value, Diagnostic> {
        match expr {
            Expr::Int(v, _) => Ok(Value::Int(*v)),
            Expr::Float(v, _) => Ok(Value::Float(*v)),
            Expr::Bool(v, _) => Ok(Value::Bool(*v)),
            Expr::Var(name, span) => self
                .find(env, name)
                .map(|var| var.value)
                .ok_or_else(|| undefined_variable(name, *span)),
            Expr::Str(_, span)
            | Expr::Interp { span, .. }
            | Expr::Array { span, .. }
            | Expr::Index { span, .. } => Err(unsupported(*span)),
            Expr::Unary { op, expr, span } => {
                let value = self.eval(env, expr)?;
                match (op, value) {
                    (UnOp::Neg, Value::Int(v)) => v
                        .checked_neg()
                        .map(Value::Int)
                        .ok_or_else(|| overflow(*span)),
                    (UnOp::Neg, Value::Float(v)) => Ok(Value::Float(-v)),
                    (UnOp::Not, Value::Bool(v)) => Ok(Value::Bool(!v)),
                    (op, value) => Err(Diagnostic::new(
                        *span,
                        format!(
                            "el operador `{}` no admite {}",
                            if *op == UnOp::Neg { "-" } else { "!" },
                            value.type_name()
                        ),
                    )),
                }
            }
            Expr::Binary { op, lhs, rhs, span } => match op {
                BinOp::And | BinOp::Or => self.eval_logical(env, *op, lhs, rhs, *span),
                _ => {
                    let a = self.eval(env, lhs)?;
                    let b = self.eval(env, rhs)?;
                    binary(*op, a, b, *span)
                }
            },
            Expr::Call { name, args, span } => {
                let Some(decl) = self.funcs.get(name).cloned() else {
                    return Err(Diagnostic::new(
                        *span,
                        format!("la función `{name}` no está definida"),
                    ));
                };
                if decl.params.len() != args.len() {
                    return Err(Diagnostic::new(
                        *span,
                        format!(
                            "la función `{name}` espera {} argumento(s), pero recibió {}",
                            decl.params.len(),
                            args.len()
                        ),
                    ));
                }
                let mut values = Vec::with_capacity(args.len());
                for arg in args {
                    values.push(self.eval(env, arg)?);
                }
                self.call(&decl, values, *span)
            }
        }
    }

    /// `&&` y `||` con cortocircuito: el lado derecho solo se evalúa si hace falta.
    fn eval_logical(
        &mut self,
        env: &mut Env,
        op: BinOp,
        lhs: &Expr,
        rhs: &Expr,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let not_bool = |value: Value| {
            Diagnostic::new(
                span,
                format!(
                    "el operador `{}` requiere Bool, pero recibió {}",
                    op.symbol(),
                    value.type_name()
                ),
            )
        };
        let left = match self.eval(env, lhs)? {
            Value::Bool(b) => b,
            other => return Err(not_bool(other)),
        };
        let short_circuit = if op == BinOp::And { !left } else { left };
        if short_circuit {
            return Ok(Value::Bool(left));
        }
        match self.eval(env, rhs)? {
            Value::Bool(b) => Ok(Value::Bool(b)),
            other => Err(not_bool(other)),
        }
    }

    fn call(
        &mut self,
        decl: &Rc<FnDecl>,
        args: Vec<Value>,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        if self.depth >= self.max_depth {
            return Err(Diagnostic::new(
                span,
                format!(
                    "desbordamiento de pila: más de {} llamadas anidadas",
                    self.max_depth
                ),
            ));
        }
        // Los parámetros son inmutables; la función no ve las variables locales del llamador.
        let params: Scope = decl
            .params
            .iter()
            .zip(args)
            .map(|(param, value)| {
                (
                    param.name.clone(),
                    Var {
                        value,
                        mutable: false,
                    },
                )
            })
            .collect();
        let mut env = Env {
            scopes: vec![params],
        };
        self.depth += 1;
        let flow = self.exec_block(&mut env, &decl.body);
        self.depth -= 1;
        Ok(match flow? {
            Flow::Return(value) => value,
            _ => Value::Unit,
        })
    }
}

fn unsupported(span: Span) -> Diagnostic {
    Diagnostic::new(
        span,
        "el intérprete de árbol no soporta cadenas ni arreglos; usa la VM",
    )
}

fn undefined_variable(name: &str, span: Span) -> Diagnostic {
    Diagnostic::new(span, format!("la variable `{name}` no está definida"))
}

fn overflow(span: Span) -> Diagnostic {
    Diagnostic::new(span, "desbordamiento de entero (i64)")
}

fn div_by_zero(span: Span) -> Diagnostic {
    Diagnostic::new(span, "división entre cero")
}

fn type_error(op: BinOp, a: Value, b: Value, span: Span) -> Diagnostic {
    Diagnostic::new(
        span,
        format!(
            "la operación `{}` no admite {} y {}",
            op.symbol(),
            a.type_name(),
            b.type_name()
        ),
    )
}

/// Operadores binarios que no cortocircuitan.
fn binary(op: BinOp, a: Value, b: Value, span: Span) -> Result<Value, Diagnostic> {
    match op {
        BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Rem => match (a, b) {
            (Value::Int(x), Value::Int(y)) => int_op(op, x, y, span),
            _ => match (a.as_f64(), b.as_f64()) {
                (Some(x), Some(y)) => float_op(op, x, y, span),
                _ => Err(type_error(op, a, b, span)),
            },
        },
        BinOp::Eq | BinOp::Ne => {
            let equal = match (a, b) {
                (Value::Int(x), Value::Int(y)) => x == y,
                (Value::Bool(x), Value::Bool(y)) => x == y,
                (Value::Unit, Value::Unit) => true,
                _ => match (a.as_f64(), b.as_f64()) {
                    (Some(x), Some(y)) => x == y,
                    _ => return Err(type_error(op, a, b, span)),
                },
            };
            Ok(Value::Bool(equal == (op == BinOp::Eq)))
        }
        BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
            let ordering = match (a, b) {
                (Value::Int(x), Value::Int(y)) => Some(x.cmp(&y)),
                _ => match (a.as_f64(), b.as_f64()) {
                    (Some(x), Some(y)) => x.partial_cmp(&y),
                    _ => return Err(type_error(op, a, b, span)),
                },
            };
            // Con NaN no hay orden: toda comparación es falsa.
            Ok(Value::Bool(ordering.is_some_and(|o| match op {
                BinOp::Lt => o.is_lt(),
                BinOp::Le => o.is_le(),
                BinOp::Gt => o.is_gt(),
                _ => o.is_ge(),
            })))
        }
        BinOp::And | BinOp::Or => Err(type_error(op, a, b, span)),
    }
}

fn int_op(op: BinOp, x: i64, y: i64, span: Span) -> Result<Value, Diagnostic> {
    let result = match op {
        BinOp::Add => x.checked_add(y),
        BinOp::Sub => x.checked_sub(y),
        BinOp::Mul => x.checked_mul(y),
        BinOp::Div | BinOp::Rem if y == 0 => return Err(div_by_zero(span)),
        BinOp::Div => x.checked_div(y),
        _ => x.checked_rem(y),
    };
    result.map(Value::Int).ok_or_else(|| overflow(span))
}

fn float_op(op: BinOp, x: f64, y: f64, span: Span) -> Result<Value, Diagnostic> {
    Ok(Value::Float(match op {
        BinOp::Add => x + y,
        BinOp::Sub => x - y,
        BinOp::Mul => x * y,
        BinOp::Div | BinOp::Rem if y == 0.0 => return Err(div_by_zero(span)),
        BinOp::Div => x / y,
        _ => x % y,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use and23_syntax::parse;

    fn interp() -> Interpreter<Vec<u8>> {
        Interpreter::new(Vec::new())
    }

    fn output(interp: &Interpreter<Vec<u8>>) -> String {
        String::from_utf8(interp.out.clone()).unwrap()
    }

    /// Ejecuta `src` como archivo (sin echo) y devuelve su salida, o el mensaje de error.
    fn out(src: &str) -> Result<String, String> {
        let mut interp = interp();
        let program = parse(src).map_err(|d| d.message)?;
        interp.run(&program, false).map_err(|d| d.message)?;
        Ok(output(&interp))
    }

    /// Igual, pero como REPL: imprime el valor de las expresiones sueltas.
    fn repl(src: &str) -> Result<String, String> {
        let mut interp = interp();
        let program = parse(src).map_err(|d| d.message)?;
        interp.run(&program, true).map_err(|d| d.message)?;
        Ok(output(&interp))
    }

    // --- M0: aritmética ---

    #[test]
    fn el_criterio_de_exito_de_m0() {
        assert_eq!(repl("1 + 2 * 3").unwrap(), "7\n");
    }

    #[test]
    fn precedencia_y_parentesis() {
        assert_eq!(repl("(1 + 2) * 3").unwrap(), "9\n");
        assert_eq!(repl("10 - 4 - 3").unwrap(), "3\n");
        assert_eq!(repl("2 * 3 + 4 * 5").unwrap(), "26\n");
    }

    #[test]
    fn division_entera_y_decimal() {
        assert_eq!(repl("10 / 4").unwrap(), "2\n");
        assert_eq!(repl("10 / 4.0").unwrap(), "2.5\n");
        assert_eq!(repl("1.5 + 1.5").unwrap(), "3.0\n");
    }

    #[test]
    fn resto_con_signo() {
        assert_eq!(repl("-7 % 3").unwrap(), "-1\n");
        assert_eq!(repl("7 % 3").unwrap(), "1\n");
    }

    #[test]
    fn menos_unario() {
        assert_eq!(repl("2 * -3").unwrap(), "-6\n");
        assert_eq!(repl("- -4").unwrap(), "4\n");
    }

    #[test]
    fn division_entre_cero() {
        assert_eq!(out("show(1 / 0)").unwrap_err(), "división entre cero");
        assert_eq!(out("show(1 % 0)").unwrap_err(), "división entre cero");
        assert_eq!(out("show(1.0 / 0)").unwrap_err(), "división entre cero");
    }

    #[test]
    fn desbordamiento_de_enteros() {
        assert!(out("show(9223372036854775807 + 1)")
            .unwrap_err()
            .contains("desbordamiento"));
        assert!(out("show(9223372036854775807 * 2)")
            .unwrap_err()
            .contains("desbordamiento"));
        assert!(out("show(-9223372036854775807 - 2)")
            .unwrap_err()
            .contains("desbordamiento"));
    }

    #[test]
    fn show_imprime_aun_sin_echo_y_las_expresiones_sueltas_no() {
        assert_eq!(out("show(2 + 2)\n5").unwrap(), "4\n");
        assert_eq!(repl("show(2 + 2)\n5").unwrap(), "4\n5\n");
    }

    #[test]
    fn el_error_de_ejecucion_apunta_a_la_operacion() {
        let src = "show(1)\nshow(4 / 0)\n";
        let program = parse(src).unwrap();
        let err = interp().run(&program, false).unwrap_err();
        assert_eq!(err.line_col(src), (2, 6));
    }

    // --- Variables ---

    #[test]
    fn variables_inmutables_y_mutables() {
        assert_eq!(
            out("fix a = 2\nmut b = a * 5\nb = b + 1\nshow(b)").unwrap(),
            "11\n"
        );
    }

    #[test]
    fn asignacion_compuesta() {
        let src = "mut x = 10\nx += 5\nx -= 3\nx *= 2\nx /= 4\nx %= 4\nshow(x)";
        // ((10+5-3)*2)/4 = 6; 6 % 4 = 2
        assert_eq!(out(src).unwrap(), "2\n");
    }

    #[test]
    fn asignar_a_fix_es_error() {
        let err = out("fix a = 1\na = 2").unwrap_err();
        assert!(err.contains("inmutable") && err.contains("`mut`"), "{err}");
    }

    #[test]
    fn variable_no_definida() {
        assert!(out("show(x)").unwrap_err().contains("`x` no está definida"));
        assert!(out("x = 1").unwrap_err().contains("`x` no está definida"));
    }

    #[test]
    fn declaracion_repetida_en_el_mismo_ambito() {
        assert!(out("fix a = 1\nfix a = 2")
            .unwrap_err()
            .contains("ya está declarada"));
    }

    #[test]
    fn los_bloques_tienen_su_propio_ambito_y_pueden_ocultar() {
        let src = "fix a = 1\nif true {\n fix a = 2\n show(a)\n}\nshow(a)";
        assert_eq!(out(src).unwrap(), "2\n1\n");
        let fuera = "if true { fix b = 1 }\nshow(b)";
        assert!(out(fuera).unwrap_err().contains("`b` no está definida"));
    }

    #[test]
    fn un_bloque_puede_modificar_una_variable_externa() {
        assert_eq!(out("mut n = 0\nif true { n = 7 }\nshow(n)").unwrap(), "7\n");
    }

    #[test]
    fn variables_unicode() {
        assert_eq!(
            out("fix dueño = 3\nfix ñandú = dueño * 2\nshow(ñandú)").unwrap(),
            "6\n"
        );
    }

    // --- Booleanos y comparaciones ---

    #[test]
    fn comparaciones() {
        assert_eq!(
            out("show(1 < 2)\nshow(2 <= 1)\nshow(3 > 3)\nshow(3 >= 3)").unwrap(),
            "true\nfalse\nfalse\ntrue\n"
        );
        assert_eq!(
            out("show(1 == 1)\nshow(1 != 1)\nshow(true == false)").unwrap(),
            "true\nfalse\nfalse\n"
        );
    }

    #[test]
    fn enteros_y_decimales_se_comparan_por_valor() {
        assert_eq!(
            out("show(1 == 1.0)\nshow(2 < 2.5)").unwrap(),
            "true\ntrue\n"
        );
    }

    #[test]
    fn logicos_y_negacion() {
        assert_eq!(
            out("show(true && false)\nshow(true || false)\nshow(!true)").unwrap(),
            "false\ntrue\nfalse\n"
        );
    }

    #[test]
    fn los_logicos_hacen_cortocircuito() {
        // Si evaluara el lado derecho fallaría por división entre cero.
        assert_eq!(out("show(false && 1 / 0 == 0)").unwrap(), "false\n");
        assert_eq!(out("show(true || 1 / 0 == 0)").unwrap(), "true\n");
        assert!(out("show(true && 1 / 0 == 0)").is_err());
    }

    #[test]
    fn errores_de_tipo() {
        assert!(out("show(1 + true)")
            .unwrap_err()
            .contains("no admite Int y Bool"));
        assert!(out("show(1 < true)")
            .unwrap_err()
            .contains("no admite Int y Bool"));
        assert!(out("show(1 == true)").unwrap_err().contains("no admite"));
        assert!(out("show(!1)").unwrap_err().contains("no admite Int"));
        assert!(out("show(-true)").unwrap_err().contains("no admite Bool"));
        assert!(out("show(1 && true)")
            .unwrap_err()
            .contains("requiere Bool"));
        assert!(out("show(true && 1)")
            .unwrap_err()
            .contains("requiere Bool"));
    }

    // --- Control de flujo ---

    #[test]
    fn if_else_if_else() {
        let programa = |n: i64| {
            format!(
                "fix n = {n}\nif n < 0 {{ show(0 - 1) }} else if n == 0 {{ show(0) }} else {{ show(1) }}"
            )
        };
        assert_eq!(out(&programa(-5)).unwrap(), "-1\n");
        assert_eq!(out(&programa(0)).unwrap(), "0\n");
        assert_eq!(out(&programa(9)).unwrap(), "1\n");
    }

    #[test]
    fn la_condicion_debe_ser_bool() {
        assert!(out("if 1 { show(1) }")
            .unwrap_err()
            .contains("debe ser Bool, pero es Int"));
        assert!(out("while 0 { }").unwrap_err().contains("debe ser Bool"));
    }

    #[test]
    fn while_suma_del_uno_al_diez() {
        assert_eq!(
            out("mut i = 1\nmut s = 0\nwhile i <= 10 { s += i; i += 1 }\nshow(s)").unwrap(),
            "55\n"
        );
    }

    #[test]
    fn break_y_continue() {
        let src = "mut i = 0\nmut s = 0\nwhile true {\n i += 1\n if i > 10 { break }\n if i % 2 == 1 { continue }\n s += i\n}\nshow(s)";
        assert_eq!(out(src).unwrap(), "30\n");
    }

    // --- Funciones ---

    #[test]
    fn factorial_y_fibonacci() {
        let src = "
fn factorial(n: Int) -> Int {
    if n <= 1 { return 1 }
    return n * factorial(n - 1)
}
fn fib(n: Int) -> Int {
    if n < 2 { return n }
    return fib(n - 1) + fib(n - 2)
}
show(factorial(10))
show(fib(20))";
        assert_eq!(out(src).unwrap(), "3628800\n6765\n");
    }

    #[test]
    fn cuerpo_de_expresion() {
        assert_eq!(
            out("fn cuadrado(x: Int) = x * x\nshow(cuadrado(12))").unwrap(),
            "144\n"
        );
    }

    #[test]
    fn una_funcion_se_puede_usar_antes_de_definirla() {
        assert_eq!(
            out("show(doble(4))\nfn doble(x: Int) = x * 2").unwrap(),
            "8\n"
        );
    }

    #[test]
    fn recursion_mutua() {
        let src = "
fn es_par(n: Int) -> Bool { if n == 0 { return true }; return es_impar(n - 1) }
fn es_impar(n: Int) -> Bool { if n == 0 { return false }; return es_par(n - 1) }
show(es_par(10))
show(es_impar(7))";
        assert_eq!(out(src).unwrap(), "true\ntrue\n");
    }

    #[test]
    fn funcion_sin_retorno_devuelve_unit() {
        assert_eq!(out("fn f() { }\nshow(f())").unwrap(), "()\n");
        assert_eq!(out("fn f() { return }\nshow(f())").unwrap(), "()\n");
    }

    #[test]
    fn return_sale_de_bucles_anidados() {
        let src = "
fn primer_multiplo(n: Int) -> Int {
    mut i = 1
    while true {
        mut j = 1
        while j <= 3 {
            if i * j == n { return i }
            j += 1
        }
        i += 1
    }
    return 0
}
show(primer_multiplo(9))";
        assert_eq!(out(src).unwrap(), "3\n");
    }

    #[test]
    fn las_funciones_ven_las_globales_pero_no_las_locales_del_llamador() {
        assert_eq!(
            out("fix base = 100\nfn suma(x: Int) = base + x\nshow(suma(5))").unwrap(),
            "105\n"
        );
        let err =
            out("fn leer() = secreto\nfn llamar() { fix secreto = 1\nshow(leer()) }\nllamar()")
                .unwrap_err();
        assert!(err.contains("`secreto` no está definida"), "{err}");
    }

    #[test]
    fn una_funcion_puede_modificar_una_global_mut() {
        assert_eq!(out("mut cuenta = 0\nfn incrementar() { cuenta += 1 }\nincrementar()\nincrementar()\nshow(cuenta)").unwrap(), "2\n");
    }

    #[test]
    fn los_parametros_son_inmutables() {
        let err = out("fn f(x: Int) { x = 2 }\nf(1)").unwrap_err();
        assert!(err.contains("inmutable"), "{err}");
    }

    #[test]
    fn errores_de_llamada() {
        assert!(out("show(nada(1))")
            .unwrap_err()
            .contains("la función `nada` no está definida"));
        let err = out("fn f(a: Int, b: Int) = a\nshow(f(1))").unwrap_err();
        assert!(
            err.contains("espera 2 argumento(s), pero recibió 1"),
            "{err}"
        );
    }

    #[test]
    fn funcion_repetida() {
        assert!(out("fn f() { }\nfn f() { }")
            .unwrap_err()
            .contains("ya está definida"));
    }

    #[test]
    fn la_recursion_infinita_da_error_y_no_aborta_el_proceso() {
        let mut interp = Interpreter::new(Vec::new()).with_max_depth(50);
        let program = parse("fn f(n: Int) -> Int = f(n + 1)\nshow(f(0))").unwrap();
        let err = interp.run(&program, false).unwrap_err();
        assert!(err.message.contains("desbordamiento de pila"), "{err}");
        // Tras el error el contador de profundidad queda en cero: el intérprete sigue útil.
        let program = parse("fn g() = 7\nshow(g())").unwrap();
        interp.run(&program, false).unwrap();
        assert_eq!(output(&interp), "7\n");
    }

    #[test]
    fn main_se_ejecuta_despues_del_codigo_del_nivel_superior() {
        let mut interp = interp();
        let program = parse("fn main() { show(2) }\nshow(1)").unwrap();
        interp.run(&program, false).unwrap();
        interp.run_main().unwrap();
        assert_eq!(output(&interp), "1\n2\n");
    }

    #[test]
    fn sin_main_no_pasa_nada_y_main_con_parametros_es_error() {
        let mut interp = interp();
        interp.run_main().unwrap();
        let program = parse("fn main(x: Int) { }").unwrap();
        interp.run(&program, false).unwrap();
        assert!(interp
            .run_main()
            .unwrap_err()
            .message
            .contains("no puede tener parámetros"));
    }

    // --- Estado persistente (REPL) ---

    #[test]
    fn el_estado_sobrevive_entre_ejecuciones() {
        let mut interp = interp();
        for line in [
            "mut total = 1",
            "fn doble(x: Int) = x * 2",
            "total = doble(total) + 1",
            "total",
        ] {
            let program = parse(line).unwrap();
            interp.run(&program, true).unwrap();
        }
        assert_eq!(output(&interp), "3\n");
    }

    #[test]
    fn echo_no_imprime_declaraciones_ni_llamadas_unit() {
        assert_eq!(repl("fix a = 1\nfn f() { }\nf()\na").unwrap(), "1\n");
    }

    #[test]
    fn echo_no_imprime_expresiones_dentro_de_bloques() {
        assert_eq!(repl("if true { 5 }").unwrap(), "");
    }

    #[test]
    fn un_error_a_mitad_de_un_bloque_no_deja_ambitos_sucios() {
        let mut interp = interp();
        let malo = parse("if true { fix interno = 1\n show(1 / 0) }").unwrap();
        assert!(interp.run(&malo, false).is_err());
        let bueno = parse("fix interno = 5\nshow(interno)").unwrap();
        interp.run(&bueno, false).unwrap();
        assert_eq!(output(&interp), "5\n");
    }
}
