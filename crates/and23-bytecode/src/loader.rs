//! Lee el texto de `dump_module` y lo convierte otra vez en un `Module`. Es la puerta de entrada de los módulos que
//! compila el compilador escrito en And23 (`mn/compilador.jb`): escribe ese texto y la VM lo ejecuta.

use crate::op::{ClassDef, Function, Module, Op};
use and23_syntax::Span;
use std::collections::HashMap;

/// Un módulo cargado de un volcado y dónde empezar a ejecutarlo.
#[derive(Debug)]
pub struct Loaded {
    pub module: Module,
    /// La función `<script>` del preludio.
    pub prelude_script: usize,
    /// La función `<script>` del programa, si el volcado del programa tiene una.
    pub program_script: Option<usize>,
    /// La función `main` del programa, si la definió.
    pub main: Option<usize>,
}

/// Une el volcado del preludio con el de un programa (que continúa sus tablas) en un solo módulo.
pub fn load_dump(prelude: &str, program: &str) -> Result<Loaded, String> {
    let mut module = Module::default();
    read_into(&mut module, prelude, "preludio")?;
    module.prelude_functions = module.functions.len();
    let prelude_script = module
        .functions
        .iter()
        .position(|f| f.name == "<script>")
        .ok_or("el volcado del preludio no tiene `<script>`")?;
    read_into(&mut module, program, "programa")?;
    let program_script = module
        .functions
        .iter()
        .enumerate()
        .skip(module.prelude_functions)
        .find(|(_, f)| f.name == "<script>")
        .map(|(i, _)| i);
    let main = module
        .functions
        .iter()
        .enumerate()
        .skip(module.prelude_functions)
        .rev()
        .find(|(_, f)| f.name == "main")
        .map(|(i, _)| i);
    Ok(Loaded {
        module,
        prelude_script,
        program_script,
        main,
    })
}

#[derive(PartialEq)]
enum Section {
    Head,
    Strings,
    Globals,
    Methods,
    Classes,
    Functions,
}

fn read_into(module: &mut Module, text: &str, what: &str) -> Result<(), String> {
    let mut section = Section::Head;
    for (n, line) in text.lines().enumerate() {
        let fail = |m: &str| format!("volcado del {what}, línea {}: {m}", n + 1);
        if line == "Modulo" || line == "---" {
            continue;
        }
        if let Some(rest) = line.strip_prefix("Error ") {
            return Err(format!("el volcado del {what} es un error: {rest}"));
        }
        if let Some(h) = header(line) {
            section = h;
            continue;
        }
        if let Some(rest) = line.strip_prefix("fn ") {
            if section != Section::Functions {
                return Err(fail("`fn` fuera de `Funciones`"));
            }
            let (index, rest) = rest
                .split_once(' ')
                .ok_or_else(|| fail("`fn` sin nombre"))?;
            let index: usize = index.parse().map_err(|_| fail("índice de función"))?;
            if index != module.functions.len() {
                return Err(fail("las funciones no continúan la tabla"));
            }
            let (head, span) = rest
                .rsplit_once(" @")
                .ok_or_else(|| fail("falta la posición"))?;
            let (name, arity) = head
                .rsplit_once('/')
                .ok_or_else(|| fail("falta la aridad"))?;
            module.functions.push(Function {
                name: name.to_string(),
                name_span: parse_span(span).ok_or_else(|| fail("posición"))?,
                arity: arity.parse().map_err(|_| fail("aridad"))?,
                code: Vec::new(),
                spans: Vec::new(),
            });
            continue;
        }
        let Some(body) = line.strip_prefix("  ") else {
            if line.is_empty() {
                continue;
            }
            return Err(fail("línea inesperada"));
        };
        match section {
            Section::Head => return Err(fail("línea fuera de una sección")),
            Section::Strings => {
                let (i, s) =
                    split_index(body, module.strings.len()).ok_or_else(|| fail("cadena"))?;
                let _ = i;
                module
                    .strings
                    .push(unescape(s).ok_or_else(|| fail("cadena mal escapada"))?);
            }
            Section::Globals => {
                let (_, s) =
                    split_index(body, module.globals.len()).ok_or_else(|| fail("global"))?;
                module.globals.push(s.to_string());
            }
            Section::Methods => {
                let (_, s) =
                    split_index(body, module.methods.len()).ok_or_else(|| fail("método"))?;
                module.methods.push(s.to_string());
            }
            Section::Classes => {
                let (_, s) =
                    split_index(body, module.classes.len()).ok_or_else(|| fail("clase"))?;
                module
                    .classes
                    .push(parse_class(s).ok_or_else(|| fail("clase"))?);
            }
            Section::Functions => {
                let function = module
                    .functions
                    .last_mut()
                    .ok_or_else(|| fail("instrucción sin función"))?;
                let (idx, rest) = body.split_once(' ').ok_or_else(|| fail("instrucción"))?;
                if idx.parse::<usize>().ok() != Some(function.code.len()) {
                    return Err(fail("las instrucciones no son consecutivas"));
                }
                let (op, span) = rest
                    .rsplit_once(" @")
                    .ok_or_else(|| fail("falta la posición"))?;
                function
                    .code
                    .push(parse_op(op).ok_or_else(|| fail(&format!("instrucción `{op}`")))?);
                function
                    .spans
                    .push(parse_span(span).ok_or_else(|| fail("posición"))?);
            }
        }
    }
    Ok(())
}

