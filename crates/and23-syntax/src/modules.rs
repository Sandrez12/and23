//! Módulos (M9): carga los archivos que importa un programa (`use "ruta"`), comprueba qué es público
//! (`open`) y los une en un único `Program`.
//!
//! Las declaraciones de cada archivo, salvo las del archivo principal, se renombran a `ruta.nombre`
//! (`util.ayuda`), así dos archivos pueden tener un `ayuda` privado sin chocar y el resto del
//! compilador no sabe que existen módulos. Todos los archivos comparten un espacio de posiciones
//! (cada uno empieza donde acabó el anterior), de modo que un `Diagnostic` sabe a qué archivo pertenece.

use crate::ast::*;
use crate::error::Diagnostic;
use crate::span::Span;
use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};

/// Archivos leídos y su posición en el espacio común, para mostrar errores con archivo y línea.
#[derive(Debug, Default)]
pub struct SourceMap {
    files: Vec<SourceFile>,
    /// Aviso extra para un error en una posición (p. ej. «`x` es privado en `util`»).
    hints: HashMap<usize, String>,
}

#[derive(Debug)]
struct SourceFile {
    name: String,
    src: String,
    base: usize,
}

impl SourceMap {
    fn add(&mut self, name: String, src: String) -> usize {
        // Un byte de separación para que el fin de un archivo no coincida con el inicio del siguiente.
        let base = self.files.last().map_or(0, |f| f.base + f.src.len() + 1);
        self.files.push(SourceFile { name, src, base });
        base
    }

    /// Los archivos leídos, unidos con un salto de línea entre cada dos: el texto sobre el que valen las posiciones.
    pub fn combined(&self) -> String {
        self.files
            .iter()
            .map(|f| f.src.as_str())
            .collect::<Vec<_>>()
            .join(
                "
",
            )
    }

    /// Los avisos asociados a posiciones, por posición.
    pub fn hints(&self) -> Vec<(usize, &str)> {
        let mut list: Vec<(usize, &str)> =
            self.hints.iter().map(|(p, h)| (*p, h.as_str())).collect();
        list.sort();
        list
    }

    fn file_at(&self, offset: usize) -> Option<&SourceFile> {
        self.files.iter().rev().find(|f| f.base <= offset)
    }

    /// Texto del error con el archivo, la línea y el subrayado que le corresponden.
    pub fn render(&self, d: &Diagnostic) -> String {
        let Some(file) = self.file_at(d.span.start) else {
            return format!("error: {}\n", d.message);
        };
        let mut local = Diagnostic::new(
            Span::new(
                d.span.start - file.base,
                d.span.end.saturating_sub(file.base),
            ),
            d.message.clone(),
        );
        if let Some(hint) = self.hints.get(&d.span.start) {
            local.message.push_str(hint);
        }
        let mut text = local.render(&file.name, &file.src);
        text.push_str(&d.trace_text(|span| match self.file_at(span.start) {
            Some(f) => {
                let at = Diagnostic::new(Span::new(span.start - f.base, span.start - f.base), "");
                let (l, c) = at.line_col(&f.src);
                format!("{}:{l}:{c}", f.name)
            }
            None => "?".to_string(),
        }));
        text
    }
}

/// Un programa ya unido y los archivos de los que salió.
#[derive(Debug)]
pub struct Loaded {
    pub program: Program,
    pub sources: SourceMap,
}

#[derive(Debug)]
pub enum Failure {
    /// No se pudo leer el archivo principal; el texto ya está listo para mostrarse.
    Io(String),
    Diag(Box<Diagnostic>, Box<SourceMap>),
}

impl Failure {
    /// Texto listo para mostrar al usuario.
    pub fn render(&self) -> String {
        match self {
            Failure::Io(text) => text.clone(),
            Failure::Diag(d, sources) => sources.render(d),
        }
    }
}

/// Lo que otros archivos ven de uno ya cargado.
struct ModuleInfo {
    id: String,
    /// Nombre abierto → nombre interno.
    exports: HashMap<String, String>,
    /// Declaraciones sin `open`, para avisar «es privado» cuando alguien las usa.
    privates: HashSet<String>,
}

struct Loader<R> {
    read: R,
    root: PathBuf,
    sources: SourceMap,
    done: HashMap<PathBuf, ModuleInfo>,
    stmts: Vec<Stmt>,
    /// Las pruebas del archivo principal (las de los módulos importados no se ejecutan).
    tests: Vec<crate::ast::TestDecl>,
}

