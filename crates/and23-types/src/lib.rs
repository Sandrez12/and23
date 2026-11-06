//! Verificador de tipos estáticos de And23. Se ejecuta entre el parser y el compilador:
//! un programa mal tipado se rechaza con su posición antes de generar bytecode.

use and23_syntax::{
    BinOp, Block, Diagnostic, Expr, FnDecl, InterpPart, KindDecl, Program, Span, Stmt, ThingDecl,
    TypeRef, UnOp,
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
    /// Instancia de una clase (`thing`) o valor cuyo tipo es una interfaz (`kind`).
    Obj(String),
    /// `T?`: un `T` o `nil`.
    Nullable(Box<Type>),
    /// El tipo del literal `nil`; solo existe mientras se verifica una expresión.
    Nil,
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
            Type::Nullable(inner) => write!(f, "{inner}?"),
            Type::Nil => f.write_str("nil"),
        }
    }
}

#[derive(Clone)]
struct Sig {
    params: Vec<Type>,
    /// `None` mientras no se infirió (función con cuerpo `= expr` sin `->`).
    ret: Option<Type>,
}

/// Variable: tipo, si es mutable y, si es un estrechamiento (`if x != nil`), su tipo declarado `T?`.
type VarEntry = (Type, bool, Option<Type>);
type Vars = HashMap<String, VarEntry>;

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
    /// Solo los campos propios; los heredados están en la clase padre.
    fields: Vec<FieldInfo>,
    ctor: Vec<Type>,
    parent: Option<String>,
    /// Interfaces que declara implementar.
    kinds: Vec<String>,
}

/// Lo que el verificador decidió y el compilador necesita (no tiene tipos propios):
/// a qué campo apunta cada acceso y a qué método cada llamada, indexados por su posición.
#[derive(Debug, Default, Clone)]
pub struct Resolutions {
    /// Posición del acceso (`obj.campo`, nombre suelto dentro de un método o nombre del campo
    /// en una asignación) -> índice del campo.
    pub fields: HashMap<Span, u32>,
    /// Posición de una llamada con destino fijo (`super.metodo()`) -> `"Clase.metodo"` de la clase
    /// que lo declara.
    pub methods: HashMap<Span, String>,
    /// Posición de una llamada virtual (`obj.metodo()` o `metodo()` dentro de un método) -> nombre
    /// del método: la VM elige la implementación según la clase real del objeto.
    pub virtuals: HashMap<Span, String>,
}

/// Verificador con estado: recuerda las funciones y globales de las entradas anteriores (REPL).
#[derive(Clone, Default)]
pub struct Checker {
    funcs: HashMap<String, Sig>,
    classes: HashMap<String, ClassInfo>,
    /// Interfaz -> nombres de sus métodos (las firmas están en `funcs` como `"Interfaz.metodo"`).
    kinds: HashMap<String, Vec<String>>,
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
            kinds: self.kinds.clone(),
            globals: self.globals.clone(),
            decls: HashMap::new(),
            state: HashMap::new(),
            declared: HashMap::new(),
            things: HashMap::new(),
            class_state: HashMap::new(),
            res: Resolutions::default(),
        };
        pass.program(program)?;
        self.funcs = pass.funcs;
        self.classes = pass.classes;
        self.kinds = pass.kinds;
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
    /// Globales `fix` estrechadas en el nivel superior (no hay ámbito donde guardarlas).
    script_narrow: HashMap<String, Type>,
    /// Clase cuyo método se está verificando (`this` y sus campos están a la vista).
    this: Option<String>,
}

