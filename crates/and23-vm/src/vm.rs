use crate::value::Value;
use and23_bytecode::{Function, Module, Op};
use and23_syntax::{Diagnostic, Span};
use std::io::Write;

/// Llamadas anidadas permitidas antes de abortar con un error.
pub const DEFAULT_MAX_DEPTH: usize = 10_000;

/// Máquina de pila. Conserva las globales entre ejecuciones (REPL).
pub struct Vm<W: Write> {
    pub out: W,
    stack: Vec<Value>,
    globals: Vec<Option<Value>>,
    max_depth: usize,
}

/// Marco de un llamador suspendido: su función, la instrucción siguiente y su base en la pila.
struct Frame<'m> {
    function: &'m Function,
    ip: usize,
    base: usize,
}

impl<W: Write> Vm<W> {
    pub fn new(out: W) -> Self {
        Vm {
            out,
            stack: Vec::with_capacity(1024),
            globals: Vec::new(),
            max_depth: DEFAULT_MAX_DEPTH,
        }
    }

    pub fn with_max_depth(mut self, max_depth: usize) -> Self {
        self.max_depth = max_depth;
        self
    }

    /// ¿La global `slot` ya tiene valor?
    pub fn is_defined(&self, slot: u32) -> bool {
        matches!(self.globals.get(slot as usize), Some(Some(_)))
    }

    /// Ejecuta la función `entry` del módulo, sin argumentos.
    pub fn run(&mut self, module: &Module, entry: usize) -> Result<(), Diagnostic> {
        if self.globals.len() < module.globals.len() {
            self.globals.resize(module.globals.len(), None);
        }
        self.stack.clear();
        let result = self.execute(module, &module.functions[entry]);
        if result.is_err() {
            self.stack.clear();
        }
        result
    }

    fn execute<'m>(&mut self, module: &'m Module, entry: &'m Function) -> Result<(), Diagnostic> {
        let Vm {
            out,
            stack,
            globals,
            max_depth,
        } = self;
        let mut frames: Vec<Frame<'m>> = Vec::new();
        let mut function = entry;
        let mut ip = 0usize;
        let mut base = 0usize;

        macro_rules! fail {
            ($($msg:tt)*) => {
                return Err(Diagnostic::new(function.spans[ip - 1], format!($($msg)*)))
            };
        }
        macro_rules! pop {
            () => {
                stack
                    .pop()
                    .expect("pila vacía: el compilador generó código inválido")
            };
        }
        macro_rules! top {
            () => {
                stack
                    .last_mut()
                    .expect("pila vacía: el compilador generó código inválido")
            };
        }
        // Operación binaria con camino rápido para Int/Int y camino general para el resto.
        macro_rules! arith {
            ($op:expr, $checked:ident) => {{
                let b = pop!();
                let a = top!();
                if let (Value::Int(x), Value::Int(y)) = (*a, b) {
                    match x.$checked(y) {
                        Some(r) => *a = Value::Int(r),
                        None => fail!("desbordamiento de entero (i64)"),
                    }
                } else {
                    match binary($op, *a, b, function.spans[ip - 1]) {
                        Ok(v) => *a = v,
                        Err(e) => return Err(e),
                    }
                }
            }};
        }
        macro_rules! compare {
            ($op:expr, $cmp:tt) => {{
                let b = pop!();
                let a = top!();
                if let (Value::Int(x), Value::Int(y)) = (*a, b) {
                    *a = Value::Bool(x $cmp y);
                } else {
                    match binary($op, *a, b, function.spans[ip - 1]) {
                        Ok(v) => *a = v,
                        Err(e) => return Err(e),
                    }
                }
            }};
        }
        macro_rules! slow {
            ($op:expr) => {{
                let b = pop!();
                let a = top!();
                match binary($op, *a, b, function.spans[ip - 1]) {
                    Ok(v) => *a = v,
                    Err(e) => return Err(e),
                }
            }};
        }