/// Carga `entry` y todo lo que importa. `read` lee un archivo (así los tests no tocan el disco).
pub fn load<R>(entry: &str, read: R) -> Result<Loaded, Failure>
where
    R: FnMut(&Path) -> std::io::Result<String>,
{
    let key = normalize(Path::new(entry));
    let root = key.parent().map(Path::to_path_buf).unwrap_or_default();
    let mut loader = Loader {
        read,
        root,
        sources: SourceMap::default(),
        done: HashMap::new(),
        stmts: Vec::new(),
        tests: Vec::new(),
    };
    let mut stack = Vec::new();
    match loader.visit(&key, entry, true, &mut stack) {
        Ok(_) => Ok(Loaded {
            program: Program {
                stmts: loader.stmts,
                uses: Vec::new(),
                exports: Vec::new(),
                tests: loader.tests,
            },
            sources: loader.sources,
        }),
        Err(Fail::Io(text)) => Err(Failure::Io(text)),
        Err(Fail::Diag(d)) => Err(Failure::Diag(Box::new(d), Box::new(loader.sources))),
    }
}

enum Fail {
    Io(String),
    Diag(Diagnostic),
}

impl From<Diagnostic> for Fail {
    fn from(d: Diagnostic) -> Self {
        Fail::Diag(d)
    }
}

/// Quita los `.` y resuelve los `..` sin tocar el disco.
fn normalize(path: &Path) -> PathBuf {
    let mut out: Vec<Component> = Vec::new();
    for c in path.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => match out.last() {
                Some(Component::Normal(_)) => {
                    out.pop();
                }
                _ => out.push(c),
            },
            _ => out.push(c),
        }
    }
    out.iter().collect()
}

impl<R> Loader<R>
where
    R: FnMut(&Path) -> std::io::Result<String>,
{
    /// Identificador del módulo: su ruta sin extensión, relativa a la carpeta del archivo principal.
    fn id_of(&self, key: &Path) -> String {
        let rel = key.strip_prefix(&self.root).unwrap_or(key);
        let text = rel.with_extension("");
        text.components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/")
    }

    fn visit(
        &mut self,
        key: &Path,
        display: &str,
        is_entry: bool,
        stack: &mut Vec<PathBuf>,
    ) -> Result<(), Fail> {
        let src = match (self.read)(key) {
            Ok(src) => src,
            Err(e) => {
                return Err(Fail::Io(format!(
                    "error: no se pudo leer `{display}`: {e}\n"
                )))
            }
        };
        let base = self.sources.add(display.to_string(), src.clone());
        let program = crate::parse_at(&src, base)?;
        stack.push(key.to_path_buf());

        let id = self.id_of(key);
        if is_entry {
            self.tests = program.tests.clone();
        }
        let mut scope = Scope::new(&id, is_entry);
        for stmt in &program.stmts {
            scope.declare(stmt);
        }
        let exports_set: HashSet<&str> = program.exports.iter().map(String::as_str).collect();

        for u in &program.uses {
            let dep_path = if u.path.ends_with(".jb") {
                u.path.clone()
            } else {
                format!("{}.jb", u.path)
            };
            let dir = key.parent().unwrap_or(Path::new(""));
            let dep_key = normalize(&dir.join(&dep_path));
            if let Some(at) = stack.iter().position(|p| *p == dep_key) {
                let chain: Vec<String> = stack[at..]
                    .iter()
                    .chain(std::iter::once(&dep_key))
                    .map(|p| self.id_of(p))
                    .collect();
                return Err(Diagnostic::new(
                    u.span,
                    format!(
                        "dependencia circular entre archivos: {}",
                        chain.join(" -> ")
                    ),
                )
                .into());
            }
            if !self.done.contains_key(&dep_key) {
                let shown = dep_key.to_string_lossy().replace('\\', "/");
                match self.visit(&dep_key, &shown, false, stack) {
                    Ok(()) => {}
                    Err(Fail::Io(_)) => {
                        return Err(Diagnostic::new(
                            u.span,
                            format!(
                                "no se encontró el archivo `{shown}` (de `use \"{}\"`)",
                                u.path
                            ),
                        )
                        .into())
                    }
                    Err(e) => return Err(e),
                }
            }
            let dep = &self.done[&dep_key];
            scope.import(u, dep)?;
        }

        let mut renamer = Renamer::new(scope);
        let mut stmts = Vec::with_capacity(program.stmts.len());
        for mut stmt in program.stmts {
            renamer.top_stmt(&mut stmt);
            stmts.push(stmt);
        }
        if let Some(d) = renamer.error.take() {
            return Err(d.into());
        }
        for (pos, hint) in renamer.hints.drain(..) {
            self.sources.hints.insert(pos, hint);
        }
        let scope = renamer.scope;
        let info = ModuleInfo {
            exports: scope
                .own
                .iter()
                .filter(|(n, _)| exports_set.contains(n.as_str()))
                .map(|(n, m)| (n.clone(), m.clone()))
                .collect(),
            privates: scope
                .own
                .keys()
                .filter(|n| !exports_set.contains(n.as_str()))
                .cloned()
                .collect(),
            id,
        };
        self.stmts.extend(stmts);
        self.done.insert(key.to_path_buf(), info);
        stack.pop();
        Ok(())
    }
}