fn header(line: &str) -> Option<Section> {
    let (name, count) = line.split_once(' ')?;
    count.parse::<usize>().ok()?;
    match name {
        "Cadenas" => Some(Section::Strings),
        "Globales" => Some(Section::Globals),
        "Metodos" => Some(Section::Methods),
        "Clases" => Some(Section::Classes),
        "Funciones" => Some(Section::Functions),
        _ => None,
    }
}

/// `"3 resto"` -> `resto`, comprobando que el número es el que toca.
fn split_index(body: &str, expected: usize) -> Option<(usize, &str)> {
    let (i, rest) = body.split_once(' ')?;
    (i.parse::<usize>().ok()? == expected).then_some((expected, rest))
}

fn parse_span(text: &str) -> Option<Span> {
    let (a, b) = text.split_once('-')?;
    Some(Span::new(a.parse().ok()?, b.parse().ok()?))
}

fn list(text: &str) -> Option<&str> {
    text.strip_prefix('[')?.strip_suffix(']')
}

/// `Nombre campos=[a,b] supers=[0,1] vtable=[2:3]`.
fn parse_class(text: &str) -> Option<ClassDef> {
    let (name, rest) = text.split_once(" campos=")?;
    let (fields, rest) = rest.split_once(" supers=")?;
    let (supers, vtable) = rest.split_once(" vtable=")?;
    let split = |s: &str| -> Vec<String> {
        if s.is_empty() {
            Vec::new()
        } else {
            s.split(',').map(str::to_string).collect()
        }
    };
    let mut table = HashMap::new();
    for pair in split(list(vtable)?) {
        let (m, f) = pair.split_once(':')?;
        table.insert(m.parse().ok()?, f.parse().ok()?);
    }
    Some(ClassDef {
        name: name.to_string(),
        fields: split(list(fields)?),
        supers: split(list(supers)?)
            .iter()
            .map(|s| s.parse().ok())
            .collect::<Option<_>>()?,
        vtable: table,
    })
}

