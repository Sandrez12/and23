# And23

And23 es un lenguaje de programación estático y orientado a objetos, de nivel similar a Java, con compilador y máquina virtual de bytecode escritos en Rust. Es un proyecto de aprendizaje y a largo plazo: crece por hitos hasta tener clases, interfaces, genéricos, excepciones y GC, y la meta final es que el compilador se compile a sí mismo.

Los archivos de código usan la extensión `.jb`. Los identificadores admiten Unicode (`dueño`, `ñandú`).

> **Estado: M2.** Hoy verifica tipos y compila programas con variables, booleanos, `if`/`while` y funciones recursivas a bytecode y los ejecuta en una VM de pila (`fib(30)` en ~0,17 s, unas 9 veces más rápido que el intérprete de árbol anterior). Un programa mal tipado se rechaza con su línea y columna antes de ejecutar nada. Strings, clases y el resto llegan en los siguientes hitos (ver [`DESIGN.md`](DESIGN.md)).

## Cómo se ve (objetivo del lenguaje)

Borrador de la sintaxis aprobada; todavía no funciona:

```
thing Animal(fix nombre: Str) {
    mut edad = 0
    fn cumple() { edad += 1 }
}

fn main() {
    fix p = Animal("Rex")
    p.cumple()
    show("${p.nombre} tiene ${p.edad}")
}
```

## Lo que funciona hoy

```
// examples/funciones.jb
fn factorial(n: Int) -> Int {
    if n <= 1 { return 1 }
    return n * factorial(n - 1)
}

fn fib(n: Int) -> Int {
    if n < 2 { return n }
    return fib(n - 1) + fib(n - 2)
}

fix limite = 10
mut suma = 0
mut i = 1
while i <= limite {
    if i % 2 == 0 { suma += i }
    i += 1
}

show(factorial(10))   // 3628800
show(fib(20))         // 6765
show(suma)            // 30
```

- Tipos estáticos: `Int` (i64), `Float`, `Bool` y `Unit`. Las anotaciones se verifican; el tipo de las variables se infiere del valor inicial y el de retorno de `fn f(x: Int) = x * 2` también. Sin conversión implícita de `Int` a `Float` al asignar, pasar argumentos o devolver (`fix y: Float = 1` es un error; mezclarlos en aritmética sí da `Float`).
- `fix` (inmutable) y `mut` (mutable), asignación compuesta (`+=`, `-=`, ...), `&&`/`||` con cortocircuito.
- `if`/`else if`/`else`, `while`, `break`, `continue`, `return`. Si el archivo define `main()`, se llama al final.
- Los enteros se verifican: el desbordamiento y la división entre cero son errores con su posición. La recursión
  infinita da un error en vez de cerrar el proceso.
- El REPL recuerda variables y funciones entre líneas y acepta bloques en varias líneas.

Más ejemplos en [`examples/`](examples/): `aritmetica.jb`, `funciones.jb` y `control.jb`.

```
error: se esperaba Int, pero es Bool
 --> err.jb:2:14
  |
2 | fix x: Int = true
  |              ^^^^
```

```
error: división entre cero
 --> err.jb:2:6
  |
2 | show(8 / (4 - 4))
  |      ^^^^^^^^^^
```

## Compilar y ejecutar

Requiere [Rust](https://rustup.rs) estable.

```
cargo run -p and23-cli -- examples/funciones.jb    # ejecuta un archivo
cargo run -p and23-cli                             # REPL (":q" sale)
cargo run -p and23-cli -- --disasm examples/funciones.jb   # muestra el bytecode
cargo run -p and23-cli -- --tree examples/funciones.jb     # intérprete de árbol (referencia)
cargo test --workspace                             # tests
```

## Estructura

| Crate | Contenido |
|---|---|
| `and23-syntax` | lexer, AST, parser Pratt y diagnósticos con línea y columna |
| `and23-types` | verificador de tipos estáticos (con estado para el REPL) |
| `and23-bytecode` | instrucciones, compilador del AST a bytecode y desensamblador |
| `and23-vm` | VM de pila y sesión (compilador + VM con estado para el REPL) |
| `and23-eval` | intérprete de árbol; queda como referencia para comparar comportamiento y velocidad |
| `and23-cli` | binario `and23`: ejecuta archivos `.jb` y ofrece un REPL |

## Hoja de ruta

M0 expresiones, M1 variables y funciones con bytecode y VM, M2 tipos estáticos, M3 strings y GC, M4 clases, M5 herencia e interfaces, M6 excepciones y null-safety, M7 genéricos, y más adelante closures, módulos, stdlib, concurrencia y auto-compilación. El detalle y los criterios de éxito están en [`DESIGN.md`](DESIGN.md).

## Licencia

[MIT](LICENSE)