struct Alias {
    id: String,
    exports: HashMap<String, String>,
}

/// Qué significa cada nombre del nivel superior dentro de un archivo.
struct Scope {
    /// Nombre → nombre interno de lo que el archivo declara.
    own: HashMap<String, String>,
    /// Nombre → nombre interno de lo que importó sin alias.
    imports: HashMap<String, String>,
    /// Alias de `use ... como alias`.
    aliases: HashMap<String, Alias>,
    /// Nombre privado de un módulo importado → su identificador (solo para el aviso).
    foreign_privates: HashMap<String, String>,
    id: String,
    entry: bool,
}

impl Scope {
    fn new(id: &str, entry: bool) -> Self {
        Scope {
            own: HashMap::new(),
            imports: HashMap::new(),
            aliases: HashMap::new(),
            foreign_privates: HashMap::new(),
            id: id.to_string(),
            entry,
        }
    }

    fn mangle(&self, name: &str) -> String {
        if self.entry {
            name.to_string()
        } else {
            format!("{}.{}", self.id, name)
        }
    }

    fn declare(&mut self, stmt: &Stmt) {
        let name = match stmt {
            Stmt::Fn(d) => &d.name,
            Stmt::Thing(d) => &d.name,
            Stmt::Kind(d) => &d.name,
            Stmt::Let { name, .. } => name,
            _ => return,
        };
        let mangled = self.mangle(name);
        self.own.insert(name.clone(), mangled);
    }

    fn import(&mut self, u: &UseDecl, dep: &ModuleInfo) -> Result<(), Diagnostic> {
        for name in &dep.privates {
            self.foreign_privates
                .entry(name.clone())
                .or_insert_with(|| dep.id.clone());
        }
        match &u.alias {
            Some(alias) => {
                if self.own.contains_key(alias)
                    || self.imports.contains_key(alias)
                    || self.aliases.contains_key(alias)
                {
                    return Err(Diagnostic::new(
                        u.span,
                        format!("el alias `{alias}` ya está en uso en este archivo"),
                    ));
                }
                self.aliases.insert(
                    alias.clone(),
                    Alias {
                        id: dep.id.clone(),
                        exports: dep.exports.clone(),
                    },
                );
            }
            None => {
                for (name, mangled) in &dep.exports {
                    if self.own.contains_key(name) || self.aliases.contains_key(name) {
                        return Err(Diagnostic::new(
                            u.span,
                            format!(
                                "`{name}` se declara en este archivo y también viene de `use \"{}\"`",
                                u.path
                            ),
                        ));
                    }
                    if let Some(prev) = self.imports.get(name) {
                        if prev != mangled {
                            return Err(Diagnostic::new(
                                u.span,
                                format!(
                                    "`{name}` viene de dos `use` distintos; usa `como` para distinguirlos"
                                ),
                            ));
                        }
                    }
                    self.imports.insert(name.clone(), mangled.clone());
                }
            }
        }
        Ok(())
    }
}

/// Recorre un archivo cambiando cada nombre del nivel superior por su nombre interno.
struct Renamer {
    scope: Scope,
    /// Nombres locales visibles (parámetros, variables, campos, parámetros de tipo), por bloque.
    locals: Vec<HashSet<String>>,
    error: Option<Diagnostic>,
    hints: Vec<(usize, String)>,
}

