use crate::op::{ClassDef, Function, Module, Op};
use and23_syntax::{
    BinOp, Block, Diagnostic, Expr, FnDecl, InterpPart, Program, Span, Stmt, ThingDecl, UnOp,
};
use and23_types::Resolutions;
use std::collections::HashMap;
use std::collections::HashSet;

/// Nombres conocidos entre compilaciones (el REPL compila entrada tras entrada).
#[derive(Clone, Default)]
struct Tables {
    /// nombre -> (ranura, mutable)
    globals: HashMap<String, (u32, bool)>,
    /// nombre -> (índice de la función, aridad)
    funcs: HashMap<String, (u32, usize)>,
    /// cadena constante -> índice en `Module::strings`
    strings: HashMap<String, u32>,
    /// clase -> sus funciones constructora e inicializadora
    classes: HashMap<String, ClassEntry>,
    /// clase o interfaz -> índice en `Module::classes`
    types: HashMap<String, u32>,
    /// nombre de método -> id (índice en `Module::methods`)
    method_ids: HashMap<String, u32>,
}

#[derive(Clone)]
struct ClassEntry {
    /// `Clase(params)`: crea el objeto y llama a `init`.
    ctor: u32,
    /// `Clase.<init>(this, params)`: llama al `init` del padre y rellena los campos.
    init: u32,
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
    ///
    /// `res` es lo que decidió el verificador de tipos (campos y métodos de cada acceso).
    pub fn compile(
        &mut self,
        program: &Program,
        res: &Resolutions,
        echo: bool,
    ) -> Result<usize, Diagnostic> {
        let saved_tables = self.tables.clone();
        let saved_classes = self.module.classes.len();
        let saved_methods = self.module.methods.len();
        let saved_functions = self.module.functions.len();
        let saved_globals = self.module.globals.len();
        let saved_strings = self.module.strings.len();
        let result = self.compile_program(program, res, echo);
        if result.is_err() {
            self.tables = saved_tables;
            self.module.classes.truncate(saved_classes);
            self.module.methods.truncate(saved_methods);
            self.module.functions.truncate(saved_functions);
            self.module.globals.truncate(saved_globals);
            self.module.strings.truncate(saved_strings);
        }
        result
    }

    /// Reserva el índice de una función (su código se compila después).
    fn declare_function(
        &mut self,
        key: String,
        name_span: Span,
        arity: usize,
    ) -> Result<usize, Diagnostic> {
        if self.tables.funcs.contains_key(&key) {
            return Err(Diagnostic::new(
                name_span,
                format!("la función `{key}` ya está definida"),
            ));
        }
        let index = self.module.functions.len();
        self.module.functions.push(Function {
            name: key.clone(),
            name_span,
            arity,
            code: Vec::new(),
            spans: Vec::new(),
        });
        self.tables.funcs.insert(key, (index as u32, arity));
        Ok(index)
    }

    /// Id del método `name` (se crea la primera vez).
    fn method_id(&mut self, name: &str) -> u32 {
        if let Some(&id) = self.tables.method_ids.get(name) {
            return id;
        }
        let id = self.module.methods.len() as u32;
        self.module.methods.push(name.to_string());
        self.tables.method_ids.insert(name.to_string(), id);
        id
    }

    /// Reserva el índice de una clase o interfaz (los datos se completan en `finalize_class`).
    fn declare_type(&mut self, name: &str, span: Span) -> Result<(), Diagnostic> {
        if self.tables.types.contains_key(name) {
            return Err(Diagnostic::new(
                span,
                format!("el tipo `{name}` ya está definido"),
            ));
        }
        let index = self.module.classes.len() as u32;
        self.module.classes.push(ClassDef {
            name: name.to_string(),
            fields: Vec::new(),
            supers: vec![index],
            vtable: HashMap::new(),
        });
        self.tables.types.insert(name.to_string(), index);
        Ok(())
    }

