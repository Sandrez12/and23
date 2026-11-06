use crate::span::Span;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    Neg,
    Not,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
}

impl BinOp {
    pub fn symbol(self) -> &'static str {
        match self {
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::Div => "/",
            BinOp::Rem => "%",
            BinOp::Eq => "==",
            BinOp::Ne => "!=",
            BinOp::Lt => "<",
            BinOp::Le => "<=",
            BinOp::Gt => ">",
            BinOp::Ge => ">=",
            BinOp::And => "&&",
            BinOp::Or => "||",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Int(i64, Span),
    Float(f64, Span),
    Bool(bool, Span),
    Var(String, Span),
    /// Cadena sin interpolaciones.
    Str(String, Span),
    /// Cadena con `${expr}`: texto y expresiones alternados.
    Interp {
        parts: Vec<InterpPart>,
        span: Span,
    },
    /// `[a, b, c]`
    Array {
        elems: Vec<Expr>,
        span: Span,
    },
    /// `a[i]`
    Index {
        target: Box<Expr>,
        index: Box<Expr>,
        span: Span,
    },
    Unary {
        op: UnOp,
        expr: Box<Expr>,
        span: Span,
    },
    Binary {
        op: BinOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
        span: Span,
    },
    Call {
        name: String,
        /// Argumentos de tipo explícitos: `Lista<Int>(...)`, `primero<Int>(xs)`.
        type_args: Vec<TypeRef>,
        args: Vec<Expr>,
        span: Span,
    },
    /// `this`: el objeto sobre el que corre un método.
    This(Span),
    /// `nil`: la ausencia de valor de un tipo opcional (`T?`).
    Nil(Span),
    /// `obj?.campo`: nil si `obj` es nil.
    SafeField {
        target: Box<Expr>,
        name: String,
        name_span: Span,
        span: Span,
    },
    /// `obj?.metodo(args)`: nil (y no llama) si `obj` es nil.
    SafeCall {
        target: Box<Expr>,
        name: String,
        name_span: Span,
        args: Vec<Expr>,
        span: Span,
    },
    /// `a ?: b`: `a` si no es nil; si lo es, `b`.
    Elvis {
        lhs: Box<Expr>,
        rhs: Box<Expr>,
        span: Span,
    },
    /// `super.metodo(args)`: el método de la clase padre, sin despacho virtual.
    SuperCall {
        name: String,
        name_span: Span,
        args: Vec<Expr>,
        span: Span,
    },
    /// `expr is Tipo`
    Is {
        expr: Box<Expr>,
        ty: TypeRef,
        span: Span,
    },
    /// `expr as Tipo`: conversión comprobada al ejecutar.
    Cast {
        expr: Box<Expr>,
        ty: TypeRef,
        span: Span,
    },
    /// `obj.campo`
    Field {
        target: Box<Expr>,
        name: String,
        name_span: Span,
        span: Span,
    },
    /// `obj.metodo(args)`
    MethodCall {
        target: Box<Expr>,
        name: String,
        name_span: Span,
        args: Vec<Expr>,
        span: Span,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum InterpPart {
    Text(String),
    Expr(Expr),
}

impl Expr {
    pub fn span(&self) -> Span {
        match self {
            Expr::Int(_, span)
            | Expr::Float(_, span)
            | Expr::Bool(_, span)
            | Expr::Str(_, span)
            | Expr::Var(_, span)
            | Expr::Nil(span)
            | Expr::This(span) => *span,
            Expr::Interp { span, .. }
            | Expr::Array { span, .. }
            | Expr::Index { span, .. }
            | Expr::Unary { span, .. }
            | Expr::Binary { span, .. }
            | Expr::Call { span, .. }
            | Expr::Field { span, .. }
            | Expr::SuperCall { span, .. }
            | Expr::SafeField { span, .. }
            | Expr::SafeCall { span, .. }
            | Expr::Elvis { span, .. }
            | Expr::Is { span, .. }
            | Expr::Cast { span, .. }
            | Expr::MethodCall { span, .. } => *span,
        }
    }
}

/// Anotación de tipo (`Int`, `Float`, `Bool`, `Unit`); la verifica `and23-types`.
#[derive(Debug, Clone, PartialEq)]
pub struct TypeRef {
    pub name: String,
    /// Argumentos de tipo: `[Int]` es `Array` con `[Int]` como argumento.
    pub args: Vec<TypeRef>,
    pub span: Span,
}

/// Parámetro de tipo de una declaración: `T` o `T: Cota`.
#[derive(Debug, Clone, PartialEq)]
pub struct TypeParam {
    pub name: String,
    pub name_span: Span,
    pub bound: Option<TypeRef>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub name: String,
    pub ty: TypeRef,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    pub stmts: Vec<Stmt>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FnDecl {
    pub name: String,
    pub name_span: Span,
    /// Solo las funciones del nivel superior pueden ser genéricas.
    pub type_params: Vec<TypeParam>,
    pub params: Vec<Param>,
    pub ret: Option<TypeRef>,
    /// El cuerpo se escribió como `= expr` (sin llaves): su tipo de retorno puede inferirse.
    pub expr_body: bool,
    pub body: Block,
    pub span: Span,
}

/// Parámetro de la cabecera de una clase. Con `fix`/`mut` (`mutable` es `Some`) además crea un campo.
#[derive(Debug, Clone, PartialEq)]
pub struct CtorParam {
    pub name: String,
    pub name_span: Span,
    pub ty: TypeRef,
    pub mutable: Option<bool>,
    pub span: Span,
}

/// Campo declarado en el cuerpo: `mut edad = 0`.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldDecl {
    pub name: String,
    pub name_span: Span,
    pub mutable: bool,
    pub ty: Option<TypeRef>,
    pub init: Expr,
    pub span: Span,
}

/// Elemento tras `:` en la cabecera de una clase: `Padre(args)` (clase) o `Interfaz` (sin paréntesis).
#[derive(Debug, Clone, PartialEq)]
pub struct SuperRef {
    pub name: String,
    pub name_span: Span,
    /// `Padre<Int>(...)` o `Interfaz<Int>`.
    pub type_args: Vec<TypeRef>,
    pub args: Option<Vec<Expr>>,
    pub span: Span,
}

/// Método sin cuerpo de una interfaz.
#[derive(Debug, Clone, PartialEq)]
pub struct KindMethod {
    pub name: String,
    pub name_span: Span,
    pub params: Vec<Param>,
    pub ret: Option<TypeRef>,
    pub span: Span,
}

/// `kind Nombre { fn m(params) -> T ... }`
#[derive(Debug, Clone, PartialEq)]
pub struct KindDecl {
    pub name: String,
    pub name_span: Span,
    pub type_params: Vec<TypeParam>,
    pub methods: Vec<KindMethod>,
    pub span: Span,
}

/// `thing Nombre(params) : Padre(args), Interfaz { campos y métodos }`
#[derive(Debug, Clone, PartialEq)]
pub struct ThingDecl {
    pub name: String,
    pub name_span: Span,
    pub type_params: Vec<TypeParam>,
    pub params: Vec<CtorParam>,
    pub supers: Vec<SuperRef>,
    pub fields: Vec<FieldDecl>,
    pub methods: Vec<FnDecl>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    /// `show(expr)`: imprime el valor.
    Show(Expr, Span),
    /// Expresión suelta; el REPL imprime su valor.
    Expr(Expr),
    /// `fix x = ...` o `mut x = ...`.
    Let {
        name: String,
        name_span: Span,
        mutable: bool,
        ty: Option<TypeRef>,
        init: Expr,
        span: Span,
    },
    /// `x = ...` o, con `op`, `x += ...`.
    Assign {
        name: String,
        name_span: Span,
        op: Option<BinOp>,
        value: Expr,
        span: Span,
    },
    /// `a[i] = v` o, con `op`, `a[i] += v`.
    IndexAssign {
        target: Expr,
        index: Expr,
        op: Option<BinOp>,
        value: Expr,
        span: Span,
    },
    /// `obj.campo = v` o, con `op`, `obj.campo += v`.
    FieldAssign {
        target: Expr,
        name: String,
        name_span: Span,
        op: Option<BinOp>,
        value: Expr,
        span: Span,
    },
    If {
        cond: Expr,
        then: Block,
        otherwise: Option<Block>,
        span: Span,
    },
    While {
        cond: Expr,
        body: Block,
        span: Span,
    },
    Return(Option<Expr>, Span),
    Break(Span),
    Continue(Span),
    /// Solo al nivel superior del archivo.
    Fn(Box<FnDecl>),
    /// Clase; solo al nivel superior del archivo.
    Thing(Box<ThingDecl>),
    /// Interfaz; solo al nivel superior del archivo.
    Kind(Box<KindDecl>),
    /// `attempt { ... } rescue e: Tipo { ... }`
    Attempt {
        body: Block,
        rescues: Vec<Rescue>,
        span: Span,
    },
    /// `fail expr`: lanza un objeto de una clase que hereda de `Fallo`.
    Fail(Expr, Span),
}

/// Cláusula `rescue nombre: Tipo { ... }` de un `attempt`.
#[derive(Debug, Clone, PartialEq)]
pub struct Rescue {
    pub name: String,
    pub name_span: Span,
    pub ty: TypeRef,
    pub body: Block,
    pub span: Span,
}

impl Stmt {
    pub fn span(&self) -> Span {
        match self {
            Stmt::Show(_, span)
            | Stmt::Fail(_, span)
            | Stmt::Return(_, span)
            | Stmt::Break(span)
            | Stmt::Continue(span) => *span,
            Stmt::Expr(expr) => expr.span(),
            Stmt::Let { span, .. }
            | Stmt::Assign { span, .. }
            | Stmt::IndexAssign { span, .. }
            | Stmt::FieldAssign { span, .. }
            | Stmt::Attempt { span, .. }
            | Stmt::If { span, .. }
            | Stmt::While { span, .. } => *span,
            Stmt::Fn(decl) => decl.span,
            Stmt::Thing(decl) => decl.span,
            Stmt::Kind(decl) => decl.span,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    pub stmts: Vec<Stmt>,
}