impl Renamer {
    fn new(scope: Scope) -> Self {
        Renamer {
            scope,
            locals: Vec::new(),
            error: None,
            hints: Vec::new(),
        }
    }

    fn fail(&mut self, span: Span, message: String) {
        if self.error.is_none() {
            self.error = Some(Diagnostic::new(span, message));
        }
    }

    fn is_local(&self, name: &str) -> bool {
        self.locals.iter().any(|s| s.contains(name))
    }

    fn push(&mut self, names: impl IntoIterator<Item = String>) {
        self.locals.push(names.into_iter().collect());
    }

    fn pop(&mut self) {
        self.locals.pop();
    }

    fn bind(&mut self, name: &str) {
        if let Some(top) = self.locals.last_mut() {
            top.insert(name.to_string());
        }
    }

    /// Nombre interno de `name` tal como se usa en este punto.
    fn resolve(&mut self, name: &str, span: Span) -> String {
        if self.is_local(name) {
            return name.to_string();
        }
        if let Some(m) = self.scope.own.get(name) {
            return m.clone();
        }
        if let Some(m) = self.scope.imports.get(name) {
            return m.clone();
        }
        if let Some(id) = self.scope.foreign_privates.get(name) {
            self.hints.push((
                span.start,
                format!(" (`{name}` es privado en `{id}`: declárala con `open` allí)"),
            ));
        }
        name.to_string()
    }

    /// `alias.nombre` → nombre interno, si `alias` es un alias de `use` que nada tapa.
    fn qualified(&mut self, alias: &str, member: &str, span: Span) -> Option<String> {
        if self.is_local(alias)
            || self.scope.own.contains_key(alias)
            || self.scope.imports.contains_key(alias)
        {
            return None;
        }
        let (id, found) = {
            let a = self.scope.aliases.get(alias)?;
            (a.id.clone(), a.exports.get(member).cloned())
        };
        match found {
            Some(m) => Some(m),
            None => {
                self.fail(
                    span,
                    format!("`{id}` no tiene `{member}` abierto (se declara con `open`)"),
                );
                Some(member.to_string())
            }
        }
    }

    fn ty(&mut self, t: &mut TypeRef) {
        for a in &mut t.args {
            self.ty(a);
        }
        if let Some((alias, member)) = t.name.split_once('.') {
            if let Some(m) = self.qualified(alias, member, t.span) {
                t.name = m;
            }
        } else {
            t.name = self.resolve(&t.name, t.span);
        }
    }

    fn type_params(&mut self, params: &mut [TypeParam]) {
        for p in params {
            if let Some(b) = &mut p.bound {
                self.ty(b);
            }
        }
    }

    fn top_stmt(&mut self, stmt: &mut Stmt) {
        match stmt {
            Stmt::Fn(d) => self.fn_decl(d, true, &[]),
            Stmt::Thing(d) => self.thing(d),
            Stmt::Kind(d) => self.kind(d),
            Stmt::Let {
                name,
                mutable: _,
                ty,
                init,
                name_span,
                ..
            } => {
                if let Some(t) = ty {
                    self.ty(t);
                }
                self.expr(init);
                *name = self.resolve_own(name, *name_span);
            }
            other => self.stmt(other),
        }
    }

    /// Nombre interno de una declaración propia del archivo.
    fn resolve_own(&self, name: &str, _span: Span) -> String {
        self.scope
            .own
            .get(name)
            .cloned()
            .unwrap_or_else(|| name.to_string())
    }

    fn fn_decl(&mut self, d: &mut FnDecl, top: bool, extra: &[String]) {
        if top {
            d.name = self.resolve_own(&d.name, d.name_span);
        }
        let mut names: Vec<String> = extra.to_vec();
        names.extend(d.type_params.iter().map(|p| p.name.clone()));
        names.extend(d.params.iter().map(|p| p.name.clone()));
        self.push(names);
        self.type_params(&mut d.type_params);
        for p in &mut d.params {
            self.ty(&mut p.ty);
        }
        if let Some(r) = &mut d.ret {
            self.ty(r);
        }
        self.block(&mut d.body);
        self.pop();
    }

    fn kind(&mut self, d: &mut KindDecl) {
        d.name = self.resolve_own(&d.name, d.name_span);
        self.push(d.type_params.iter().map(|p| p.name.clone()));
        self.type_params(&mut d.type_params);
        for m in &mut d.methods {
            for p in &mut m.params {
                self.ty(&mut p.ty);
            }
            if let Some(r) = &mut m.ret {
                self.ty(r);
            }
        }
        self.pop();
    }

