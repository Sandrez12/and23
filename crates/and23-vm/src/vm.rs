use crate::heap::{Heap, HeapStats, ObjRef, Object};
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
    heap: Heap,
    /// Objeto de cada cadena constante, creado la primera vez que se usa. Son raíces del GC.
    consts: Vec<Option<ObjRef>>,
    max_depth: usize,
}

/// Manejador instalado por un `attempt`: a dónde volver si se lanza una excepción.
struct Handler<'m> {
    /// Marcos suspendidos cuando se instaló (los de las llamadas más profundas se descartan).
    frames_len: usize,
    stack_len: usize,
    function: &'m Function,
    base: usize,
    catch_ip: usize,
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
            heap: Heap::new(),
            consts: Vec::new(),
            max_depth: DEFAULT_MAX_DEPTH,
        }
    }

    pub fn with_max_depth(mut self, max_depth: usize) -> Self {
        self.max_depth = max_depth;
        self
    }

    /// Recolecta en cada punto seguro (para probar que no falta ninguna raíz).
    pub fn set_gc_stress(&mut self, stress: bool) {
        self.heap.set_stress(stress);
    }

    pub fn heap_stats(&self) -> HeapStats {
        self.heap.stats()
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
        let defs: Vec<(&str, &[String])> = module
            .classes
            .iter()
            .map(|c| (c.name.as_str(), c.fields.as_slice()))
            .collect();
        self.heap.sync_classes(&defs);
        if self.consts.len() < module.strings.len() {
            self.consts.resize(module.strings.len(), None);
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
            heap,
            consts,
            max_depth,
        } = self;
        let mut frames: Vec<Frame<'m>> = Vec::new();
        let mut handlers: Vec<Handler<'m>> = Vec::new();
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
                    match binary_h(heap, $op, *a, b, function.spans[ip - 1]) {
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
                    match binary_h(heap, $op, *a, b, function.spans[ip - 1]) {
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
                match binary_h(heap, $op, *a, b, function.spans[ip - 1]) {
                    Ok(v) => *a = v,
                    Err(e) => return Err(e),
                }
            }};
        }

        // Punto seguro: aquí todo valor vivo está en la pila, las globales o las constantes.
        macro_rules! gc_point {
            () => {
                if heap.should_collect() {
                    let roots = stack
                        .iter()
                        .copied()
                        .chain(globals.iter().flatten().copied())
                        .chain(consts.iter().flatten().map(|r| Value::Obj(*r)));
                    heap.collect(roots);
                }
            };
        }

        loop {
            let op = function.code[ip];
            ip += 1;
            match op {
                Op::Int(v) => stack.push(Value::Int(v)),
                Op::Float(v) => stack.push(Value::Float(v)),
                Op::Bool(v) => stack.push(Value::Bool(v)),
                Op::Unit => stack.push(Value::Unit),
                Op::Str(n) => {
                    let r = match consts[n as usize] {
                        Some(r) => r,
                        None => {
                            gc_point!();
                            let Value::Obj(r) = heap.alloc_str(module.strings[n as usize].clone())
                            else {
                                unreachable!()
                            };
                            consts[n as usize] = Some(r);
                            r
                        }
                    };
                    stack.push(Value::Obj(r));
                }
                Op::Interp(n) => {
                    gc_point!();
                    let start = stack.len() - n as usize;
                    let mut text = String::new();
                    for v in &stack[start..] {
                        heap.render_into(*v, false, &mut text);
                    }
                    stack.truncate(start);
                    let v = heap.alloc_str(text);
                    stack.push(v);
                }
                Op::Array(n) => {
                    gc_point!();
                    let start = stack.len() - n as usize;
                    let items = stack.split_off(start);
                    let v = heap.alloc_array(items);
                    stack.push(v);
                }
                Op::Index => {
                    let index = pop!();
                    let target = pop!();
                    let (Value::Int(i), Value::Obj(r)) = (index, target) else {
                        fail!("no se puede indexar este valor");
                    };
                    let Object::Array(items) = heap.get(r) else {
                        fail!("no se puede indexar este valor");
                    };
                    if i < 0 || i as usize >= items.len() {
                        fail!(
                            "índice fuera de rango: {i} en un arreglo de {} elemento(s)",
                            items.len()
                        );
                    }
                    stack.push(items[i as usize]);
                }
                Op::SetIndex => {
                    let value = pop!();
                    let index = pop!();
                    let target = pop!();
                    let (Value::Int(i), Value::Obj(r)) = (index, target) else {
                        fail!("no se puede indexar este valor");
                    };
                    let items = heap.array_mut(r);
                    if i < 0 || i as usize >= items.len() {
                        fail!(
                            "índice fuera de rango: {i} en un arreglo de {} elemento(s)",
                            items.len()
                        );
                    }
                    items[i as usize] = value;
                }
                Op::Dup => {
                    let top = stack[stack.len() - 1];
                    stack.push(top);
                }
                Op::GetField(n) => {
                    let Value::Obj(r) = pop!() else {
                        fail!("no se puede leer un campo de este valor");
                    };
                    stack.push(heap.field(r, n));
                }
                Op::SetField(n) => {
                    let value = pop!();
                    let Value::Obj(r) = pop!() else {
                        fail!("no se puede escribir un campo de este valor");
                    };
                    heap.set_field(r, n, value);
                }
                Op::Alloc { class, n } => {
                    gc_point!();
                    let v = heap.alloc_instance(class, vec![Value::Unit; n as usize]);
                    stack.push(v);
                }
                Op::CallMethod { method, argc } => {
                    if frames.len() >= *max_depth {
                        fail!(
                            "desbordamiento de pila: más de {} llamadas anidadas",
                            max_depth
                        );
                    }
                    let Value::Obj(receiver) = stack[stack.len() - argc as usize - 1] else {
                        fail!("no se puede llamar a un método sobre este valor");
                    };
                    let Some(class) = heap.class_of(receiver) else {
                        fail!("no se puede llamar a un método sobre este valor");
                    };
                    let Some(&target) = module.classes[class as usize].vtable.get(&method) else {
                        fail!(
                            "la clase `{}` no tiene el método `{}`",
                            module.classes[class as usize].name,
                            module.methods[method as usize]
                        );
                    };
                    let callee = &module.functions[target as usize];
                    frames.push(Frame { function, ip, base });
                    base = stack.len() - callee.arity;
                    function = callee;
                    ip = 0;
                }
                Op::IsInstance(ty) => {
                    let Value::Obj(r) = pop!() else {
                        fail!("`is` necesita un objeto");
                    };
                    let is = heap
                        .class_of(r)
                        .is_some_and(|c| module.classes[c as usize].supers.contains(&ty));
                    stack.push(Value::Bool(is));
                }
                Op::Cast(ty) => {
                    let Value::Obj(r) = *top!() else {
                        fail!("`as` necesita un objeto");
                    };
                    let class = heap.class_of(r);
                    if !class.is_some_and(|c| module.classes[c as usize].supers.contains(&ty)) {
                        fail!(
                            "no se puede convertir un `{}` a `{}`",
                            class.map_or("?", |c| module.classes[c as usize].name.as_str()),
                            module.classes[ty as usize].name
                        );
                    }
                }
                Op::Dup2 => {
                    let n = stack.len();
                    stack.push(stack[n - 2]);
                    stack.push(stack[n - 1]);
                }
                Op::Len => {
                    let Value::Obj(r) = pop!() else {
                        fail!("`len` necesita una cadena o un arreglo");
                    };
                    let len = match heap.get(r) {
                        Object::Str(s) => s.chars().count(),
                        Object::Array(items) => items.len(),
                        Object::Instance { .. } => fail!("`len` necesita una cadena o un arreglo"),
                    };
                    stack.push(Value::Int(len as i64));
                }
                Op::Push => {
                    let value = pop!();
                    let Value::Obj(r) = pop!() else {
                        fail!("`push` necesita un arreglo");
                    };
                    heap.push(r, value);
                    stack.push(Value::Unit);
                }
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
                Op::Add => {
                    let n = stack.len();
                    match (stack[n - 2], stack[n - 1]) {
                        (Value::Int(x), Value::Int(y)) => match x.checked_add(y) {
                            Some(r) => {
                                stack.pop();
                                stack[n - 2] = Value::Int(r);
                            }
                            None => fail!("desbordamiento de entero (i64)"),
                        },
                        (Value::Obj(x), Value::Obj(y)) => {
                            // Los operandos siguen en la pila mientras el GC puede correr.
                            gc_point!();
                            let mut text =
                                String::with_capacity(heap.str(x).len() + heap.str(y).len());
                            text.push_str(heap.str(x));
                            text.push_str(heap.str(y));
                            stack.truncate(n - 2);
                            let v = heap.alloc_str(text);
                            stack.push(v);
                        }
                        _ => slow!(op),
                    }
                }
                Op::Sub => arith!(op, checked_sub),
                Op::Mul => arith!(op, checked_mul),
                Op::Div | Op::Rem => slow!(op),
                Op::Eq | Op::Ne => slow!(op),
                Op::Lt => compare!(op, <),
                Op::Le => compare!(op, <=),
                Op::Gt => compare!(op, >),
                Op::Ge => compare!(op, >=),
                Op::Jump(target) => ip = target as usize,
                Op::Nil => stack.push(Value::Nil),
                Op::JumpIfNil(target) => {
                    if *top!() == Value::Nil {
                        ip = target as usize;
                    }
                }
                Op::JumpIfNotNil(target) => {
                    if *top!() != Value::Nil {
                        ip = target as usize;
                    }
                }
                Op::Try(target) => handlers.push(Handler {
                    frames_len: frames.len(),
                    stack_len: stack.len(),
                    function,
                    base,
                    catch_ip: target as usize,
                }),
                Op::EndTry => {
                    handlers.pop();
                }
                Op::Fail => {
                    let exception = pop!();
                    match handlers.pop() {
                        Some(handler) => {
                            frames.truncate(handler.frames_len);
                            function = handler.function;
                            base = handler.base;
                            ip = handler.catch_ip;
                            stack.truncate(handler.stack_len);
                            stack.push(exception);
                        }
                        None => {
                            let Value::Obj(r) = exception else {
                                fail!("`fail` necesita un objeto");
                            };
                            let name = heap
                                .class_of(r)
                                .map_or("?", |c| module.classes[c as usize].name.as_str());
                            // `msg` es el primer campo de toda clase que hereda de `Fallo`.
                            let msg = heap.render(heap.field(r, 0));
                            fail!("excepción sin capturar `{name}`: {msg}");
                        }
                    }
                }
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
                    // Los `attempt` de esta función que no llegaron a su `EndTry` ya no valen.
                    while handlers
                        .last()
                        .is_some_and(|h| h.frames_len >= frames.len())
                    {
                        handlers.pop();
                    }
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
                    if let Err(e) = writeln!(out, "{}", heap.render(v)) {
                        fail!("no se pudo escribir la salida: {e}");
                    }
                }
                Op::Echo => {
                    let v = pop!();
                    if v != Value::Unit {
                        if let Err(e) = writeln!(out, "{}", heap.render(v)) {
                            fail!("no se pudo escribir la salida: {e}");
                        }
                    }
                }
            }
        }
    }
}

/// Como `binary`, pero también compara cadenas (necesita el heap para leerlas).
fn binary_h(heap: &Heap, op: Op, a: Value, b: Value, span: Span) -> Result<Value, Diagnostic> {
    if let (Value::Obj(x), Value::Obj(y)) = (a, b) {
        let (x, y) = (heap.str(x), heap.str(y));
        let result = match op {
            Op::Eq => x == y,
            Op::Ne => x != y,
            Op::Lt => x < y,
            Op::Le => x <= y,
            Op::Gt => x > y,
            _ => x >= y,
        };
        return Ok(Value::Bool(result));
    }
    binary(op, a, b, span)
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
                (Value::Unit, Value::Unit) | (Value::Nil, Value::Nil) => true,
                (Value::Nil, _) | (_, Value::Nil) => false,
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
