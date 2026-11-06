//! Verificador de tipos estáticos de And23. Se ejecuta entre el parser y el compilador:
//! un programa mal tipado se rechaza con su posición antes de generar bytecode.

use and23_syntax::{
    BinOp, Block, Diagnostic, Expr, FnDecl, InterpPart, Program, Span, Stmt, ThingDecl, TypeRef,
    UnOp,
};
use std::collections::HashMap;
use std::fmt;

/// Funciones integradas; no se pueden redefinir.
pub const BUILTINS: [&str; 2] = ["len", "push"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Type {
    Int,
    Float,
    Bool,
    Str,
    Unit,
    Array(Box<Type>),
    /// Instancia de una clase (`thing`).
    Obj(String),
}

impl Type {
    fn is_numeric(&self) -> bool {
        matches!(self, Type::Int | Type::Float)
    }
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Type::Int => f.write_str("Int"),
            Type::Float => f.write_str("Float"),
            Type::Bool => f.write_str("Bool"),
            Type::Str => f.write_str("Str"),
            Type::Unit => f.write_str("Unit"),
            Type::Array(elem) => write!(f, "[{elem}]"),
            Type::Obj(name) => f.write_str(name),
        }
    }
}

#[derive(Clone)]
struct Sig {
    params: Vec<Type>,
    /// `None` mientras no se infirió (función con cuerpo `= expr` sin `->`).
    ret: Option<Type>,
}

type Vars = HashMap<String, (Type, bool)>;

#[derive(Clone)]
struct FieldInfo {
    name: String,
    ty: Type,
    mutable: bool,
}

/// Una clase: sus campos (los de la cabecera primero, luego los del cuerpo) y los tipos de su
/// constructor. Las firmas de los métodos viven en `funcs` bajo `"Clase.metodo"`.
#[derive(Clone, Default)]
struct ClassInfo {
    fields: Vec<FieldInfo>,
    ctor: Vec<Type>,
}

/// Lo que el verificador decidió y el compilador necesita (no tiene tipos propios):
/// a qué campo apunta cada acceso y a qué método cada llamada, indexados por su posición.
#[derive(Debug, Default, Clone)]
pub struct Resolutions {
    /// Posición del acceso (`obj.campo`, nombre suelto dentro de un método o nombre del campo
    /// en una asignación) -> índice del campo.
    pub fields: HashMap<Span, u32>,
    /// Posición de la llamada (`obj.metodo()` o `metodo()` dentro de un método) -> `"Clase.metodo"`.
    pub methods: HashMap<Span, String>,
}

/// Verificador con estado: recuerda las funciones y globales de las entradas anteriores (REPL).
#[derive(Clone, Default)]
pub struct Checker {
    funcs: HashMap<String, Sig>,
    classes: HashMap<String, ClassInfo>,
    globals: Vars,
}

impl Checker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Verifica el programa. Si falla, el estado del verificador no cambia.
    pub fn check(&mut self, program: &Program) -> Result<Resolutions, Diagnostic> {
        let mut pass = Pass {
            funcs: self.funcs.clone(),
            classes: self.classes.clone(),
            globals: self.globals.clone(),
            decls: HashMap::new(),
            state: HashMap::new(),
            things: HashMap::new(),
            class_state: HashMap::new(),
            res: Resolutions::default(),
        };
        pass.program(program)?;
        self.funcs = pass.funcs;
        self.classes = pass.classes;
        self.globals = pass.globals;
        Ok(pass.res)
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
    /// Clase cuyo método se está verificando (`this` y sus campos están a la vista).
    this: Option<String>,
}

struct Pass<'p> {
    funcs: HashMap<String, Sig>,
    classes: HashMap<String, ClassInfo>,
    globals: Vars,
    /// Funciones y métodos de este programa (clave `"f"` o `"Clase.m"`) y su clase, si es método.
    decls: HashMap<String, (&'p FnDecl, Option<String>)>,
    state: HashMap<String, State>,
    /// Clases de este programa y si ya se infirieron los tipos de los campos de su cuerpo.
    things: HashMap<String, &'p ThingDecl>,
    class_state: HashMap<String, State>,
    res: Resolutions,
}

/// Nombres que una clase no puede usar.
const RESERVED_TYPES: [&str; 6] = ["Int", "Float", "Bool", "Str", "Unit", "Array"];