    fn thing(&mut self, d: &mut ThingDecl) {
        d.name = self.resolve_own(&d.name, d.name_span);
        let tparams: Vec<String> = d.type_params.iter().map(|p| p.name.clone()).collect();
        // Cabecera, padres y campos ven los parámetros del constructor.
        let mut init_scope = tparams.clone();
        init_scope.extend(d.params.iter().map(|p| p.name.clone()));
        self.push(init_scope);
        self.type_params(&mut d.type_params);
        for p in &mut d.params {
            self.ty(&mut p.ty);
        }
        for s in &mut d.supers {
            self.super_ref(s);
        }
        for f in &mut d.fields {
            if let Some(t) = &mut f.ty {
                self.ty(t);
            }
            self.expr(&mut f.init);
        }
        self.pop();
        // Los métodos ven los campos (y los parámetros de tipo), no los parámetros sueltos.
        let mut member_names = tparams;
        member_names.extend(
            d.params
                .iter()
                .filter(|p| p.mutable.is_some())
                .map(|p| p.name.clone()),
        );
        member_names.extend(d.fields.iter().map(|f| f.name.clone()));
        for m in &mut d.methods {
            self.fn_decl(m, false, &member_names);
        }
    }

    fn super_ref(&mut self, s: &mut SuperRef) {
        for t in &mut s.type_args {
            self.ty(t);
        }
        if let Some((alias, member)) = s.name.split_once('.') {
            if let Some(m) = self.qualified(alias, member, s.name_span) {
                s.name = m;
            }
        } else {
            s.name = self.resolve(&s.name, s.name_span);
        }
        if let Some(args) = &mut s.args {
            for a in args {
                self.expr(a);
            }
        }
    }

    fn block(&mut self, b: &mut Block) {
        self.push([]);
        for s in &mut b.stmts {
            self.stmt(s);
        }
        self.pop();
    }

    fn stmt(&mut self, stmt: &mut Stmt) {
        match stmt {
            Stmt::Show(e, _) | Stmt::Fail(e, _) | Stmt::Expr(e) => self.expr(e),
            Stmt::Return(e, _) => {
                if let Some(e) = e {
                    self.expr(e);
                }
            }
            Stmt::Break(_) | Stmt::Continue(_) => {}
            Stmt::Let { name, ty, init, .. } => {
                if let Some(t) = ty {
                    self.ty(t);
                }
                self.expr(init);
                let name = name.clone();
                self.bind(&name);
            }
            Stmt::Assign {
                name,
                name_span,
                value,
                ..
            } => {
                *name = self.resolve(name, *name_span);
                self.expr(value);
            }
            Stmt::IndexAssign {
                target,
                index,
                value,
                ..
            } => {
                self.expr(target);
                self.expr(index);
                self.expr(value);
            }
            Stmt::FieldAssign {
                target,
                name_span,
                value,
                ..
            } => {
                if let Expr::Var(alias, _) = target {
                    if self.scope.aliases.contains_key(alias.as_str())
                        && !self.is_local(alias)
                        && !self.scope.own.contains_key(alias.as_str())
                    {
                        let span = *name_span;
                        self.fail(
                            span,
                            "no se puede asignar a una declaración de otro archivo".into(),
                        );
                    }
                }
                self.expr(target);
                self.expr(value);
            }
            Stmt::If {
                cond,
                then,
                otherwise,
                ..
            } => {
                self.expr(cond);
                self.block(then);
                if let Some(o) = otherwise {
                    self.block(o);
                }
            }
            Stmt::While { cond, body, .. } => {
                self.expr(cond);
                self.block(body);
            }
            Stmt::Fn(_) | Stmt::Thing(_) | Stmt::Kind(_) => {}
            Stmt::Attempt { body, rescues, .. } => {
                self.block(body);
                for r in rescues {
                    self.ty(&mut r.ty);
                    self.push([r.name.clone()]);
                    self.block(&mut r.body);
                    self.pop();
                }
            }
        }
    }

    fn exprs(&mut self, list: &mut [Expr]) {
        for e in list {
            self.expr(e);
        }
    }

    fn types(&mut self, list: &mut [TypeRef]) {
        for t in list {
            self.ty(t);
        }
    }

