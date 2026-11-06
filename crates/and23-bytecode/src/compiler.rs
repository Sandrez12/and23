use crate::op::{Function, Module, Op};
use and23_syntax::{BinOp, Block, Diagnostic, Expr, FnDecl, InterpPart, Program, Span, Stmt, UnOp};
use std::collections::HashMap;

/// Nombres conocidos entre compilaciones (el REPL compila entrada tras entrada).
#[derive(Clone, Default)]
struct Tables {
    /// nombre -> (ranura, mutable)
    globals: HashMap<String, (u32, bool)>,
    /// nombre -> (índice de la función, aridad)
    funcs: HashMap<String, (u32, usize)>,
    /// cadena constante -> índice en `Module::strings`
    strings: HashMap<String, u32>,
}

/// Compilador con estado: cada llamada a `compile` añade funciones y globales al módulo.
#[derive(Default)]
pub struct Compiler {
    module: Module,
    tables: Tables,
}

impl Compiler {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn module(&self) -> &Module {
        &self.module
    }

    /// Índice de `main`, si el programa la definió.
    pub fn main_index(&self) -> Option<usize> {
        self.tables.funcs.get("main").map(|(i, _)| *i as usize)
    }

    /// Olvida las globales nuevas (ranura >= `from`) que `keep` rechaza y devuelve sus nombres.
    /// El REPL lo usa cuando un error de ejecución dejó una global declarada pero sin valor.
    pub fn forget_globals(&mut self, from: usize, keep: impl Fn(u32) -> bool) -> Vec<String> {
        let forgotten: Vec<String> = self
            .tables
            .globals
            .iter()
            .filter(|(_, (slot, _))| (*slot as usize) >= from && !keep(*slot))
            .map(|(name, _)| name.clone())
            .collect();
        for name in &forgotten {
            self.tables.globals.remove(name);
        }
        forgotten
    }

    /// Compila el programa. Devuelve el índice de la función "script" con el código del nivel
    /// superior. Si falla, el compilador queda como estaba.
    pub fn compile(&mut self, program: &Program, echo: bool) -> Result<usize, Diagnostic> {
        let saved_tables = self.tables.clone();
        let saved_functions = self.module.functions.len();
        let saved_globals = self.module.globals.len();
        let saved_strings = self.module.strings.len();
        let result = self.compile_program(program, echo);
        if result.is_err() {
            self.tables = saved_tables;
            self.module.functions.truncate(saved_functions);
            self.module.globals.truncate(saved_globals);
            self.module.strings.truncate(saved_strings);
        }
        result
    }

    fn compile_program(&mut self, program: &Program, echo: bool) -> Result<usize, Diagnostic> {
        // Las funciones se registran antes de compilar: pueden usarse antes de su definición.
        let mut decls: Vec<(usize, &FnDecl)> = Vec::new();
        for stmt in &program.stmts {
            if let Stmt::Fn(decl) = stmt {
                if self.tables.funcs.contains_key(&decl.name) {
                    return Err(Diagnostic::new(
                        decl.name_span,
                        format!("la función `{}` ya está definida", decl.name),
                    ));
                }
                let index = self.module.functions.len();
                self.module.functions.push(Function {
                    name: decl.name.clone(),
                    name_span: decl.name_span,
                    arity: decl.params.len(),
                    code: Vec::new(),
                    spans: Vec::new(),
                });
                self.tables
                    .funcs
                    .insert(decl.name.clone(), (index as u32, decl.params.len()));
                decls.push((index, decl));
            }
        }
        // Lo mismo con las globales del nivel superior: una función puede nombrar una global
        // declarada más abajo (fallará al ejecutar si todavía no tiene valor).
        for stmt in &program.stmts {
            if let Stmt::Let {
                name,
                name_span,
                mutable,
                ..
            } = stmt
            {
                if self.tables.globals.contains_key(name) {
                    return Err(already_declared(name, *name_span));
                }
                let slot = self.module.globals.len() as u32;
                self.module.globals.push(name.clone());
                self.tables.globals.insert(name.clone(), (slot, *mutable));
            }
        }

        for (index, decl) in decls {
            let function = FnCompiler::new(
                &mut self.tables,
                &mut self.module.strings,
                decl.name.clone(),
                decl.name_span,
            )
            .function(decl)?;
            self.module.functions[index] = function;
        }

        let mut script = FnCompiler::new(
            &mut self.tables,
            &mut self.module.strings,
            "<script>".to_string(),
            Span::new(0, 0),
        );
        script.script = true;
        script.echo = echo;
        script.top_level(program)?;
        let function = script.finish(Span::new(0, 0));
        self.module.functions.push(function);
        Ok(self.module.functions.len() - 1)
    }
}

