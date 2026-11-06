//! Verificador de tipos estáticos de And23. Se ejecuta entre el parser y el compilador:
//! un programa mal tipado se rechaza con su posición antes de generar bytecode.

use and23_syntax::{
    BinOp, Block, Diagnostic, Expr, FnDecl, InterpPart, KindDecl, LambdaParam, Program, Span, Stmt,
    ThingDecl, TypeParam, TypeRef, UnOp,
};
use std::collections::{HashMap, HashSet};
use std::fmt;

/// Funciones integradas; no se pueden redefinir.
pub const BUILTINS: [&str; 4] = ["len", "push", "pop", "hash"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Type {
    Int,
    Float,
    Bool,
    Str,
    Unit,
    Array(Box<Type>),
    /// Instancia de una clase (`thing`) o valor cuyo tipo es una interfaz (`kind`).
    Obj(String, Vec<Type>),
    /// Un parámetro de tipo (`T`) dentro de la declaración genérica que lo introduce.
    Param(String),
    /// `T?`: un `T` o `nil`.
    Nullable(Box<Type>),
    /// Función o lambda: `(Int, Str) -> Bool`.
    Fn(Vec<Type>, Box<Type>),
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
            Type::Obj(name, args) if args.is_empty() => f.write_str(name),
            Type::Obj(name, args) => {
                let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
                write!(f, "{name}<{}>", args.join(", "))
            }
            Type::Param(name) => f.write_str(name),
            Type::Nullable(inner) if matches!(**inner, Type::Fn(..)) => write!(f, "({inner})?"),
            Type::Nullable(inner) => write!(f, "{inner}?"),
            Type::Fn(params, ret) => {
                let params: Vec<String> = params.iter().map(|p| p.to_string()).collect();
                write!(f, "({}) -> {ret}", params.join(", "))
            }
            Type::Nil => f.write_str("nil"),
        }
    }
}

/// Parámetro de tipo con su cota (`T: Interfaz`).
#[derive(Clone)]
struct TParam {
    name: String,
    bound: Option<Type>,
}

/// Qué tipo concreto toma cada parámetro de tipo.
type TMap = HashMap<String, Type>;

/// Una interfaz: sus parámetros de tipo y los nombres de sus métodos (las firmas están en `funcs`
/// como `"Interfaz.metodo"`, escritas con los parámetros de la interfaz).
#[derive(Clone, Default)]
struct KindInfo {
    tparams: Vec<TParam>,
    methods: Vec<String>,
}

#[derive(Clone)]
struct Sig {
    /// Solo las funciones del nivel superior tienen parámetros de tipo propios.
    tparams: Vec<TParam>,
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
    tparams: Vec<TParam>,
    /// Solo los campos propios; los heredados están en la clase padre. Sus tipos usan los
    /// parámetros de tipo de esta clase.
    fields: Vec<FieldInfo>,
    ctor: Vec<Type>,
    /// Clase padre con sus argumentos de tipo (escritos con los parámetros de esta clase).
    parent: Option<(String, Vec<Type>)>,
    /// Interfaces que declara implementar, con sus argumentos de tipo.
    kinds: Vec<(String, Vec<Type>)>,
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
    /// Posición de una llamada `f(x)` donde `f` es una variable o campo de tipo función (no una
    /// función con nombre). Si `f` es un campo, `fields` trae su índice bajo la misma posición.
    pub valuecalls: HashSet<Span>,
    /// Posición de la última expresión de una lambda cuyo valor se descarta (la lambda devuelve Unit).
    pub discards: HashSet<Span>,
}

/// Verificador con estado: recuerda las funciones y globales de las entradas anteriores (REPL).
#[derive(Clone, Default)]
pub struct Checker {
    funcs: HashMap<String, Sig>,
    classes: HashMap<String, ClassInfo>,
    kinds: HashMap<String, KindInfo>,
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
            scope_params: Vec::new(),
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
    /// Índice (en `scopes`) del ámbito de parámetros de cada lambda abierta.
    lambda_marks: Vec<usize>,
}