    fn finalize_class(
        &mut self,
        decl: &ThingDecl,
        all: &HashMap<&str, &ThingDecl>,
        done: &mut HashSet<String>,
    ) -> Result<(), Diagnostic> {
        if !done.insert(decl.name.clone()) {
            return Ok(());
        }
        let unknown = |name: &str, span: Span| {
            Diagnostic::new(
                span,
                format!("el tipo `{name}` no existe (¿faltó verificar tipos?)"),
            )
        };
        let (mut fields, mut supers, mut vtable) =
            match decl.supers.iter().find(|s| s.args.is_some()) {
                Some(parent) => {
                    if let Some(parent_decl) = all.get(parent.name.as_str()) {
                        self.finalize_class(parent_decl, all, done)?;
                    }
                    let index = *self
                        .tables
                        .types
                        .get(&parent.name)
                        .ok_or_else(|| unknown(&parent.name, parent.name_span))?;
                    let class = &self.module.classes[index as usize];
                    (
                        class.fields.clone(),
                        class.supers.clone(),
                        class.vtable.clone(),
                    )
                }
                None => (Vec::new(), Vec::new(), HashMap::new()),
            };
        let index = self.tables.types[&decl.name];
        fields.extend(
            decl.params
                .iter()
                .filter(|p| p.mutable.is_some())
                .map(|p| p.name.clone()),
        );
        fields.extend(decl.fields.iter().map(|f| f.name.clone()));
        supers.push(index);
        for kind in decl.supers.iter().filter(|s| s.args.is_none()) {
            let id = *self
                .tables
                .types
                .get(&kind.name)
                .ok_or_else(|| unknown(&kind.name, kind.name_span))?;
            supers.push(id);
        }
        for method in &decl.methods {
            let id = self.method_id(&method.name);
            let func = self.tables.funcs[&format!("{}.{}", decl.name, method.name)].0;
            vtable.insert(id, func);
        }
        self.module.classes[index as usize] = ClassDef {
            name: decl.name.clone(),
            fields,
            supers,
            vtable,
        };
        Ok(())
    }