fn parse_op(text: &str) -> Option<Op> {
    let (name, args) = match text.split_once(' ') {
        Some((n, a)) => (n, a),
        None => (text, ""),
    };
    let u = || args.parse::<u32>().ok();
    let two = || -> Option<(u32, u32)> {
        let (a, b) = args.split_once(' ')?;
        Some((a.parse().ok()?, b.parse().ok()?))
    };
    Some(match name {
        "Int" => Op::Int(args.parse().ok()?),
        "Float" => Op::Float(args.parse().ok()?),
        "Bool" => Op::Bool(args.parse().ok()?),
        "Unit" => Op::Unit,
        "Nil" => Op::Nil,
        "Str" => Op::Str(u()?),
        "GetLocal" => Op::GetLocal(u()?),
        "SetLocal" => Op::SetLocal(u()?),
        "GetGlobal" => Op::GetGlobal(u()?),
        "SetGlobal" => Op::SetGlobal(u()?),
        "DefGlobal" => Op::DefGlobal(u()?),
        "Pop" => Op::Pop,
        "PopN" => Op::PopN(u()?),
        "Neg" => Op::Neg,
        "Not" => Op::Not,
        "Add" => Op::Add,
        "Sub" => Op::Sub,
        "Mul" => Op::Mul,
        "Div" => Op::Div,
        "Rem" => Op::Rem,
        "Eq" => Op::Eq,
        "Ne" => Op::Ne,
        "Lt" => Op::Lt,
        "Le" => Op::Le,
        "Gt" => Op::Gt,
        "Ge" => Op::Ge,
        "Jump" => Op::Jump(u()?),
        "JumpIfFalse" => Op::JumpIfFalse(u()?),
        "AndJump" => Op::AndJump(u()?),
        "OrJump" => Op::OrJump(u()?),
        "CheckBool" => Op::CheckBool(args.parse().ok()?),
        "Interp" => Op::Interp(u()?),
        "Array" => Op::Array(u()?),
        "Index" => Op::Index,
        "SetIndex" => Op::SetIndex,
        "GetField" => Op::GetField(u()?),
        "SetField" => Op::SetField(u()?),
        "Dup" => Op::Dup,
        "Alloc" => {
            let (class, n) = two()?;
            Op::Alloc { class, n }
        }
        "CallMethod" => {
            let (method, argc) = two()?;
            Op::CallMethod { method, argc }
        }
        "JumpIfNil" => Op::JumpIfNil(u()?),
        "JumpIfNotNil" => Op::JumpIfNotNil(u()?),
        "Try" => Op::Try(u()?),
        "EndTry" => Op::EndTry,
        "Fail" => Op::Fail,
        "IsInstance" => Op::IsInstance(u()?),
        "Cast" => Op::Cast(u()?),
        "Dup2" => Op::Dup2,
        "Len" => Op::Len,
        "Push" => Op::Push,
        "ArrayPop" => Op::ArrayPop,
        "Native" => Op::Native(u()?),
        "Hash" => Op::Hash,
        "NewCell" => Op::NewCell,
        "CellGet" => Op::CellGet,
        "CellSet" => Op::CellSet,
        "GetCapture" => Op::GetCapture(u()?),
        "Closure" => {
            let (func, n) = two()?;
            Op::Closure { func, n }
        }
        "CallValue" => Op::CallValue(u()?),
        "Call" => Op::Call(u()?),
        "Return" => Op::Return,
        "Show" => Op::Show,
        "Echo" => Op::Echo,
        _ => return None,
    })
}

/// Deshace el `{:?}` de Rust de una cadena: entre comillas, con `\n`, `\"`, `\u{..}`, etc.
fn unescape(text: &str) -> Option<String> {
    let inner = text.strip_prefix('"')?.strip_suffix('"')?;
    let mut out = String::new();
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next()? {
            'n' => out.push('\n'),
            'r' => out.push('\r'),
            't' => out.push('\t'),
            '0' => out.push('\0'),
            '\\' => out.push('\\'),
            '"' => out.push('"'),
            '\'' => out.push('\''),
            'u' => {
                if chars.next()? != '{' {
                    return None;
                }
                let mut hex = String::new();
                loop {
                    match chars.next()? {
                        '}' => break,
                        h => hex.push(h),
                    }
                }
                out.push(char::from_u32(u32::from_str_radix(&hex, 16).ok()?)?);
            }
            _ => return None,
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{dump_module, Compiler, DumpFrom};
    use and23_types::Checker;

    #[test]
    fn deshace_el_escape_de_rust() {
        for s in ["hola", "a\nb\t\"c\"\\", "\u{feff}x\u{0}\u{7f}é😀", "'"] {
            assert_eq!(unescape(&format!("{s:?}")).as_deref(), Some(s));
        }
    }

    #[test]
    fn el_volcado_se_vuelve_a_cargar_igual() {
        let src = and23_syntax::PRELUDE;
        let prelude = and23_syntax::parse(src).unwrap();
        let mut checker = Checker::new();
        let mut compiler = Compiler::new();
        let res = checker.check(&prelude).unwrap();
        compiler.compile(&prelude, &res, false).unwrap();
        checker.seal_prelude();
        compiler.seal_prelude();
        let id = |b: usize| b;
        let prelude_dump = dump_module(compiler.module(), DumpFrom::default(), &id, &id);
        let from = DumpFrom::of(compiler.module());
        let program = and23_syntax::parse(
            "thing P(fix x: Int)
fn main() { show(P(1).x + 2)
show(2.5e3 * 2.0)
show(\"a\n${1}\") }",
        )
        .unwrap();
        let res = checker.check(&program).unwrap();
        compiler.compile(&program, &res, false).unwrap();
        let program_dump = dump_module(compiler.module(), from, &id, &id);
        let loaded = load_dump(&prelude_dump, &program_dump).unwrap();
        let again = dump_module(&loaded.module, DumpFrom::default(), &id, &id);
        let original = dump_module(compiler.module(), DumpFrom::default(), &id, &id);
        assert_eq!(again, original);
        assert!(loaded.main.is_some());
    }
}