struct Pass<'p> {
    funcs: HashMap<String, Sig>,
    classes: HashMap<String, ClassInfo>,
    kinds: HashMap<String, KindInfo>,
    globals: Vars,
    /// Parámetros de tipo a la vista (los de la clase o función que se está declarando o verificando).
    scope_params: Vec<TParam>,
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
const RESERVED_TYPES: [&str; 7] = ["Int", "Float", "Bool", "Str", "Unit", "Array", "Fn"];

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
                    self.classes.insert(
                        name.clone(),
                        ClassInfo {
                            tparams: unbounded(&decl.type_params),
                            ..ClassInfo::default()
                        },
                    );
                    self.things.insert(name.clone(), decl);
                    self.class_state.insert(name.clone(), State::Unchecked);
                    self.declared.insert(name.clone(), State::Unchecked);
                }
                Stmt::Kind(decl) => {
                    self.kinds.insert(
                        name.clone(),
                        KindInfo {
                            tparams: unbounded(&decl.type_params),
                            methods: Vec::new(),
                        },
                    );
                }
                _ => {}
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
            lambda_marks: Vec::new(),
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
    fn signature(&mut self, decl: &FnDecl) -> Result<Sig, Diagnostic> {
        let saved = self.scope_params.clone();
        // Un método ve los parámetros de su clase; una función, solo los suyos.
        let tparams = if decl.type_params.is_empty() {
            Vec::new()
        } else {
            let base = self.scope_params.clone();
            if let Some(dup) = decl
                .type_params
                .iter()
                .find(|d| base.iter().any(|b| b.name == d.name))
            {
                return Err(Diagnostic::new(
                    dup.span,
                    format!("`{}` ya es un parámetro de tipo de la clase", dup.name),
                ));
            }
            let own = self.enter_params(&decl.type_params)?;
            let mut all = base;
            all.extend(own.iter().cloned());
            self.scope_params = all;
            own
        };
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
        self.scope_params = saved;
        Ok(Sig {
            tparams,
            params,
            ret,
        })
    }

    /// Pone `decls` a la vista como parámetros de tipo (con sus cotas ya resueltas) y los devuelve.
    /// El llamador guarda y restaura `scope_params`.
    fn enter_params(&mut self, decls: &[TypeParam]) -> Result<Vec<TParam>, Diagnostic> {
        self.scope_params = unbounded(decls);
        let mut out = Vec::new();
        for decl in decls {
            let bound = match &decl.bound {
                Some(bound) => {
                    let ty = self.resolve(bound)?;
                    if !matches!(ty, Type::Obj(..)) {
                        return Err(Diagnostic::new(
                            bound.span,
                            format!(
                                "la cota de `{}` debe ser una clase o una interfaz, pero es {ty}",
                                decl.name
                            ),
                        ));
                    }
                    Some(ty)
                }
                None => None,
            };
            out.push(TParam {
                name: decl.name.clone(),
                bound,
            });
        }
        self.scope_params = out.clone();
        Ok(out)
    }

    fn param_bound(&self, name: &str) -> Option<Type> {
        self.scope_params
            .iter()
            .find(|p| p.name == name)
            .and_then(|p| p.bound.clone())
    }

    /// Los parámetros de tipo de `class` como tipos (`T`, `U`): los argumentos de su propio `this`.
    fn identity_args(&self, class: &str) -> Vec<Type> {
        self.classes[class]
            .tparams
            .iter()
            .map(|p| Type::Param(p.name.clone()))
            .collect()
    }

    /// Registra las firmas de los métodos de una interfaz como `"Interfaz.metodo"`.
    fn declare_kind(&mut self, decl: &KindDecl) -> Result<(), Diagnostic> {
        let saved = self.scope_params.clone();
        let tparams = self.enter_params(&decl.type_params)?;
        self.kinds
            .get_mut(&decl.name)
            .expect("registrada antes")
            .tparams = tparams;
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
                    tparams: Vec::new(),
                    params,
                    ret: Some(ret),
                },
            );
            names.push(method.name.clone());
        }
        self.kinds
            .get_mut(&decl.name)
            .expect("registrada antes")
            .methods = names;
        self.scope_params = saved;
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

    /// Todos los campos de `class<args>`: primero los heredados, con los tipos ya sustituidos.
    fn all_fields(&self, class: &str, args: &[Type]) -> Vec<FieldInfo> {
        let info = &self.classes[class];
        let map = tmap(&info.tparams, args);
        let mut fields = match &info.parent {
            Some((parent, parent_args)) => {
                let parent_args: Vec<Type> = parent_args.iter().map(|a| subst(a, &map)).collect();
                self.all_fields(parent, &parent_args)
            }
            None => Vec::new(),
        };
        fields.extend(info.fields.iter().map(|f| FieldInfo {
            ty: subst(&f.ty, &map),
            ..f.clone()
        }));
        fields
    }

    /// El método `name` visible en `ty`: el de la propia clase, el heredado más cercano o, si `ty` es
    /// una interfaz o un parámetro de tipo con cota, el de ella. Devuelve la clave `"Clase.metodo"` de
    /// quien lo declara y cómo se sustituyen los parámetros de tipo de esa clase.
    fn find_method(&self, ty: &Type, name: &str) -> Option<(String, TMap)> {
        match ty {
            Type::Param(param) => self
                .param_bound(param)
                .and_then(|bound| self.find_method(&bound, name)),
            Type::Obj(class, args) => {
                if let Some(kind) = self.kinds.get(class) {
                    let key = format!("{class}.{name}");
                    return self
                        .funcs
                        .contains_key(&key)
                        .then(|| (key, tmap(&kind.tparams, args)));
                }
                let mut current = class.clone();
                let mut current_args = args.clone();
                loop {
                    let info = self.classes.get(&current)?;
                    let map = tmap(&info.tparams, &current_args);
                    let key = format!("{current}.{name}");
                    if self.funcs.contains_key(&key) {
                        return Some((key, map));
                    }
                    let (parent, parent_args) = info.parent.as_ref()?;
                    current_args = parent_args.iter().map(|a| subst(a, &map)).collect();
                    current = parent.clone();
                }
            }
            _ => None,
        }
    }

    /// `ty` visto como `target` (un ancestro o una interfaz que cumple), con sus argumentos de tipo.
    fn as_super(&self, ty: &Type, target: &str) -> Option<Type> {
        let Type::Obj(class, args) = ty else {
            return None;
        };
        if class == target {
            return Some(ty.clone());
        }
        let info = self.classes.get(class)?;
        let map = tmap(&info.tparams, args);
        for (kind, kind_args) in &info.kinds {
            if kind == target {
                let kind_args = kind_args.iter().map(|a| subst(a, &map)).collect();
                return Some(Type::Obj(kind.clone(), kind_args));
            }
        }
        let (parent, parent_args) = info.parent.as_ref()?;
        let parent_args = parent_args.iter().map(|a| subst(a, &map)).collect();
        self.as_super(&Type::Obj(parent.clone(), parent_args), target)
    }

    /// ¿`sub` es `sup` o hereda de él o lo implementa? (por nombre, sin mirar argumentos de tipo)
    fn is_sub(&self, sub: &str, sup: &str) -> bool {
        let mut current = Some(sub.to_string());
        while let Some(class) = current {
            if class == sup {
                return true;
            }
            let Some(info) = self.classes.get(&class) else {
                return false;
            };
            if info.kinds.iter().any(|(k, _)| k == sup) {
                return true;
            }
            current = info.parent.as_ref().map(|(p, _)| p.clone());
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
            // Los argumentos de tipo son invariantes: `Caja<Perro>` no es `Caja<Animal>`.
            (Type::Obj(e, _), Type::Obj(..)) => {
                self.as_super(found, e).is_some_and(|t| t == *expected)
            }
            // Las funciones aceptan lo que se les pase de más general y devuelven lo que prometen;
            // una que devuelve Unit no admite una que devuelve otra cosa.
            (Type::Fn(ep, er), Type::Fn(fp, fr)) => {
                ep.len() == fp.len()
                    && ep.iter().zip(fp).all(|(e, f)| self.assignable(f, e))
                    && if **er == Type::Unit {
                        **fr == Type::Unit
                    } else {
                        self.assignable(er, fr)
                    }
            }
            // Un parámetro de tipo con cota vale como su cota.
            (_, Type::Param(name)) => self
                .param_bound(name)
                .is_some_and(|bound| self.assignable(expected, &bound)),
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
        let instantiate = |sig: &Sig, map: &TMap| Sig {
            tparams: Vec::new(),
            params: sig.params.iter().map(|p| subst(p, map)).collect(),
            ret: sig.ret.as_ref().map(|r| subst(r, map)),
        };
        if let Some((parent, parent_args)) = &info.parent {
            let parent_ty = Type::Obj(parent.clone(), parent_args.clone());
            for method in &decl.methods {
                let Some((inherited, map)) = self.find_method(&parent_ty, &method.name) else {
                    continue;
                };
                let own = &self.funcs[&format!("{}.{}", decl.name, method.name)];
                let base = instantiate(&self.funcs[&inherited], &map);
                if own.params != base.params || own.ret != base.ret {
                    return Err(Diagnostic::new(
                        method.name_span,
                        format!(
                            "el método `{}` sobrescribe el de `{}`, pero su firma {} no coincide con {}",
                            method.name,
                            inherited.split('.').next().unwrap_or(parent),
                            Self::sig_text(own),
                            Self::sig_text(&base)
                        ),
                    ));
                }
            }
        }
        let this_ty = Type::Obj(decl.name.clone(), self.identity_args(&decl.name));
        for (kind, kind_args) in &info.kinds {
            let kind_info = &self.kinds[kind];
            let kind_map = tmap(&kind_info.tparams, kind_args);
            for name in &kind_info.methods {
                let wanted = instantiate(&self.funcs[&format!("{kind}.{name}")], &kind_map);
                let Some((key, map)) = self.find_method(&this_ty, name) else {
                    return Err(Diagnostic::new(
                        decl.name_span,
                        format!(
                            "la clase `{}` no implementa el método `{name}` de la interfaz `{kind}`",
                            decl.name
                        ),
                    ));
                };
                let found = instantiate(&self.funcs[&key], &map);
                if found.params != wanted.params || found.ret != wanted.ret {
                    return Err(Diagnostic::new(
                        decl.name_span,
                        format!(
                            "el método `{name}` de `{}` tiene la firma {}, pero la interfaz `{kind}` pide {}",
                            decl.name,
                            Self::sig_text(&found),
                            Self::sig_text(&wanted)
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
        let saved = self.scope_params.clone();
        let tparams = self.enter_params(&decl.type_params)?;
        self.classes
            .get_mut(&decl.name)
            .expect("registrada antes")
            .tparams = tparams;
        let mut parent: Option<(String, Vec<Type>)> = None;
        let mut kinds: Vec<(String, Vec<Type>)> = Vec::new();
        for sup in &decl.supers {
            let unknown =
                || Diagnostic::new(sup.name_span, format!("el tipo `{}` no existe", sup.name));
            let is_class = self.classes.contains_key(&sup.name);
            let is_kind = self.kinds.contains_key(&sup.name);
            if !is_class && !is_kind {
                return Err(unknown());
            }
            let wanted = if is_class {
                self.classes[&sup.name].tparams.len()
            } else {
                self.kinds[&sup.name].tparams.len()
            };
            if sup.type_args.len() != wanted {
                return Err(Diagnostic::new(
                    sup.span,
                    format!(
                        "`{}` necesita {wanted} argumento(s) de tipo, pero recibió {}",
                        sup.name,
                        sup.type_args.len()
                    ),
                ));
            }
            let type_args = sup
                .type_args
                .iter()
                .map(|t| self.resolve(t))
                .collect::<Result<Vec<_>, _>>()?;
            match (&sup.args, is_class) {
                (Some(_), true) => {
                    if parent.is_some() {
                        return Err(Diagnostic::new(
                            sup.span,
                            "una clase solo puede heredar de una clase",
                        ));
                    }
                    parent = Some((sup.name.clone(), type_args));
                }
                (Some(_), false) => {
                    return Err(Diagnostic::new(
                        sup.span,
                        format!(
                            "`{}` es una interfaz: se implementa sin paréntesis",
                            sup.name
                        ),
                    ));
                }
                (None, false) => {
                    if kinds.iter().any(|(k, _)| *k == sup.name) {
                        return Err(Diagnostic::new(
                            sup.span,
                            format!("la interfaz `{}` está repetida", sup.name),
                        ));
                    }
                    kinds.push((sup.name.clone(), type_args));
                }
                (None, true) => {
                    return Err(Diagnostic::new(
                        sup.span,
                        format!(
                            "`{0}` es una clase: para heredarla escribe `{0}(argumentos)`",
                            sup.name
                        ),
                    ));
                }
            }
        }
        let inherited: Vec<FieldInfo> = match &parent {
            Some((parent, parent_args)) => self.all_fields(parent, parent_args),
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
        self.scope_params = saved;
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
                if let Some((parent, _)) = self.classes[name].parent.clone() {
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
        let saved = std::mem::replace(
            &mut self.scope_params,
            self.classes[&decl.name].tparams.clone(),
        );
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
            lambda_marks: Vec::new(),
        };
        // Argumentos para el constructor de la clase padre: ven los parámetros de esta.
        if let Some(sup) = decl.supers.iter().find(|s| s.args.is_some()) {
            let args = sup.args.as_deref().unwrap_or(&[]);
            let (_, parent_args) = self.classes[&decl.name]
                .parent
                .clone()
                .expect("declarada antes");
            let parent_info = &self.classes[&sup.name];
            let map = tmap(&parent_info.tparams, &parent_args);
            let wanted: Vec<Type> = parent_info.ctor.iter().map(|t| subst(t, &map)).collect();
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
        self.scope_params = saved;
        Ok(())
    }

    fn check_fn(&mut self, key: &str) -> Result<(), Diagnostic> {
        let (decl, class) = self.decls[key].clone();
        if let Some(class) = &class {
            self.ensure_class(class)?;
        }
        self.state.insert(key.to_string(), State::InProgress);
        let sig = self.funcs[key].clone();
        // Los métodos ven los parámetros de tipo de su clase; las funciones, los suyos.
        let params_here = match &class {
            Some(class) => {
                let mut all = self.classes[class].tparams.clone();
                all.extend(sig.tparams.iter().cloned());
                all
            }
            None => sig.tparams.clone(),
        };
        let saved_params = std::mem::replace(&mut self.scope_params, params_here);
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
            lambda_marks: Vec::new(),
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
        self.scope_params = saved_params;
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
        for (i, scope) in ctx.scopes.iter().enumerate().rev() {
            if let Some((ty, mutable, declared)) = scope.get(name) {
                // Dentro de una lambda, una variable `mut` de fuera vuelve a su tipo declarado: la
                // lambda puede ejecutarse cuando ya no vale lo que se probó al crearla.
                let captured = ctx.lambda_marks.last().is_some_and(|mark| i < *mark);
                let ty = match (captured, mutable, declared) {
                    (true, true, Some(declared)) => declared.clone(),
                    _ => ty.clone(),
                };
                return Some((ty, *mutable, None));
            }
        }
        if let Some(class) = &ctx.this {
            let fields = self.all_fields(class, &self.identity_args(class));
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
            if self
                .all_fields(class, &self.identity_args(class))
                .iter()
                .any(|f| f.name == name)
            {
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
            Type::Obj(class, _) => self.ensure_class(class),
            Type::Param(name) => match self.param_bound(name) {
                Some(Type::Obj(class, _)) => self.ensure_class(&class),
                _ => Ok(()),
            },
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
        // Un parámetro de tipo con cota de clase muestra los campos de su cota.
        let bound = match ty {
            Type::Param(name) => self.param_bound(name),
            _ => None,
        };
        let ty = bound.as_ref().unwrap_or(ty);
        let Type::Obj(class, args) = ty else {
            return Err(Diagnostic::new(target_span, not_object(ty, "campos")));
        };
        if self.kinds.contains_key(class) {
            return Err(Diagnostic::new(
                target_span,
                format!("`{class}` es una interfaz y no tiene campos"),
            ));
        }
        let fields = self.all_fields(class, args);
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
        // Una lambda toma los tipos de sus parámetros y de su resultado del tipo esperado.
        if let Expr::Lambda { params, body, span } = expr {
            let target = match expected {
                Type::Nullable(inner) => &**inner,
                other => other,
            };
            if let Type::Fn(ps, ret) = target {
                let known: Vec<Option<Type>> = ps.iter().cloned().map(Some).collect();
                let found = self.lambda(
                    ctx,
                    params,
                    body,
                    *span,
                    Some((&known, Some((**ret).clone()))),
                )?;
                return self.expect(expected, &found, expr.span());
            }
        }
        if let (Expr::Array { elems, .. }, Type::Array(elem)) = (expr, expected) {
            if elems.is_empty() {
                return Ok(());
            }
            // Un arreglo de objetos acepta elementos de subclases: `[Perro(..), Gato(..)]` como `[Animal]`.
            if matches!(**elem, Type::Obj(..) | Type::Nullable(_)) {
                return elems.iter().try_for_each(|e| self.expr_as(ctx, e, elem));
            }
        }
        // `fix l: Lista<Int> = Lista()`: el tipo esperado completa los argumentos de tipo.
        if let Expr::Call {
            name,
            args,
            type_args,
            span,
        } = expr
        {
            if !BUILTINS.contains(&name.as_str()) {
                let found = self.call(ctx, name, args, type_args, *span, Some(expected))?;
                return self.expect(expected, &found, expr.span());
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
                // `fix fact: (Int) -> Int = { n -> ... fact(n - 1) }` en el nivel superior: la global
                // ya existe dentro de la lambda (se llama cuando ya tiene valor).
                if let (Some(ty), true, Expr::Lambda { .. }) = (ty, ctx.scopes.is_empty(), init) {
                    let expected = self.resolve(ty)?;
                    if matches!(expected, Type::Fn(..)) {
                        if self.globals.contains_key(name) {
                            return Err(Diagnostic::new(
                                *name_span,
                                format!("`{name}` ya está declarada en este ámbito"),
                            ));
                        }
                        self.globals
                            .insert(name.clone(), (expected.clone(), *mutable, None));
                        return self.expr_as(ctx, init, &expected);
                    }
                }
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
                    Type::Obj(class, _) if self.is_sub(class, "Fallo") => {}
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
                    let Type::Obj(class, _) = &ty else {
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
            Expr::Lambda { params, body, span } => self.lambda(ctx, params, body, *span, None),
            Expr::CallValue { callee, args, span } => {
                let found = self.expr(ctx, callee)?;
                match found {
                    Type::Fn(ps, ret) => self.call_value(ctx, "esta función", &ps, &ret, args, *span),
                    Type::Nullable(inner) if matches!(*inner, Type::Fn(..)) => Err(Diagnostic::new(
                        callee.span(),
                        "esta función es opcional y puede ser nil: comprueba que no lo sea antes de llamarla",
                    )),
                    other => Err(Diagnostic::new(
                        callee.span(),
                        format!("solo se pueden llamar funciones, pero esto es {other}"),
                    )),
                }
            }
            Expr::Var(name, span) => {
                let Some((ty, _, field)) = self.lookup(ctx, name) else {
                    return self.function_value(name, *span);
                };
                if let Some(i) = field {
                    self.res.fields.insert(*span, i);
                }
                Ok(ty)
            }
            Expr::This(span) => match &ctx.this {
                Some(class) => Ok(Type::Obj(class.clone(), self.identity_args(class))),
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
                type_args,
                args,
                span,
            } => {
                let target_ty = self.expr(ctx, target)?;
                let receiver = self.method_receiver(&target_ty, target.span())?;
                let Some((key, map)) = self.find_method(&receiver, name) else {
                    // `obj.alClic(x)`: llamada a un campo de tipo función.
                    self.ensure_class_of(&receiver)?;
                    if let Ok((i, Type::Fn(ps, ret), _)) =
                        self.field_of(&receiver, name, *name_span, target.span())
                    {
                        self.res.fields.insert(*span, i);
                        self.res.valuecalls.insert(*span);
                        return self.call_value(ctx, name, &ps, &ret, args, *span);
                    }
                    let what = match &receiver {
                        Type::Obj(class, _) if self.kinds.contains_key(class) => "la interfaz",
                        _ => "la clase",
                    };
                    return Err(Diagnostic::new(
                        *name_span,
                        format!("{what} `{receiver}` no tiene el método `{name}`"),
                    ));
                };
                self.res.virtuals.insert(*span, name.clone());
                self.call_sig(
                    ctx,
                    &key,
                    name,
                    "el método",
                    args,
                    type_args,
                    *span,
                    map,
                    None,
                )
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
                    .and_then(|class| self.classes[class].parent.clone())
                    .map(|(name, args)| Type::Obj(name, args));
                let Some(parent) = parent else {
                    return Err(Diagnostic::new(
                        *span,
                        "`super` solo se puede usar en un método de una clase que hereda de otra",
                    ));
                };
                let Some((key, map)) = self.find_method(&parent, name) else {
                    return Err(Diagnostic::new(
                        *name_span,
                        format!("la clase padre `{parent}` no tiene el método `{name}`"),
                    ));
                };
                self.res.methods.insert(*span, key.clone());
                self.call_sig(ctx, &key, name, "el método", args, &[], *span, map, None)
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
                let b = b?;
                if matches!(op, BinOp::Eq | BinOp::Ne) {
                    return self.equality_type(&a, &b, *span);
                }
                binary_type(*op, &a, &b, *span)
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
                let receiver = self.method_receiver(&inner, target.span())?;
                let Some((key, map)) = self.find_method(&receiver, name) else {
                    return Err(Diagnostic::new(
                        *name_span,
                        format!("`{receiver}` no tiene el método `{name}`"),
                    ));
                };
                self.res.virtuals.insert(*span, name.clone());
                let ret =
                    self.call_sig(ctx, &key, name, "el método", args, &[], *span, map, None)?;
                Ok(if ret == Type::Unit {
                    ret
                } else {
                    optional(ret)
                })
            }
            Expr::Call {
                name,
                args,
                type_args,
                span,
            } => {
                if BUILTINS.contains(&name.as_str()) {
                    self.builtin(ctx, name, args, *span)
                } else {
                    self.call(ctx, name, args, type_args, *span, None)
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
        let wanted = if name == "push" { 2 } else { 1 };
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
        match name {
            "len" => match first {
                Type::Str | Type::Array(_) => Ok(Type::Int),
                other => Err(Diagnostic::new(
                    args[0].span(),
                    format!("`len` admite Str o un arreglo, pero recibió {other}"),
                )),
            },
            "hash" => {
                if first == Type::Unit || first == Type::Nil {
                    return Err(Diagnostic::new(
                        args[0].span(),
                        format!("`hash` necesita un valor, pero recibió {first}"),
                    ));
                }
                Ok(Type::Int)
            }
            "pop" => match first {
                Type::Array(elem) => Ok(*elem),
                other => Err(Diagnostic::new(
                    args[0].span(),
                    format!("`pop` necesita un arreglo, pero recibió {other}"),
                )),
            },
            _ => {
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
        }
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
        let Type::Obj(from, _) = &found else {
            return Err(Diagnostic::new(
                expr.span(),
                format!("`{op}` compara un objeto, pero {found} no lo es"),
            ));
        };
        // Los argumentos de tipo no existen en ejecución: `is` mira solo la clase.
        let (to, result) = if op == "is" {
            if !self.classes.contains_key(&ty.name) && !self.kinds.contains_key(&ty.name) {
                let other = self.resolve(ty)?;
                return Err(Diagnostic::new(
                    ty.span,
                    format!("`is` compara con una clase o interfaz, pero {other} no lo es"),
                ));
            }
            if !ty.args.is_empty() {
                return Err(Diagnostic::new(
                    ty.span,
                    format!(
                        "los argumentos de tipo se borran en ejecución: escribe `is {}`",
                        ty.name
                    ),
                ));
            }
            (ty.name.clone(), Type::Bool)
        } else {
            let target = self.resolve(ty)?;
            let Type::Obj(to, args) = &target else {
                return Err(Diagnostic::new(
                    ty.span,
                    format!("`as` convierte a una clase o interfaz, pero {target} no lo es"),
                ));
            };
            if !args.is_empty() {
                return Err(Diagnostic::new(
                    ty.span,
                    "no se puede convertir a un tipo genérico: los argumentos de tipo se borran en ejecución",
                ));
            }
            (to.clone(), target)
        };
        // Dos clases sin relación nunca coinciden; con una interfaz de por medio sí puede.
        let unrelated = self.classes.contains_key(from)
            && self.classes.contains_key(&to)
            && !self.is_sub(from, &to)
            && !self.is_sub(&to, from);
        if unrelated {
            return Err(Diagnostic::new(
                ty.span,
                format!("un `{from}` nunca puede ser un `{to}`: no están emparentadas"),
            ));
        }
        Ok(result)
    }

    /// El tipo donde buscar métodos al llamar sobre `ty`: el propio objeto o, para un parámetro de tipo,
    /// su cota.
    fn method_receiver(&self, ty: &Type, span: Span) -> Result<Type, Diagnostic> {
        match ty {
            Type::Obj(..) => Ok(ty.clone()),
            Type::Param(name) => self.param_bound(name).ok_or_else(|| {
                Diagnostic::new(
                    span,
                    format!(
                        "`{name}` no tiene métodos: añade una cota al parámetro de tipo, por ejemplo `<{name}: Interfaz>`"
                    ),
                )
            }),
            other => Err(Diagnostic::new(span, not_object(other, "métodos"))),
        }
    }

    /// Tipo de `a == b` / `a != b`: dos valores del mismo tipo (o numéricos, o emparentados) se pueden
    /// comparar; `nil` solo con un opcional. Los objetos y arreglos se comparan por referencia.
    fn equality_type(&self, a: &Type, b: &Type, span: Span) -> Result<Type, Diagnostic> {
        let invalid = |hint: &str| {
            Diagnostic::new(span, format!("la operación `==` no admite {a} y {b}{hint}"))
        };
        match (a, b) {
            (Type::Nil, Type::Nullable(_) | Type::Nil) | (Type::Nullable(_), Type::Nil) => {
                Ok(Type::Bool)
            }
            _ if *a == Type::Nil || *b == Type::Nil => Err(invalid(
                "; ese valor nunca es nil, así que no hace falta compararlo",
            )),
            _ if *a == Type::Unit || *b == Type::Unit => Err(invalid("")),
            _ if (a.is_numeric() && b.is_numeric())
                || self.assignable(a, b)
                || self.assignable(b, a) =>
            {
                Ok(Type::Bool)
            }
            _ => Err(invalid("")),
        }
    }

    /// Llamada `nombre(args)`: constructor de una clase, método de `this` o función. `expected` es el
    /// tipo que pide el contexto (completa los argumentos de tipo de `Lista()`).
    fn call(
        &mut self,
        ctx: &mut Ctx,
        name: &str,
        args: &[Expr],
        type_args: &[TypeRef],
        span: Span,
        expected: Option<&Type>,
    ) -> Result<Type, Diagnostic> {
        // `f(x)` con `f` una variable, parámetro o campo de tipo función.
        if !self.classes.contains_key(name) && !self.kinds.contains_key(name) {
            match self.lookup(ctx, name) {
                Some((Type::Fn(ps, ret), _, field)) => {
                    if let Some(i) = field {
                        self.res.fields.insert(span, i);
                    }
                    self.res.valuecalls.insert(span);
                    return self.call_value(ctx, name, &ps, &ret, args, span);
                }
                Some((Type::Nullable(inner), _, _)) if matches!(*inner, Type::Fn(..)) => {
                    return Err(Diagnostic::new(
                        span,
                        format!(
                            "`{name}` es una función opcional y puede ser nil: comprueba `if {name} != nil` antes de llamarla"
                        ),
                    ));
                }
                _ => {}
            }
        }
        if let Some(info) = self.classes.get(name) {
            let tparams = info.tparams.clone();
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
            let template = Type::Obj(
                name.to_string(),
                tparams
                    .iter()
                    .map(|p| Type::Param(p.name.clone()))
                    .collect(),
            );
            let map = self.infer(
                ctx,
                &Inference {
                    what: "la clase",
                    shown: name,
                    tparams: &tparams,
                    params: &params,
                    args,
                    explicit: type_args,
                    template: Some(&template),
                    expected,
                    span,
                },
                TMap::new(),
            )?;
            for (arg, param) in args.iter().zip(&params) {
                self.expr_as(ctx, arg, &subst(param, &map))?;
            }
            return Ok(subst(&template, &map));
        }
        if self.kinds.contains_key(name) {
            return Err(Diagnostic::new(
                span,
                format!("`{name}` es una interfaz y no se puede instanciar"),
            ));
        }
        if let Some(class) = ctx.this.clone() {
            let this_ty = Type::Obj(class.clone(), self.identity_args(&class));
            if let Some((key, map)) = self.find_method(&this_ty, name) {
                self.res.virtuals.insert(span, name.to_string());
                return self.call_sig(ctx, &key, name, "el método", args, &[], span, map, None);
            }
        }
        if !self.funcs.contains_key(name) {
            if let Some((ty, _, _)) = self.lookup(ctx, name) {
                return Err(Diagnostic::new(
                    span,
                    format!("`{name}` no es una función: es una variable de tipo {ty}"),
                ));
            }
        }
        self.call_sig(
            ctx,
            name,
            name,
            "la función",
            args,
            type_args,
            span,
            TMap::new(),
            None,
        )
    }

    /// Llamada a un valor de tipo función: comprueba los argumentos contra sus parámetros.
    fn call_value(
        &mut self,
        ctx: &mut Ctx,
        shown: &str,
        params: &[Type],
        ret: &Type,
        args: &[Expr],
        span: Span,
    ) -> Result<Type, Diagnostic> {
        if params.len() != args.len() {
            return Err(Diagnostic::new(
                span,
                format!(
                    "`{shown}` espera {} argumento(s), pero recibió {}",
                    params.len(),
                    args.len()
                ),
            ));
        }
        for (arg, param) in args.iter().zip(params) {
            self.expr_as(ctx, arg, param)?;
        }
        Ok(ret.clone())
    }

    /// Una función con nombre usada como valor (`aplica(doble, 3)`): su tipo de función.
    fn function_value(&mut self, name: &str, span: Span) -> Result<Type, Diagnostic> {
        if BUILTINS.contains(&name) {
            return Err(Diagnostic::new(
                span,
                format!("`{name}` es una función integrada y no se puede usar como valor; envuélvela en una lambda"),
            ));
        }
        let Some(sig) = self.funcs.get(name).cloned() else {
            return Err(undefined_variable(name, span));
        };
        if !sig.tparams.is_empty() {
            return Err(Diagnostic::new(
                span,
                format!("`{name}` es genérica y no se puede usar como valor; envuélvela en una lambda con tipos concretos"),
            ));
        }
        if sig.ret.is_none() {
            match self.state.get(name) {
                Some(State::InProgress) => {
                    return Err(Diagnostic::new(
                        span,
                        format!("`{name}` se usa antes de conocer su tipo; añade `-> Tipo` a su declaración"),
                    ));
                }
                _ => {
                    if self.decls.contains_key(name) {
                        self.check_fn(name)?;
                    }
                }
            }
        }
        let ret = self.funcs[name].ret.clone().unwrap_or(Type::Unit);
        Ok(Type::Fn(sig.params, Box::new(ret)))
    }

    /// Verifica una lambda. `expected` trae, si el contexto lo sabe, el tipo de cada parámetro y el
    /// del resultado; sin ellos los parámetros deben llevar anotación y el resultado se infiere.
    fn lambda(
        &mut self,
        ctx: &mut Ctx,
        params: &[LambdaParam],
        body: &Block,
        span: Span,
        expected: Option<(&[Option<Type>], Option<Type>)>,
    ) -> Result<Type, Diagnostic> {
        let wanted_params = expected.as_ref().map(|e| e.0);
        if let Some(wanted) = wanted_params {
            if wanted.len() != params.len() {
                return Err(Diagnostic::new(
                    span,
                    format!(
                        "esta lambda tiene {} parámetro(s), pero aquí se espera una función de {}",
                        params.len(),
                        wanted.len()
                    ),
                ));
            }
        }
        let mut types = Vec::new();
        let mut scope = Vars::new();
        for (i, param) in params.iter().enumerate() {
            let given = match &param.ty {
                Some(ty) => Some(self.resolve(ty)?),
                None => None,
            };
            let wanted = wanted_params.and_then(|w| w[i].clone());
            let ty = match (given, wanted) {
                (Some(given), Some(wanted)) => {
                    if !self.assignable(&given, &wanted) {
                        return Err(Diagnostic::new(
                            param.name_span,
                            format!(
                                "el parámetro `{}` es {given}, pero aquí se espera {wanted}",
                                param.name
                            ),
                        ));
                    }
                    given
                }
                (Some(given), None) => given,
                (None, Some(wanted)) => wanted,
                (None, None) => {
                    return Err(Diagnostic::new(
                        param.name_span,
                        format!(
                            "no se puede deducir el tipo del parámetro `{0}`; escríbelo, por ejemplo `{0}: Int`",
                            param.name
                        ),
                    ));
                }
            };
            if ty == Type::Unit {
                return Err(Diagnostic::new(
                    param.name_span,
                    format!("el parámetro `{}` no puede ser de tipo Unit", param.name),
                ));
            }
            scope.insert(param.name.clone(), (ty.clone(), false, None));
            types.push(ty);
        }
        // Las variables `mut` estrechadas que la lambda asigna dejan de estarlo: puede ejecutarse
        // después y cambiarlas.
        let narrowed: Vec<String> = ctx
            .scopes
            .iter()
            .flat_map(|sc| sc.iter())
            .filter(|(_, entry)| entry.1 && entry.2.is_some())
            .map(|(name, _)| name.clone())
            .collect();
        for name in narrowed {
            if block_assigns_deep(body, &name) {
                Self::widen(ctx, &name);
            }
        }
        let ret_expected = expected.and_then(|e| e.1);
        let saved_ret = std::mem::replace(
            &mut ctx.ret,
            match &ret_expected {
                Some(ty) => Ret::Declared(ty.clone()),
                None => Ret::Infer(None),
            },
        );
        ctx.lambda_marks.push(ctx.scopes.len());
        ctx.scopes.push(scope);
        let result = self.lambda_body(ctx, body, ret_expected.as_ref());
        ctx.scopes.pop();
        ctx.lambda_marks.pop();
        let used_ret = std::mem::replace(&mut ctx.ret, saved_ret);
        result?;
        let ret = match (ret_expected, used_ret) {
            (Some(ty), _) => ty,
            (None, Ret::Infer(slot)) => slot.unwrap_or(Type::Unit),
            _ => Type::Unit,
        };
        Ok(Type::Fn(types, Box::new(ret)))
    }

    /// Las sentencias de una lambda; si la última es una expresión, es su valor.
    fn lambda_body(
        &mut self,
        ctx: &mut Ctx,
        body: &Block,
        ret: Option<&Type>,
    ) -> Result<(), Diagnostic> {
        let (init, tail) = match body.stmts.split_last() {
            Some((Stmt::Expr(tail), rest)) => (rest, Some(tail)),
            _ => (&body.stmts[..], None),
        };
        for stmt in init {
            self.stmt(ctx, stmt)?;
        }
        match (tail, ret) {
            (Some(tail), Some(Type::Unit)) => {
                if self.expr(ctx, tail)? != Type::Unit {
                    self.res.discards.insert(tail.span());
                }
            }
            (Some(tail), Some(ret)) => self.expr_as(ctx, tail, ret)?,
            (Some(tail), None) => {
                let found = self.expr(ctx, tail)?;
                no_nil(&found, tail.span())?;
                let previous = match &ctx.ret {
                    Ret::Infer(slot) => slot.clone(),
                    _ => None,
                };
                match previous {
                    Some(previous) => self.expect(&previous, &found, tail.span())?,
                    None => {
                        if let Ret::Infer(slot) = &mut ctx.ret {
                            *slot = Some(found);
                        }
                    }
                }
            }
            (None, Some(ret)) => {
                if *ret != Type::Unit && !block_returns(body) {
                    return Err(Diagnostic::new(
                        body.span,
                        format!("la lambda debe devolver un valor de tipo {ret}"),
                    ));
                }
            }
            (None, None) => {}
        }
        Ok(())
    }

    /// Completa los argumentos de tipo de una llamada: los explícitos (`f<Int>(x)`), los que salen de
    /// los argumentos y, si aún faltan, los que pide el contexto. Comprueba las cotas.
    fn infer(
        &mut self,
        ctx: &mut Ctx,
        call: &Inference,
        mut map: TMap,
    ) -> Result<TMap, Diagnostic> {
        let Inference {
            what,
            shown,
            tparams,
            params,
            args,
            explicit,
            template,
            expected,
            span,
        } = *call;
        if tparams.is_empty() {
            if !explicit.is_empty() {
                return Err(Diagnostic::new(
                    span,
                    format!("{what} `{shown}` no es genérica y no admite argumentos de tipo"),
                ));
            }
            return Ok(map);
        }
        let names: Vec<&str> = tparams.iter().map(|p| p.name.as_str()).collect();
        if !explicit.is_empty() {
            if explicit.len() != tparams.len() {
                return Err(Diagnostic::new(
                    span,
                    format!(
                        "{what} `{shown}` necesita {} argumento(s) de tipo, pero recibió {}",
                        tparams.len(),
                        explicit.len()
                    ),
                ));
            }
            for (param, given) in tparams.iter().zip(explicit) {
                let ty = self.resolve(given)?;
                if ty == Type::Unit {
                    return Err(Diagnostic::new(
                        given.span,
                        "un argumento de tipo no puede ser Unit",
                    ));
                }
                map.insert(param.name.clone(), ty);
            }
        } else {
            for (arg, param) in args.iter().zip(params) {
                if !mentions_unresolved(param, &names, &map) {
                    continue;
                }
                // `nil` y `[]` no dicen de qué tipo son; una lambda se mira después, con lo ya deducido.
                if matches!(arg, Expr::Nil(_) | Expr::Lambda { .. })
                    || matches!(arg, Expr::Array { elems, .. } if elems.is_empty())
                {
                    continue;
                }
                let found = self.expr(ctx, arg)?;
                self.unify(param, &found, &names, &mut map);
            }
            for (arg, param) in args.iter().zip(params) {
                let Expr::Lambda {
                    params: lambda_params,
                    body,
                    span: lambda_span,
                } = arg
                else {
                    continue;
                };
                let target = match param {
                    Type::Nullable(inner) => &**inner,
                    other => other,
                };
                let Type::Fn(param_types, param_ret) = target else {
                    continue;
                };
                if !mentions_unresolved(target, &names, &map) {
                    continue;
                }
                let known: Vec<Option<Type>> = param_types
                    .iter()
                    .map(|p| (!mentions_unresolved(p, &names, &map)).then(|| subst(p, &map)))
                    .collect();
                let ret =
                    (!mentions_unresolved(param_ret, &names, &map)).then(|| subst(param_ret, &map));
                let found =
                    self.lambda(ctx, lambda_params, body, *lambda_span, Some((&known, ret)))?;
                self.unify(target, &found, &names, &mut map);
            }
            if let (Some(template), Some(expected)) = (template, expected) {
                // La clase vista como el tipo esperado (puede ser un ancestro o una interfaz).
                if let Some(Type::Obj(target, _)) = Some(expected) {
                    if let Some(seen) = self.as_super(template, target) {
                        self.unify(&seen, expected, &names, &mut map);
                    }
                }
            }
        }
        for param in tparams {
            if !map.contains_key(&param.name) {
                return Err(Diagnostic::new(
                    span,
                    format!(
                        "no se puede deducir el tipo `{}` de {what} `{shown}`; escríbelo, por ejemplo `{shown}<Int>(...)`",
                        param.name
                    ),
                ));
            }
        }
        for param in tparams {
            if let Some(bound) = &param.bound {
                let bound = subst(bound, &map);
                let actual = &map[&param.name];
                if !self.assignable(&bound, actual) {
                    return Err(Diagnostic::new(
                        span,
                        format!(
                            "el tipo {actual} no cumple la cota `{}: {bound}` de {what} `{shown}`",
                            param.name
                        ),
                    ));
                }
            }
        }
        Ok(map)
    }

    /// Anota en `map` qué toma cada parámetro de tipo de `param` para que case con `found`.
    fn unify(&self, param: &Type, found: &Type, names: &[&str], map: &mut TMap) {
        match (param, found) {
            (Type::Param(name), _) if names.contains(&name.as_str()) => {
                if !map.contains_key(name) && *found != Type::Nil && *found != Type::Unit {
                    map.insert(name.clone(), found.clone());
                }
            }
            (Type::Array(p), Type::Array(f)) => self.unify(p, f, names, map),
            (Type::Fn(p_params, p_ret), Type::Fn(f_params, f_ret)) => {
                for (p, f) in p_params.iter().zip(f_params) {
                    self.unify(p, f, names, map);
                }
                self.unify(p_ret, f_ret, names, map);
            }
            (Type::Nullable(p), Type::Nullable(f)) => self.unify(p, f, names, map),
            (Type::Nullable(p), f) if *f != Type::Nil => self.unify(p, f, names, map),
            (Type::Obj(class, p_args), Type::Obj(..)) => {
                if let Some(Type::Obj(_, f_args)) = self.as_super(found, class) {
                    for (p, f) in p_args.iter().zip(&f_args) {
                        self.unify(p, f, names, map);
                    }
                }
            }
            _ => {}
        }
    }

    /// Llamada a la función o método de clave `key`; `shown` y `what` son para los mensajes. `map` ya
    /// trae los parámetros de tipo de la clase del receptor; los de la función se infieren aquí.
    #[allow(clippy::too_many_arguments)]
    fn call_sig(
        &mut self,
        ctx: &mut Ctx,
        key: &str,
        shown: &str,
        what: &str,
        args: &[Expr],
        type_args: &[TypeRef],
        span: Span,
        map: TMap,
        expected: Option<&Type>,
    ) -> Result<Type, Diagnostic> {
        let Some(sig) = self.funcs.get(key).cloned() else {
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
        let map = self.infer(
            ctx,
            &Inference {
                what,
                shown,
                tparams: &sig.tparams,
                params: &sig.params,
                args,
                explicit: type_args,
                template: None,
                expected,
                span,
            },
            map,
        )?;
        for (arg, param) in args.iter().zip(&sig.params) {
            self.expr_as(ctx, arg, &subst(param, &map))?;
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
        let ret = self.funcs[key].ret.clone().unwrap_or(Type::Unit);
        Ok(subst(&ret, &map))
    }

    fn resolve(&self, ty: &TypeRef) -> Result<Type, Diagnostic> {
        let unknown = || {
            Diagnostic::new(
                ty.span,
                format!(
                    "el tipo `{}` no existe (hay Int, Float, Bool, Str, Unit, [T], las clases, las interfaces y los parámetros de tipo declarados)",
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
            ("Fn", [params @ .., ret]) => {
                let params = params
                    .iter()
                    .map(|p| {
                        let t = self.resolve(p)?;
                        if t == Type::Unit {
                            return Err(Diagnostic::new(
                                p.span,
                                "un parámetro de una función no puede ser Unit",
                            ));
                        }
                        Ok(t)
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(Type::Fn(params, Box::new(self.resolve(ret)?)))
            }
            (name, args) => {
                if args.is_empty() && self.scope_params.iter().any(|p| p.name == name) {
                    return Ok(Type::Param(name.to_string()));
                }
                let arity = if let Some(class) = self.classes.get(name) {
                    class.tparams.len()
                } else if let Some(kind) = self.kinds.get(name) {
                    kind.tparams.len()
                } else {
                    return Err(unknown());
                };
                if args.len() != arity {
                    return Err(Diagnostic::new(
                        ty.span,
                        if arity == 0 {
                            format!("`{name}` no es genérica y no admite argumentos de tipo")
                        } else {
                            format!(
                                "`{name}` necesita {arity} argumento(s) de tipo, por ejemplo `{name}<...>`"
                            )
                        },
                    ));
                }
                let resolved = args
                    .iter()
                    .map(|a| {
                        let t = self.resolve(a)?;
                        if t == Type::Unit {
                            return Err(Diagnostic::new(
                                a.span,
                                "un argumento de tipo no puede ser Unit",
                            ));
                        }
                        Ok(t)
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(Type::Obj(name.to_string(), resolved))
            }
        }
    }
}

/// Los parámetros de tipo `decls` sin cotas todavía (para poder nombrarse unos a otros).
fn unbounded(decls: &[TypeParam]) -> Vec<TParam> {
    decls
        .iter()
        .map(|d| TParam {
            name: d.name.clone(),
            bound: None,
        })
        .collect()
}

/// Empareja los parámetros de tipo de una declaración con los argumentos concretos.
fn tmap(tparams: &[TParam], args: &[Type]) -> TMap {
    tparams
        .iter()
        .map(|p| p.name.clone())
        .zip(args.iter().cloned())
        .collect()
}

/// Sustituye los parámetros de tipo de `ty` según `map`. Un opcional de un opcional se aplana.
fn subst(ty: &Type, map: &TMap) -> Type {
    match ty {
        Type::Param(name) => map.get(name).cloned().unwrap_or_else(|| ty.clone()),
        Type::Array(elem) => Type::Array(Box::new(subst(elem, map))),
        Type::Fn(params, ret) => Type::Fn(
            params.iter().map(|p| subst(p, map)).collect(),
            Box::new(subst(ret, map)),
        ),
        Type::Nullable(inner) => optional(subst(inner, map)),
        Type::Obj(name, args) => {
            Type::Obj(name.clone(), args.iter().map(|a| subst(a, map)).collect())
        }
        other => other.clone(),
    }
}

/// ¿`ty` nombra algún parámetro de tipo de `names` que `map` aún no resuelve?
fn mentions_unresolved(ty: &Type, names: &[&str], map: &TMap) -> bool {
    match ty {
        Type::Param(name) => names.contains(&name.as_str()) && !map.contains_key(name),
        Type::Array(inner) | Type::Nullable(inner) => mentions_unresolved(inner, names, map),
        Type::Fn(params, ret) => {
            params.iter().any(|p| mentions_unresolved(p, names, map))
                || mentions_unresolved(ret, names, map)
        }
        Type::Obj(_, args) => args.iter().any(|a| mentions_unresolved(a, names, map)),
        _ => false,
    }
}

/// Lo que hace falta para completar los argumentos de tipo de una llamada.
struct Inference<'a> {
    what: &'a str,
    shown: &'a str,
    tparams: &'a [TParam],
    params: &'a [Type],
    args: &'a [Expr],
    explicit: &'a [TypeRef],
    /// El tipo que produce la llamada, con sus parámetros de tipo (solo los constructores).
    template: Option<&'a Type>,
    expected: Option<&'a Type>,
    span: Span,
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

/// ¿Se asigna `name` en `block`, incluso dentro de las lambdas que contiene?
fn block_assigns_deep(block: &Block, name: &str) -> bool {
    block.stmts.iter().any(|stmt| stmt_assigns_deep(stmt, name))
}

fn stmt_assigns_deep(stmt: &Stmt, name: &str) -> bool {
    let e = |expr: &Expr| expr_assigns(expr, name);
    let b = |block: &Block| block_assigns_deep(block, name);
    match stmt {
        Stmt::Assign { name: n, value, .. } => n == name || e(value),
        Stmt::Show(expr, _) | Stmt::Expr(expr) | Stmt::Fail(expr, _) => e(expr),
        Stmt::Let { init, .. } => e(init),
        Stmt::IndexAssign {
            target,
            index,
            value,
            ..
        } => e(target) || e(index) || e(value),
        Stmt::FieldAssign { target, value, .. } => e(target) || e(value),
        Stmt::If {
            cond,
            then,
            otherwise,
            ..
        } => e(cond) || b(then) || otherwise.as_ref().is_some_and(b),
        Stmt::While { cond, body, .. } => e(cond) || b(body),
        Stmt::Return(Some(expr), _) => e(expr),
        Stmt::Attempt { body, rescues, .. } => b(body) || rescues.iter().any(|r| b(&r.body)),
        _ => false,
    }
}

fn expr_assigns(expr: &Expr, name: &str) -> bool {
    let e = |expr: &Expr| expr_assigns(expr, name);
    match expr {
        Expr::Lambda { body, .. } => block_assigns_deep(body, name),
        Expr::Interp { parts, .. } => parts.iter().any(|p| match p {
            InterpPart::Expr(inner) => e(inner),
            InterpPart::Text(_) => false,
        }),
        Expr::Array { elems, .. } => elems.iter().any(e),
        Expr::Index { target, index, .. } => e(target) || e(index),
        Expr::Unary { expr, .. }
        | Expr::Is { expr, .. }
        | Expr::Cast { expr, .. }
        | Expr::Field { target: expr, .. }
        | Expr::SafeField { target: expr, .. } => e(expr),
        Expr::Binary { lhs, rhs, .. } | Expr::Elvis { lhs, rhs, .. } => e(lhs) || e(rhs),
        Expr::CallValue { callee, args, .. } => e(callee) || args.iter().any(e),
        Expr::Call { args, .. } | Expr::SuperCall { args, .. } => args.iter().any(e),
        Expr::MethodCall { target, args, .. } | Expr::SafeCall { target, args, .. } => {
            e(target) || args.iter().any(e)
        }
        _ => false,
    }
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
            let comparable = !matches!(a, Type::Array(_) | Type::Obj(..) | Type::Nullable(_));
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
        assert!(err("show([1] == 1)").contains("no admite [Int] y Int"));
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
            ("show(p == 1)", "no admite P y Int"),
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
                "compara con una clase o interfaz",
            ),
            ("thing A()\nthing B()\nshow(A() == B())", "no admite A y B"),
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
            ("fix x: Int? = 1\nshow(x == \"a\")", "no admite Int? y Str"),
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

    // --- M7: genéricos ---

    #[test]
    fn genericos_bien_tipados() {
        for src in [
            "thing Caja<T>(fix v: T) { fn dame() -> T = v }\nfix c = Caja(1)\nshow(c.dame() + 1)",
            "thing Caja<T>(fix v: T)\nfix c: Caja<Int> = Caja<Int>(1)\nshow(c.v)",
            "fix l: Lista<Str> = Lista()\nl.agrega(\"a\")\nshow(l.obtiene(0) + \"b\")",
            "fn id<T>(x: T) -> T = x\nshow(id(1) + 1)\nshow(id(\"a\") + \"b\")\nshow(id<Int>(2))",
            "fn dos<A, B>(a: A, b: B) -> Par<B, A> = Par(b, a)\nthing Par<A, B>(fix a: A, fix b: B)\nshow(dos(1, \"x\").a)",
            "thing Base<T>(fix x: T)\nthing Hijo<U>(y: U) : Base<U>(y)\nfix b: Base<Int> = Hijo(1)\nshow(b.x + 1)",
            "kind K<T> { fn f() -> T }\nthing A() : K<Int> { fn f() -> Int = 1 }\nfix k: K<Int> = A()\nshow(k.f() + 1)",
            "kind O<T> { fn antes(o: T) -> Bool }\nfn min<T: O<T>>(a: T, b: T) -> T {\n if a.antes(b) { return a }\n return b\n}",
            "thing A(fix n: Int)\nfn f<T: A>(x: T) -> Int = x.n",
            "fn f<T>(xs: [T]) -> Int = len(xs)\nshow(f([1, 2]))\nshow(f<Str>([]))",
            "fn eq<T>(a: T, b: T) -> Bool = a == b",
            "fn mu<T>(x: T) -> T? = x\nshow(mu(1) ?: 0)",
            "thing Nodo<T>(fix v: T, mut sig: Nodo<T>?)\nfix n = Nodo(1, nil)\nshow(n.sig == nil)",
            "fix m = Mapa<Str, Int>()\nm.pone(\"a\", 1)\nshow((m.obtiene(\"a\") ?: 0) + 1)",
            "fix p: Lista<Int>? = nil\nshow(p?.tam())",
            "show(hash(1) + 1)\nfix xs = [1]\nshow(pop(xs) + 1)",
        ] {
            assert_eq!(check(src), Ok(()), "{src}");
        }
    }

    #[test]
    fn errores_de_genericos() {
        let cases = [
            ("fix l: Lista<Int, Int> = Lista()", "necesita 1 argumento(s) de tipo"),
            ("fix l: Lista = Lista<Int>()", "necesita 1 argumento(s) de tipo"),
            ("thing A()\nfix a: A<Int> = A()", "no es genérica"),
            ("fix l = Lista()", "no se puede deducir el tipo `T`"),
            ("fix m = Mapa<Int>()", "necesita 2 argumento(s) de tipo, pero recibió 1"),
            ("fix l = Lista<Int>()\nl.agrega(\"a\")", "se esperaba Int, pero es Str"),
            ("fix l = Lista<Int>()\nfix s: Str = l.obtiene(0)", "se esperaba Str, pero es Int"),
            ("thing A()\nthing B() : A()\nfix l: Lista<A> = Lista<B>()", "se esperaba Lista<A>, pero es Lista<B>"),
            ("fix l = Lista<Cosa>()", "el tipo `Cosa` no existe"),
            ("fix l = Lista<Unit>()", "no puede ser Unit"),
            ("fn id<T>(x: T) -> T = x\nshow(id(1, 2))", "espera 1 argumento(s), pero recibió 2"),
            ("fn id<T>(x: T) -> T = x\nshow(id<Int, Int>(1))", "necesita 1 argumento(s) de tipo, pero recibió 2"),
            ("fn id(x: Int) -> Int = x\nshow(id<Int>(1))", "no es genérica"),
            ("fn mismo<T>(a: T, b: T) -> T = a\nshow(mismo(1, \"a\"))", "se esperaba Int, pero es Str"),
            ("fn vacia<T>() -> [T] = []\nfix x = vacia()", "no se puede deducir el tipo `T`"),
            ("fn f<T>(x: T) -> Int = x + 1", "no admite T y Int"),
            ("fn f<T>(x: T) -> Int = x", "se esperaba Int, pero es T"),
            ("fn f<T>(x: T) -> T = 1", "se esperaba T, pero es Int"),
            ("fn f<T>(x: T) -> Str = x.nombre", "solo los objetos tienen campos, pero esto es T"),
            ("fn f<T>(x: T) { x.m() }", "no tiene métodos: añade una cota"),
            ("fn f<T>(a: T, b: T) -> Bool = a < b", "no admite T y T"),
            ("fn f<T>(x: T) -> Bool = x is Lista", "compara un objeto"),
            ("fn f<T, T>(x: T) { }", "está repetido"),
            ("fn f<T: Int>(x: T) { }", "debe ser una clase o una interfaz"),
            ("thing A(fix n: Int)\nfn f<T: A>(x: T) { }\nf(1)", "no cumple la cota"),
            ("thing A(fix n: Int)\nthing B()\nfn f<T: A>(x: T) { }\nf(B())", "no cumple la cota"),
            ("kind O<T> { fn antes(o: T) -> Bool }\nthing N() : O<Int> { fn antes(o: Int) -> Bool = true }\nfn m<T: O<T>>(a: T) { }\nm(N())", "no cumple la cota"),
            ("thing Base<T>(fix x: T)\nthing H() : Base(1)", "necesita 1 argumento(s) de tipo"),
            ("thing Base<T>(fix x: T)\nthing H() : Base<Int>(\"a\")", "se esperaba Int, pero es Str"),
            ("kind K<T> { fn f() -> T }\nthing A() : K<Int> { fn f() -> Str = \"\" }", "pide () -> Int"),
            ("kind K<T> { fn f() -> T }\nthing A() : K { fn f() -> Int = 1 }", "necesita 1 argumento(s) de tipo"),
            ("thing Base<T>(fix x: T) { fn dame() -> T = x }\nthing H() : Base<Int>(1) { fn dame() -> Str = \"\" }", "sobrescribe el de `Base`"),
            ("fix l = Lista<Int>()\nshow(l is Lista<Int>)", "los argumentos de tipo se borran en ejecución"),
            ("fix l = Lista<Int>()\nshow(l as Lista<Int>)", "no se puede convertir a un tipo genérico"),
            ("show(pop(1))", "`pop` necesita un arreglo"),
            ("show(hash())", "espera 1 argumento(s)"),
            ("fn hash(x: Int) = x", "función integrada"),
            ("thing Lista()", "ya está en uso"),
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
    fn la_variancia_es_invariante_y_los_subtipos_funcionan_con_argumentos() {
        // Caja<Perro> no es Caja<Animal>, pero una subclase de Caja<T> sí es Caja<T>.
        let pre = "thing Animal()\nthing Perro() : Animal()\nthing Caja<T>(fix v: T)\nthing CajaP() : Caja<Perro>(Perro())\n";
        assert_eq!(check(&format!("{pre}fix c: Caja<Perro> = CajaP()")), Ok(()));
        assert_eq!(
            check(&format!("{pre}fix c: Caja<Animal> = Caja<Animal>(Perro())")),
            Ok(())
        );
        let message = err(&format!("{pre}fix c: Caja<Animal> = CajaP()"));
        assert!(
            message.contains("se esperaba Caja<Animal>, pero es CajaP"),
            "{message}"
        );
        let message = err(&format!("{pre}fix c: Caja<Animal> = Caja<Perro>(Perro())"));
        assert!(
            message.contains("se esperaba Caja<Animal>, pero es Caja<Perro>"),
            "{message}"
        );
    }

    #[test]
    fn los_parametros_de_tipo_no_se_filtran_a_otras_declaraciones() {
        let message = err("fn f<T>(x: T) -> T = x\nfn g(y: T) -> Int = 1");
        assert!(message.contains("el tipo `T` no existe"), "{message}");
        let message = err("thing A<T>(fix v: T)\nfix x: T = 1");
        assert!(message.contains("el tipo `T` no existe"), "{message}");
    }

    #[test]
    fn los_genericos_se_recuerdan_entre_entradas() {
        let mut c = Checker::new();
        let run = |c: &mut Checker, src: &str| {
            c.check(&parse(src).unwrap())
                .map(|_| ())
                .map_err(|d| d.message)
        };
        assert_eq!(
            run(
                &mut c,
                &format!("{}\nthing Caja<T>(fix v: T)", and23_syntax::PRELUDE)
            ),
            Ok(())
        );
        assert_eq!(run(&mut c, "fix a = Caja(1)"), Ok(()));
        assert_eq!(run(&mut c, "fix b: Caja<Int> = a"), Ok(()));
        assert!(run(&mut c, "fix d: Caja<Str> = a").is_err());
        assert!(run(&mut c, "fix e: Caja = a").is_err());
    }

    // --- M8: lambdas ---
    #[test]
    fn lambdas_bien_tipadas() {
        for src in [
            "fix f = { x: Int -> x + 1 }
show(f(2))",
            "fix f: (Int, Int) -> Int = { a, b -> a * b }",
            "fn ap(f: (Int) -> Int) -> Int = f(1)
show(ap({ x -> x }))
fn t(n: Int) -> Int = n
show(ap(t))",
            "fn id<T>(x: T) -> T = x
fn ap<A, B>(x: A, f: (A) -> B) -> B = f(x)
show(ap(1, { n -> \"s${n}\" }))",
            "thing C<T>(fix v: T) { fn mapea<R>(f: (T) -> R) -> C<R> = C(f(v)) }
fix c = C(1).mapea { x -> x > 0 }",
            "fix l = Lista<Int>()
fix m: Lista<Str> = l.map { x -> \"a\" }",
            "fix f: ((Int) -> Int)? = nil",
            "mut n = 0
fix f = { n += 1 }
f()",
            "fix f = { return 1 }",
        ] {
            assert_eq!(check(src), Ok(()), "{src}");
        }
    }

    #[test]
    fn errores_de_lambdas() {
        let cases = [
            (
                "fix f = { x -> x }",
                "no se puede deducir el tipo del parámetro `x`",
            ),
            (
                "fix f: (Int) -> Int = { a, b -> a }",
                "tiene 2 parámetro(s)",
            ),
            (
                "fix f: (Int) -> Int = { a -> true }",
                "se esperaba Int, pero es Bool",
            ),
            (
                "fix f: (Str) -> Int = { a: Int -> a }",
                "el parámetro `a` es Int, pero aquí se espera Str",
            ),
            ("fix f: Int = { 1 }", "se esperaba Int"),
            (
                "fix f = { a: Int -> a }
f(true)",
                "se esperaba Int, pero es Bool",
            ),
            (
                "fix n = 1
n(2)",
                "no es una función",
            ),
            (
                "fix f: ((Int) -> Int)? = nil
f(1)",
                "puede ser nil",
            ),
            ("fix f = len", "función integrada"),
            (
                "fn id<T>(x: T) -> T = x
fix f = id",
                "genérica",
            ),
            (
                "fn g(f: (Int) -> Int) = f(1)
g({ a, b -> a })",
                "tiene 2 parámetro(s)",
            ),
            ("thing A() { fn m<T, T>(x: T) { } }", "repetido"),
            (
                "thing A<T>() { fn m<T>(x: T) { } }",
                "ya es un parámetro de tipo de la clase",
            ),
        ];
        for (src, expected) in cases {
            let message = err(src);
            assert!(
                message.contains(expected),
                "{src}
{message}"
            );
        }
    }

    #[test]
    fn los_tipos_de_funcion_se_muestran_con_flecha() {
        let message = err("fix f: (Int) -> Int = 1");
        assert!(message.contains("(Int) -> Int"), "{message}");
        let message = err("fix f: ((Int) -> Int)? = 1");
        assert!(message.contains("((Int) -> Int)?"), "{message}");
    }

    #[test]
    fn la_asignabilidad_de_funciones() {
        assert_eq!(
            check(
                "fn a(x: Animal) -> Perro = Perro()
thing Animal()
thing Perro() : Animal()
fix f: (Perro) -> Animal = a"
            ),
            Ok(())
        );
        assert!(check(
            "thing Animal()
thing Perro() : Animal()
fn a(x: Perro) -> Perro = x
fix f: (Animal) -> Perro = a"
        )
        .is_err());
        assert!(check(
            "fn a(x: Int) -> Int = x
fix f: (Int) -> Unit = a"
        )
        .is_err());
    }
}
