use crate::op::{ClassDef, Function, Module, Op};
use and23_syntax::{
    BinOp, Block, Diagnostic, Expr, FnDecl, InterpPart, LambdaParam, Program, Span, Stmt,
    ThingDecl, UnOp,
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
    /// Funciones del preludio: un programa puede definir otra con el mismo nombre.
    overridable: HashSet<String>,
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

    /// Marca las funciones definidas hasta ahora (el preludio) como redefinibles por el programa.
    pub fn seal_prelude(&mut self) {
        self.tables.overridable = self.tables.funcs.keys().cloned().collect();
        self.module.prelude_functions = self.module.functions.len();
    }

    pub fn module(&self) -> &Module {
        &self.module
    }

    /// Índice de la función de nivel superior `name`, si existe.
    pub fn function_index(&self, name: &str) -> Option<usize> {
        self.tables.funcs.get(name).map(|(i, _)| *i as usize)
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
        if self.tables.funcs.contains_key(&key) && !self.tables.overridable.remove(&key) {
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
        // En orden de posición, para que los ids de método no dependan del orden del `HashMap`.
        let mut virtuals: Vec<(&Span, &String)> = res.virtuals.iter().collect();
        virtuals.sort_by_key(|(span, _)| (span.start, span.end));
        for (_, name) in virtuals {
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
            let function = build(
                &mut self.tables,
                &mut self.module.strings,
                &mut self.module.functions,
                res,
                decl.name.clone(),
                decl.name_span,
                &|fc| fc.function(decl),
            )?;
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
            let function = build(
                &mut self.tables,
                &mut self.module.strings,
                &mut self.module.functions,
                res,
                thing.name.clone(),
                thing.name_span,
                &|fc| fc.constructor(thing, class, total, init as u32),
            )?;
            self.module.functions[ctor] = function;
            let function = build(
                &mut self.tables,
                &mut self.module.strings,
                &mut self.module.functions,
                res,
                format!("{}.<init>", thing.name),
                thing.name_span,
                &|fc| fc.init(thing, offset, parent_init),
            )?;
            self.module.functions[init] = function;
        }
        for (index, thing, method) in methods {
            let function = build(
                &mut self.tables,
                &mut self.module.strings,
                &mut self.module.functions,
                res,
                format!("{}.{}", thing.name, method.name),
                method.name_span,
                &|fc| fc.method(method),
            )?;
            self.module.functions[index] = function;
        }

        let function = build(
            &mut self.tables,
            &mut self.module.strings,
            &mut self.module.functions,
            res,
            "<script>".to_string(),
            Span::new(0, 0),
            &|mut fc| {
                fc.script = true;
                fc.echo = echo;
                fc.top_level(program)?;
                Ok(fc.finish(Span::new(0, 0)))
            },
        )?;
        self.module.functions.push(function);
        Ok(self.module.functions.len() - 1)
    }
}

/// Función compilada y las variables `mut` que una lambda captura.
type Built = Result<(Function, HashSet<usize>), Diagnostic>;

/// Compila una función. Una variable `mut` que una lambda captura vive en una celda, y eso hay que
/// saberlo al declararla: si la primera pasada descubre capturas nuevas, se repite con ellas.
#[allow(clippy::too_many_arguments)]
fn build(
    tables: &mut Tables,
    strings: &mut Vec<String>,
    functions: &mut Vec<Function>,
    res: &Resolutions,
    name: String,
    span: Span,
    run: &dyn Fn(FnCompiler<'_>) -> Built,
) -> Result<Function, Diagnostic> {
    let mark = functions.len();
    let mut boxed = HashSet::new();
    loop {
        let mut fc = FnCompiler::new(
            &mut *tables,
            &mut *strings,
            &mut *functions,
            res,
            name.clone(),
            span,
        );
        fc.box_ids = boxed.clone();
        let (function, need) = run(fc)?;
        if need.is_subset(&boxed) {
            return Ok(function);
        }
        boxed.extend(need);
        // Las lambdas de la pasada descartada ya no sirven.
        functions.truncate(mark);
    }
}

struct Local {
    name: String,
    mutable: bool,
    depth: u32,
    /// Número de orden entre las variables `mut` locales de la función (identifica la declaración
    /// entre una compilación y la siguiente).
    ord: Option<usize>,
    /// Vive en una celda porque una lambda la captura.
    boxed: bool,
}

/// Dónde está una variable desde la función que se compila.
#[derive(Clone, Copy)]
enum Access {
    Local(u32),
    Capture(u32),
}

/// Una variable de una función de fuera que la lambda en curso usa.
struct Capture {
    name: String,
    mutable: bool,
    boxed: bool,
    /// Cómo se lee en la función que crea la lambda.
    from: Access,
}

/// Lo que una función envolvente guarda mientras se compila una lambda suya.
struct Outer {
    locals: Vec<Local>,
    captures: Vec<Capture>,
}

/// Dónde se lee o escribe una variable.
#[derive(Clone, Copy)]
enum Place {
    Local { slot: u32, boxed: bool },
    Capture { idx: u32, boxed: bool },
    Global(u32),
}

/// Resultado de buscar una variable por nombre.
struct Found {
    access: Access,
    mutable: bool,
    boxed: bool,
    /// Número de orden de la declaración, si es un local `mut` sin caja todavía.
    ord: Option<usize>,
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
    /// Las funciones del módulo: aquí se añaden las lambdas (y los envoltorios de funciones).
    functions: &'a mut Vec<Function>,
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
    /// Variables capturadas por la lambda que se compila (vacío fuera de una lambda).
    captures: Vec<Capture>,
    /// Estado de las funciones que envuelven a la lambda en curso, de fuera hacia dentro.
    outer: Vec<Outer>,
    /// Variables `mut` (por número de orden) que van en una celda.
    box_ids: HashSet<usize>,
    /// Variables `mut` que una lambda captura: si faltan en `box_ids`, hay que recompilar.
    need_box: HashSet<usize>,
    mut_count: usize,
}

impl<'a> FnCompiler<'a> {
    fn new(
        tables: &'a mut Tables,
        strings: &'a mut Vec<String>,
        functions: &'a mut Vec<Function>,
        res: &'a Resolutions,
        name: String,
        name_span: Span,
    ) -> Self {
        FnCompiler {
            tables,
            strings,
            functions,
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
            captures: Vec::new(),
            outer: Vec::new(),
            box_ids: HashSet::new(),
            need_box: HashSet::new(),
            mut_count: 0,
        }
    }

    fn finish(mut self, span: Span) -> (Function, HashSet<usize>) {
        self.emit(Op::Unit, span);
        self.emit(Op::Return, span);
        (
            Function {
                name: self.name,
                name_span: self.name_span,
                arity: self.arity,
                code: self.code,
                spans: self.spans,
            },
            self.need_box,
        )
    }

    fn function(mut self, decl: &FnDecl) -> Result<(Function, HashSet<usize>), Diagnostic> {
        self.arity = decl.params.len();
        for param in &decl.params {
            // Los parámetros son inmutables; el cuerpo es un ámbito aparte (puede sombrearlos).
            self.locals.push(Local {
                name: param.name.clone(),
                mutable: false,
                depth: 0,
                ord: None,
                boxed: false,
            });
        }
        self.block(&decl.body)?;
        Ok(self.finish(decl.span))
    }

    /// Método: `this` es el local 0 y los parámetros vienen después.
    fn method(mut self, decl: &FnDecl) -> Result<(Function, HashSet<usize>), Diagnostic> {
        self.arity = decl.params.len() + 1;
        self.locals.push(Local {
            name: "this".to_string(),
            mutable: false,
            depth: 0,
            ord: None,
            boxed: false,
        });
        for param in &decl.params {
            self.locals.push(Local {
                name: param.name.clone(),
                mutable: false,
                depth: 0,
                ord: None,
                boxed: false,
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
    ) -> Result<(Function, HashSet<usize>), Diagnostic> {
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
        Ok((
            Function {
                name: self.name,
                name_span: self.name_span,
                arity: self.arity,
                code: self.code,
                spans: self.spans,
            },
            HashSet::new(),
        ))
    }

    /// `Clase.<init>(this, params)`: llama al `init` del padre con los argumentos de `: Padre(...)`,
    /// guarda los parámetros que son campos y evalúa los campos del cuerpo (que ven los parámetros
    /// y los campos anteriores como locales). `offset` es la cantidad de campos heredados.
    fn init(
        mut self,
        decl: &ThingDecl,
        offset: u32,
        parent_init: Option<u32>,
    ) -> Result<(Function, HashSet<usize>), Diagnostic> {
        self.arity = decl.params.len() + 1;
        self.locals.push(Local {
            name: "this".to_string(),
            mutable: false,
            depth: 0,
            ord: None,
            boxed: false,
        });
        for param in &decl.params {
            self.locals.push(Local {
                name: param.name.clone(),
                mutable: false,
                depth: 0,
                ord: None,
                boxed: false,
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
                ord: None,
                boxed: false,
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
                ty,
                init,
                span,
            } => {
                // `fix f: (Int) -> Int = { ... f(x) ... }` dentro de una función o un bloque: la lambda se captura a sí
                // misma. El verificador ya declaró `f` antes de la lambda; aquí `f` se registra antes de compilarla y, si la
                // lambda la captura, vive en una celda que se rellena con la lambda ya creada.
                let recursive = !(self.script && self.depth == 0)
                    && matches!(init, Expr::Lambda { .. })
                    && ty.as_ref().is_some_and(|t| t.name == "Fn");
                if recursive {
                    return self.recursive_let(name, *name_span, *mutable, init, *span);
                }
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
                    // El valor ya está en la cima de la pila: esa posición es el local. Una variable
                    // `mut` que una lambda captura vive en una celda para que todos vean los cambios.
                    let (ord, boxed) = if *mutable {
                        let ord = self.mut_count;
                        self.mut_count += 1;
                        (Some(ord), self.box_ids.contains(&ord))
                    } else {
                        (None, false)
                    };
                    if boxed {
                        self.emit(Op::NewCell, *span);
                    }
                    self.locals.push(Local {
                        name: name.clone(),
                        mutable: *mutable,
                        depth: self.depth,
                        ord,
                        boxed,
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
                self.load_this(*name_span)?;
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
                let (place, mutable) = self.place(name, *name_span)?;
                if !mutable {
                    return Err(Diagnostic::new(
                        *name_span,
                        format!("no se puede asignar a `{name}` porque es inmutable; decláralo con `mut`"),
                    ));
                }
                if let Some(op) = op {
                    self.emit_load(place, *name_span);
                    self.expr(value)?;
                    self.emit(binary_op(*op), *span);
                } else {
                    self.expr(value)?;
                }
                self.emit_store(place, *span);
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
                        ord: None,
                        boxed: false,
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

    /// Un `let` local cuya lambda puede llamarse a sí misma. La variable se registra antes de compilar la lambda, en la posición
    /// de pila donde quedará su valor. Si la lambda no la captura, el resultado es el de un `let` normal. Si la captura, la
    /// variable es una celda (como una `mut` capturada, con la repetición de `build`): `Nil`, `NewCell`, la lambda (que captura
    /// la celda) y `CellSet`.
    fn recursive_let(
        &mut self,
        name: &str,
        name_span: Span,
        mutable: bool,
        init: &Expr,
        span: Span,
    ) -> Result<(), Diagnostic> {
        if self
            .locals
            .iter()
            .any(|local| local.depth == self.depth && local.name == name)
        {
            return Err(already_declared(name, name_span));
        }
        let ord = self.mut_count;
        self.mut_count += 1;
        let boxed = self.box_ids.contains(&ord);
        let slot = self.locals.len() as u32;
        if boxed {
            self.emit(Op::Nil, span);
            self.emit(Op::NewCell, span);
        }
        // `mutable: true` hace que `find` pida la celda si una lambda captura la variable (la asignación a una `fix` ya la
        // rechazó el verificador).
        self.locals.push(Local {
            name: name.to_string(),
            mutable: true,
            depth: self.depth,
            ord: Some(ord),
            boxed,
        });
        self.expr(init)?;
        if boxed {
            self.emit(Op::GetLocal(slot), span);
            self.emit(Op::CellSet, span);
        } else {
            // El valor de la lambda queda en la posición de la variable; la que se registró era la de un `let` normal.
            self.locals[slot as usize].mutable = mutable;
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

    /// Busca `name` en el nivel de función `level` (0 es la más externa; `outer.len()` es la que se
    /// compila). Si está en una función de fuera, cada lambda del camino lo captura.
    fn find(&mut self, level: usize, name: &str) -> Option<Found> {
        let top = self.outer.len();
        {
            let locals = if level == top {
                &self.locals
            } else {
                &self.outer[level].locals
            };
            if let Some(slot) = locals.iter().rposition(|local| local.name == name) {
                let local = &locals[slot];
                return Some(Found {
                    access: Access::Local(slot as u32),
                    mutable: local.mutable,
                    boxed: local.boxed,
                    ord: local.ord,
                });
            }
            let captures = if level == top {
                &self.captures
            } else {
                &self.outer[level].captures
            };
            if let Some(i) = captures.iter().position(|c| c.name == name) {
                let c = &captures[i];
                return Some(Found {
                    access: Access::Capture(i as u32),
                    mutable: c.mutable,
                    boxed: c.boxed,
                    ord: None,
                });
            }
        }
        if level == 0 {
            return None;
        }
        let up = self.find(level - 1, name)?;
        if up.mutable && !up.boxed {
            if let Some(ord) = up.ord {
                self.need_box.insert(ord);
            }
        }
        let captures = if level == top {
            &mut self.captures
        } else {
            &mut self.outer[level].captures
        };
        captures.push(Capture {
            name: name.to_string(),
            mutable: up.mutable,
            boxed: up.boxed,
            from: up.access,
        });
        Some(Found {
            access: Access::Capture((captures.len() - 1) as u32),
            mutable: up.mutable,
            boxed: up.boxed,
            ord: None,
        })
    }

    /// Dónde está una variable y si es mutable: local, capturada por la lambda en curso o global.
    fn place(&mut self, name: &str, span: Span) -> Result<(Place, bool), Diagnostic> {
        let top = self.outer.len();
        if let Some(found) = self.find(top, name) {
            let place = match found.access {
                Access::Local(slot) => Place::Local {
                    slot,
                    boxed: found.boxed,
                },
                Access::Capture(idx) => Place::Capture {
                    idx,
                    boxed: found.boxed,
                },
            };
            return Ok((place, found.mutable));
        }
        if let Some((slot, mutable)) = self.tables.globals.get(name) {
            return Ok((Place::Global(*slot), *mutable));
        }
        Err(Diagnostic::new(
            span,
            format!("la variable `{name}` no está definida"),
        ))
    }

    fn emit_load(&mut self, place: Place, span: Span) {
        match place {
            Place::Local { slot, boxed } => {
                self.emit(Op::GetLocal(slot), span);
                if boxed {
                    self.emit(Op::CellGet, span);
                }
            }
            Place::Capture { idx, boxed } => {
                self.emit(Op::GetCapture(idx), span);
                if boxed {
                    self.emit(Op::CellGet, span);
                }
            }
            Place::Global(slot) => {
                self.emit(Op::GetGlobal(slot), span);
            }
        }
    }

    /// Guarda la cima de la pila en la variable.
    fn emit_store(&mut self, place: Place, span: Span) {
        match place {
            Place::Local { slot, boxed: false } => {
                self.emit(Op::SetLocal(slot), span);
            }
            Place::Local { slot, boxed: true } => {
                self.emit(Op::GetLocal(slot), span);
                self.emit(Op::CellSet, span);
            }
            Place::Capture { idx, .. } => {
                self.emit(Op::GetCapture(idx), span);
                self.emit(Op::CellSet, span);
            }
            Place::Global(slot) => {
                self.emit(Op::SetGlobal(slot), span);
            }
        }
    }

    /// Carga `this` (en una lambda de un método, capturado).
    fn load_this(&mut self, span: Span) -> Result<(), Diagnostic> {
        let (place, _) = self.place("this", span)?;
        self.emit_load(place, span);
        Ok(())
    }

    /// Una lambda: se compila como una función aparte cuyo local 0 es la propia lambda y cuyos
    /// valores capturados se leen con `GetCapture`. Aquí se deja la lambda creada en la pila.
    fn lambda(
        &mut self,
        params: &[LambdaParam],
        body: &Block,
        span: Span,
    ) -> Result<(), Diagnostic> {
        let saved_code = std::mem::take(&mut self.code);
        let saved_spans = std::mem::take(&mut self.spans);
        let saved_loops = std::mem::take(&mut self.loops);
        let saved_depth = std::mem::replace(&mut self.depth, 1);
        let saved_tries = std::mem::replace(&mut self.tries, 0);
        let saved_arity = std::mem::replace(&mut self.arity, params.len() + 1);
        self.outer.push(Outer {
            locals: std::mem::take(&mut self.locals),
            captures: std::mem::take(&mut self.captures),
        });
        self.locals.push(Local {
            name: "<lambda>".to_string(),
            mutable: false,
            depth: 0,
            ord: None,
            boxed: false,
        });
        for param in params {
            self.locals.push(Local {
                name: param.name.clone(),
                mutable: false,
                depth: 0,
                ord: None,
                boxed: false,
            });
        }
        let result = self.lambda_body(body, span);
        let captures = std::mem::take(&mut self.captures);
        let outer = self.outer.pop().expect("la lambda abrió un nivel");
        self.locals = outer.locals;
        self.captures = outer.captures;
        let code = std::mem::replace(&mut self.code, saved_code);
        let spans = std::mem::replace(&mut self.spans, saved_spans);
        self.loops = saved_loops;
        self.depth = saved_depth;
        self.tries = saved_tries;
        let arity = std::mem::replace(&mut self.arity, saved_arity);
        result?;
        self.functions.push(Function {
            name: format!("{}.<lambda>", self.name),
            name_span: span,
            arity,
            code,
            spans,
        });
        let func = (self.functions.len() - 1) as u32;
        // Los valores capturados se leen tal cual (la celda, no su contenido) en la función de fuera.
        for capture in &captures {
            match capture.from {
                Access::Local(slot) => self.emit(Op::GetLocal(slot), span),
                Access::Capture(idx) => self.emit(Op::GetCapture(idx), span),
            };
        }
        self.emit(
            Op::Closure {
                func,
                n: captures.len() as u32,
            },
            span,
        );
        Ok(())
    }

    /// Las sentencias de la lambda; si la última es una expresión, es lo que devuelve.
    fn lambda_body(&mut self, body: &Block, span: Span) -> Result<(), Diagnostic> {
        let (init, tail) = match body.stmts.split_last() {
            Some((Stmt::Expr(tail), rest)) => (rest, Some(tail)),
            _ => (&body.stmts[..], None),
        };
        for stmt in init {
            self.stmt(stmt)?;
        }
        match tail {
            Some(tail) => {
                self.expr(tail)?;
                if self.res.discards.contains(&tail.span()) {
                    self.emit(Op::Pop, tail.span());
                    self.emit(Op::Unit, tail.span());
                }
                self.emit(Op::Return, span);
            }
            None => {
                self.emit(Op::Unit, span);
                self.emit(Op::Return, span);
            }
        }
        Ok(())
    }

    /// Una función con nombre usada como valor: una lambda sin capturas que la llama.
    fn function_ref(&mut self, name: &str, span: Span) -> Result<(), Diagnostic> {
        let Some(&(index, arity)) = self.tables.funcs.get(name) else {
            return Err(Diagnostic::new(
                span,
                format!("la variable `{name}` no está definida"),
            ));
        };
        let mut code: Vec<Op> = (0..arity).map(|i| Op::GetLocal(i as u32 + 1)).collect();
        code.push(Op::Call(index));
        code.push(Op::Return);
        let spans = vec![span; code.len()];
        self.functions.push(Function {
            name: format!("{name}.<ref>"),
            name_span: span,
            arity: arity + 1,
            code,
            spans,
        });
        let func = (self.functions.len() - 1) as u32;
        self.emit(Op::Closure { func, n: 0 }, span);
        Ok(())
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
                self.load_this(*span)?;
                self.emit(Op::GetField(field), *span);
            }
            Expr::Var(name, span) => match self.place(name, *span) {
                Ok((place, _)) => self.emit_load(place, *span),
                Err(_) if self.tables.funcs.contains_key(name) => {
                    self.function_ref(name, *span)?;
                }
                Err(err) => return Err(err),
            },
            Expr::Lambda { params, body, span } => self.lambda(params, body, *span)?,
            Expr::CallValue { callee, args, span } => {
                self.expr(callee)?;
                for arg in args {
                    self.expr(arg)?;
                }
                self.emit(Op::CallValue(args.len() as u32), *span);
            }
            Expr::This(span) => self.load_this(*span)?,
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
            } if self.res.redirects.contains_key(span) => {
                // `s?.metodo(..)` sobre un `Str?` o un `Float?`: la función del preludio, si `s` no es nil.
                let (key, _) = self.res.redirects[span].clone();
                let &(index, _) = self.tables.funcs.get(&key).ok_or_else(|| {
                    Diagnostic::new(*span, format!("la función `{key}` no está definida"))
                })?;
                self.expr(target)?;
                let skip = self.emit(Op::JumpIfNil(0), *span);
                for arg in args {
                    self.expr(arg)?;
                }
                self.emit(Op::Call(index), *span);
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
            } if self.res.redirects.contains_key(span) => {
                // `s.metodo(..)` sobre una cadena o `Int.parse(..)`: función del preludio.
                let (key, has_receiver) = self.res.redirects[span].clone();
                let &(index, _) = self.tables.funcs.get(&key).ok_or_else(|| {
                    Diagnostic::new(*span, format!("la función `{key}` no está definida"))
                })?;
                if has_receiver {
                    self.expr(target)?;
                }
                for arg in args {
                    self.expr(arg)?;
                }
                self.emit(Op::Call(index), *span);
            }
            Expr::MethodCall {
                target, args, span, ..
            } if self.res.valuecalls.contains(span) => {
                // `obj.alClic(x)`: el campo guarda una función.
                let field = self.field_index(*span)?;
                self.expr(target)?;
                self.emit(Op::GetField(field), *span);
                for arg in args {
                    self.expr(arg)?;
                }
                self.emit(Op::CallValue(args.len() as u32), *span);
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
                self.load_this(*span)?;
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
            Expr::Call {
                name, args, span, ..
            } if and23_types::native(name).is_some() => {
                let (id, _) = and23_types::native(name).expect("comprobado arriba");
                for arg in args {
                    self.expr(arg)?;
                }
                self.emit(Op::Native(id), *span);
            }
            Expr::Call {
                name, args, span, ..
            } if matches!(name.as_str(), "len" | "push" | "pop" | "hash") => {
                for arg in args {
                    self.expr(arg)?;
                }
                let op = match name.as_str() {
                    "len" => Op::Len,
                    "push" => Op::Push,
                    "pop" => Op::ArrayPop,
                    _ => Op::Hash,
                };
                self.emit(op, *span);
            }
            Expr::Call {
                name, args, span, ..
            } if self.res.valuecalls.contains(span) => {
                // `f(x)` con `f` una variable, parámetro o campo que guarda una función.
                if let Some(&field) = self.res.fields.get(span) {
                    self.load_this(*span)?;
                    self.emit(Op::GetField(field), *span);
                } else {
                    let (place, _) = self.place(name, *span)?;
                    self.emit_load(place, *span);
                }
                for arg in args {
                    self.expr(arg)?;
                }
                self.emit(Op::CallValue(args.len() as u32), *span);
            }
            Expr::Call { args, span, .. } if self.res.virtuals.contains_key(span) => {
                // Método de la propia clase llamado por su nombre: `this` va primero.
                let method = self.virtual_id(*span)?;
                self.load_this(*span)?;
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
            Expr::Call {
                name, args, span, ..
            } if self.tables.classes.contains_key(name) => {
                let ctor = self.tables.classes[name].ctor;
                for arg in args {
                    self.expr(arg)?;
                }
                self.emit(Op::Call(ctor), *span);
            }
            Expr::Call {
                name, args, span, ..
            } => {
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
