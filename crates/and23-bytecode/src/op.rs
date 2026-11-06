use and23_syntax::Span;

/// Instrucción de la VM de pila. Los saltos llevan la posición absoluta de destino dentro de
/// la función. Los operandos viven en la pila; los locales ocupan sus primeras posiciones.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Op {
    Int(i64),
    Float(f64),
    Bool(bool),
    Unit,
    /// Local `n` del marco actual (relativo a su base).
    GetLocal(u32),
    /// Saca la cima y la guarda en el local `n`.
    SetLocal(u32),
    GetGlobal(u32),
    SetGlobal(u32),
    /// Saca la cima y define la global (primera asignación).
    DefGlobal(u32),
    Pop,
    PopN(u32),
    Neg,
    Not,
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
    Jump(u32),
    /// Saca la cima, que debe ser Bool; salta si es falsa.
    JumpIfFalse(u32),
    /// `&&`: la cima debe ser Bool; si es falsa salta conservándola, si no la saca.
    AndJump(u32),
    /// `||`: la cima debe ser Bool; si es verdadera salta conservándola, si no la saca.
    OrJump(u32),
    /// Exige que la cima sea Bool (lado derecho de `&&` si es `true`, de `||` si es `false`).
    CheckBool(bool),
    /// Llama a la función `n`; sus argumentos ya están en la pila.
    Call(u32),
    /// Devuelve la cima al llamador.
    Return,
    /// Saca e imprime la cima.
    Show,
    /// Como `Show`, pero no imprime `()`. Es lo que usa el REPL.
    Echo,
}

#[derive(Debug, Clone)]
pub struct Function {
    pub name: String,
    pub name_span: Span,
    pub arity: usize,
    pub code: Vec<Op>,
    /// Posición en el fuente de cada instrucción, para los errores de ejecución.
    pub spans: Vec<Span>,
}

#[derive(Debug, Clone, Default)]
pub struct Module {
    pub functions: Vec<Function>,
    /// Nombres de las variables globales, por número de ranura.
    pub globals: Vec<String>,
}
