//! Volcado del AST en texto, con posiciones en caracteres. Sirve para comparar este parser con el escrito en And23
//! (`mn/parser.jb`): los dos tienen que producir exactamente las mismas líneas.
//!
//! Cada nodo es una línea `Etiqueta atributo atributo...` y sus hijos van debajo, con dos espacios más de
//! sangría. Un hijo opcional que falta es un nodo `None`. Los textos con espacios o escapes van entre comillas.
//! Al final de la línea van `@desde-hasta` (el nodo) y `#desde-hasta` (su nombre), si los tiene.

use crate::ast::*;
use crate::lexer::escape_dump;
use crate::span::Span;

struct N {
    tag: &'static str,
    attrs: Vec<String>,
    kids: Vec<N>,
    /// `@desde-hasta` del nodo y `#desde-hasta` de su nombre, en caracteres; vacíos si no los tiene.
    span: String,
    name: String,
}

thread_local! {
    /// Índice de byte -> índice de carácter del texto que se vuelca.
    static AT: std::cell::RefCell<Vec<usize>> = const { std::cell::RefCell::new(Vec::new()) };
}

fn chars(span: Span) -> (usize, usize) {
    AT.with(|at| {
        let at = at.borrow();
        let last = at.len() - 1;
        (at[span.start.min(last)], at[span.end.min(last)])
    })
}

impl N {
    fn at(mut self, span: Span) -> N {
        let (a, b) = chars(span);
        self.span = format!("@{a}-{b}");
        self
    }

    fn named(mut self, span: Span) -> N {
        let (a, b) = chars(span);
        self.name = format!("#{a}-{b}");
        self
    }
}

fn n(tag: &'static str, attrs: Vec<String>, kids: Vec<N>) -> N {
    N {
        tag,
        attrs,
        kids,
        span: String::new(),
        name: String::new(),
    }
}

fn quoted(s: &str) -> String {
    format!("\"{}\"", escape_dump(s))
}

fn none() -> N {
    n("None", vec![], vec![])
}

fn opt<T>(x: &Option<T>, f: impl Fn(&T) -> N) -> N {
    x.as_ref().map_or_else(none, f)
}

fn write(node: &N, depth: usize, out: &mut String) {
    out.push_str(&"  ".repeat(depth));
    out.push_str(node.tag);
    for a in &node.attrs {
        out.push(' ');
        out.push_str(a);
    }
    for extra in [&node.span, &node.name] {
        if !extra.is_empty() {
            out.push(' ');
            out.push_str(extra);
        }
    }
    out.push('\n');
    for k in &node.kids {
        write(k, depth + 1, out);
    }
}

/// El programa completo como texto.
pub fn dump_program(p: &Program, src: &str) -> String {
    AT.with(|at| {
        let mut at = at.borrow_mut();
        at.clear();
        at.resize(src.len() + 1, 0);
        let mut count = 0;
        for (b, c) in src.char_indices() {
            at[b] = count;
            count += 1;
            for k in 1..c.len_utf8() {
                at[b + k] = count;
            }
        }
        at[src.len()] = count;
    });
    let uses = n(
        "Uses",
        vec![],
        p.uses
            .iter()
            .map(|u| {
                n(
                    "Use",
                    vec![
                        quoted(&u.path),
                        u.alias.clone().unwrap_or_else(|| "-".into()),
                    ],
                    vec![],
                )
                .at(u.span)
            })
            .collect(),
    );
    let exports = n("Exports", p.exports.clone(), vec![]);
    let tests = n(
        "Tests",
        vec![],
        p.tests
            .iter()
            .map(|t| n("Test", vec![quoted(&t.name), t.func.clone()], vec![]).at(t.span))
            .collect(),
    );
    let stmts = n("Stmts", vec![], p.stmts.iter().map(stmt).collect());
    let root = n("Program", vec![], vec![uses, exports, tests, stmts]);
    let mut out = String::new();
    write(&root, 0, &mut out);
    out
}

fn ty(t: &TypeRef) -> N {
    n(
        "Type",
        vec![t.name.clone()],
        t.args.iter().map(ty).collect(),
    )
    .at(t.span)
}

fn targs(list: &[TypeRef]) -> N {
    n("TArgs", vec![], list.iter().map(ty).collect())
}

fn args(list: &[Expr]) -> N {
    n("Args", vec![], list.iter().map(expr).collect())
}

fn tparams(list: &[TypeParam]) -> N {
    n(
        "TypeParams",
        vec![],
        list.iter()
            .map(|p| {
                n("TypeParam", vec![p.name.clone()], vec![opt(&p.bound, ty)])
                    .at(p.span)
                    .named(p.name_span)
            })
            .collect(),
    )
}

