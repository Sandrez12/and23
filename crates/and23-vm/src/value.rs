use crate::heap::ObjRef;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Value {
    Int(i64),
    Float(f64),
    Bool(bool),
    /// Cadena o arreglo en el heap.
    Obj(ObjRef),
    /// Resultado de una función sin valor de retorno.
    Unit,
    /// La ausencia de valor de un tipo `T?`.
    Nil,
}

impl Value {
    pub fn type_name(self) -> &'static str {
        match self {
            Value::Int(_) => "Int",
            Value::Float(_) => "Float",
            Value::Bool(_) => "Bool",
            Value::Obj(_) => "Obj",
            Value::Unit => "Unit",
            Value::Nil => "nil",
        }
    }

    pub(crate) fn as_f64(self) -> Option<f64> {
        match self {
            Value::Int(v) => Some(v as f64),
            Value::Float(v) => Some(v),
            _ => None,
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Int(v) => write!(f, "{v}"),
            // `{:?}` conserva el `.0` en decimales enteros (3.0 y no 3).
            Value::Float(v) => write!(f, "{v:?}"),
            Value::Bool(v) => write!(f, "{v}"),
            // Los objetos se imprimen con `Heap::render`, que conoce su contenido.
            Value::Obj(_) => f.write_str("<obj>"),
            Value::Unit => f.write_str("()"),
            Value::Nil => f.write_str("nil"),
        }
    }
}