    fn expr(&mut self, e: &mut Expr) {
        // `alias.nombre` y `alias.nombre(args)` se convierten en la variable o la llamada interna.
        match e {
            Expr::Field {
                target,
                name,
                name_span,
                span,
            } => {
                // `alias.Color.Rojo`: una variante de un enumerado de otro archivo.
                if let Expr::Field {
                    target: inner,
                    name: owner,
                    ..
                } = &**target
                {
                    if let Expr::Var(alias, _) = &**inner {
                        if !self.is_local(alias)
                            && !self.scope.own.contains_key(alias)
                            && !self.scope.imports.contains_key(alias)
                        {
                            let found =
                                self.scope.aliases.get(alias).and_then(|a| {
                                    a.exports.get(&format!("{owner}.{name}")).cloned()
                                });
                            if let Some(m) = found {
                                *e = Expr::Var(m, *span);
                                return;
                            }
                        }
                    }
                }
                if let Expr::Var(alias, _) = &**target {
                    if let Some(m) = self.qualified(alias, name, *name_span) {
                        *e = Expr::Var(m, *span);
                        return;
                    }
                    // `Color.Rojo`: una variante de un enumerado (se declara como `Color.Rojo`).
                    if !self.is_local(alias) {
                        let full = format!("{alias}.{name}");
                        if self.scope.own.contains_key(&full)
                            || self.scope.imports.contains_key(&full)
                        {
                            *e = Expr::Var(self.resolve(&full, *span), *span);
                            return;
                        }
                    }
                }
            }
            Expr::MethodCall {
                target,
                name,
                name_span,
                type_args,
                args,
                span,
            } => {
                if let Expr::Var(alias, _) = &**target {
                    if let Some(m) = self.qualified(alias, name, *name_span) {
                        let mut type_args = std::mem::take(type_args);
                        let mut args = std::mem::take(args);
                        self.types(&mut type_args);
                        self.exprs(&mut args);
                        *e = Expr::Call {
                            name: m,
                            type_args,
                            args,
                            span: *span,
                        };
                        return;
                    }
                }
            }
            _ => {}
        }
        match e {
            Expr::Int(..) | Expr::Float(..) | Expr::Bool(..) | Expr::Str(..) => {}
            Expr::Nil(_) | Expr::This(_) => {}
            Expr::Var(name, span) => *name = self.resolve(name, *span),
            Expr::Interp { parts, .. } => {
                for p in parts {
                    if let InterpPart::Expr(x) = p {
                        self.expr(x);
                    }
                }
            }
            Expr::Array { elems, .. } => self.exprs(elems),
            Expr::Index { target, index, .. } => {
                self.expr(target);
                self.expr(index);
            }
            Expr::Unary { expr, .. } => self.expr(expr),
            Expr::Binary { lhs, rhs, .. } | Expr::Elvis { lhs, rhs, .. } => {
                self.expr(lhs);
                self.expr(rhs);
            }
            Expr::Call {
                name,
                type_args,
                args,
                span,
            } => {
                *name = self.resolve(name, *span);
                self.types(type_args);
                self.exprs(args);
            }
            Expr::SafeField { target, .. } | Expr::Field { target, .. } => self.expr(target),
            Expr::SafeCall { target, args, .. } => {
                self.expr(target);
                self.exprs(args);
            }
            Expr::SuperCall { args, .. } => self.exprs(args),
            Expr::Is { expr, ty, .. } | Expr::Cast { expr, ty, .. } => {
                self.expr(expr);
                self.ty(ty);
            }
            Expr::MethodCall {
                target,
                type_args,
                args,
                ..
            } => {
                self.expr(target);
                self.types(type_args);
                self.exprs(args);
            }
            Expr::CallValue { callee, args, .. } => {
                self.expr(callee);
                self.exprs(args);
            }
            Expr::Lambda { params, body, .. } => {
                for p in params.iter_mut() {
                    if let Some(t) = &mut p.ty {
                        self.ty(t);
                    }
                }
                self.push(params.iter().map(|p| p.name.clone()));
                self.block(body);
                self.pop();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn files(list: &[(&str, &str)]) -> HashMap<PathBuf, String> {
        list.iter()
            .map(|(p, s)| (normalize(Path::new(p)), s.to_string()))
            .collect()
    }

    fn load_mem(entry: &str, list: &[(&str, &str)]) -> Result<Loaded, Failure> {
        let fs = files(list);
        load(entry, |p| {
            fs.get(p)
                .cloned()
                .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "no existe"))
        })
    }

    fn names(l: &Loaded) -> Vec<String> {
        l.program
            .stmts
            .iter()
            .filter_map(|s| match s {
                Stmt::Fn(d) => Some(d.name.clone()),
                Stmt::Thing(d) => Some(d.name.clone()),
                Stmt::Let { name, .. } => Some(name.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn los_nombres_de_otros_archivos_llevan_su_ruta() {
        let l = load_mem(
            "p/main.jb",
            &[
                ("p/main.jb", "use \"util\"\nfn main() { show(doble(2)) }"),
                (
                    "p/util.jb",
                    "open fn doble(x: Int) -> Int = x * 2\nfn ayuda() {}",
                ),
            ],
        )
        .unwrap();
        assert_eq!(names(&l), vec!["util.doble", "util.ayuda", "main"]);
    }

    #[test]
    fn un_nombre_privado_no_se_ve_desde_fuera() {
        let err = load_mem(
            "main.jb",
            &[
                ("main.jb", "use \"util\"\nshow(ayuda())"),
                ("util.jb", "fn ayuda() -> Int = 1"),
            ],
        );
        // El cargador no falla: el verificador dirá que `ayuda` no existe, con el aviso asociado.
        let l = err.unwrap();
        assert!(l.sources.hints.values().any(|h| h.contains("privado")));
    }

    #[test]
    fn dos_archivos_pueden_tener_el_mismo_nombre_privado() {
        let l = load_mem(
            "main.jb",
            &[
                ("main.jb", "use \"a\"\nuse \"b\"\nshow(fa() + fb())"),
                (
                    "a.jb",
                    "fn ayuda() -> Int = 1\nopen fn fa() -> Int = ayuda()",
                ),
                (
                    "b.jb",
                    "fn ayuda() -> Int = 2\nopen fn fb() -> Int = ayuda()",
                ),
            ],
        )
        .unwrap();
        let n = names(&l);
        assert!(n.contains(&"a.ayuda".to_string()) && n.contains(&"b.ayuda".to_string()));
    }

    #[test]
    fn un_ciclo_se_rechaza_con_la_cadena() {
        let Err(Failure::Diag(d, _)) = load_mem(
            "main.jb",
            &[
                ("main.jb", "use \"a\""),
                ("a.jb", "use \"b\""),
                ("b.jb", "use \"a\""),
            ],
        ) else {
            panic!("debía fallar")
        };
        assert!(d.message.contains("a -> b -> a"), "{}", d.message);
    }

    #[test]
    fn un_archivo_inexistente_senala_el_use() {
        let Err(f) = load_mem("main.jb", &[("main.jb", "use \"nada\"")]) else {
            panic!()
        };
        let text = f.render();
        assert!(
            text.contains("no se encontró el archivo `nada.jb`"),
            "{text}"
        );
        assert!(text.contains("main.jb:1:1"), "{text}");
    }

    #[test]
    fn el_alias_cualifica_y_los_errores_dicen_el_archivo() {
        let l = load_mem(
            "main.jb",
            &[
                (
                    "main.jb",
                    "use \"u\" como u\nshow(u.f(1))\nfix x: u.T = u.T(1)",
                ),
                (
                    "u.jb",
                    "open fn f(x: Int) -> Int = x\nopen thing T(fix v: Int)",
                ),
            ],
        )
        .unwrap();
        let text = format!("{:?}", l.program);
        assert!(text.contains("u.f") && text.contains("u.T"));
        let nf = load_mem(
            "main.jb",
            &[
                ("main.jb", "use \"u\" como u\nshow(u.g(1))"),
                ("u.jb", "open fn f(x: Int) -> Int = x"),
            ],
        );
        let Err(f) = nf else { panic!() };
        assert!(f.render().contains("no tiene `g` abierto"));
    }

    #[test]
    fn una_variable_local_tapa_al_nombre_importado() {
        let l = load_mem(
            "main.jb",
            &[
                (
                    "main.jb",
                    "use \"u\"\nfn g() -> Int {\n fix v = 1\n return v\n}",
                ),
                ("u.jb", "open fix v = 5"),
            ],
        )
        .unwrap();
        let text = format!("{:?}", l.program.stmts);
        assert!(text.contains("Var(\"v\""), "{text}");
        assert!(!text.contains("Var(\"u.v\""), "{text}");
    }
}
