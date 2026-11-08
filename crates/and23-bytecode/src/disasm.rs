use crate::op::{Function, Module, Op};
use std::fmt::Write;

/// Texto legible del bytecode de una función, al estilo de `javap -c`.
pub fn disassemble(module: &Module, function: &Function) -> String {
    let mut out = format!("fn {}/{}\n", function.name, function.arity);
    for (i, op) in function.code.iter().enumerate() {
        let detail = match op {
            Op::GetGlobal(n) | Op::SetGlobal(n) | Op::DefGlobal(n) => {
                format!("  ; {}", module.globals[*n as usize])
            }
            Op::Str(n) => format!("  ; {:?}", module.strings[*n as usize]),
            Op::Call(n) => format!("  ; {}", module.functions[*n as usize].name),
            Op::Closure { func, n } => {
                format!(
                    "  ; {} (captura {n})",
                    module.functions[*func as usize].name
                )
            }
            Op::Native(n) => format!("  ; {}", and23_types::NATIVES[*n as usize].name),
            Op::Alloc { class, .. } => format!("  ; {}", module.classes[*class as usize].name),
            Op::IsInstance(n) | Op::Cast(n) => {
                format!("  ; {}", module.classes[*n as usize].name)
            }
            Op::CallMethod { method, .. } => format!("  ; {}", module.methods[*method as usize]),
            _ => String::new(),
        };
        let _ = writeln!(out, "  {i:04}  {op:?}{detail}");
    }
    out
}

/// Cuántas entradas de cada tabla del módulo hay antes de lo que se quiere volcar (las del preludio).
#[derive(Debug, Clone, Copy, Default)]
pub struct DumpFrom {
    pub functions: usize,
    pub classes: usize,
    pub methods: usize,
    pub globals: usize,
    pub strings: usize,
}

impl DumpFrom {
    /// El estado de las tablas de `module` ahora.
    pub fn of(module: &Module) -> Self {
        DumpFrom {
            functions: module.functions.len(),
            classes: module.classes.len(),
            methods: module.methods.len(),
            globals: module.globals.len(),
            strings: module.strings.len(),
        }
    }
}

/// El módulo compilado, a partir de `from`, en un texto exacto y estable para comparar compiladores (el de Rust y
/// el escrito en And23, `mn/`): las tablas con su número, y cada función con sus instrucciones y la posición de
/// cada una. `at_prelude` y `at_program` pasan una posición en bytes del fuente del preludio o del programa a
/// una en caracteres; las funciones con índice menor a `Module::prelude_functions` son del preludio.
pub fn dump_module(
    module: &Module,
    from: DumpFrom,
    at_prelude: &dyn Fn(usize) -> usize,
    at_program: &dyn Fn(usize) -> usize,
) -> String {
    let mut out = String::from("Modulo\n");
    let _ = writeln!(out, "Cadenas {}", module.strings.len());
    for (i, s) in module.strings.iter().enumerate().skip(from.strings) {
        let _ = writeln!(out, "  {i} {s:?}");
    }
    let _ = writeln!(out, "Globales {}", module.globals.len());
    for (i, g) in module.globals.iter().enumerate().skip(from.globals) {
        let _ = writeln!(out, "  {i} {g}");
    }
    let _ = writeln!(out, "Metodos {}", module.methods.len());
    for (i, m) in module.methods.iter().enumerate().skip(from.methods) {
        let _ = writeln!(out, "  {i} {m}");
    }
    let _ = writeln!(out, "Clases {}", module.classes.len());
    for (i, c) in module.classes.iter().enumerate().skip(from.classes) {
        let mut vtable: Vec<(u32, u32)> = c.vtable.iter().map(|(m, f)| (*m, *f)).collect();
        vtable.sort();
        let vtable: Vec<String> = vtable.iter().map(|(m, f)| format!("{m}:{f}")).collect();
        let _ = writeln!(
            out,
            "  {i} {} campos=[{}] supers=[{}] vtable=[{}]",
            c.name,
            c.fields.join(","),
            c.supers
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join(","),
            vtable.join(",")
        );
    }
    let _ = writeln!(out, "Funciones {}", module.functions.len());
    for (index, function) in module.functions.iter().enumerate().skip(from.functions) {
        let at = if index < module.prelude_functions {
            at_prelude
        } else {
            at_program
        };
        let _ = writeln!(
            out,
            "fn {index} {}/{} @{}-{}",
            function.name,
            function.arity,
            at(function.name_span.start),
            at(function.name_span.end)
        );
        for (i, op) in function.code.iter().enumerate() {
            let span = function.spans[i];
            let _ = writeln!(
                out,
                "  {i:04} {} @{}-{}",
                op_text(op),
                at(span.start),
                at(span.end)
            );
        }
    }
    out
}

/// Una instrucción como `Nombre arg arg` (sin llaves ni paréntesis).
fn op_text(op: &Op) -> String {
    match op {
        Op::Alloc { class, n } => format!("Alloc {class} {n}"),
        Op::CallMethod { method, argc } => format!("CallMethod {method} {argc}"),
        Op::Closure { func, n } => format!("Closure {func} {n}"),
        other => {
            let text = format!("{other:?}");
            match text.split_once('(') {
                Some((name, rest)) => format!("{name} {}", rest.trim_end_matches(')')),
                None => text,
            }
        }
    }
}