struct Local {
    name: String,
    mutable: bool,
    depth: u32,
}

struct LoopCtx {
    /// Posición de la condición, destino de `continue`.
    head: u32,
    /// Cantidad de locales al entrar en el bucle.
    locals: usize,
    /// Saltos de `break` por completar.
    breaks: Vec<usize>,
}

struct FnCompiler<'a> {
    tables: &'a mut Tables,
    strings: &'a mut Vec<String>,
    name: String,
    name_span: Span,
    arity: usize,
    code: Vec<Op>,
    spans: Vec<Span>,
    locals: Vec<Local>,
    depth: u32,
    loops: Vec<LoopCtx>,
    /// Es el código del nivel superior: sus `fix`/`mut` de profundidad 0 son globales.
    script: bool,
    echo: bool,
}

impl<'a> FnCompiler<'a> {
    fn new(
        tables: &'a mut Tables,
        strings: &'a mut Vec<String>,
        name: String,
        name_span: Span,
    ) -> Self {
        FnCompiler {
            tables,
            strings,
            name,
            name_span,
            arity: 0,
            code: Vec::new(),
            spans: Vec::new(),
            locals: Vec::new(),
            depth: 0,
            loops: Vec::new(),
            script: false,
            echo: false,
        }
    }

    fn finish(mut self, span: Span) -> Function {
        self.emit(Op::Unit, span);
        self.emit(Op::Return, span);
        Function {
            name: self.name,
            name_span: self.name_span,
            arity: self.arity,
            code: self.code,
            spans: self.spans,
        }
    }

    fn function(mut self, decl: &FnDecl) -> Result<Function, Diagnostic> {
        self.arity = decl.params.len();
        for param in &decl.params {
            // Los parámetros son inmutables; el cuerpo es un ámbito aparte (puede sombrearlos).
            self.locals.push(Local {
                name: param.name.clone(),
                mutable: false,
                depth: 0,
            });
        }
        self.block(&decl.body)?;
        Ok(self.finish(decl.span))
    }

    fn top_level(&mut self, program: &Program) -> Result<(), Diagnostic> {
        for stmt in &program.stmts {
            match stmt {
                Stmt::Fn(_) => {}
                Stmt::Expr(expr) => {
                    self.expr(expr)?;
                    self.emit(if self.echo { Op::Echo } else { Op::Pop }, expr.span());
                }
                _ => self.stmt(stmt)?,
            }
        }
        Ok(())
    }

    fn emit(&mut self, op: Op, span: Span) -> usize {
        self.code.push(op);
        self.spans.push(span);
        self.code.len() - 1
    }

    /// Índice de la cadena constante `text` (se reutilizan las repetidas).
    fn string(&mut self, text: &str) -> u32 {
        if let Some(&index) = self.tables.strings.get(text) {
            return index;
        }
        let index = self.strings.len() as u32;
        self.strings.push(text.to_string());
        self.tables.strings.insert(text.to_string(), index);
        index
    }

    fn here(&self) -> u32 {
        self.code.len() as u32
    }

    /// Completa un salto emitido antes con el destino actual.
    fn patch(&mut self, at: usize) {
        let target = self.here();
        self.code[at] = match self.code[at] {
            Op::Jump(_) => Op::Jump(target),
            Op::JumpIfFalse(_) => Op::JumpIfFalse(target),
            Op::AndJump(_) => Op::AndJump(target),
            Op::OrJump(_) => Op::OrJump(target),
            other => other,
        };
    }