fn params(list: &[Param]) -> N {
    n(
        "Params",
        vec![],
        list.iter()
            .map(|p| n("Param", vec![p.name.clone()], vec![ty(&p.ty)]).at(p.span))
            .collect(),
    )
}

fn block(b: &Block) -> N {
    n("Block", vec![], b.stmts.iter().map(stmt).collect()).at(b.span)
}

fn assign_op(op: Option<BinOp>) -> String {
    op.map_or_else(|| "=".to_string(), |o| format!("{}=", o.symbol()))
}

fn expr(e: &Expr) -> N {
    let node = expr_node(e);
    let node = node.at(e.span());
    match e {
        Expr::SafeField { name_span, .. }
        | Expr::SafeCall { name_span, .. }
        | Expr::SuperCall { name_span, .. }
        | Expr::Field { name_span, .. }
        | Expr::MethodCall { name_span, .. } => node.named(*name_span),
        _ => node,
    }
}

fn expr_node(e: &Expr) -> N {
    match e {
        Expr::Int(v, _) => n("Int", vec![v.to_string()], vec![]),
        Expr::Float(v, _) => n("Float", vec![format!("{v:?}")], vec![]),
        Expr::Bool(v, _) => n("Bool", vec![v.to_string()], vec![]),
        Expr::Var(name, _) => n("Var", vec![name.clone()], vec![]),
        Expr::Str(s, _) => n("Str", vec![quoted(s)], vec![]),
        Expr::Interp { parts, .. } => n(
            "Interp",
            vec![],
            parts
                .iter()
                .map(|p| match p {
                    InterpPart::Text(t) => n("Text", vec![quoted(t)], vec![]),
                    InterpPart::Expr(e) => expr(e),
                })
                .collect(),
        ),
        Expr::Array { elems, .. } => n("Array", vec![], elems.iter().map(expr).collect()),
        Expr::Index { target, index, .. } => n("Index", vec![], vec![expr(target), expr(index)]),
        Expr::Unary { op, expr: e, .. } => n(
            "Unary",
            vec![match op {
                UnOp::Neg => "Neg".into(),
                UnOp::Not => "Not".into(),
            }],
            vec![expr(e)],
        ),
        Expr::Binary { op, lhs, rhs, .. } => n(
            "Binary",
            vec![op.symbol().into()],
            vec![expr(lhs), expr(rhs)],
        ),
        Expr::Call {
            name,
            type_args,
            args: a,
            ..
        } => n("Call", vec![name.clone()], vec![targs(type_args), args(a)]),
        Expr::This(_) => n("This", vec![], vec![]),
        Expr::Nil(_) => n("Nil", vec![], vec![]),
        Expr::SafeField { target, name, .. } => {
            n("SafeField", vec![name.clone()], vec![expr(target)])
        }
        Expr::SafeCall {
            target,
            name,
            args: a,
            ..
        } => n("SafeCall", vec![name.clone()], vec![expr(target), args(a)]),
        Expr::Elvis { lhs, rhs, .. } => n("Elvis", vec![], vec![expr(lhs), expr(rhs)]),
        Expr::SuperCall { name, args: a, .. } => n("SuperCall", vec![name.clone()], vec![args(a)]),
        Expr::Is { expr: e, ty: t, .. } => n("Is", vec![], vec![expr(e), ty(t)]),
        Expr::Cast { expr: e, ty: t, .. } => n("Cast", vec![], vec![expr(e), ty(t)]),
        Expr::Field { target, name, .. } => n("Field", vec![name.clone()], vec![expr(target)]),
        Expr::MethodCall {
            target,
            name,
            type_args,
            args: a,
            ..
        } => n(
            "MethodCall",
            vec![name.clone()],
            vec![expr(target), targs(type_args), args(a)],
        ),
        Expr::CallValue {
            callee, args: a, ..
        } => n("CallValue", vec![], vec![expr(callee), args(a)]),
        Expr::Lambda {
            params: ps, body, ..
        } => n(
            "Lambda",
            vec![],
            vec![
                n(
                    "LParams",
                    vec![],
                    ps.iter()
                        .map(|p| {
                            n("LParam", vec![p.name.clone()], vec![opt(&p.ty, ty)])
                                .named(p.name_span)
                        })
                        .collect(),
                ),
                block(body),
            ],
        ),
    }
}

fn stmt(s: &Stmt) -> N {
    let node = stmt_node(s);
    match s {
        Stmt::Let { name_span, .. }
        | Stmt::Assign { name_span, .. }
        | Stmt::FieldAssign { name_span, .. } => node.at(s.span()).named(*name_span),
        Stmt::Expr(_) | Stmt::Fn(_) | Stmt::Thing(_) | Stmt::Kind(_) => node,
        _ => node.at(s.span()),
    }
}

