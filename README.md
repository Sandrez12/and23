# And23

And23 es un lenguaje de programación estático y orientado a objetos, de nivel similar a Java, con compilador y máquina virtual de bytecode escritos en Rust. Es un proyecto de aprendizaje y a largo plazo: crece por hitos hasta tener clases, interfaces, genéricos, excepciones y GC, y la meta final es que el compilador se compile a sí mismo.

Los archivos de código usan la extensión `.jb`. Los identificadores admiten Unicode (`dueño`, `ñandú`).

> **Estado: M3.** Hoy verifica tipos y compila a bytecode programas con variables, `if`/`while`, funciones recursivas, cadenas con interpolación y arreglos, y los ejecuta en una VM de pila con recolector de basura (`fib(30)` en ~0,17 s, unas 9 veces más rápido que el intérprete de árbol anterior). Un programa mal tipado se rechaza con su línea y columna antes de ejecutar nada. Clases y el resto llegan en los siguientes hitos (ver [`DESIGN.md`](DESIGN.md)).

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

- Tipos estáticos: `Int` (i64), `Float`, `Bool`, `Str`, `Unit` y arreglos `[T]`. Las anotaciones se verifican; el tipo de las variables se infiere del valor inicial y el de retorno de `fn f(x: Int) = x * 2` también. Sin conversión implícita de `Int` a `Float` al asignar, pasar argumentos o devolver (`fix y: Float = 1` es un error; mezclarlos en aritmética sí da `Float`).
- `fix` (inmutable) y `mut` (mutable), asignación compuesta (`+=`, `-=`, ...), `&&`/`||` con cortocircuito.
- `if`/`else if`/`else`, `while`, `break`, `continue`, `return`. Si el archivo define `main()`, se llama al final.
- Cadenas UTF-8 con escapes (`\n \t \" \\ \$ \u{1F600}`), `+` para unirlas, comparación (`==`, `<`...) e interpolación `"hola ${nombre}, ${a + b}"`.
- Arreglos `[1, 2, 3]` con `a[i]`, `a[i] = v`, `a[i] += v`, `len(x)` (cadenas y arreglos) y `push(a, v)`. Son referencias: `fix b = a` comparte el mismo arreglo. Uno vacío necesita tipo: `fix a: [Int] = []`.
- Recolector de basura mark-sweep propio: 10 millones de asignaciones corren con el heap acotado. `--gc-stress` recolecta en cada punto seguro para probarlo.
- Los enteros se verifican: el desbordamiento y la división entre cero son errores con su posición. La recursión
  infinita da un error en vez de cerrar el proceso.
- El REPL recuerda variables y funciones entre líneas y acepta bloques en varias líneas.

Más ejemplos en [`examples/`](examples/): `aritmetica.jb`, `funciones.jb`, `control.jb` y `cadenas.jb`.

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
cargo run -p and23-cli -- --tree examples/funciones.jb     # intérprete de árbol (referencia, sin cadenas)
cargo run -p and23-cli -- --gc-stress examples/cadenas.jb  # GC en cada punto seguro
cargo test --workspace                             # tests
```

## Estructura

| Crate | Contenido |
|---|---|
| `and23-syntax` | lexer, AST, parser Pratt y diagnósticos con línea y columna |
| `and23-types` | verificador de tipos estáticos (con estado para el REPL) |
| `and23-bytecode` | instrucciones, compilador del AST a bytecode y desensamblador |
| `and23-vm` | VM de pila, heap con recolector mark-sweep y sesión (verificador + compilador + VM con estado para el REPL) |
| `and23-eval` | intérprete de árbol de M0/M1a; queda como referencia para comparar comportamiento y velocidad (no soporta cadenas ni arreglos) |
| `and23-cli` | binario `and23`: ejecuta archivos `.jb` y ofrece un REPL |

## Hoja de ruta

M0 expresiones, M1 variables y funciones con bytecode y VM, M2 tipos estáticos, M3 strings y GC, M4 clases, M5 herencia e interfaces, M6 excepciones y null-safety, M7 genéricos, y más adelante closures, módulos, stdlib, concurrencia y auto-compilación. El detalle y los criterios de éxito están en [`DESIGN.md`](DESIGN.md).

## Licencia

[MIT](LICENSE)
