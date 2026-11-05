//! Evaluador de árbol de And23. En M1 lo reemplaza el compilador a bytecode y la VM.

use and23_syntax::{BinOp, Diagnostic, Expr, Program, Span, Stmt, UnOp};
use std::fmt;
use std::io::Write;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Value {
    Int(i64),
    Float(f64),
}

impl Value {
    fn as_f64(self) -> f64 {
        match self {
            Value::Int(v) => v as f64,
            Value::Float(v) => v,
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Int(v) => write!(f, "{v}"),
            // `{:?}` conserva el `.0` en decimales enteros (3.0 y no 3).
            Value::Float(v) => write!(f, "{v:?}"),
        }
    }
}

pub fn eval(expr: &Expr) -> Result<Value, Diagnostic> {
    match expr {
        Expr::Int(v, _) => Ok(Value::Int(*v)),
        Expr::Float(v, _) => Ok(Value::Float(*v)),
        Expr::Unary {
            op: UnOp::Neg,
            expr,
            span,
        } => match eval(expr)? {
            Value::Int(v) => v
                .checked_neg()
                .map(Value::Int)
                .ok_or_else(|| overflow(*span)),
            Value::Float(v) => Ok(Value::Float(-v)),
        },
        Expr::Binary { op, lhs, rhs, span } => {
            let (a, b) = (eval(lhs)?, eval(rhs)?);
            match (a, b) {
                (Value::Int(x), Value::Int(y)) => int_op(*op, x, y, *span),
                _ => float_op(*op, a.as_f64(), b.as_f64(), *span),
            }
        }
    }
}

fn overflow(span: Span) -> Diagnostic {
    Diagnostic::new(span, "desbordamiento de entero (i64)")
}

fn div_by_zero(span: Span) -> Diagnostic {
    Diagnostic::new(span, "división entre cero")
}

fn int_op(op: BinOp, x: i64, y: i64, span: Span) -> Result<Value, Diagnostic> {
    let result = match op {
        BinOp::Add => x.checked_add(y),
        BinOp::Sub => x.checked_sub(y),
        BinOp::Mul => x.checked_mul(y),
        BinOp::Div | BinOp::Rem if y == 0 => return Err(div_by_zero(span)),
        BinOp::Div => x.checked_div(y),
        BinOp::Rem => x.checked_rem(y),
    };
    result.map(Value::Int).ok_or_else(|| overflow(span))
}

fn float_op(op: BinOp, x: f64, y: f64, span: Span) -> Result<Value, Diagnostic> {
    Ok(Value::Float(match op {
        BinOp::Add => x + y,
        BinOp::Sub => x - y,
        BinOp::Mul => x * y,
        BinOp::Div | BinOp::Rem if y == 0.0 => return Err(div_by_zero(span)),
        BinOp::Div => x / y,
        BinOp::Rem => x % y,
    }))
}

/// Ejecuta el programa. `echo` imprime también el valor de las expresiones sueltas (modo REPL).
pub fn run(program: &Program, echo: bool, out: &mut dyn Write) -> Result<(), Diagnostic> {
    for stmt in &program.stmts {
        let line = match stmt {
            Stmt::Show(expr, _) => Some(eval(expr)?),
            Stmt::Expr(expr) => {
                let value = eval(expr)?;
                echo.then_some(value)
            }
        };
        if let Some(value) = line {
            writeln!(out, "{value}").map_err(|e| {
                Diagnostic::new(
                    Span::new(0, 0),
                    format!("no se pudo escribir la salida: {e}"),
                )
            })?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use and23_syntax::parse;

    /// Ejecuta `src` y devuelve su salida, o el mensaje de error.
    fn out(src: &str) -> Result<String, String> {
        let program = parse(src).map_err(|d| d.message)?;
        let mut buf = Vec::new();
        run(&program, true, &mut buf).map_err(|d| d.message)?;
        Ok(String::from_utf8(buf).unwrap())
    }

    #[test]
    fn el_criterio_de_exito_del_hito() {
        assert_eq!(out("1 + 2 * 3").unwrap(), "7\n");
    }

    #[test]
    fn precedencia_y_parentesis() {
        assert_eq!(out("(1 + 2) * 3").unwrap(), "9\n");
        assert_eq!(out("10 - 4 - 3").unwrap(), "3\n");
        assert_eq!(out("2 * 3 + 4 * 5").unwrap(), "26\n");
    }

    #[test]
    fn division_entera_y_decimal() {
        assert_eq!(out("10 / 4").unwrap(), "2\n");
        assert_eq!(out("10 / 4.0").unwrap(), "2.5\n");
        assert_eq!(out("1.5 + 1.5").unwrap(), "3.0\n");
    }

    #[test]
    fn resto_con_signo() {
        assert_eq!(out("-7 % 3").unwrap(), "-1\n");
        assert_eq!(out("7 % 3").unwrap(), "1\n");
    }

    #[test]
    fn menos_unario() {
        assert_eq!(out("2 * -3").unwrap(), "-6\n");
        assert_eq!(out("- -4").unwrap(), "4\n");
    }

    #[test]
    fn show_imprime_aun_sin_echo() {
        let program = parse("show(2 + 2)\n5").unwrap();
        let mut buf = Vec::new();
        run(&program, false, &mut buf).unwrap();
        assert_eq!(String::from_utf8(buf).unwrap(), "4\n");
    }

    #[test]
    fn division_entre_cero() {
        assert_eq!(out("1 / 0").unwrap_err(), "división entre cero");
        assert_eq!(out("1 % 0").unwrap_err(), "división entre cero");
        assert_eq!(out("1.0 / 0").unwrap_err(), "división entre cero");
    }

    #[test]
    fn desbordamiento_de_enteros() {
        assert!(out("9223372036854775807 + 1")
            .unwrap_err()
            .contains("desbordamiento"));
        assert!(out("9223372036854775807 * 2")
            .unwrap_err()
            .contains("desbordamiento"));
        // i64::MIN no se puede escribir como literal; se obtiene restando.
        assert!(out("-9223372036854775807 - 2")
            .unwrap_err()
            .contains("desbordamiento"));
    }

    #[test]
    fn el_error_de_ejecucion_apunta_a_la_operacion() {
        let src = "show(1)\nshow(4 / 0)\n";
        let program = parse(src).unwrap();
        let err = run(&program, false, &mut Vec::new()).unwrap_err();
        assert_eq!(err.line_col(src), (2, 6));
    }
}