    fn pop_locals(&mut self, count: usize, span: Span) {
        match count {
            0 => {}
            1 => {
                self.emit(Op::Pop, span);
            }
            n => {
                self.emit(Op::PopN(n as u32), span);
            }
        }
    }

    fn block(&mut self, block: &Block) -> Result<(), Diagnostic> {
        self.depth += 1;
        for stmt in &block.stmts {
            self.stmt(stmt)?;
        }
        self.depth -= 1;
        let keep = self
            .locals
            .iter()
            .take_while(|local| local.depth <= self.depth)
            .count();
        let dropped = self.locals.len() - keep;
        self.locals.truncate(keep);
        self.pop_locals(dropped, block.span);
        Ok(())
    }

    fn stmt(&mut self, stmt: &Stmt) -> Result<(), Diagnostic> {
        match stmt {
            Stmt::Show(expr, span) => {
                self.expr(expr)?;
                self.emit(Op::Show, *span);
            }
            Stmt::Expr(expr) => {
                self.expr(expr)?;
                self.emit(Op::Pop, expr.span());
            }
            Stmt::Let {
                name,
                name_span,
                mutable,
                init,
                span,
                ..
            } => {
                self.expr(init)?;
                if self.script && self.depth == 0 {
                    let (slot, _) = self.tables.globals[name];
                    self.emit(Op::DefGlobal(slot), *span);
                } else {
                    if self
                        .locals
                        .iter()
                        .any(|local| local.depth == self.depth && local.name == *name)
                    {
                        return Err(already_declared(name, *name_span));
                    }
                    // El valor ya está en la cima de la pila: esa posición es el local.
                    self.locals.push(Local {
                        name: name.clone(),
                        mutable: *mutable,
                        depth: self.depth,
                    });
                }
            }
            Stmt::Assign {
                name,
                name_span,
                op,
                value,
                span,
            } => {
                let (get, set, mutable) = self.resolve(name, *name_span)?;
                if !mutable {
                    return Err(Diagnostic::new(
                        *name_span,
                        format!("no se puede asignar a `{name}` porque es inmutable; decláralo con `mut`"),
                    ));
                }
                if let Some(op) = op {
                    self.emit(get, *name_span);
                    self.expr(value)?;
                    self.emit(binary_op(*op), *span);
                } else {
                    self.expr(value)?;
                }
                self.emit(set, *span);
            }
            Stmt::IndexAssign {
                target,
                index,
                op,
                value,
                span,
            } => {
                self.expr(target)?;
                self.expr(index)?;
                if let Some(op) = op {
                    self.emit(Op::Dup2, *span);
                    self.emit(Op::Index, *span);
                    self.expr(value)?;
                    self.emit(binary_op(*op), *span);
                } else {
                    self.expr(value)?;
                }
                self.emit(Op::SetIndex, *span);
            }
            Stmt::If {
                cond,
                then,
                otherwise,
                ..
            } => {
                self.expr(cond)?;
                let to_else = self.emit(Op::JumpIfFalse(0), cond.span());
                self.block(then)?;
                if let Some(otherwise) = otherwise {
                    let to_end = self.emit(Op::Jump(0), then.span);
                    self.patch(to_else);
                    self.block(otherwise)?;
                    self.patch(to_end);
                } else {
                    self.patch(to_else);
                }
            }
            Stmt::While { cond, body, .. } => {
                let head = self.here();
                self.expr(cond)?;
                let exit = self.emit(Op::JumpIfFalse(0), cond.span());
                self.loops.push(LoopCtx {
                    head,
                    locals: self.locals.len(),
                    breaks: Vec::new(),
                });
                self.block(body)?;
                self.emit(Op::Jump(head), body.span);
                self.patch(exit);
                if let Some(ctx) = self.loops.pop() {
                    for at in ctx.breaks {
                        self.patch(at);
                    }
                }
            }
            Stmt::Return(expr, span) => {
                match expr {
                    Some(expr) => self.expr(expr)?,
                    None => {
                        self.emit(Op::Unit, *span);
                    }
                }
                self.emit(Op::Return, *span);
            }
            Stmt::Break(span) => {
                if let Some(ctx) = self.loops.last() {
                    let extra = self.locals.len() - ctx.locals;
                    self.pop_locals(extra, *span);
                    let at = self.emit(Op::Jump(0), *span);
                    if let Some(ctx) = self.loops.last_mut() {
                        ctx.breaks.push(at);
                    }
                }
            }
            Stmt::Continue(span) => {
                if let Some(ctx) = self.loops.last() {
                    let (head, extra) = (ctx.head, self.locals.len() - ctx.locals);
                    self.pop_locals(extra, *span);
                    self.emit(Op::Jump(head), *span);
                }
            }
            // Las registró `compile_program`.
            Stmt::Fn(_) => {}
        }
        Ok(())
    }