fn stmt_node(s: &Stmt) -> N {
    match s {
        Stmt::Show(e, _) => n("Show", vec![], vec![expr(e)]),
        Stmt::Expr(e) => n("ExprStmt", vec![], vec![expr(e)]),
        Stmt::Let {
            name,
            mutable,
            ty: t,
            init,
            ..
        } => n(
            "Let",
            vec![name.clone(), if *mutable { "mut" } else { "fix" }.into()],
            vec![opt(t, ty), expr(init)],
        ),
        Stmt::Assign {
            name, op, value, ..
        } => n(
            "Assign",
            vec![name.clone(), assign_op(*op)],
            vec![expr(value)],
        ),
        Stmt::IndexAssign {
            target,
            index,
            op,
            value,
            ..
        } => n(
            "IndexAssign",
            vec![assign_op(*op)],
            vec![expr(target), expr(index), expr(value)],
        ),
        Stmt::FieldAssign {
            target,
            name,
            op,
            value,
            ..
        } => n(
            "FieldAssign",
            vec![name.clone(), assign_op(*op)],
            vec![expr(target), expr(value)],
        ),
        Stmt::If {
            cond,
            then,
            otherwise,
            ..
        } => n(
            "If",
            vec![],
            vec![expr(cond), block(then), opt(otherwise, block)],
        ),
        Stmt::While { cond, body, .. } => n("While", vec![], vec![expr(cond), block(body)]),
        Stmt::Return(e, _) => n("Return", vec![], vec![opt(e, expr)]),
        Stmt::Break(_) => n("Break", vec![], vec![]),
        Stmt::Continue(_) => n("Continue", vec![], vec![]),
        Stmt::Fn(d) => fn_decl(d),
        Stmt::Thing(d) => n(
            "Thing",
            vec![d.name.clone()],
            vec![
                tparams(&d.type_params),
                n(
                    "CtorParams",
                    vec![],
                    d.params
                        .iter()
                        .map(|p| {
                            n(
                                "CtorParam",
                                vec![
                                    p.name.clone(),
                                    match p.mutable {
                                        Some(true) => "mut",
                                        Some(false) => "fix",
                                        None => "-",
                                    }
                                    .into(),
                                ],
                                vec![ty(&p.ty)],
                            )
                            .at(p.span)
                            .named(p.name_span)
                        })
                        .collect(),
                ),
                n(
                    "Supers",
                    vec![],
                    d.supers
                        .iter()
                        .map(|s| {
                            n(
                                "Super",
                                vec![s.name.clone()],
                                vec![targs(&s.type_args), opt(&s.args, |a| args(a))],
                            )
                            .at(s.span)
                            .named(s.name_span)
                        })
                        .collect(),
                ),
                n(
                    "Fields",
                    vec![],
                    d.fields
                        .iter()
                        .map(|f| {
                            n(
                                "FieldDecl",
                                vec![f.name.clone(), if f.mutable { "mut" } else { "fix" }.into()],
                                vec![opt(&f.ty, ty), expr(&f.init)],
                            )
                            .at(f.span)
                            .named(f.name_span)
                        })
                        .collect(),
                ),
                n("Methods", vec![], d.methods.iter().map(fn_decl).collect()),
            ],
        )
        .at(d.span)
        .named(d.name_span),
        Stmt::Kind(d) => n(
            "Kind",
            vec![d.name.clone()],
            vec![
                tparams(&d.type_params),
                n(
                    "KMethods",
                    vec![],
                    d.methods
                        .iter()
                        .map(|m| {
                            n(
                                "KMethod",
                                vec![m.name.clone()],
                                vec![params(&m.params), opt(&m.ret, ty)],
                            )
                            .at(m.span)
                            .named(m.name_span)
                        })
                        .collect(),
                ),
            ],
        )
        .at(d.span)
        .named(d.name_span),
        Stmt::Attempt { body, rescues, .. } => n(
            "Attempt",
            vec![],
            std::iter::once(block(body))
                .chain(rescues.iter().map(|r| {
                    n(
                        "Rescue",
                        vec![r.name.clone()],
                        vec![ty(&r.ty), block(&r.body)],
                    )
                    .at(r.span)
                    .named(r.name_span)
                }))
                .collect(),
        ),
        Stmt::Fail(e, _) => n("Fail", vec![], vec![expr(e)]),
    }
}

fn fn_decl(d: &FnDecl) -> N {
    n(
        "Fn",
        vec![d.name.clone(), d.expr_body.to_string()],
        vec![
            tparams(&d.type_params),
            params(&d.params),
            opt(&d.ret, ty),
            block(&d.body),
        ],
    )
    .at(d.span)
    .named(d.name_span)
}
