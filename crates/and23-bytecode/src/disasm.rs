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
            _ => String::new(),
        };
        let _ = writeln!(out, "  {i:04}  {op:?}{detail}");
    }
    out
}
