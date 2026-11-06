use and23_syntax::Span;

/// Instrucción de la VM de pila. Los saltos llevan la posición absoluta de destino dentro de
/// la función. Los operandos viven en la pila; los locales ocupan sus primeras posiciones.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Op {
    Int(i64),
    Float(f64),
    Bool(bool),
    Unit,
    /// `nil`.
    Nil,
    /// Cadena constante número `n` del módulo.
    Str(u32),
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
    /// Saca `n` valores, los convierte en texto y los une en una cadena nueva (`"a${x}"`).
    Interp(u32),
    /// Saca `n` valores y crea un arreglo con ellos.
    Array(u32),
    /// Saca índice y arreglo; deja el elemento.
    Index,
    /// Saca valor, índice y arreglo; guarda el valor en esa posición.
    SetIndex,
    /// Saca un objeto y deja su campo `n`.
    GetField(u32),
    /// Saca valor y objeto; guarda el valor en el campo `n` del objeto.
    SetField(u32),
    /// Duplica la cima (`obj.campo += v` evalúa `obj` una sola vez).
    Dup,
    /// Crea una instancia de la clase `class` con `n` campos en `()`; el constructor los rellena.
    Alloc {
        class: u32,
        n: u32,
    },
    /// Llamada virtual: el receptor está bajo los `argc` argumentos; la VM busca el método `method`
    /// en la tabla de la clase real del objeto.
    CallMethod {
        method: u32,
        argc: u32,
    },
    /// Si la cima es `nil`, salta conservándola (`a?.b`); si no, sigue sin tocarla.
    JumpIfNil(u32),
    /// Si la cima no es `nil`, salta conservándola (`a ?: b`); si no, sigue sin tocarla.
    JumpIfNotNil(u32),
    /// Instala un manejador de excepciones (`attempt`): una excepción salta al destino con el
    /// objeto lanzado en la cima y la pila como estaba aquí.
    Try(u32),
    /// Quita el manejador instalado por el `Try` más reciente (el cuerpo del `attempt` terminó).
    EndTry,
    /// Saca un objeto y lo lanza hasta el manejador más cercano; sin manejador es un error.
    Fail,
    /// Saca un objeto y deja si es instancia del tipo `n` (clase, subclase o interfaz).
    IsInstance(u32),
    /// Exige que la cima sea instancia del tipo `n`; si no, es un error de ejecución.
    Cast(u32),
    /// Duplica los dos valores de la cima (`a[i] += v` evalúa `a` e `i` una sola vez).
    Dup2,
    /// Saca una cadena o un arreglo y deja su largo (`len`).
    Len,
    /// Saca valor y arreglo, agrega el valor al final y deja `()` (`push`).
    Push,
    /// Saca un arreglo y deja su último elemento, que quita del arreglo (`pop`).
    ArrayPop,
    /// Saca un valor y deja su hash (`hash`): igual para valores iguales, nunca negativo.
    Hash,
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

/// Forma de una clase en tiempo de ejecución: su nombre y los nombres de sus campos.
#[derive(Debug, Clone)]
pub struct ClassDef {
    pub name: String,
    /// Todos los campos, los heredados primero.
    pub fields: Vec<String>,
    /// Índices (en `Module::classes`) de este tipo, de sus ancestros y de las interfaces que cumple.
    pub supers: Vec<u32>,
    /// Tabla de métodos: id de método -> índice de la función que lo implementa (heredados incluidos).
    pub vtable: std::collections::HashMap<u32, u32>,
}

#[derive(Debug, Clone, Default)]
pub struct Module {
    pub functions: Vec<Function>,
    /// Clases e interfaces (estas con campos y tabla vacíos).
    pub classes: Vec<ClassDef>,
    /// Nombres de método con id: índice en este vector.
    pub methods: Vec<String>,
    /// Nombres de las variables globales, por número de ranura.
    pub globals: Vec<String>,
    /// Cadenas constantes del programa.
    pub strings: Vec<String>,
}
