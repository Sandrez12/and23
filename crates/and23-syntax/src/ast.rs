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
            | Expr::Var(_, span) => *span,
            Expr::Interp { span, .. }
            | Expr::Array { span, .. }
            | Expr::Index { span, .. }
            | Expr::Unary { span, .. }
            | Expr::Binary { span, .. }
            | Expr::Call { span, .. } => *span,
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
    pub params: Vec<Param>,
    pub ret: Option<TypeRef>,
    /// El cuerpo se escribió como `= expr` (sin llaves): su tipo de retorno puede inferirse.
    pub expr_body: bool,
    pub body: Block,
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
}

impl Stmt {
    pub fn span(&self) -> Span {
        match self {
            Stmt::Show(_, span)
            | Stmt::Return(_, span)
            | Stmt::Break(span)
            | Stmt::Continue(span) => *span,
            Stmt::Expr(expr) => expr.span(),
            Stmt::Let { span, .. }
            | Stmt::Assign { span, .. }
            | Stmt::IndexAssign { span, .. }
            | Stmt::If { span, .. }
            | Stmt::While { span, .. } => *span,
            Stmt::Fn(decl) => decl.span,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    pub stmts: Vec<Stmt>,
}
