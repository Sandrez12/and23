//! Funciones nativas (M10): las que la biblioteca estándar no puede escribir en And23 mismo
//! (texto, números y archivos). El verificador las tipa con esta tabla, el compilador emite
//! `Op::Native(id)` y la VM las ejecuta; el preludio construye el resto encima.

use crate::Type;

/// Tipos que aparecen en las firmas nativas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NTy {
    Int,
    Float,
    Str,
    Bool,
    OptStr,
    OptInt,
    OptFloat,
    /// `[Str]`.
    ArrStr,
    /// `[Str]?`.
    OptArrStr,
    Unit,
    /// `() -> Unit`, la lambda de una tarea.
    Fn0,
}

impl NTy {
    pub fn to_type(self) -> Type {
        let opt = |t: Type| Type::Nullable(Box::new(t));
        match self {
            NTy::Int => Type::Int,
            NTy::Float => Type::Float,
            NTy::Str => Type::Str,
            NTy::Bool => Type::Bool,
            NTy::OptStr => opt(Type::Str),
            NTy::OptInt => opt(Type::Int),
            NTy::OptFloat => opt(Type::Float),
            NTy::ArrStr => Type::Array(Box::new(Type::Str)),
            NTy::OptArrStr => opt(Type::Array(Box::new(Type::Str))),
            NTy::Unit => Type::Unit,
            NTy::Fn0 => Type::Fn(vec![], Box::new(Type::Unit)),
        }
    }
}

#[derive(Debug)]
pub struct Native {
    pub name: &'static str,
    pub params: &'static [NTy],
    pub ret: NTy,
}

macro_rules! natives {
    ($($name:literal ($($p:ident),*) -> $ret:ident;)*) => {
        pub const NATIVES: &[Native] = &[
            $(Native { name: $name, params: &[$(NTy::$p),*], ret: NTy::$ret },)*
        ];
    };
}

// El orden fija el id que lleva `Op::Native` y que la VM interpreta: no reordenar sin cambiar la VM.
natives! {
    "_trozo"(Str, Int, Int) -> Str;
    "_busca"(Str, Str, Int) -> Int;
    "_mayus"(Str) -> Str;
    "_minus"(Str) -> Str;
    "_recorta"(Str) -> Str;
    "_reemplaza"(Str, Str, Str) -> Str;
    "_empieza"(Str, Str) -> Bool;
    "_termina"(Str, Str) -> Bool;
    "_parseInt"(Str) -> OptInt;
    "_parseFloat"(Str) -> OptFloat;
    "raiz"(Float) -> Float;
    "potencia"(Float, Float) -> Float;
    "piso"(Float) -> Int;
    "techo"(Float) -> Int;
    "redondea"(Float) -> Int;
    "aFloat"(Int) -> Float;
    "_leeArchivo"(Str) -> OptStr;
    "_escribeArchivo"(Str, Str) -> Bool;
    "_agregaArchivo"(Str, Str) -> Bool;
    "existe"(Str) -> Bool;
    "leeLinea"() -> OptStr;
    "_errorES"() -> Str;
    "_borraArchivo"(Str) -> Bool;
    // M12: biblioteca estándar 2.
    "_args"() -> ArrStr;
    "_entorno"(Str) -> OptStr;
    "_ahora"() -> Int;
    "_pausa"(Int) -> Unit;
    "_azar"(Int) -> Int;
    "_azarF"() -> Float;
    "_semilla"(Int) -> Unit;
    "_listaDir"(Str) -> OptArrStr;
    "_creaDir"(Str) -> Bool;
    "_esDir"(Str) -> Bool;
    // Texto para escribir un lexer en And23 (Mn).
    "_esLetra"(Str) -> Bool;
    "_esAlfanum"(Str) -> Bool;
    "_esDigito"(Str) -> Bool;
    "_caracteres"(Str) -> ArrStr;
    "_desdeCodigo"(Int) -> OptStr;
    "_une"(ArrStr, Str) -> Str;
    "_parte"(Str, Str) -> ArrStr;
    "_formato"(Float, Int) -> Str;
    // M11: las atiende el planificador de la VM (desde `FIRST_SCHED_NATIVE`), no `natives::call`.
    "_lanza"(Fn0) -> Unit;
    "cede"() -> Unit;
    "_bloquea"() -> Unit;
    "_avisa"() -> Unit;
    "_hayListas"() -> Bool;
}

/// Id de la primera nativa que atiende el planificador de la VM: todas las siguientes también.
pub const FIRST_SCHED_NATIVE: u32 = 41;

/// Id y firma de la función nativa `name`.
pub fn native(name: &str) -> Option<(u32, &'static Native)> {
    NATIVES
        .iter()
        .position(|n| n.name == name)
        .map(|i| (i as u32, &NATIVES[i]))
}

/// ¿`name` es una función integrada (las de siempre o una nativa)?
pub fn is_builtin(name: &str) -> bool {
    crate::BUILTINS.contains(&name) || native(name).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn las_nativas_del_planificador_van_al_final() {
        assert_eq!(NATIVES[FIRST_SCHED_NATIVE as usize].name, "_lanza");
        assert_eq!(NATIVES.len() as u32 - FIRST_SCHED_NATIVE, 5);
    }
}
