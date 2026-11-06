# And23

And23 es un lenguaje de programación estático y orientado a objetos, de nivel similar a Java, con compilador y máquina virtual de bytecode escritos en Rust. Es un proyecto de aprendizaje y a largo plazo: crece por hitos hasta tener clases, interfaces, genéricos, excepciones y GC, y la meta final es que el compilador se compile a sí mismo.

Los archivos de código usan la extensión `.jb`. Los identificadores admiten Unicode (`dueño`, `ñandú`).

> **Estado: M0.** Hoy solo evalúa expresiones aritméticas. Variables, funciones, clases y la VM llegan en los siguientes hitos (ver [`DESIGN.md`](DESIGN.md)).

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
// examples/aritmetica.jb
show(1 + 2 * 3)      // 7
show((1 + 2) * 3)    // 9
show(10 / 4)         // 2   (división entera entre enteros)
show(10 / 4.0)       // 2.5
show(-7 % 3)         // -1
```

Los enteros son `i64` verificados: el desbordamiento y la división entre cero son errores con su posición.

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
cargo run -p and23-cli -- examples/aritmetica.jb   # ejecuta un archivo
cargo run -p and23-cli                             # REPL (":q" sale)
cargo test --workspace                             # tests
```

## Estructura

| Crate | Contenido |
|---|---|
| `and23-syntax` | lexer, AST, parser Pratt y diagnósticos con línea y columna |
| `and23-eval` | evaluador de árbol (provisional; lo reemplaza la VM en M1) |
| `and23-cli` | binario `and23`: ejecuta archivos `.jb` y ofrece un REPL |

## Hoja de ruta

M0 expresiones, M1 variables y funciones con bytecode y VM, M2 tipos estáticos, M3 strings y GC, M4 clases, M5 herencia e interfaces, M6 excepciones y null-safety, M7 genéricos, y más adelante closures, módulos, stdlib, concurrencia y auto-compilación. El detalle y los criterios de éxito están en [`DESIGN.md`](DESIGN.md).

## Licencia

[MIT](LICENSE)