    fn compile_program(
        &mut self,
        program: &Program,
        res: &Resolutions,
        echo: bool,
    ) -> Result<usize, Diagnostic> {
        // Las clases y funciones se registran antes de compilar: pueden usarse antes de su definición.
        for name in res.virtuals.values() {
            self.method_id(name);
        }
        for stmt in &program.stmts {
            let Stmt::Kind(decl) = stmt else { continue };
            self.declare_type(&decl.name, decl.name_span)?;
        }
        let mut things: Vec<(usize, usize, &ThingDecl)> = Vec::new();
        let mut methods: Vec<(usize, &ThingDecl, &FnDecl)> = Vec::new();
        for stmt in &program.stmts {
            let Stmt::Thing(decl) = stmt else { continue };
            self.declare_type(&decl.name, decl.name_span)?;
            let ctor = self.module.functions.len();
            self.module.functions.push(Function {
                name: decl.name.clone(),
                name_span: decl.name_span,
                arity: decl.params.len(),
                code: Vec::new(),
                spans: Vec::new(),
            });
            let init = self.declare_function(
                format!("{}.<init>", decl.name),
                decl.name_span,
                decl.params.len() + 1,
            )?;
            self.tables.classes.insert(
                decl.name.clone(),
                ClassEntry {
                    ctor: ctor as u32,
                    init: init as u32,
                },
            );
            things.push((ctor, init, decl));
            for method in &decl.methods {
                let key = format!("{}.{}", decl.name, method.name);
                let index =
                    self.declare_function(key, method.name_span, method.params.len() + 1)?;
                methods.push((index, decl, method));
            }
        }
        // Campos, ancestros y tabla de métodos de cada clase, el padre antes que el hijo.
        let by_name: HashMap<&str, &ThingDecl> = things
            .iter()
            .map(|(_, _, t)| (t.name.as_str(), *t))
            .collect();
        let mut done = HashSet::new();
        for (_, _, thing) in &things {
            self.finalize_class(thing, &by_name, &mut done)?;
        }
        let mut decls: Vec<(usize, &FnDecl)> = Vec::new();
        for stmt in &program.stmts {
            if let Stmt::Fn(decl) = stmt {
                if self.tables.types.contains_key(&decl.name) {
                    return Err(Diagnostic::new(
                        decl.name_span,
                        format!("la función `{}` ya está definida", decl.name),
                    ));
                }
                let index =
                    self.declare_function(decl.name.clone(), decl.name_span, decl.params.len())?;
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
                res,
                decl.name.clone(),
                decl.name_span,
            )
            .function(decl)?;
            self.module.functions[index] = function;
        }
        for (ctor, init, thing) in things {
            let class = self.tables.types[&thing.name];
            let total = self.module.classes[class as usize].fields.len() as u32;
            let parent = thing.supers.iter().find(|s| s.args.is_some());
            let (offset, parent_init) = match parent {
                Some(p) => {
                    let parent_class = self.tables.types[&p.name];
                    (
                        self.module.classes[parent_class as usize].fields.len() as u32,
                        Some(self.tables.classes[&p.name].init),
                    )
                }
                None => (0, None),
            };
            let function = FnCompiler::new(
                &mut self.tables,
                &mut self.module.strings,
                res,
                thing.name.clone(),
                thing.name_span,
            )
            .constructor(thing, class, total, init as u32)?;
            self.module.functions[ctor] = function;
            let function = FnCompiler::new(
                &mut self.tables,
                &mut self.module.strings,
                res,
                format!("{}.<init>", thing.name),
                thing.name_span,
            )
            .init(thing, offset, parent_init)?;
            self.module.functions[init] = function;
        }
        for (index, thing, method) in methods {
            let function = FnCompiler::new(
                &mut self.tables,
                &mut self.module.strings,
                res,
                format!("{}.{}", thing.name, method.name),
                method.name_span,
            )
            .method(method)?;
            self.module.functions[index] = function;
        }

        let mut script = FnCompiler::new(
            &mut self.tables,
            &mut self.module.strings,
            res,
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
    /// `attempt` abiertos al entrar en el bucle: salir de él debe cerrar los que se abrieron dentro.
    tries: usize,
}

struct FnCompiler<'a> {
    tables: &'a mut Tables,
    strings: &'a mut Vec<String>,
    res: &'a Resolutions,
    name: String,
    name_span: Span,
    arity: usize,
    code: Vec<Op>,
    spans: Vec<Span>,
    locals: Vec<Local>,
    depth: u32,
    loops: Vec<LoopCtx>,
    /// `attempt` abiertos en este punto de la función.
    tries: usize,
    /// Es el código del nivel superior: sus `fix`/`mut` de profundidad 0 son globales.
    script: bool,
    echo: bool,
}

impl<'a> FnCompiler<'a> {
    fn new(
        tables: &'a mut Tables,
        strings: &'a mut Vec<String>,
        res: &'a Resolutions,
        name: String,
        name_span: Span,
    ) -> Self {
        FnCompiler {
            tables,
            strings,
            res,
            name,
            name_span,
            arity: 0,
            code: Vec::new(),
            spans: Vec::new(),
            locals: Vec::new(),
            depth: 0,
            loops: Vec::new(),
            tries: 0,
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

    /// Método: `this` es el local 0 y los parámetros vienen después.
    fn method(mut self, decl: &FnDecl) -> Result<Function, Diagnostic> {
        self.arity = decl.params.len() + 1;
        self.locals.push(Local {
            name: "this".to_string(),
            mutable: false,
            depth: 0,
        });
        for param in &decl.params {
            self.locals.push(Local {
                name: param.name.clone(),
                mutable: false,
                depth: 0,
            });
        }
        self.block(&decl.body)?;
        Ok(self.finish(decl.span))
    }

    /// Constructor `Clase(params)`: reserva el objeto (campos en `()`), llama a `Clase.<init>` con
    /// él y los parámetros, y lo devuelve.
    fn constructor(
        mut self,
        decl: &ThingDecl,
        class: u32,
        total: u32,
        init: u32,
    ) -> Result<Function, Diagnostic> {
        self.arity = decl.params.len();
        let span = decl.span;
        self.emit(Op::Alloc { class, n: total }, span);
        self.emit(Op::Dup, span);
        for i in 0..decl.params.len() {
            self.emit(Op::GetLocal(i as u32), span);
        }
        self.emit(Op::Call(init), span);
        self.emit(Op::Pop, span);
        self.emit(Op::Return, span);
        Ok(Function {
            name: self.name,
            name_span: self.name_span,
            arity: self.arity,
            code: self.code,
            spans: self.spans,
        })
    }

    /// `Clase.<init>(this, params)`: llama al `init` del padre con los argumentos de `: Padre(...)`,
    /// guarda los parámetros que son campos y evalúa los campos del cuerpo (que ven los parámetros
    /// y los campos anteriores como locales). `offset` es la cantidad de campos heredados.
    fn init(
        mut self,
        decl: &ThingDecl,
        offset: u32,
        parent_init: Option<u32>,
    ) -> Result<Function, Diagnostic> {
        self.arity = decl.params.len() + 1;
        self.locals.push(Local {
            name: "this".to_string(),
            mutable: false,
            depth: 0,
        });
        for param in &decl.params {
            self.locals.push(Local {
                name: param.name.clone(),
                mutable: false,
                depth: 0,
            });
        }
        let span = decl.span;
        if let (Some(parent_init), Some(sup)) =
            (parent_init, decl.supers.iter().find(|s| s.args.is_some()))
        {
            self.emit(Op::GetLocal(0), sup.span);
            for arg in sup.args.as_deref().unwrap_or(&[]) {
                self.expr(arg)?;
            }
            self.emit(Op::Call(parent_init), sup.span);
            self.emit(Op::Pop, sup.span);
        }
        let mut next = offset;
        for (i, param) in decl.params.iter().enumerate() {
            if param.mutable.is_some() {
                self.emit(Op::GetLocal(0), param.span);
                self.emit(Op::GetLocal(i as u32 + 1), param.span);
                self.emit(Op::SetField(next), param.span);
                next += 1;
            }
        }
        for field in &decl.fields {
            self.expr(&field.init)?;
            self.locals.push(Local {
                name: field.name.clone(),
                mutable: false,
                depth: 0,
            });
            let slot = (self.locals.len() - 1) as u32;
            self.emit(Op::GetLocal(0), field.span);
            self.emit(Op::GetLocal(slot), field.span);
            self.emit(Op::SetField(next), field.span);
            next += 1;
        }
        Ok(self.finish(span))
    }

    fn top_level(&mut self, program: &Program) -> Result<(), Diagnostic> {
        for stmt in &program.stmts {
            match stmt {
                Stmt::Fn(_) | Stmt::Thing(_) | Stmt::Kind(_) => {}
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
            Op::JumpIfNil(_) => Op::JumpIfNil(target),
            Op::JumpIfNotNil(_) => Op::JumpIfNotNil(target),
            Op::Try(_) => Op::Try(target),
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
            } if self.res.fields.contains_key(name_span) => {
                // Campo de `this` por su nombre: `edad += 1`.
                let field = self.res.fields[name_span];
                self.emit(Op::GetLocal(0), *name_span);
                if let Some(op) = op {
                    self.emit(Op::Dup, *name_span);
                    self.emit(Op::GetField(field), *name_span);
                    self.expr(value)?;
                    self.emit(binary_op(*op), *span);
                } else {
                    self.expr(value)?;
                }
                self.emit(Op::SetField(field), *span);
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
            Stmt::FieldAssign {
                target,
                name_span,
                op,
                value,
                span,
                ..
            } => {
                let field = self.field_index(*name_span)?;
                self.expr(target)?;
                if let Some(op) = op {
                    self.emit(Op::Dup, *span);
                    self.emit(Op::GetField(field), *span);
                    self.expr(value)?;
                    self.emit(binary_op(*op), *span);
                } else {
                    self.expr(value)?;
                }
                self.emit(Op::SetField(field), *span);
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
                    tries: self.tries,
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
                    let open = self.tries - ctx.tries;
                    self.pop_locals(extra, *span);
                    for _ in 0..open {
                        self.emit(Op::EndTry, *span);
                    }
                    let at = self.emit(Op::Jump(0), *span);
                    if let Some(ctx) = self.loops.last_mut() {
                        ctx.breaks.push(at);
                    }
                }
            }
            Stmt::Continue(span) => {
                if let Some(ctx) = self.loops.last() {
                    let (head, extra) = (ctx.head, self.locals.len() - ctx.locals);
                    let open = self.tries - ctx.tries;
                    self.pop_locals(extra, *span);
                    for _ in 0..open {
                        self.emit(Op::EndTry, *span);
                    }
                    self.emit(Op::Jump(head), *span);
                }
            }
            Stmt::Fail(value, span) => {
                self.expr(value)?;
                self.emit(Op::Fail, *span);
            }
            Stmt::Attempt {
                body,
                rescues,
                span,
            } => {
                let try_at = self.emit(Op::Try(0), *span);
                self.tries += 1;
                self.block(body)?;
                self.tries -= 1;
                self.emit(Op::EndTry, *span);
                let mut ends = vec![self.emit(Op::Jump(0), *span)];
                // Aquí cae una excepción: el objeto lanzado está en la cima de la pila.
                self.patch(try_at);
                for rescue in rescues {
                    let ty = self.type_index(&rescue.ty.name, rescue.ty.span)?;
                    self.emit(Op::Dup, rescue.span);
                    self.emit(Op::IsInstance(ty), rescue.span);
                    let next = self.emit(Op::JumpIfFalse(0), rescue.span);
                    // El objeto de la cima pasa a ser el local de la cláusula.
                    self.locals.push(Local {
                        name: rescue.name.clone(),
                        mutable: false,
                        depth: self.depth,
                    });
                    self.block(&rescue.body)?;
                    self.locals.pop();
                    self.emit(Op::Pop, rescue.span);
                    ends.push(self.emit(Op::Jump(0), rescue.span));
                    self.patch(next);
                }
                // Ninguna cláusula la quiso: sigue subiendo.
                self.emit(Op::Fail, *span);
                for at in ends {
                    self.patch(at);
                }
            }
            // Las registró `compile_program`.
            Stmt::Fn(_) | Stmt::Thing(_) | Stmt::Kind(_) => {}
        }
        Ok(())
    }

    /// Índice de campo que el verificador resolvió para esta posición.
    fn field_index(&self, span: Span) -> Result<u32, Diagnostic> {
        self.res.fields.get(&span).copied().ok_or_else(|| {
            Diagnostic::new(
                span,
                "error interno: campo sin resolver (¿faltó verificar tipos?)",
            )
        })
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
            Expr::Var(_, span) if self.res.fields.contains_key(span) => {
                let field = self.res.fields[span];
                self.emit(Op::GetLocal(0), *span);
                self.emit(Op::GetField(field), *span);
            }
            Expr::Var(name, span) => {
                let (get, _, _) = self.resolve(name, *span)?;
                self.emit(get, *span);
            }
            Expr::This(span) => {
                self.emit(Op::GetLocal(0), *span);
            }
            Expr::Nil(span) => {
                self.emit(Op::Nil, *span);
            }
            Expr::Elvis { lhs, rhs, span } => {
                self.expr(lhs)?;
                let done = self.emit(Op::JumpIfNotNil(0), *span);
                self.emit(Op::Pop, *span);
                self.expr(rhs)?;
                self.patch(done);
            }
            Expr::SafeField { target, span, .. } => {
                let field = self.field_index(*span)?;
                self.expr(target)?;
                let skip = self.emit(Op::JumpIfNil(0), *span);
                self.emit(Op::GetField(field), *span);
                self.patch(skip);
            }
            Expr::SafeCall {
                target, args, span, ..
            } => {
                let method = self.virtual_id(*span)?;
                self.expr(target)?;
                let skip = self.emit(Op::JumpIfNil(0), *span);
                for arg in args {
                    self.expr(arg)?;
                }
                self.emit(
                    Op::CallMethod {
                        method,
                        argc: args.len() as u32,
                    },
                    *span,
                );
                self.patch(skip);
            }
            Expr::Field { target, span, .. } => {
                let field = self.field_index(*span)?;
                self.expr(target)?;
                self.emit(Op::GetField(field), *span);
            }
            Expr::MethodCall {
                target, args, span, ..
            } => {
                let method = self.virtual_id(*span)?;
                self.expr(target)?;
                for arg in args {
                    self.expr(arg)?;
                }
                self.emit(
                    Op::CallMethod {
                        method,
                        argc: args.len() as u32,
                    },
                    *span,
                );
            }
            Expr::SuperCall { args, span, .. } => {
                let index = self.method_index(*span)?;
                self.emit(Op::GetLocal(0), *span);
                for arg in args {
                    self.expr(arg)?;
                }
                self.emit(Op::Call(index), *span);
            }
            Expr::Is { expr, ty, span } => {
                let id = self.type_index(&ty.name, ty.span)?;
                self.expr(expr)?;
                self.emit(Op::IsInstance(id), *span);
            }
            Expr::Cast { expr, ty, span } => {
                let id = self.type_index(&ty.name, ty.span)?;
                self.expr(expr)?;
                self.emit(Op::Cast(id), *span);
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
            Expr::Call { args, span, .. } if self.res.virtuals.contains_key(span) => {
                // Método de la propia clase llamado por su nombre: `this` va primero.
                let method = self.virtual_id(*span)?;
                self.emit(Op::GetLocal(0), *span);
                for arg in args {
                    self.expr(arg)?;
                }
                self.emit(
                    Op::CallMethod {
                        method,
                        argc: args.len() as u32,
                    },
                    *span,
                );
            }
            Expr::Call { name, args, span } if self.tables.classes.contains_key(name) => {
                let ctor = self.tables.classes[name].ctor;
                for arg in args {
                    self.expr(arg)?;
                }
                self.emit(Op::Call(ctor), *span);
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

impl FnCompiler<'_> {
    /// Id del método virtual que el verificador resolvió para esta llamada.
    fn virtual_id(&self, span: Span) -> Result<u32, Diagnostic> {
        self.res
            .virtuals
            .get(&span)
            .and_then(|name| self.tables.method_ids.get(name))
            .copied()
            .ok_or_else(|| {
                Diagnostic::new(
                    span,
                    "error interno: método sin resolver (¿faltó verificar tipos?)",
                )
            })
    }

    /// Índice de clase o interfaz por nombre.
    fn type_index(&self, name: &str, span: Span) -> Result<u32, Diagnostic> {
        self.tables
            .types
            .get(name)
            .copied()
            .ok_or_else(|| Diagnostic::new(span, format!("el tipo `{name}` no existe")))
    }

    /// Índice de función del método que el verificador resolvió para esta llamada.
    fn method_index(&self, span: Span) -> Result<u32, Diagnostic> {
        self.res
            .methods
            .get(&span)
            .and_then(|key| self.tables.funcs.get(key))
            .map(|(index, _)| *index)
            .ok_or_else(|| {
                Diagnostic::new(
                    span,
                    "error interno: método sin resolver (¿faltó verificar tipos?)",
                )
            })
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
        let script = c
            .compile(&program, &Resolutions::default(), false)
            .map_err(|d| d.message)?;
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
        assert!(c.compile(&bad, &Resolutions::default(), false).is_err());
        assert!(c.module().functions.is_empty());
        assert!(c.module().globals.is_empty());
        let ok = parse("fn f() = 2\nmut a = 1").unwrap();
        assert!(c.compile(&ok, &Resolutions::default(), false).is_ok());
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