        loop {
            let op = function.code[ip];
            ip += 1;
            match op {
                Op::Int(v) => stack.push(Value::Int(v)),
                Op::Float(v) => stack.push(Value::Float(v)),
                Op::Bool(v) => stack.push(Value::Bool(v)),
                Op::Unit => stack.push(Value::Unit),
                Op::GetLocal(n) => {
                    let v = stack[base + n as usize];
                    stack.push(v);
                }
                Op::SetLocal(n) => {
                    let v = pop!();
                    stack[base + n as usize] = v;
                }
                Op::GetGlobal(n) => match globals[n as usize] {
                    Some(v) => stack.push(v),
                    None => fail!(
                        "la variable `{}` no está definida",
                        module.globals[n as usize]
                    ),
                },
                Op::SetGlobal(n) => {
                    let v = pop!();
                    match &mut globals[n as usize] {
                        Some(slot) => *slot = v,
                        None => {
                            fail!(
                                "la variable `{}` no está definida",
                                module.globals[n as usize]
                            )
                        }
                    }
                }
                Op::DefGlobal(n) => {
                    let v = pop!();
                    globals[n as usize] = Some(v);
                }
                Op::Pop => {
                    pop!();
                }
                Op::PopN(n) => {
                    let len = stack.len() - n as usize;
                    stack.truncate(len);
                }
                Op::Neg => {
                    let v = top!();
                    match *v {
                        Value::Int(x) => match x.checked_neg() {
                            Some(r) => *v = Value::Int(r),
                            None => fail!("desbordamiento de entero (i64)"),
                        },
                        Value::Float(x) => *v = Value::Float(-x),
                        other => fail!("el operador `-` no admite {}", other.type_name()),
                    }
                }
                Op::Not => {
                    let v = top!();
                    match *v {
                        Value::Bool(x) => *v = Value::Bool(!x),
                        other => fail!("el operador `!` no admite {}", other.type_name()),
                    }
                }
                Op::Add => arith!(op, checked_add),
                Op::Sub => arith!(op, checked_sub),
                Op::Mul => arith!(op, checked_mul),
                Op::Div | Op::Rem => slow!(op),
                Op::Eq | Op::Ne => slow!(op),
                Op::Lt => compare!(op, <),
                Op::Le => compare!(op, <=),
                Op::Gt => compare!(op, >),
                Op::Ge => compare!(op, >=),
                Op::Jump(target) => ip = target as usize,
                Op::JumpIfFalse(target) => match pop!() {
                    Value::Bool(true) => {}
                    Value::Bool(false) => ip = target as usize,
                    other => fail!("la condición debe ser Bool, pero es {}", other.type_name()),
                },
                Op::AndJump(target) | Op::OrJump(target) => {
                    let is_and = matches!(op, Op::AndJump(_));
                    match *top!() {
                        Value::Bool(b) if b != is_and => ip = target as usize,
                        Value::Bool(_) => {
                            pop!();
                        }
                        other => fail!(
                            "el operador `{}` requiere Bool, pero recibió {}",
                            if is_and { "&&" } else { "||" },
                            other.type_name()
                        ),
                    }
                }
                Op::CheckBool(is_and) => {
                    if let other @ (Value::Int(_) | Value::Float(_) | Value::Unit) = *top!() {
                        fail!(
                            "el operador `{}` requiere Bool, pero recibió {}",
                            if is_and { "&&" } else { "||" },
                            other.type_name()
                        );
                    }
                }
                Op::Call(n) => {
                    if frames.len() >= *max_depth {
                        fail!(
                            "desbordamiento de pila: más de {} llamadas anidadas",
                            max_depth
                        );
                    }
                    let callee = &module.functions[n as usize];
                    frames.push(Frame { function, ip, base });
                    base = stack.len() - callee.arity;
                    function = callee;
                    ip = 0;
                }
                Op::Return => {
                    let result = pop!();
                    stack.truncate(base);
                    match frames.pop() {
                        Some(frame) => {
                            function = frame.function;
                            ip = frame.ip;
                            base = frame.base;
                            stack.push(result);
                        }
                        None => return Ok(()),
                    }
                }
                Op::Show => {
                    let v = pop!();
                    if let Err(e) = writeln!(out, "{v}") {
                        fail!("no se pudo escribir la salida: {e}");
                    }
                }
                Op::Echo => {
                    let v = pop!();
                    if v != Value::Unit {
                        if let Err(e) = writeln!(out, "{v}") {
                            fail!("no se pudo escribir la salida: {e}");
                        }
                    }
                }
            }
        }
    }
}