    /// Instrucciones para leer y escribir una variable, y si es mutable.
    fn resolve(&self, name: &str, span: Span) -> Result<(Op, Op, bool), Diagnostic> {
        if let Some(slot) = self.locals.iter().rposition(|local| local.name == name) {
            let local = &self.locals[slot];
            let slot = slot as u32;
            return Ok((Op::GetLocal(slot), Op::SetLocal(slot), local.mutable));
        }
        if let Some((slot, mutable)) = self.tables.globals.get(name) {
            return Ok((Op::GetGlobal(*slot), Op::SetGlobal(*slot), *mutable));
        }
        Err(Diagnostic::new(
            span,
            format!("la variable `{name}` no está definida"),
        ))
    }

    fn expr(&mut self, expr: &Expr) -> Result<(), Diagnostic> {
        match expr {
            Expr::Int(v, span) => {
                self.emit(Op::Int(*v), *span);
            }
            Expr::Float(v, span) => {
                self.emit(Op::Float(*v), *span);
            }
            Expr::Bool(v, span) => {
                self.emit(Op::Bool(*v), *span);
            }
            Expr::Var(name, span) => {
                let (get, _, _) = self.resolve(name, *span)?;
                self.emit(get, *span);
            }
            Expr::Str(text, span) => {
                let index = self.string(text);
                self.emit(Op::Str(index), *span);
            }
            Expr::Interp { parts, span } => {
                for part in parts {
                    match part {
                        InterpPart::Text(text) => {
                            let index = self.string(text);
                            self.emit(Op::Str(index), *span);
                        }
                        InterpPart::Expr(inner) => self.expr(inner)?,
                    }
                }
                self.emit(Op::Interp(parts.len() as u32), *span);
            }
            Expr::Array { elems, span } => {
                for elem in elems {
                    self.expr(elem)?;
                }
                self.emit(Op::Array(elems.len() as u32), *span);
            }
            Expr::Index {
                target,
                index,
                span,
            } => {
                self.expr(target)?;
                self.expr(index)?;
                self.emit(Op::Index, *span);
            }
            Expr::Unary { op, expr, span } => {
                self.expr(expr)?;
                self.emit(
                    match op {
                        UnOp::Neg => Op::Neg,
                        UnOp::Not => Op::Not,
                    },
                    *span,
                );
            }
            Expr::Binary { op, lhs, rhs, span } => match op {
                BinOp::And | BinOp::Or => {
                    let is_and = *op == BinOp::And;
                    self.expr(lhs)?;
                    let skip = self.emit(
                        if is_and {
                            Op::AndJump(0)
                        } else {
                            Op::OrJump(0)
                        },
                        *span,
                    );
                    self.expr(rhs)?;
                    self.emit(Op::CheckBool(is_and), *span);
                    self.patch(skip);
                }
                _ => {
                    self.expr(lhs)?;
                    self.expr(rhs)?;
                    self.emit(binary_op(*op), *span);
                }
            },
            Expr::Call { name, args, span } if name == "len" || name == "push" => {
                for arg in args {
                    self.expr(arg)?;
                }
                self.emit(if name == "len" { Op::Len } else { Op::Push }, *span);
            }
            Expr::Call { name, args, span } => {
                let Some(&(index, arity)) = self.tables.funcs.get(name) else {
                    return Err(Diagnostic::new(
                        *span,
                        format!("la función `{name}` no está definida"),
                    ));
                };
                if arity != args.len() {
                    return Err(Diagnostic::new(
                        *span,
                        format!(
                            "la función `{name}` espera {arity} argumento(s), pero recibió {}",
                            args.len()
                        ),
                    ));
                }
                for arg in args {
                    self.expr(arg)?;
                }
                self.emit(Op::Call(index), *span);
            }
        }
        Ok(())
    }
}