struct Pass<'p> {
    funcs: HashMap<String, Sig>,
    classes: HashMap<String, ClassInfo>,
    kinds: HashMap<String, Vec<String>>,
    globals: Vars,
    /// Orden de declaración de las clases de este programa: el padre antes que el hijo.
    declared: HashMap<String, State>,
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
            let (name, name_span) = match stmt {
                Stmt::Thing(decl) => (&decl.name, decl.name_span),
                Stmt::Kind(decl) => (&decl.name, decl.name_span),
                _ => continue,
            };
            if BUILTINS.contains(&name.as_str())
                || RESERVED_TYPES.contains(&name.as_str())
                || self.funcs.contains_key(name)
                || self.classes.contains_key(name)
                || self.kinds.contains_key(name)
            {
                return Err(Diagnostic::new(
                    name_span,
                    format!("el nombre `{name}` ya está en uso o está reservado"),
                ));
            }
            match stmt {
                Stmt::Thing(decl) => {
                    self.classes.insert(name.clone(), ClassInfo::default());
                    self.things.insert(name.clone(), decl);
                    self.class_state.insert(name.clone(), State::Unchecked);
                    self.declared.insert(name.clone(), State::Unchecked);
                }
                _ => {
                    self.kinds.insert(name.clone(), Vec::new());
                }
            }
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
            if self.funcs.contains_key(&decl.name)
                || self.classes.contains_key(&decl.name)
                || self.kinds.contains_key(&decl.name)
            {
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
            if let Stmt::Kind(decl) = stmt {
                self.declare_kind(decl)?;
            }
        }
        for stmt in &program.stmts {
            if let Stmt::Thing(decl) = stmt {
                self.declare_ordered(&decl.name)?;
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
            script_narrow: HashMap::new(),
            this: None,
        };
        for stmt in &program.stmts {
            if !matches!(stmt, Stmt::Fn(_) | Stmt::Thing(_) | Stmt::Kind(_)) {
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
        for stmt in &program.stmts {
            if let Stmt::Thing(thing) = stmt {
                self.validate_class(thing)?;
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

    /// Registra las firmas de los métodos de una interfaz como `"Interfaz.metodo"`.
    fn declare_kind(&mut self, decl: &KindDecl) -> Result<(), Diagnostic> {
        let mut names: Vec<String> = Vec::new();
        for method in &decl.methods {
            if names.contains(&method.name) {
                return Err(Diagnostic::new(
                    method.name_span,
                    format!(
                        "el método `{}` está repetido en `{}`",
                        method.name, decl.name
                    ),
                ));
            }
            let params = method
                .params
                .iter()
                .map(|p| self.resolve(&p.ty))
                .collect::<Result<Vec<_>, _>>()?;
            let ret = match &method.ret {
                Some(ty) => self.resolve(ty)?,
                None => Type::Unit,
            };
            self.funcs.insert(
                format!("{}.{}", decl.name, method.name),
                Sig {
                    params,
                    ret: Some(ret),
                },
            );
            names.push(method.name.clone());
        }
        self.kinds.insert(decl.name.clone(), names);
        Ok(())
    }

    /// Declara una clase después de su padre (los campos heredados van primero).
    fn declare_ordered(&mut self, name: &str) -> Result<(), Diagnostic> {
        match self.declared.get(name).copied() {
            None | Some(State::Done) => return Ok(()),
            Some(State::InProgress) => {
                return Err(Diagnostic::new(
                    self.things[name].name_span,
                    format!("la herencia de `{name}` es circular"),
                ));
            }
            Some(State::Unchecked) => {}
        }
        self.declared.insert(name.to_string(), State::InProgress);
        let decl: &'p ThingDecl = self.things[name];
        if let Some(parent) = decl.supers.iter().find(|s| s.args.is_some()) {
            self.declare_ordered(&parent.name)?;
        }
        self.declare_class(decl)?;
        self.declared.insert(name.to_string(), State::Done);
        Ok(())
    }

    /// Todos los campos de una clase: primero los heredados.
    fn all_fields(&self, class: &str) -> Vec<FieldInfo> {
        let info = &self.classes[class];
        let mut fields = match &info.parent {
            Some(parent) => self.all_fields(parent),
            None => Vec::new(),
        };
        fields.extend(info.fields.iter().cloned());
        fields
    }

    /// Clave `"Clase.metodo"` del método `name` visible en `ty`: el de la propia clase, el heredado
    /// más cercano o, si `ty` es una interfaz, el declarado en ella.
    fn find_method(&self, ty: &str, name: &str) -> Option<String> {
        if self.kinds.contains_key(ty) {
            let key = format!("{ty}.{name}");
            return self.funcs.contains_key(&key).then_some(key);
        }
        let mut current = Some(ty.to_string());
        while let Some(class) = current {
            let key = format!("{class}.{name}");
            if self.funcs.contains_key(&key) {
                return Some(key);
            }
            current = self.classes.get(&class).and_then(|i| i.parent.clone());
        }
        None
    }

    /// ¿`sub` es `sup` o hereda de él o lo implementa?
    fn is_sub(&self, sub: &str, sup: &str) -> bool {
        let mut current = Some(sub.to_string());
        while let Some(class) = current {
            if class == sup {
                return true;
            }
            let Some(info) = self.classes.get(&class) else {
                return false;
            };
            if info.kinds.iter().any(|k| k == sup) {
                return true;
            }
            current = info.parent.clone();
        }
        false
    }

    fn assignable(&self, expected: &Type, found: &Type) -> bool {
        if expected == found {
            return true;
        }
        match (expected, found) {
            (Type::Nullable(_), Type::Nil) => true,
            (Type::Nullable(e), Type::Nullable(f)) => self.assignable(e, f),
            (Type::Nullable(e), f) => self.assignable(e, f),
            (Type::Obj(e), Type::Obj(f)) => self.is_sub(f, e),
            _ => false,
        }
    }

    fn expect(&self, expected: &Type, found: &Type, span: Span) -> Result<(), Diagnostic> {
        if self.assignable(expected, found) {
            return Ok(());
        }
        if *found == Type::Nil {
            return Err(Diagnostic::new(
                span,
                format!("`nil` solo cabe en un tipo opcional (`T?`), pero se esperaba {expected}"),
            ));
        }
        let hint = if *expected == Type::Float && *found == Type::Int {
            "; no hay conversión implícita de Int a Float (escribe `1.0` en vez de `1`)".to_string()
        } else if matches!(found, Type::Nullable(inner) if self.assignable(expected, inner)) {
            "; el valor puede ser nil: comprueba `if x != nil` o usa `x ?: otro`".to_string()
        } else {
            String::new()
        };
        Err(Diagnostic::new(
            span,
            format!("se esperaba {expected}, pero es {found}{hint}"),
        ))
    }

    /// Texto de una firma para los mensajes: `(Int, Str) -> Bool`.
    fn sig_text(sig: &Sig) -> String {
        let params: Vec<String> = sig.params.iter().map(|p| p.to_string()).collect();
        format!(
            "({}) -> {}",
            params.join(", "),
            sig.ret.clone().unwrap_or(Type::Unit)
        )
    }

    /// Tras verificar los cuerpos: los métodos que sobrescriben conservan la firma y la clase
    /// implementa todo lo que prometen sus interfaces.
    fn validate_class(&self, decl: &ThingDecl) -> Result<(), Diagnostic> {
        let info = &self.classes[&decl.name];
        if let Some(parent) = &info.parent {
            for method in &decl.methods {
                let Some(inherited) = self.find_method(parent, &method.name) else {
                    continue;
                };
                let own = &self.funcs[&format!("{}.{}", decl.name, method.name)];
                let base = &self.funcs[&inherited];
                if own.params != base.params || own.ret != base.ret {
                    return Err(Diagnostic::new(
                        method.name_span,
                        format!(
                            "el método `{}` sobrescribe el de `{}`, pero su firma {} no coincide con {}",
                            method.name,
                            inherited.split('.').next().unwrap_or(parent),
                            Self::sig_text(own),
                            Self::sig_text(base)
                        ),
                    ));
                }
            }
        }
        for kind in &info.kinds {
            for name in &self.kinds[kind] {
                let wanted = &self.funcs[&format!("{kind}.{name}")];
                let Some(key) = self.find_method(&decl.name, name) else {
                    return Err(Diagnostic::new(
                        decl.name_span,
                        format!(
                            "la clase `{}` no implementa el método `{name}` de la interfaz `{kind}`",
                            decl.name
                        ),
                    ));
                };
                let found = &self.funcs[&key];
                if found.params != wanted.params || found.ret != wanted.ret {
                    return Err(Diagnostic::new(
                        decl.name_span,
                        format!(
                            "el método `{name}` de `{}` tiene la firma {}, pero la interfaz `{kind}` pide {}",
                            decl.name,
                            Self::sig_text(found),
                            Self::sig_text(wanted)
                        ),
                    ));
                }
            }
        }
        Ok(())
    }

    /// Registra los campos, el constructor y las firmas de los métodos de una clase.
    /// Los campos del cuerpo sin anotación llevan `Unit` provisional hasta `ensure_class`.
    fn declare_class(&mut self, decl: &'p ThingDecl) -> Result<(), Diagnostic> {
        let mut parent: Option<String> = None;
        let mut kinds: Vec<String> = Vec::new();
        for sup in &decl.supers {
            let unknown =
                || Diagnostic::new(sup.name_span, format!("el tipo `{}` no existe", sup.name));
            match (
                &sup.args,
                self.classes.contains_key(&sup.name),
                self.kinds.contains_key(&sup.name),
            ) {
                (Some(_), true, _) => {
                    if parent.is_some() {
                        return Err(Diagnostic::new(
                            sup.span,
                            "una clase solo puede heredar de una clase",
                        ));
                    }
                    parent = Some(sup.name.clone());
                }
                (Some(_), _, true) => {
                    return Err(Diagnostic::new(
                        sup.span,
                        format!(
                            "`{}` es una interfaz: se implementa sin paréntesis",
                            sup.name
                        ),
                    ));
                }
                (None, _, true) => {
                    if kinds.contains(&sup.name) {
                        return Err(Diagnostic::new(
                            sup.span,
                            format!("la interfaz `{}` está repetida", sup.name),
                        ));
                    }
                    kinds.push(sup.name.clone());
                }
                (None, true, _) => {
                    return Err(Diagnostic::new(
                        sup.span,
                        format!(
                            "`{0}` es una clase: para heredarla escribe `{0}(argumentos)`",
                            sup.name
                        ),
                    ));
                }
                _ => return Err(unknown()),
            }
        }
        let inherited: Vec<FieldInfo> = match &parent {
            Some(parent) => self.all_fields(parent),
            None => Vec::new(),
        };
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
                if inherited.iter().any(|f| f.name == param.name) {
                    return Err(Diagnostic::new(
                        param.name_span,
                        format!("el campo `{}` ya existe en la clase padre", param.name),
                    ));
                }
                fields.push(FieldInfo {
                    name: param.name.clone(),
                    ty,
                    mutable,
                });
            }
        }
        for field in &decl.fields {
            if fields
                .iter()
                .chain(&inherited)
                .any(|f| f.name == field.name)
            {
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
            if fields
                .iter()
                .chain(&inherited)
                .any(|f| f.name == method.name)
            {
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
        info.parent = parent;
        info.kinds = kinds;
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
                if let Some(parent) = self.classes[name].parent.clone() {
                    self.ensure_class(&parent)?;
                }
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
            scope.insert(param.name.clone(), (ty, false, None));
        }
        let mut ctx = Ctx {
            scopes: vec![scope],
            ret: Ret::Script,
            script_narrow: HashMap::new(),
            this: None,
        };
        // Argumentos para el constructor de la clase padre: ven los parámetros de esta.
        if let Some(sup) = decl.supers.iter().find(|s| s.args.is_some()) {
            let args = sup.args.as_deref().unwrap_or(&[]);
            let wanted = self.classes[&sup.name].ctor.clone();
            if wanted.len() != args.len() {
                return Err(Diagnostic::new(
                    sup.span,
                    format!(
                        "la clase `{}` espera {} argumento(s), pero recibió {}",
                        sup.name,
                        wanted.len(),
                        args.len()
                    ),
                ));
            }
            for (arg, ty) in args.iter().zip(&wanted) {
                self.expr_as(&mut ctx, arg, ty)?;
            }
        }
        let first = decl.params.iter().filter(|p| p.mutable.is_some()).count();
        for (i, field) in decl.fields.iter().enumerate() {
            let ty = match &field.ty {
                Some(ty) => {
                    let expected = self.resolve(ty)?;
                    self.expr_as(&mut ctx, &field.init, &expected)?;
                    expected
                }
                None => {
                    let found = self.expr(&mut ctx, &field.init)?;
                    no_nil(&found, field.init.span())?;
                    found
                }
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
            scope.insert(param.name.clone(), (ty.clone(), false, None));
        }
        let mut ctx = Ctx {
            scopes: vec![scope],
            ret: match sig.ret {
                Some(ty) => Ret::Declared(ty),
                None => Ret::Infer(None),
            },
            script_narrow: HashMap::new(),
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
        if let Some((ty, mutable, _)) = ctx.scopes.iter().rev().find_map(|scope| scope.get(name)) {
            return Some((ty.clone(), *mutable, None));
        }
        if let Some(class) = &ctx.this {
            let fields = self.all_fields(class);
            if let Some(i) = fields.iter().position(|f| f.name == name) {
                let f = &fields[i];
                return Some((f.ty.clone(), f.mutable, Some(i as u32)));
            }
        }
        if let Some(ty) = ctx.script_narrow.get(name) {
            return Some((ty.clone(), false, None));
        }
        self.globals
            .get(name)
            .map(|(ty, mutable, _)| (ty.clone(), *mutable, None))
    }

    /// Como `lookup` pero solo para estrechar: tipo, mutabilidad, tipo declarado si ya estaba
    /// estrechada y si es un local (los campos no se estrechan).
    fn narrowable(&self, ctx: &Ctx, name: &str) -> Option<(Type, bool, Option<Type>, bool)> {
        if let Some((ty, mutable, declared)) =
            ctx.scopes.iter().rev().find_map(|scope| scope.get(name))
        {
            return Some((ty.clone(), *mutable, declared.clone(), true));
        }
        if let Some(class) = &ctx.this {
            if self.all_fields(class).iter().any(|f| f.name == name) {
                return None;
            }
        }
        if ctx.script_narrow.contains_key(name) {
            return None;
        }
        self.globals
            .get(name)
            .map(|(ty, mutable, declared)| (ty.clone(), *mutable, declared.clone(), false))
    }

    /// Variables que `cond` prueba como no nulas cuando es verdadera y cuando es falsa.
    fn facts(cond: &Expr) -> (Vec<String>, Vec<String>) {
        fn nil_test(lhs: &Expr, rhs: &Expr) -> Option<String> {
            match (lhs, rhs) {
                (Expr::Var(name, _), Expr::Nil(_)) | (Expr::Nil(_), Expr::Var(name, _)) => {
                    Some(name.clone())
                }
                _ => None,
            }
        }
        match cond {
            Expr::Binary {
                op: BinOp::Ne,
                lhs,
                rhs,
                ..
            } => (nil_test(lhs, rhs).into_iter().collect(), Vec::new()),
            Expr::Binary {
                op: BinOp::Eq,
                lhs,
                rhs,
                ..
            } => (Vec::new(), nil_test(lhs, rhs).into_iter().collect()),
            Expr::Binary {
                op: BinOp::And,
                lhs,
                rhs,
                ..
            } => {
                let (mut t, _) = Self::facts(lhs);
                t.extend(Self::facts(rhs).0);
                (t, Vec::new())
            }
            Expr::Binary {
                op: BinOp::Or,
                lhs,
                rhs,
                ..
            } => {
                let (_, mut f) = Self::facts(lhs);
                f.extend(Self::facts(rhs).1);
                (Vec::new(), f)
            }
            Expr::Unary {
                op: UnOp::Not,
                expr,
                ..
            } => {
                let (t, f) = Self::facts(expr);
                (f, t)
            }
            _ => (Vec::new(), Vec::new()),
        }
    }

    /// Entradas de ámbito que dejan `names` sin `nil`. Un `mut` solo se estrecha si no se asigna dentro
    /// de un bucle anidado en `body` (en línea recta una asignación basta con volver a ensancharlo).
    fn narrowing(
        &self,
        ctx: &Ctx,
        names: &[String],
        body: Option<&Block>,
    ) -> Vec<(String, VarEntry)> {
        let mut out = Vec::new();
        for name in names {
            let Some((ty, mutable, declared, local)) = self.narrowable(ctx, name) else {
                continue;
            };
            let Type::Nullable(inner) = &ty else { continue };
            if mutable && (!local || body.is_some_and(|b| assigned_in_loop(b, name))) {
                continue;
            }
            out.push((
                name.clone(),
                (
                    (**inner).clone(),
                    mutable,
                    Some(declared.unwrap_or(ty.clone())),
                ),
            ));
        }
        out
    }

    /// Estrecha en el resto del ámbito actual (`if x == nil { return }`).
    fn narrow_here(&self, ctx: &mut Ctx, names: &[String]) {
        for (name, entry) in self.narrowing(ctx, names, None) {
            match ctx.scopes.last_mut() {
                Some(scope) => {
                    scope.insert(name, entry);
                }
                None => {
                    ctx.script_narrow.insert(name, entry.0);
                }
            }
        }
    }

    /// Verifica un bloque con `names` estrechadas dentro de él.
    fn block_narrowed(
        &mut self,
        ctx: &mut Ctx,
        block: &Block,
        names: &[String],
    ) -> Result<(), Diagnostic> {
        let entries = self.narrowing(ctx, names, Some(block));
        // Un ámbito para los estrechamientos y otro para el bloque: así el bloque puede volver a
        // declarar el mismo nombre.
        ctx.scopes.push(entries.into_iter().collect());
        ctx.scopes.push(Vars::new());
        let result = block.stmts.iter().try_for_each(|stmt| self.stmt(ctx, stmt));
        ctx.scopes.pop();
        ctx.scopes.pop();
        result
    }

    /// Tras asignar un valor posiblemente nulo, la variable vuelve a su tipo declarado.
    fn widen(ctx: &mut Ctx, name: &str) {
        for scope in ctx.scopes.iter_mut().rev() {
            if let Some(entry) = scope.get_mut(name) {
                if let Some(declared) = entry.2.clone() {
                    entry.0 = declared;
                }
                return;
            }
        }
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
            return Err(Diagnostic::new(target_span, not_object(ty, "campos")));
        };
        if self.kinds.contains_key(class) {
            return Err(Diagnostic::new(
                target_span,
                format!("`{class}` es una interfaz y no tiene campos"),
            ));
        }
        let fields = self.all_fields(class);
        match fields.iter().position(|f| f.name == name) {
            Some(i) => Ok((i as u32, fields[i].ty.clone(), fields[i].mutable)),
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
        scope.insert(name.to_string(), (info.0, info.1, None));
        Ok(())
    }

    /// Tipo de `expr` exigiendo `expected`. Un arreglo vacío `[]` toma el tipo esperado.
    fn expr_as(&mut self, ctx: &mut Ctx, expr: &Expr, expected: &Type) -> Result<(), Diagnostic> {
        if let (Expr::Array { elems, .. }, Type::Array(elem)) = (expr, expected) {
            if elems.is_empty() {
                return Ok(());
            }
            // Un arreglo de objetos acepta elementos de subclases: `[Perro(..), Gato(..)]` como `[Animal]`.
            if matches!(**elem, Type::Obj(_) | Type::Nullable(_)) {
                return elems.iter().try_for_each(|e| self.expr_as(ctx, e, elem));
            }
        }
        let found = self.expr(ctx, expr)?;
        self.expect(expected, &found, expr.span())
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
                    None => {
                        let found = self.expr(ctx, init)?;
                        no_nil(&found, init.span())?;
                        found
                    }
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
                // Una variable estrechada acepta lo que admite su tipo declarado; si el valor puede
                // ser nil, deja de estar estrechada.
                let declared = ctx
                    .scopes
                    .iter()
                    .rev()
                    .find_map(|scope| scope.get(name.as_str()))
                    .and_then(|entry| entry.2.clone());
                match (&declared, op) {
                    (Some(declared), None) => {
                        self.assigned(ctx, declared, None, value, *span)?;
                        let may_be_nil = !matches!(
                            self.expr(ctx, value),
                            Ok(found) if !matches!(found, Type::Nullable(_) | Type::Nil)
                        );
                        if may_be_nil && !matches!(value, Expr::Array { .. }) {
                            Self::widen(ctx, name);
                        }
                    }
                    _ => self.assigned(ctx, &ty, *op, value, *span)?,
                }
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
                let (when_true, when_false) = Self::facts(cond);
                self.block_narrowed(ctx, then, &when_true)?;
                if let Some(otherwise) = otherwise {
                    self.block_narrowed(ctx, otherwise, &when_false)?;
                }
                // `if x == nil { return }`: después del `if`, `x` no es nil.
                if exits(then) {
                    self.narrow_here(ctx, &when_false);
                }
                if otherwise.as_ref().is_some_and(exits) {
                    self.narrow_here(ctx, &when_true);
                }
            }
            Stmt::While { cond, body, .. } => {
                self.condition(ctx, cond)?;
                let (when_true, _) = Self::facts(cond);
                self.block_narrowed(ctx, body, &when_true)?;
            }
            Stmt::Fail(value, _) => {
                let found = self.expr(ctx, value)?;
                match &found {
                    Type::Obj(class) if self.is_sub(class, "Fallo") => {}
                    other => {
                        return Err(Diagnostic::new(
                            value.span(),
                            format!(
                                "solo se pueden lanzar objetos de clases que hereden de `Fallo`, pero esto es {other}"
                            ),
                        ));
                    }
                }
            }
            Stmt::Attempt { body, rescues, .. } => {
                self.block(ctx, body)?;
                for rescue in rescues {
                    let ty = self.resolve(&rescue.ty)?;
                    let Type::Obj(class) = &ty else {
                        return Err(Diagnostic::new(
                            rescue.ty.span,
                            format!("`rescue` necesita una clase que herede de `Fallo`, pero esto es {ty}"),
                        ));
                    };
                    if !self.is_sub(class, "Fallo") {
                        return Err(Diagnostic::new(
                            rescue.ty.span,
                            format!("solo se pueden rescatar clases que hereden de `Fallo`; `{class}` no lo hace"),
                        ));
                    }
                    let mut scope = Vars::new();
                    scope.insert(rescue.name.clone(), (ty.clone(), false, None));
                    ctx.scopes.push(scope);
                    let result = self.block(ctx, &rescue.body);
                    ctx.scopes.pop();
                    result?;
                }
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
                        no_nil(&found, expr.span())?;
                        if let Ret::Infer(slot) = &mut ctx.ret {
                            *slot = Some(found);
                        }
                    }
                    (None, Some(expected)) => self.expect(&expected, &Type::Unit, *span)?,
                    (None, None) => {
                        if let Ret::Infer(slot) = &mut ctx.ret {
                            *slot = Some(Type::Unit);
                        }
                    }
                }
            }
            Stmt::Break(_) | Stmt::Continue(_) | Stmt::Fn(_) | Stmt::Thing(_) | Stmt::Kind(_) => {}
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
                self.expect(ty, &result, value.span())
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
                        not_object(&target_ty, "métodos"),
                    ));
                };
                let Some(key) = self.find_method(class, name) else {
                    let what = if self.kinds.contains_key(class) {
                        "la interfaz"
                    } else {
                        "la clase"
                    };
                    return Err(Diagnostic::new(
                        *name_span,
                        format!("{what} `{class}` no tiene el método `{name}`"),
                    ));
                };
                self.res.virtuals.insert(*span, name.clone());
                self.call_sig(ctx, &key, name, "el método", args, *span)
            }
            Expr::SuperCall {
                name,
                name_span,
                args,
                span,
            } => {
                let parent = ctx
                    .this
                    .as_ref()
                    .and_then(|class| self.classes[class].parent.clone());
                let Some(parent) = parent else {
                    return Err(Diagnostic::new(
                        *span,
                        "`super` solo se puede usar en un método de una clase que hereda de otra",
                    ));
                };
                let Some(key) = self.find_method(&parent, name) else {
                    return Err(Diagnostic::new(
                        *name_span,
                        format!("la clase padre `{parent}` no tiene el método `{name}`"),
                    ));
                };
                self.res.methods.insert(*span, key.clone());
                self.call_sig(ctx, &key, name, "el método", args, *span)
            }
            Expr::Is { expr, ty, .. } => {
                self.type_test(ctx, expr, ty, "is")?;
                Ok(Type::Bool)
            }
            Expr::Cast { expr, ty, .. } => self.type_test(ctx, expr, ty, "as"),
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
                no_nil(&elem, first.span())?;
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
                // `x != nil && x.f`: el lado derecho ve lo que el izquierdo probó.
                let proven = match op {
                    BinOp::And => Self::facts(lhs).0,
                    BinOp::Or => Self::facts(lhs).1,
                    _ => Vec::new(),
                };
                let entries = self.narrowing(ctx, &proven, None);
                ctx.scopes.push(entries.into_iter().collect());
                let b = self.expr(ctx, rhs);
                ctx.scopes.pop();
                binary_type(*op, &a, &b?, *span)
            }
            Expr::Nil(_) => Ok(Type::Nil),
            Expr::Elvis { lhs, rhs, span } => {
                let found = self.expr(ctx, lhs)?;
                let Type::Nullable(inner) = found else {
                    return Err(Diagnostic::new(
                        lhs.span(),
                        format!("el operador `?:` necesita un valor opcional (`T?`), pero esto es {found}"),
                    ));
                };
                let other = self.expr(ctx, rhs)?;
                if self.assignable(&inner, &other) {
                    Ok(*inner)
                } else if matches!(&other, Type::Nullable(o) if self.assignable(&inner, o))
                    || other == Type::Nil
                {
                    Ok(Type::Nullable(inner))
                } else {
                    let _ = span;
                    self.expect(&inner, &other, rhs.span())?;
                    Ok(*inner)
                }
            }
            Expr::SafeField {
                target,
                name,
                name_span,
                span,
            } => {
                let target_ty = self.expr(ctx, target)?;
                let inner = match target_ty {
                    Type::Nullable(inner) => *inner,
                    other => other,
                };
                self.ensure_class_of(&inner)?;
                let (i, ty, _) = self.field_of(&inner, name, *name_span, target.span())?;
                self.res.fields.insert(*span, i);
                Ok(optional(ty))
            }
            Expr::SafeCall {
                target,
                name,
                name_span,
                args,
                span,
            } => {
                let target_ty = self.expr(ctx, target)?;
                let inner = match target_ty {
                    Type::Nullable(inner) => *inner,
                    other => other,
                };
                let Type::Obj(class) = &inner else {
                    return Err(Diagnostic::new(
                        target.span(),
                        not_object(&inner, "métodos"),
                    ));
                };
                let Some(key) = self.find_method(class, name) else {
                    return Err(Diagnostic::new(
                        *name_span,
                        format!("`{class}` no tiene el método `{name}`"),
                    ));
                };
                self.res.virtuals.insert(*span, name.clone());
                let ret = self.call_sig(ctx, &key, name, "el método", args, *span)?;
                Ok(if ret == Type::Unit {
                    ret
                } else {
                    optional(ret)
                })
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

    /// `expr is T` / `expr as T`: ambos lados deben ser objetos y estar emparentados.
    fn type_test(
        &mut self,
        ctx: &mut Ctx,
        expr: &Expr,
        ty: &TypeRef,
        op: &str,
    ) -> Result<Type, Diagnostic> {
        let found = self.expr(ctx, expr)?;
        let target = self.resolve(ty)?;
        let (Type::Obj(from), Type::Obj(to)) = (&found, &target) else {
            let bad = if matches!(found, Type::Obj(_)) {
                (ty.span, target.to_string(), "un objeto")
            } else {
                (expr.span(), found.to_string(), "un objeto")
            };
            return Err(Diagnostic::new(
                bad.0,
                format!("`{op}` compara {}, pero {} no lo es", bad.2, bad.1),
            ));
        };
        // Dos clases sin relación nunca coinciden; con una interfaz de por medio sí puede.
        let unrelated = self.classes.contains_key(from)
            && self.classes.contains_key(to)
            && !self.is_sub(from, to)
            && !self.is_sub(to, from);
        if unrelated {
            return Err(Diagnostic::new(
                ty.span,
                format!("un `{from}` nunca puede ser un `{to}`: no están emparentadas"),
            ));
        }
        Ok(target)
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
        if self.kinds.contains_key(name) {
            return Err(Diagnostic::new(
                span,
                format!("`{name}` es una interfaz y no se puede instanciar"),
            ));
        }
        if let Some(class) = ctx.this.clone() {
            if let Some(key) = self.find_method(&class, name) {
                self.res.virtuals.insert(span, name.to_string());
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
                    "el tipo `{}` no existe (hay Int, Float, Bool, Str, Unit, [T], las clases y las interfaces declaradas)",
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
            ("Nullable", [inner]) => {
                let inner_ty = self.resolve(inner)?;
                if matches!(inner_ty, Type::Unit | Type::Nullable(_)) {
                    return Err(Diagnostic::new(
                        ty.span,
                        format!("`{inner_ty}?` no tiene sentido: un tipo opcional no puede ser Unit ni otro opcional"),
                    ));
                }
                Ok(Type::Nullable(Box::new(inner_ty)))
            }
            (name, []) if self.classes.contains_key(name) || self.kinds.contains_key(name) => {
                Ok(Type::Obj(name.to_string()))
            }
            _ => Err(unknown()),
        }
    }
}

/// Mensaje cuando se usa `.` sobre algo que no es un objeto (con pista si es un opcional).
fn not_object(ty: &Type, what: &str) -> String {
    match ty {
        Type::Nullable(inner) => format!(
            "este valor es {ty} y puede ser nil: usa `?.`, `?:` o comprueba `if x != nil` antes de acceder a los {what} de {inner}"
        ),
        other => format!("solo los objetos tienen {what}, pero esto es {other}"),
    }
}

/// `nil` no dice de qué tipo es: no se puede deducir el tipo de una variable, arreglo o campo.
fn no_nil(ty: &Type, span: Span) -> Result<(), Diagnostic> {
    if *ty == Type::Nil {
        return Err(Diagnostic::new(
            span,
            "no se puede deducir el tipo de `nil`; anótalo, por ejemplo `fix x: Perro? = nil`",
        ));
    }
    Ok(())
}

/// Un `T` visto como opcional (`T?`); un opcional se queda como está.
fn optional(ty: Type) -> Type {
    match ty {
        Type::Nullable(_) => ty,
        other => Type::Nullable(Box::new(other)),
    }
}

/// ¿El bloque siempre sale (`return`, `fail`, `break` o `continue`) antes de terminar?
fn exits(block: &Block) -> bool {
    block.stmts.iter().any(|stmt| match stmt {
        Stmt::Return(..) | Stmt::Fail(..) | Stmt::Break(_) | Stmt::Continue(_) => true,
        Stmt::If {
            then,
            otherwise: Some(otherwise),
            ..
        } => exits(then) && exits(otherwise),
        _ => false,
    })
}

/// ¿Se asigna `name` dentro de algún `while` anidado en `block`?
fn assigned_in_loop(block: &Block, name: &str) -> bool {
    block.stmts.iter().any(|stmt| match stmt {
        Stmt::While { body, .. } => assigns(body, name),
        Stmt::If {
            then, otherwise, ..
        } => {
            assigned_in_loop(then, name)
                || otherwise
                    .as_ref()
                    .is_some_and(|b| assigned_in_loop(b, name))
        }
        Stmt::Attempt { body, rescues, .. } => {
            assigned_in_loop(body, name) || rescues.iter().any(|r| assigned_in_loop(&r.body, name))
        }
        _ => false,
    })
}

/// ¿Se asigna `name` en algún punto de `block`, a cualquier profundidad?
fn assigns(block: &Block, name: &str) -> bool {
    block.stmts.iter().any(|stmt| match stmt {
        Stmt::Assign { name: n, .. } => n == name,
        Stmt::While { body, .. } => assigns(body, name),
        Stmt::If {
            then, otherwise, ..
        } => assigns(then, name) || otherwise.as_ref().is_some_and(|b| assigns(b, name)),
        Stmt::Attempt { body, rescues, .. } => {
            assigns(body, name) || rescues.iter().any(|r| assigns(&r.body, name))
        }
        _ => false,
    })
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
        BinOp::Eq | BinOp::Ne
            if matches!(
                (a, b),
                (Type::Nil, Type::Nullable(_) | Type::Nil) | (Type::Nullable(_), Type::Nil)
            ) =>
        {
            Ok(Type::Bool)
        }
        BinOp::Eq | BinOp::Ne if *a == Type::Nil || *b == Type::Nil => Err(invalid(
            "; ese valor nunca es nil, así que no hace falta compararlo",
        )),
        BinOp::Eq | BinOp::Ne => {
            let comparable = !matches!(a, Type::Array(_) | Type::Obj(_) | Type::Nullable(_));
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
        Stmt::Return(..) | Stmt::Fail(..) => true,
        Stmt::Attempt { body, rescues, .. } => {
            block_returns(body) && rescues.iter().all(|r| block_returns(&r.body))
        }
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
        // Como en una sesión, el preludio (la clase `Fallo`) se verifica antes.
        let mut checker = Checker::new();
        checker
            .check(&parse(and23_syntax::PRELUDE).map_err(|d| d.message)?)
            .map_err(|d| d.message)?;
        checker
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
        assert_eq!(res.virtuals.len(), 1);
        assert_eq!(res.virtuals.values().next().unwrap(), "f");
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

    // --- M5: herencia, interfaces, is/as/super ---

    #[test]
    fn herencia_e_interfaces_bien_tipadas() {
        for src in [
            "thing A(fix x: Int)\nthing B() : A(1)\nfix a: A = B()\nshow(a.x)",
            "thing B() : A(1)\nthing A(fix x: Int)\nshow(B().x)",
            "kind K { fn f() -> Int }\nthing A() : K { fn f() -> Int = 1 }\nfix k: K = A()\nshow(k.f())",
            "thing A() { fn f() -> Int = 1 }\nthing B() : A() { fn f() -> Int = super.f() + 1 }",
            "thing A()\nthing B() : A()\nfix xs: [A] = [A(), B()]\nshow(xs[1] is B)",
            "thing A()\nthing B() : A()\nfix x: A = B()\nfix b = x as B\nshow(b is A)",
            "kind K { fn f() }\nthing A() : K { fn f() { } }\nfix x: K = A()\nshow(x is A)",
            "thing A(fix n: Int)\nthing B(m: Int) : A(m + 1) { fix k = m }",
        ] {
            assert_eq!(check(src), Ok(()), "{src}");
        }
    }

    #[test]
    fn errores_de_herencia_e_interfaces() {
        let cases = [
            ("thing A() : B()\nthing B() : A()", "es circular"),
            ("thing A() : A()", "es circular"),
            ("thing A() : Nada()", "no existe"),
            (
                "thing A(fix x: Int)\nthing B() : A",
                "es una clase: para heredarla escribe `A(argumentos)`",
            ),
            (
                "kind K { }\nthing B() : K()",
                "es una interfaz: se implementa sin paréntesis",
            ),
            (
                "thing A()\nthing B()\nthing C() : A(), B()",
                "solo puede heredar de una clase",
            ),
            ("kind K { }\nthing A() : K, K", "está repetida"),
            (
                "thing A(fix x: Int)\nthing B() : A()",
                "espera 1 argumento(s), pero recibió 0",
            ),
            (
                "thing A(fix x: Int)\nthing B() : A(true)",
                "se esperaba Int, pero es Bool",
            ),
            (
                "thing A(fix x: Int)\nthing B(fix x: Int) : A(1)",
                "ya existe en la clase padre",
            ),
            (
                "thing A(fix x: Int)\nthing B() : A(1) { fix x = 2 }",
                "el campo `x` está repetido",
            ),
            (
                "thing A(fix x: Int)\nthing B() : A(1) { fn x() { } }",
                "ya es un campo",
            ),
            (
                "kind K { fn f() -> Int }\nthing A() : K",
                "no implementa el método `f` de la interfaz `K`",
            ),
            (
                "kind K { fn f() -> Int }\nthing A() : K { fn f() -> Str = \"\" }",
                "pide () -> Int",
            ),
            (
                "kind K { fn f(a: Int) }\nthing A() : K { fn f(a: Str) { } }",
                "pide (Int) -> Unit",
            ),
            (
                "thing A() { fn f() -> Int = 1 }\nthing B() : A() { fn f() -> Str = \"\" }",
                "sobrescribe el de `A`",
            ),
            (
                "thing A() { fn f(x: Int) { } }\nthing B() : A() { fn f() { } }",
                "no coincide con (Int) -> Unit",
            ),
            (
                "kind K { fn f() }\nshow(K())",
                "es una interfaz y no se puede instanciar",
            ),
            (
                "kind K { fn f() }\nfix k: K = 1",
                "se esperaba K, pero es Int",
            ),
            (
                "kind K { fn f() }\nthing A() : K { fn f() { } }\nfix x: K = A()\nx.zz()",
                "la interfaz `K` no tiene el método `zz`",
            ),
            (
                "kind K { fn f() }\nthing A() : K { fn f() { } }\nfix x: K = A()\nshow(x.q)",
                "es una interfaz y no tiene campos",
            ),
            ("kind K { fn f() }\nkind K { fn g() }", "ya está en uso"),
            ("kind K { fn f()\n fn f() }", "está repetido"),
            (
                "thing A()\nthing B() : A()\nfix b: B = A()",
                "se esperaba B, pero es A",
            ),
            (
                "thing A()\nthing B() : A()\nfix xs: [A] = [B()]\nfix ys: [B] = xs",
                "se esperaba [B], pero es [A]",
            ),
            (
                "thing A() { fn f() { super.f() } }",
                "`super` solo se puede usar",
            ),
            (
                "thing A()\nthing B() : A() { fn f() { super.zz() } }",
                "la clase padre `A` no tiene el método `zz`",
            ),
            (
                "thing A()\nthing B()\nfix a = A()\nshow(a is B)",
                "nunca puede ser un `B`",
            ),
            ("fix n = 1\nshow(n is Int)", "compara un objeto"),
            (
                "thing A()\nfix a = A()\nshow(a is Int)",
                "compara un objeto",
            ),
            ("thing A()\nshow(A() == A())", "no admite A y A"),
        ];
        for (src, expected) in cases {
            let message = err(src);
            assert!(
                message.contains(expected),
                "{src}: se esperaba «{expected}», pero fue «{message}»"
            );
        }
    }

    #[test]
    fn las_llamadas_a_metodos_son_virtuales_salvo_super() {
        let src = "thing A() { fn f() -> Int = 1 }\nthing B() : A() { fn f() -> Int = super.f() }\nB().f()";
        let program = parse(src).unwrap();
        let res = Checker::new().check(&program).unwrap();
        assert_eq!(res.methods.values().collect::<Vec<_>>(), vec!["A.f"]);
        assert_eq!(res.virtuals.values().collect::<Vec<_>>(), vec!["f"]);
    }

    #[test]
    fn los_campos_heredados_van_antes_en_los_indices() {
        let src = "thing A(fix a: Int, fix b: Int)\nthing B() : A(1, 2) { mut c = 3 }\nfix x = B()\nshow(x.a)\nshow(x.b)\nshow(x.c)";
        let program = parse(src).unwrap();
        let res = Checker::new().check(&program).unwrap();
        let mut fields: Vec<u32> = res.fields.values().copied().collect();
        fields.sort();
        assert_eq!(fields, vec![0, 1, 2]);
    }

    // --- M6: excepciones y opcionales ---

    /// `check` ya incluye el preludio con `Fallo`.
    const FALLO: &str = "";

    fn check_con_fallo(src: &str) -> Result<(), String> {
        check(&format!("{FALLO}{src}"))
    }

    fn err_con_fallo(src: &str) -> String {
        err(&format!("{FALLO}{src}"))
    }

    #[test]
    fn excepciones_y_opcionales_bien_tipados() {
        for src in [
            "thing E() : Fallo(\"x\")\nattempt { fail E() }\nrescue e: E { show(e.msg) }",
            "attempt { fail Fallo(\"a\") }\nrescue e: Fallo { show(e) }\nrescue e: Fallo { show(1) }",
            "fn f() -> Int {\n attempt { return 1 }\n rescue e: Fallo { return 2 }\n}",
            "fn f() -> Int { fail Fallo(\"x\") }",
            "fix a: Int? = nil\nfix b: Int? = 3\nshow(a ?: b ?: 0)",
            "thing P(fix n: Int)\nfix p: P? = nil\nshow(p?.n)\nshow(p ?: P(1))",
            "thing P(fix n: Int)\nfn f(p: P?) -> Int {\n if p == nil { return 0 }\n return p.n\n}",
            "thing P(fix n: Int)\nfn f(p: P?) -> Bool = p != nil && p.n > 1",
            "thing P(fix n: Int)\nfn f(p: P?) -> Bool = p == nil || p.n > 1",
            "thing P(fix n: Int)\nfn f(p: P?) -> Int {\n if p != nil { return p.n } else { return 0 }\n}",
            "thing N(fix v: Int, mut s: N?)\nfn f(c: N?) -> Int {\n mut x = c\n mut t = 0\n while x != nil {\n t += x.v\n x = x.s\n }\n return t\n}",
            "fix a: [Int?] = [1, nil]\nshow(a[0] ?: 2)",
            "fn f() -> Int? = nil\nshow(f() == nil)",
        ] {
            assert_eq!(check_con_fallo(src), Ok(()), "{src}");
        }
    }

    #[test]
    fn errores_de_opcionales() {
        let pre = "thing P(fix n: Int) { fn m() -> Int = n }\nfix p: P? = nil\n";
        let cases = [
            (
                "show(p.n)",
                "puede ser nil: usa `?.`, `?:` o comprueba `if x != nil`",
            ),
            ("p.m()", "puede ser nil"),
            ("p.n = 1", "puede ser nil"),
            ("fix q: P = nil", "`nil` solo cabe en un tipo opcional"),
            ("fix q: P = p", "el valor puede ser nil"),
            ("fix x = nil", "no se puede deducir el tipo de `nil`"),
            ("fix xs = [nil]", "no se puede deducir el tipo de `nil`"),
            ("fix n: Int = 1\nshow(n ?: 2)", "necesita un valor opcional"),
            ("fix n: Int = 1\nshow(n == nil)", "nunca es nil"),
            (
                "fn f(a: Int) { }\nfix x: Int? = 1\nf(x)",
                "se esperaba Int, pero es Int?",
            ),
            ("fix x: Int? = 1\nshow(x + 1)", "no admite Int? y Int"),
            (
                "fix x: Int? = 1\nfix y: Int? = 2\nshow(x == y)",
                "no admite Int? y Int?",
            ),
            ("fix x: Int?? = 1", "no tiene sentido"),
            ("fix x: Unit? = nil", "no tiene sentido"),
            ("fix x: Cosa? = nil", "el tipo `Cosa` no existe"),
            (
                "fix x: Int? = 1\nshow(x ?: \"a\")",
                "se esperaba Int, pero es Str",
            ),
            (
                "fn f() -> Int { return nil }",
                "`nil` solo cabe en un tipo opcional",
            ),
            (
                "fix n: Int = 5\nshow(n?.x)",
                "solo los objetos tienen campos",
            ),
            ("show(p?.zz)", "no tiene el campo `zz`"),
            ("show(p?.zz())", "no tiene el método `zz`"),
            ("fix x: Int? = 1\nif x { }", "la condición debe ser Bool"),
        ];
        for (tail, expected) in cases {
            let message = err_con_fallo(&format!("{pre}{tail}"));
            assert!(
                message.contains(expected),
                "{tail}: se esperaba «{expected}», pero fue «{message}»"
            );
        }
    }

    #[test]
    fn el_estrechamiento_tiene_limites_claros() {
        let pre = "thing P(fix n: Int)\n";
        let cases = [
            // no se estrecha sin comprobar, ni fuera del bloque
            ("fn f(p: P?) -> Int {\n if p != nil { show(1) }\n return p.n\n}", "puede ser nil"),
            // `if p == nil { show(1) }` no sale: no estrecha
            ("fn f(p: P?) -> Int {\n if p == nil { show(1) }\n return p.n\n}", "puede ser nil"),
            // el else de `!=` es el caso nil
            ("fn f(p: P?) -> Int {\n if p != nil { return 1 } else { return p.n }\n}", "puede ser nil"),
            // una asignación de nil deshace el estrechamiento
            ("fn f(p: P?) -> Int {\n mut q = p\n if q != nil {\n q = nil\n return q.n\n }\n return 0\n}", "puede ser nil"),
            // un `mut` asignado dentro de un bucle anidado no se estrecha
            ("fn f(p: P?) -> Int {\n mut q = p\n if q != nil {\n while true { show(q.n)\n q = nil }\n }\n return 0\n}", "puede ser nil"),
            // las globales `mut` no se estrechan (una función podría cambiarlas)
            ("mut g: P? = nil\nfn f() -> Int {\n if g != nil { return g.n }\n return 0\n}", "puede ser nil"),
            // `||` y `&&` solo estrechan al lado derecho
            ("fn f(p: P?) -> Bool = p != nil || p.n > 1", "puede ser nil"),
            ("fn f(p: P?) -> Bool = p == nil && p.n > 1", "puede ser nil"),
            // los campos no se estrechan
            ("thing Q(fix p: P?) { fn f() -> Int {\n if p != nil { return p.n }\n return 0\n} }", "puede ser nil"),
            // una variable distinta con el mismo nombre en un bloque interior no hereda el estrechamiento
            ("fn f(p: P?) -> Int {\n if p != nil {\n fix p: P? = nil\n return p.n\n }\n return 0\n}", "puede ser nil"),
        ];
        for (tail, expected) in cases {
            let message = err_con_fallo(&format!("{pre}{tail}"));
            assert!(
                message.contains(expected),
                "{tail}: se esperaba «{expected}», pero fue «{message}»"
            );
        }
    }

    #[test]
    fn el_estrechamiento_de_globales_fix_funciona_en_el_nivel_superior() {
        for src in [
            "thing P(fix n: Int)\nfix p: P? = P(1)\nif p != nil { show(p.n) }",
            "thing P(fix n: Int)\nfix p: P? = P(1)\nif p == nil { fail Fallo(\"x\") }\nshow(p.n)",
            "thing P(fix n: Int)\nfix p: P? = P(1)\nfn f() -> Int {\n if p != nil { return p.n }\n return 0\n}",
        ] {
            assert_eq!(check_con_fallo(src), Ok(()), "{src}");
        }
        // pero el estrechamiento del nivel superior no se filtra a las funciones
        let message = err_con_fallo(
            "thing P(fix n: Int)\nfix p: P? = P(1)\nif p == nil { fail Fallo(\"x\") }\nfn f() -> Int = p.n",
        );
        assert!(message.contains("puede ser nil"), "{message}");
    }

    #[test]
    fn errores_de_excepciones() {
        let cases = [
            (
                "fail 1",
                "solo se pueden lanzar objetos de clases que hereden de `Fallo`, pero esto es Int",
            ),
            ("thing X()\nfail X()", "que hereden de `Fallo`"),
            (
                "kind K { }\nthing X() : K\nfail X()",
                "que hereden de `Fallo`",
            ),
            (
                "thing X()\nattempt { }\nrescue e: X { }",
                "solo se pueden rescatar clases que hereden de `Fallo`",
            ),
            (
                "attempt { }\nrescue e: Int { }",
                "necesita una clase que herede de `Fallo`",
            ),
            (
                "attempt { }\nrescue e: Nada { }",
                "el tipo `Nada` no existe",
            ),
            (
                "attempt { show(zz) }\nrescue e: Fallo { }",
                "la variable `zz` no está definida",
            ),
            (
                "attempt { }\nrescue e: Fallo { show(zz) }",
                "la variable `zz` no está definida",
            ),
            (
                "attempt { }\nrescue e: Fallo { show(e.otro) }",
                "no tiene el campo `otro`",
            ),
            ("thing Fallo()", "ya está en uso"),
            (
                "fn f() -> Int {\n attempt { return 1 }\n rescue e: Fallo { show(1) }\n}",
                "puede terminar sin devolver",
            ),
            (
                "fn f() -> Int {\n attempt { show(1) }\n rescue e: Fallo { return 1 }\n}",
                "puede terminar sin devolver",
            ),
        ];
        for (src, expected) in cases {
            let message = err_con_fallo(src);
            assert!(
                message.contains(expected),
                "{src}: se esperaba «{expected}», pero fue «{message}»"
            );
        }
    }

    #[test]
    fn el_tipo_de_la_variable_del_rescue_es_el_de_la_clausula() {
        assert_eq!(
            check_con_fallo(
                "thing E(fix c: Int) : Fallo(\"x\")\nattempt { }\nrescue e: E { show(e.c + 1) }"
            ),
            Ok(())
        );
        let message = err_con_fallo(
            "thing E(fix c: Int) : Fallo(\"x\")\nattempt { }\nrescue e: Fallo { show(e.c) }",
        );
        assert!(message.contains("no tiene el campo `c`"), "{message}");
    }
}