impl<'p> Pass<'p> {
    fn program(&mut self, program: &'p Program) -> Result<(), Diagnostic> {
        // Los nombres de clase primero: los tipos de campos, parámetros y firmas pueden nombrarlas.
        for stmt in &program.stmts {
            let Stmt::Thing(decl) = stmt else { continue };
            if BUILTINS.contains(&decl.name.as_str())
                || RESERVED_TYPES.contains(&decl.name.as_str())
                || self.funcs.contains_key(&decl.name)
                || self.classes.contains_key(&decl.name)
            {
                return Err(Diagnostic::new(
                    decl.name_span,
                    format!("el nombre `{}` ya está en uso o está reservado", decl.name),
                ));
            }
            self.classes.insert(decl.name.clone(), ClassInfo::default());
        }

        // Las firmas se registran antes: las funciones pueden usarse antes de su definición.
        for stmt in &program.stmts {
            let Stmt::Fn(decl) = stmt else { continue };
            if BUILTINS.contains(&decl.name.as_str()) {
                return Err(Diagnostic::new(
                    decl.name_span,
                    format!(
                        "`{}` es una función integrada y no se puede redefinir",
                        decl.name
                    ),
                ));
            }
            if self.funcs.contains_key(&decl.name) || self.classes.contains_key(&decl.name) {
                return Err(Diagnostic::new(
                    decl.name_span,
                    format!("la función `{}` ya está definida", decl.name),
                ));
            }
            let sig = self.signature(decl)?;
            self.funcs.insert(decl.name.clone(), sig);
            self.decls.insert(decl.name.clone(), (decl, None));
            self.state.insert(decl.name.clone(), State::Unchecked);
        }

        for stmt in &program.stmts {
            if let Stmt::Thing(decl) = stmt {
                self.declare_class(decl)?;
            }
        }
        for stmt in &program.stmts {
            if let Stmt::Thing(decl) = stmt {
                self.ensure_class(&decl.name)?;
            }
        }

        let mut script = Ctx {
            scopes: Vec::new(),
            ret: Ret::Script,
            this: None,
        };
        for stmt in &program.stmts {
            if !matches!(stmt, Stmt::Fn(_) | Stmt::Thing(_)) {
                self.stmt(&mut script, stmt)?;
            }
        }
        for stmt in &program.stmts {
            match stmt {
                Stmt::Fn(decl) => {
                    if self.state[&decl.name] == State::Unchecked {
                        self.check_fn(&decl.name)?;
                    }
                }
                Stmt::Thing(thing) => {
                    for method in &thing.methods {
                        let key = format!("{}.{}", thing.name, method.name);
                        if self.state[&key] == State::Unchecked {
                            self.check_fn(&key)?;
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// Firma de una función o método: parámetros y tipo de retorno (`None` si hay que inferirlo).
    fn signature(&self, decl: &FnDecl) -> Result<Sig, Diagnostic> {
        let params = decl
            .params
            .iter()
            .map(|p| self.resolve(&p.ty))
            .collect::<Result<Vec<_>, _>>()?;
        let ret = match (&decl.ret, decl.expr_body) {
            (Some(ty), _) => Some(self.resolve(ty)?),
            (None, false) => Some(Type::Unit),
            (None, true) => None,
        };
        Ok(Sig { params, ret })
    }

    /// Registra los campos, el constructor y las firmas de los métodos de una clase.
    /// Los campos del cuerpo sin anotación llevan `Unit` provisional hasta `ensure_class`.
    fn declare_class(&mut self, decl: &'p ThingDecl) -> Result<(), Diagnostic> {
        self.things.insert(decl.name.clone(), decl);
        self.class_state.insert(decl.name.clone(), State::Unchecked);
        let mut fields: Vec<FieldInfo> = Vec::new();
        let mut ctor = Vec::new();
        for param in &decl.params {
            let ty = self.resolve(&param.ty)?;
            if ty == Type::Unit {
                return Err(Diagnostic::new(
                    param.ty.span,
                    "un parámetro no puede ser de tipo Unit",
                ));
            }
            ctor.push(ty.clone());
            if let Some(mutable) = param.mutable {
                fields.push(FieldInfo {
                    name: param.name.clone(),
                    ty,
                    mutable,
                });
            }
        }
        for field in &decl.fields {
            if fields.iter().any(|f| f.name == field.name) {
                return Err(Diagnostic::new(
                    field.name_span,
                    format!("el campo `{}` está repetido", field.name),
                ));
            }
            let ty = match &field.ty {
                Some(ty) => self.resolve(ty)?,
                None => Type::Unit,
            };
            fields.push(FieldInfo {
                name: field.name.clone(),
                ty,
                mutable: field.mutable,
            });
        }
        for method in &decl.methods {
            let key = format!("{}.{}", decl.name, method.name);
            if fields.iter().any(|f| f.name == method.name) {
                return Err(Diagnostic::new(
                    method.name_span,
                    format!(
                        "`{}` ya es un campo de `{}`; un método no puede llamarse igual",
                        method.name, decl.name
                    ),
                ));
            }
            if self.funcs.contains_key(&key) {
                return Err(Diagnostic::new(
                    method.name_span,
                    format!(
                        "el método `{}` ya está definido en `{}`",
                        method.name, decl.name
                    ),
                ));
            }
            let sig = self.signature(method)?;
            self.funcs.insert(key.clone(), sig);
            self.decls
                .insert(key.clone(), (method, Some(decl.name.clone())));
            self.state.insert(key, State::Unchecked);
        }
        let info = self.classes.get_mut(&decl.name).expect("registrada antes");
        info.fields = fields;
        info.ctor = ctor;
        Ok(())
    }

    /// Infiere los tipos de los campos del cuerpo la primera vez que hacen falta (como el tipo de
    /// retorno de una función `= expr`). Una clase de una entrada anterior del REPL ya está completa.
    fn ensure_class(&mut self, name: &str) -> Result<(), Diagnostic> {
        match self.class_state.get(name).copied() {
            None | Some(State::Done) => Ok(()),
            Some(State::InProgress) => Err(Diagnostic::new(
                self.things[name].name_span,
                format!(
                    "los campos de `{name}` se usan antes de conocer su tipo; anota el tipo del campo"
                ),
            )),
            Some(State::Unchecked) => {
                self.class_state
                    .insert(name.to_string(), State::InProgress);
                let decl: &'p ThingDecl = self.things[name];
                self.class_initializers(decl)?;
                self.class_state.insert(name.to_string(), State::Done);
                Ok(())
            }
        }
    }

    /// Verifica los valores iniciales de los campos del cuerpo. Ven los parámetros del
    /// constructor y los campos anteriores, pero no `this`.
    fn class_initializers(&mut self, decl: &'p ThingDecl) -> Result<(), Diagnostic> {
        let mut scope = Vars::new();
        for (param, ty) in decl
            .params
            .iter()
            .zip(self.classes[&decl.name].ctor.clone())
        {
            scope.insert(param.name.clone(), (ty, false));
        }
        let mut ctx = Ctx {
            scopes: vec![scope],
            ret: Ret::Script,
            this: None,
        };
        let first = decl.params.iter().filter(|p| p.mutable.is_some()).count();
        for (i, field) in decl.fields.iter().enumerate() {
            let ty = match &field.ty {
                Some(ty) => {
                    let expected = self.resolve(ty)?;
                    self.expr_as(&mut ctx, &field.init, &expected)?;
                    expected
                }
                None => self.expr(&mut ctx, &field.init)?,
            };
            if ty == Type::Unit {
                return Err(Diagnostic::new(
                    field.init.span(),
                    format!(
                        "el campo `{}` no puede guardar un valor de tipo Unit",
                        field.name
                    ),
                ));
            }
            let info = self.classes.get_mut(&decl.name).expect("registrada antes");
            info.fields[first + i].ty = ty.clone();
            self.declare(&mut ctx, &field.name, (ty, false), field.name_span)?;
        }
        Ok(())
    }

    fn check_fn(&mut self, key: &str) -> Result<(), Diagnostic> {
        let (decl, class) = self.decls[key].clone();
        if let Some(class) = &class {
            self.ensure_class(class)?;
        }
        self.state.insert(key.to_string(), State::InProgress);
        let sig = self.funcs[key].clone();
        let mut scope = Vars::new();
        for (param, ty) in decl.params.iter().zip(&sig.params) {
            scope.insert(param.name.clone(), (ty.clone(), false));
        }
        let mut ctx = Ctx {
            scopes: vec![scope],
            ret: match sig.ret {
                Some(ty) => Ret::Declared(ty),
                None => Ret::Infer(None),
            },
            this: class,
        };
        self.block(&mut ctx, &decl.body)?;
        match ctx.ret {
            Ret::Infer(found) => {
                let ty = found.unwrap_or(Type::Unit);
                if let Some(sig) = self.funcs.get_mut(key) {
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
        self.state.insert(key.to_string(), State::Done);
        Ok(())
    }

    fn block(&mut self, ctx: &mut Ctx, block: &Block) -> Result<(), Diagnostic> {
        ctx.scopes.push(Vars::new());
        let result = block.stmts.iter().try_for_each(|stmt| self.stmt(ctx, stmt));
        ctx.scopes.pop();
        result
    }

    /// Busca una variable: ámbitos locales, campos de `this` y globales, en ese orden.
    /// Devuelve tipo, si es mutable y, si es un campo, su índice.
    fn lookup(&self, ctx: &Ctx, name: &str) -> Option<(Type, bool, Option<u32>)> {
        if let Some((ty, mutable)) = ctx.scopes.iter().rev().find_map(|scope| scope.get(name)) {
            return Some((ty.clone(), *mutable, None));
        }
        if let Some(info) = ctx.this.as_ref().and_then(|class| self.classes.get(class)) {
            if let Some(i) = info.fields.iter().position(|f| f.name == name) {
                let f = &info.fields[i];
                return Some((f.ty.clone(), f.mutable, Some(i as u32)));
            }
        }
        self.globals
            .get(name)
            .map(|(ty, mutable)| (ty.clone(), *mutable, None))
    }

    fn ensure_class_of(&mut self, ty: &Type) -> Result<(), Diagnostic> {
        match ty {
            Type::Obj(class) => self.ensure_class(class),
            _ => Ok(()),
        }
    }

    /// Campo `name` de un valor de tipo `ty`: índice, tipo y mutabilidad.
    fn field_of(
        &self,
        ty: &Type,
        name: &str,
        name_span: Span,
        target_span: Span,
    ) -> Result<(u32, Type, bool), Diagnostic> {
        let Type::Obj(class) = ty else {
            return Err(Diagnostic::new(
                target_span,
                format!("solo los objetos tienen campos, pero esto es {ty}"),
            ));
        };
        let info = &self.classes[class];
        match info.fields.iter().position(|f| f.name == name) {
            Some(i) => Ok((i as u32, info.fields[i].ty.clone(), info.fields[i].mutable)),
            None => Err(Diagnostic::new(
                name_span,
                format!("la clase `{class}` no tiene el campo `{name}`"),
            )),
        }
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

    /// Tipo de `expr` exigiendo `expected`. Un arreglo vacío `[]` toma el tipo esperado.
    fn expr_as(&mut self, ctx: &mut Ctx, expr: &Expr, expected: &Type) -> Result<(), Diagnostic> {
        if let (Expr::Array { elems, .. }, Type::Array(_)) = (expr, expected) {
            if elems.is_empty() {
                return Ok(());
            }
        }
        let found = self.expr(ctx, expr)?;
        expect(expected, &found, expr.span())
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
                let declared = match ty {
                    Some(ty) => {
                        let expected = self.resolve(ty)?;
                        self.expr_as(ctx, init, &expected)?;
                        expected
                    }
                    None => self.expr(ctx, init)?,
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
                let Some((ty, mutable, field)) = self.lookup(ctx, name) else {
                    return Err(undefined_variable(name, *name_span));
                };
                if !mutable {
                    return Err(Diagnostic::new(
                        *name_span,
                        format!("no se puede asignar a `{name}` porque es inmutable; decláralo con `mut`"),
                    ));
                }
                if let Some(i) = field {
                    self.res.fields.insert(*name_span, i);
                }
                self.assigned(ctx, &ty, *op, value, *span)?;
            }
            Stmt::FieldAssign {
                target,
                name,
                name_span,
                op,
                value,
                span,
            } => {
                let target_ty = self.expr(ctx, target)?;
                self.ensure_class_of(&target_ty)?;
                let (i, ty, mutable) =
                    self.field_of(&target_ty, name, *name_span, target.span())?;
                if !mutable {
                    return Err(Diagnostic::new(
                        *name_span,
                        format!("no se puede asignar al campo `{name}` porque es inmutable; decláralo con `mut`"),
                    ));
                }
                self.res.fields.insert(*name_span, i);
                self.assigned(ctx, &ty, *op, value, *span)?;
            }
            Stmt::IndexAssign {
                target,
                index,
                op,
                value,
                span,
            } => {
                let elem = self.indexed_type(ctx, target, index)?;
                self.assigned(ctx, &elem, *op, value, *span)?;
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
                let expected = match &ctx.ret {
                    Ret::Script => None,
                    Ret::Declared(ty) => Some(ty.clone()),
                    Ret::Infer(slot) => slot.clone(),
                };
                match (expr, expected) {
                    (Some(expr), Some(expected)) => {
                        if expected == Type::Unit && !matches!(ctx.ret, Ret::Infer(_)) {
                            let found = self.expr(ctx, expr)?;
                            return Err(Diagnostic::new(
                                expr.span(),
                                format!(
                                    "la función no declara un tipo de retorno, pero devuelve {found}; añade `-> {found}`"
                                ),
                            ));
                        }
                        self.expr_as(ctx, expr, &expected)?;
                    }
                    (Some(expr), None) => {
                        let found = self.expr(ctx, expr)?;
                        if let Ret::Infer(slot) = &mut ctx.ret {
                            *slot = Some(found);
                        }
                    }
                    (None, Some(expected)) => expect(&expected, &Type::Unit, *span)?,
                    (None, None) => {
                        if let Ret::Infer(slot) = &mut ctx.ret {
                            *slot = Some(Type::Unit);
                        }
                    }
                }
            }
            Stmt::Break(_) | Stmt::Continue(_) | Stmt::Fn(_) | Stmt::Thing(_) => {}
        }
        Ok(())
    }

    /// Verifica el valor asignado a algo de tipo `ty` (`=` o `op=`).
    fn assigned(
        &mut self,
        ctx: &mut Ctx,
        ty: &Type,
        op: Option<BinOp>,
        value: &Expr,
        span: Span,
    ) -> Result<(), Diagnostic> {
        match op {
            Some(op) => {
                let found = self.expr(ctx, value)?;
                let result = binary_type(op, ty, &found, span)?;
                expect(ty, &result, value.span())
            }
            None => self.expr_as(ctx, value, ty),
        }
    }

    /// Tipo del elemento de `target[index]`.
    fn indexed_type(
        &mut self,
        ctx: &mut Ctx,
        target: &Expr,
        index: &Expr,
    ) -> Result<Type, Diagnostic> {
        let target_ty = self.expr(ctx, target)?;
        let index_ty = self.expr(ctx, index)?;
        if index_ty != Type::Int {
            return Err(Diagnostic::new(
                index.span(),
                format!("el índice debe ser Int, pero es {index_ty}"),
            ));
        }
        match target_ty {
            Type::Array(elem) => Ok(*elem),
            other => Err(Diagnostic::new(
                target.span(),
                format!("solo se puede indexar un arreglo, pero esto es {other}"),
            )),
        }
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
            Expr::Str(..) => Ok(Type::Str),
            Expr::Var(name, span) => {
                let Some((ty, _, field)) = self.lookup(ctx, name) else {
                    return Err(undefined_variable(name, *span));
                };
                if let Some(i) = field {
                    self.res.fields.insert(*span, i);
                }
                Ok(ty)
            }
            Expr::This(span) => match &ctx.this {
                Some(class) => Ok(Type::Obj(class.clone())),
                None => Err(Diagnostic::new(
                    *span,
                    "`this` solo se puede usar dentro de un método",
                )),
            },
            Expr::Field {
                target,
                name,
                name_span,
                span,
            } => {
                let target_ty = self.expr(ctx, target)?;
                self.ensure_class_of(&target_ty)?;
                let (i, ty, _) = self.field_of(&target_ty, name, *name_span, target.span())?;
                self.res.fields.insert(*span, i);
                Ok(ty)
            }
            Expr::MethodCall {
                target,
                name,
                name_span,
                args,
                span,
            } => {
                let target_ty = self.expr(ctx, target)?;
                let Type::Obj(class) = &target_ty else {
                    return Err(Diagnostic::new(
                        target.span(),
                        format!("solo los objetos tienen métodos, pero esto es {target_ty}"),
                    ));
                };
                let key = format!("{class}.{name}");
                if !self.funcs.contains_key(&key) {
                    return Err(Diagnostic::new(
                        *name_span,
                        format!("la clase `{class}` no tiene el método `{name}`"),
                    ));
                }
                self.res.methods.insert(*span, key.clone());
                self.call_sig(ctx, &key, name, "el método", args, *span)
            }
            Expr::Interp { parts, .. } => {
                for part in parts {
                    if let InterpPart::Expr(inner) = part {
                        if self.expr(ctx, inner)? == Type::Unit {
                            return Err(Diagnostic::new(
                                inner.span(),
                                "no se puede interpolar un valor de tipo Unit",
                            ));
                        }
                    }
                }
                Ok(Type::Str)
            }
            Expr::Array { elems, span } => {
                let Some((first, rest)) = elems.split_first() else {
                    return Err(Diagnostic::new(
                        *span,
                        "no se puede deducir el tipo de un arreglo vacío; anótalo, por ejemplo `fix a: [Int] = []`",
                    ));
                };
                let elem = self.expr(ctx, first)?;
                if elem == Type::Unit {
                    return Err(Diagnostic::new(
                        first.span(),
                        "un arreglo no puede contener valores de tipo Unit",
                    ));
                }
                for other in rest {
                    self.expr_as(ctx, other, &elem)?;
                }
                Ok(Type::Array(Box::new(elem)))
            }
            Expr::Index { target, index, .. } => self.indexed_type(ctx, target, index),
            Expr::Unary { op, expr, span } => {
                let ty = self.expr(ctx, expr)?;
                match (op, &ty) {
                    (UnOp::Neg, ty) if ty.is_numeric() => Ok(ty.clone()),
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
                binary_type(*op, &a, &b, *span)
            }
            Expr::Call { name, args, span } => {
                if BUILTINS.contains(&name.as_str()) {
                    self.builtin(ctx, name, args, *span)
                } else {
                    self.call(ctx, name, args, *span)
                }
            }
        }
    }

    fn builtin(
        &mut self,
        ctx: &mut Ctx,
        name: &str,
        args: &[Expr],
        span: Span,
    ) -> Result<Type, Diagnostic> {
        let wanted = if name == "len" { 1 } else { 2 };
        if args.len() != wanted {
            return Err(Diagnostic::new(
                span,
                format!(
                    "la función `{name}` espera {wanted} argumento(s), pero recibió {}",
                    args.len()
                ),
            ));
        }
        let first = self.expr(ctx, &args[0])?;
        if name == "len" {
            return match first {
                Type::Str | Type::Array(_) => Ok(Type::Int),
                other => Err(Diagnostic::new(
                    args[0].span(),
                    format!("`len` admite Str o un arreglo, pero recibió {other}"),
                )),
            };
        }
        // push(arreglo, valor)
        let Type::Array(elem) = first else {
            return Err(Diagnostic::new(
                args[0].span(),
                format!("`push` necesita un arreglo, pero recibió {first}"),
            ));
        };
        self.expr_as(ctx, &args[1], &elem)?;
        Ok(Type::Unit)
    }

    /// Llamada `nombre(args)`: constructor de una clase, método de `this` o función.
    fn call(
        &mut self,
        ctx: &mut Ctx,
        name: &str,
        args: &[Expr],
        span: Span,
    ) -> Result<Type, Diagnostic> {
        if let Some(info) = self.classes.get(name) {
            let params = info.ctor.clone();
            if params.len() != args.len() {
                return Err(Diagnostic::new(
                    span,
                    format!(
                        "la clase `{name}` espera {} argumento(s), pero recibió {}",
                        params.len(),
                        args.len()
                    ),
                ));
            }
            for (arg, expected) in args.iter().zip(&params) {
                self.expr_as(ctx, arg, expected)?;
            }
            return Ok(Type::Obj(name.to_string()));
        }
        if let Some(class) = ctx.this.clone() {
            let key = format!("{class}.{name}");
            if self.funcs.contains_key(&key) {
                self.res.methods.insert(span, key.clone());
                return self.call_sig(ctx, &key, name, "el método", args, span);
            }
        }
        self.call_sig(ctx, name, name, "la función", args, span)
    }

    /// Llamada a la función o método de clave `key`; `shown` y `what` son para los mensajes.
    fn call_sig(
        &mut self,
        ctx: &mut Ctx,
        key: &str,
        shown: &str,
        what: &str,
        args: &[Expr],
        span: Span,
    ) -> Result<Type, Diagnostic> {
        let Some(sig) = self.funcs.get(key) else {
            return Err(Diagnostic::new(
                span,
                format!("la función `{shown}` no está definida"),
            ));
        };
        if sig.params.len() != args.len() {
            return Err(Diagnostic::new(
                span,
                format!(
                    "{what} `{shown}` espera {} argumento(s), pero recibió {}",
                    sig.params.len(),
                    args.len()
                ),
            ));
        }
        let params = sig.params.clone();
        for (arg, expected) in args.iter().zip(&params) {
            self.expr_as(ctx, arg, expected)?;
        }
        // Si el tipo de retorno aún no se infirió, se verifica ahora el cuerpo de esa función.
        if self.funcs[key].ret.is_none() {
            match self.state.get(key) {
                Some(State::InProgress) => {
                    return Err(Diagnostic::new(
                        span,
                        format!(
                            "`{shown}` se llama a sí misma antes de conocer su tipo; añade `-> Tipo` a su declaración"
                        ),
                    ));
                }
                _ => {
                    if self.decls.contains_key(key) {
                        self.check_fn(key)?;
                    }
                }
            }
        }
        Ok(self.funcs[key].ret.clone().unwrap_or(Type::Unit))
    }

    fn resolve(&self, ty: &TypeRef) -> Result<Type, Diagnostic> {
        let unknown = || {
            Diagnostic::new(
                ty.span,
                format!(
                    "el tipo `{}` no existe (hay Int, Float, Bool, Str, Unit, [T] y las clases declaradas)",
                    ty.name
                ),
            )
        };
        match (ty.name.as_str(), ty.args.as_slice()) {
            ("Int", []) => Ok(Type::Int),
            ("Float", []) => Ok(Type::Float),
            ("Bool", []) => Ok(Type::Bool),
            ("Str", []) => Ok(Type::Str),
            ("Unit", []) => Ok(Type::Unit),
            ("Array", [elem]) => {
                let elem = self.resolve(elem)?;
                if elem == Type::Unit {
                    return Err(Diagnostic::new(
                        ty.span,
                        "un arreglo no puede contener valores de tipo Unit",
                    ));
                }
                Ok(Type::Array(Box::new(elem)))
            }
            (name, []) if self.classes.contains_key(name) => Ok(Type::Obj(name.to_string())),
            _ => Err(unknown()),
        }
    }
}

fn expect(expected: &Type, found: &Type, span: Span) -> Result<(), Diagnostic> {
    if expected == found {
        return Ok(());
    }
    let hint = if *expected == Type::Float && *found == Type::Int {
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
fn binary_type(op: BinOp, a: &Type, b: &Type, span: Span) -> Result<Type, Diagnostic> {
    let invalid = |hint: &str| {
        Diagnostic::new(
            span,
            format!("la operación `{}` no admite {a} y {b}{hint}", op.symbol()),
        )
    };
    match op {
        BinOp::Add if *a == Type::Str && *b == Type::Str => Ok(Type::Str),
        BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Rem => match (a, b) {
            (Type::Int, Type::Int) => Ok(Type::Int),
            _ if a.is_numeric() && b.is_numeric() => Ok(Type::Float),
            _ if op == BinOp::Add && (*a == Type::Str || *b == Type::Str) => Err(invalid(
                "; para unir texto con otros valores usa una interpolación: \"${...}\"",
            )),
            _ => Err(invalid("")),
        },
        BinOp::Eq | BinOp::Ne => {
            let comparable = !matches!(a, Type::Array(_) | Type::Obj(_));
            if (a == b && comparable) || (a.is_numeric() && b.is_numeric()) {
                Ok(Type::Bool)
            } else {
                Err(invalid(""))
            }
        }
        BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
            if (a.is_numeric() && b.is_numeric()) || (*a == Type::Str && *b == Type::Str) {
                Ok(Type::Bool)
            } else {
                Err(invalid(""))
            }
        }
        BinOp::And | BinOp::Or => match [a, b].into_iter().find(|t| **t != Type::Bool) {
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
            .map(|_| ())
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
        let run = |c: &mut Checker, src: &str| {
            c.check(&parse(src).unwrap())
                .map(|_| ())
                .map_err(|d| d.message)
        };
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

    // --- M3: cadenas y arreglos ---

    #[test]
    fn cadenas_y_arreglos_bien_tipados() {
        for src in [
            "fix s = \"hola\"\nshow(s + \" mundo\")\nshow(\"x = ${1 + 2}\")",
            "fix a = [1, 2, 3]\nshow(a[0] + len(a))\nmut b: [Str] = []\npush(b, \"x\")\nb[0] = \"y\"",
            "mut a = [1.5, 2.5]\na[1] += 1\nshow(a)",
            "fn suma(v: [Int]) -> Int {\n mut t = 0\n mut i = 0\n while i < len(v) { t += v[i]\n i += 1 }\n return t }\nshow(suma([1, 2]))",
            "fix m = [[1, 2], [3]]\nshow(m[0][1])",
            "show(\"a\" < \"b\")\nshow(\"a\" == \"a\")\nshow(len(\"ñandú\"))",
            "fn f(v: [Int]) { }\nf([])",
        ] {
            assert_eq!(check(src), Ok(()), "{src}");
        }
    }

    #[test]
    fn errores_de_cadenas() {
        assert!(err("show(\"a\" + 1)").contains("no admite Str y Int"));
        assert!(err("show(\"a\" + 1)").contains("interpolación"));
        assert!(err("fix s: Int = \"x\"").contains("se esperaba Int, pero es Str"));
        assert!(err("show(\"${f()}\")\nfn f() { }").contains("Unit"));
        assert!(err("show(\"a\" - \"b\")").contains("no admite Str y Str"));
    }

    #[test]
    fn errores_de_arreglos() {
        assert!(err("fix a = []").contains("no se puede deducir el tipo"));
        assert!(err("fix a = [1, true]").contains("se esperaba Int, pero es Bool"));
        assert!(err("fix a = [1]\nshow(a[true])").contains("el índice debe ser Int"));
        assert!(err("fix a = 5\nshow(a[0])").contains("solo se puede indexar un arreglo"));
        assert!(err("fix a = [1]\na[0] = 1.5").contains("se esperaba Int, pero es Float"));
        assert!(err("fix a = [1]\npush(a, \"x\")").contains("se esperaba Int, pero es Str"));
        assert!(err("push(1, 2)").contains("`push` necesita un arreglo"));
        assert!(err("show(len(1))").contains("`len` admite Str o un arreglo"));
        assert!(err("show(len())").contains("espera 1 argumento(s)"));
        assert!(err("show([1] == [1])").contains("no admite [Int] y [Int]"));
        assert!(err("fix a: [Int] = [true]").contains("se esperaba [Int], pero es [Bool]"));
        assert!(err("fix a: [Cosa] = []").contains("`Cosa`"));
    }

    #[test]
    fn las_funciones_integradas_no_se_redefinen() {
        assert!(err("fn len(a: Int) = a").contains("función integrada"));
    }

    #[test]
    fn tipos_de_arreglos_en_texto() {
        assert!(err("fix a: [[Int]] = [1]").contains("se esperaba [[Int]], pero es [Int]"));
    }

    // --- M4: clases ---

    #[test]
    fn clases_bien_tipadas() {
        for src in [
            "thing P(fix x: Int, mut y: Int) { fn suma() -> Int = x + y }\nshow(P(1, 2).suma())",
            "show(P(1).x)\nthing P(fix x: Int)",
            "thing A(fix b: B)\nthing B(fix a: Int)\nshow(A(B(1)).b.a)",
            "thing C() { fn f(x: Int) -> Int = x + 1\n fn g() -> Int = f(2) + this.f(3) }\nshow(C().g())",
            "thing C(n: Int) { fix d = n * 2\n fix e = d + n }\nshow(C(1).e)",
            "thing C(mut v: Int) { fn dup() -> C = C(v) }\nfix xs: [C] = [C(1).dup()]\nshow(xs[0].v)",
            "thing C() { mut items: [Int] = [] }\nC().items[0] = 1",
        ] {
            assert_eq!(check(src), Ok(()), "{src}");
        }
    }

    #[test]
    fn errores_de_clases() {
        let pre = "thing P(fix x: Int, mut y: Int) { fn m() -> Int = x }\nfix p = P(1, 2)\n";
        let cases = [
            (
                "p.x = 5",
                "no se puede asignar al campo `x` porque es inmutable",
            ),
            ("p.z", "la clase `P` no tiene el campo `z`"),
            ("p.zz()", "la clase `P` no tiene el método `zz`"),
            (
                "p.m(1)",
                "el método `m` espera 0 argumento(s), pero recibió 1",
            ),
            ("P(1)", "la clase `P` espera 2 argumento(s), pero recibió 1"),
            ("P(1, true)", "se esperaba Int, pero es Bool"),
            ("p.y = \"a\"", "se esperaba Int, pero es Str"),
            (
                "show(this)",
                "`this` solo se puede usar dentro de un método",
            ),
            ("show(p == p)", "no admite P y P"),
            (
                "show(len(p))",
                "`len` admite Str o un arreglo, pero recibió P",
            ),
            (
                "fix n = 1\nshow(n.x)",
                "solo los objetos tienen campos, pero esto es Int",
            ),
            (
                "fix n = 1\nn.f()",
                "solo los objetos tienen métodos, pero esto es Int",
            ),
            ("fix q: Cosa = p", "el tipo `Cosa` no existe"),
            ("fix q: P = 1", "se esperaba P, pero es Int"),
        ];
        for (tail, expected) in cases {
            let message = err(&format!("{pre}{tail}"));
            assert!(
                message.contains(expected),
                "{tail}: se esperaba «{expected}», pero fue «{message}»"
            );
        }
    }

    #[test]
    fn errores_de_declaracion_de_clases() {
        assert!(err("thing A()\nthing A()").contains("ya está en uso"));
        assert!(err("fn A() { }\nthing A()").contains("ya está definida"));
        assert!(err("thing A()\nfn A() { }").contains("ya está definida"));
        assert!(err("thing Int()").contains("ya está en uso o está reservado"));
        assert!(err("thing len()").contains("ya está en uso"));
        assert!(err("thing A(fix x: Int) { fix x = 1 }").contains("el campo `x` está repetido"));
        assert!(err("thing A(fix x: Int) { fn x() { } }").contains("ya es un campo"));
        assert!(err("thing A() { fn f() { }\n fn f() { } }").contains("ya está definido"));
        assert!(err("thing A(n: Int) { fix n = 1 }").contains("ya está declarada"));
        assert!(err("thing A(x: Unit)").contains("Unit"));
        assert!(err("thing A() { fix x = this }").contains("`this` solo se puede usar"));
    }

    #[test]
    fn asignar_a_un_campo_fix_por_su_nombre_se_rechaza() {
        let message = err("thing A(fix x: Int) { fn f() { x = 2 } }");
        assert!(
            message.contains("no se puede asignar a `x` porque es inmutable"),
            "{message}"
        );
    }

    #[test]
    fn un_local_tapa_al_campo() {
        // `x` es el parámetro (Str), no el campo Int.
        assert_eq!(
            check("thing A(fix x: Int) { fn f(x: Str) -> Str = x }"),
            Ok(())
        );
        let message = err("thing A(fix x: Int) { fn f(x: Str) -> Int = x }");
        assert!(
            message.contains("se esperaba Int, pero es Str"),
            "{message}"
        );
    }

    #[test]
    fn las_resoluciones_se_indexan_por_posicion() {
        let src = "thing A(fix x: Int, mut y: Int) { fn f() { y += x } }\nfix a = A(1, 2)\nshow(a.y)\na.y = 3\na.f()";
        let program = parse(src).unwrap();
        let res = Checker::new().check(&program).unwrap();
        // `y` y `x` dentro de `f` (campos 1 y 0), `a.y` (1) y el nombre `y` de `a.y = 3` (1).
        let mut fields: Vec<u32> = res.fields.values().copied().collect();
        fields.sort();
        assert_eq!(fields, vec![0, 1, 1, 1]);
        assert_eq!(res.methods.len(), 1);
        assert_eq!(res.methods.values().next().unwrap(), "A.f");
    }

    #[test]
    fn el_verificador_recuerda_las_clases_entre_entradas() {
        let mut c = Checker::new();
        let run = |c: &mut Checker, src: &str| {
            c.check(&parse(src).unwrap())
                .map(|_| ())
                .map_err(|d| d.message)
        };
        assert_eq!(run(&mut c, "thing P(fix x: Int)"), Ok(()));
        assert_eq!(run(&mut c, "fix p = P(1)"), Ok(()));
        assert_eq!(run(&mut c, "show(p.x)"), Ok(()));
        assert!(run(&mut c, "thing P()").is_err());
        // Una entrada fallida no registra su clase.
        assert!(run(&mut c, "thing Q(fix a: Int)\nshow(Q(1).zz)").is_err());
        assert_eq!(run(&mut c, "thing Q(fix a: Str)"), Ok(()));
    }

    #[test]
    fn los_campos_sin_anotar_se_infieren_bajo_demanda() {
        // `Uno` necesita el campo `x` de `Dos`, declarada después y sin anotar.
        assert_eq!(
            check("fn f() = Dos().x\nthing Uno() { fix a = f() }\nthing Dos() { fix x = 5 }\nshow(Uno().a + 1)"),
            Ok(())
        );
        // Un campo que depende de sí mismo no se puede inferir.
        let message = err("fn f() = Uno().a\nthing Uno() { fix a = f() }");
        assert!(
            message.contains("se usan antes de conocer su tipo"),
            "{message}"
        );
    }
}