fn binary_op(op: BinOp) -> Op {
    match op {
        BinOp::Add => Op::Add,
        BinOp::Sub => Op::Sub,
        BinOp::Mul => Op::Mul,
        BinOp::Div => Op::Div,
        BinOp::Rem => Op::Rem,
        BinOp::Eq => Op::Eq,
        BinOp::Ne => Op::Ne,
        BinOp::Lt => Op::Lt,
        BinOp::Le => Op::Le,
        BinOp::Gt => Op::Gt,
        BinOp::Ge => Op::Ge,
        // `&&` y `||` se compilan aparte, con cortocircuito.
        BinOp::And | BinOp::Or => unreachable!("los operadores lógicos no pasan por aquí"),
    }
}

fn already_declared(name: &str, span: Span) -> Diagnostic {
    Diagnostic::new(span, format!("`{name}` ya está declarada en este ámbito"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use and23_syntax::parse;

    fn compile(src: &str) -> Result<(Compiler, usize), String> {
        let mut c = Compiler::new();
        let program = parse(src).map_err(|d| d.message)?;
        let script = c.compile(&program, false).map_err(|d| d.message)?;
        Ok((c, script))
    }

    fn script_ops(src: &str) -> Vec<Op> {
        let (c, script) = compile(src).unwrap();
        c.module().functions[script].code.clone()
    }

    #[test]
    fn suma_de_enteros() {
        assert_eq!(
            script_ops("show(1 + 2)"),
            vec![
                Op::Int(1),
                Op::Int(2),
                Op::Add,
                Op::Show,
                Op::Unit,
                Op::Return
            ]
        );
    }

    #[test]
    fn variable_no_definida_se_detecta_al_compilar() {
        assert_eq!(
            compile("show(x)").err().unwrap(),
            "la variable `x` no está definida"
        );
    }

    #[test]
    fn asignar_a_fix_se_detecta_al_compilar() {
        assert!(compile("fix x = 1\nx = 2")
            .err()
            .unwrap()
            .contains("inmutable"));
    }

    #[test]
    fn aridad_incorrecta_se_detecta_al_compilar() {
        let err = compile("fn f(a: Int) = a\nshow(f(1, 2))").err().unwrap();
        assert!(err.contains("espera 1 argumento(s), pero recibió 2"));
    }

    #[test]
    fn un_error_deja_el_compilador_intacto() {
        let mut c = Compiler::new();
        let bad = parse("fn f() = 1\nmut a = 1\nshow(zzz)").unwrap();
        assert!(c.compile(&bad, false).is_err());
        assert!(c.module().functions.is_empty());
        assert!(c.module().globals.is_empty());
        let ok = parse("fn f() = 2\nmut a = 1").unwrap();
        assert!(c.compile(&ok, false).is_ok());
    }

    #[test]
    fn los_locales_de_un_bloque_se_sacan_al_salir() {
        let ops = script_ops("if true { fix a = 1\nfix b = 2 }");
        assert!(ops.contains(&Op::PopN(2)));
    }

    #[test]
    fn el_nivel_superior_declara_globales() {
        let ops = script_ops("fix a = 1");
        assert_eq!(ops[1], Op::DefGlobal(0));
    }
}