fn symbol(op: Op) -> &'static str {
    match op {
        Op::Add => "+",
        Op::Sub => "-",
        Op::Mul => "*",
        Op::Div => "/",
        Op::Rem => "%",
        Op::Eq => "==",
        Op::Ne => "!=",
        Op::Lt => "<",
        Op::Le => "<=",
        Op::Gt => ">",
        _ => ">=",
    }
}

fn type_error(op: Op, a: Value, b: Value, span: Span) -> Diagnostic {
    Diagnostic::new(
        span,
        format!(
            "la operación `{}` no admite {} y {}",
            symbol(op),
            a.type_name(),
            b.type_name()
        ),
    )
}

/// Camino general de los operadores binarios (mezcla de tipos, división, igualdad...).
fn binary(op: Op, a: Value, b: Value, span: Span) -> Result<Value, Diagnostic> {
    let overflow = || Diagnostic::new(span, "desbordamiento de entero (i64)");
    let div_zero = || Diagnostic::new(span, "división entre cero");
    match op {
        Op::Add | Op::Sub | Op::Mul | Op::Div | Op::Rem => match (a, b) {
            (Value::Int(x), Value::Int(y)) => {
                let result = match op {
                    Op::Add => x.checked_add(y),
                    Op::Sub => x.checked_sub(y),
                    Op::Mul => x.checked_mul(y),
                    Op::Div | Op::Rem if y == 0 => return Err(div_zero()),
                    Op::Div => x.checked_div(y),
                    _ => x.checked_rem(y),
                };
                result.map(Value::Int).ok_or_else(overflow)
            }
            _ => match (a.as_f64(), b.as_f64()) {
                (Some(x), Some(y)) => Ok(Value::Float(match op {
                    Op::Add => x + y,
                    Op::Sub => x - y,
                    Op::Mul => x * y,
                    Op::Div | Op::Rem if y == 0.0 => return Err(div_zero()),
                    Op::Div => x / y,
                    _ => x % y,
                })),
                _ => Err(type_error(op, a, b, span)),
            },
        },
        Op::Eq | Op::Ne => {
            let equal = match (a, b) {
                (Value::Int(x), Value::Int(y)) => x == y,
                (Value::Bool(x), Value::Bool(y)) => x == y,
                (Value::Unit, Value::Unit) => true,
                _ => match (a.as_f64(), b.as_f64()) {
                    (Some(x), Some(y)) => x == y,
                    _ => return Err(type_error(op, a, b, span)),
                },
            };
            Ok(Value::Bool(equal == (op == Op::Eq)))
        }
        _ => {
            let ordering = match (a, b) {
                (Value::Int(x), Value::Int(y)) => Some(x.cmp(&y)),
                _ => match (a.as_f64(), b.as_f64()) {
                    (Some(x), Some(y)) => x.partial_cmp(&y),
                    _ => return Err(type_error(op, a, b, span)),
                },
            };
            // Con NaN no hay orden: toda comparación es falsa.
            Ok(Value::Bool(ordering.is_some_and(|o| match op {
                Op::Lt => o.is_lt(),
                Op::Le => o.is_le(),
                Op::Gt => o.is_gt(),
                _ => o.is_ge(),
            })))
        }
    }
}
