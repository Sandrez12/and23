# And23: decisiones de diseño

Lenguaje estático orientado a objetos de nivel similar a Java, con compilador y VM de bytecode escritos en Rust.
Meta: aprendizaje y proyecto serio a largo plazo; crecer por hitos hasta nivel Java y, al final, auto-compilarse.

## Decisiones tomadas

| Tema | Decisión |
|---|---|
| Nombre | **And23** |
| Extensión | **`.jb`** |
| Implementación | Rust |
| Ejecución | VM de pila con bytecode propio (estilo JVM) |
| Tipado | Estático, orientado a objetos |
| Identificadores | Unicode (`dueño`, `ñandú`) |
| Genéricos | Con erasure (un solo bytecode, casts insertados por el compilador) |
| GC | Mark-sweep propio detrás de una interfaz de heap (migrable a `gc-arena`) |
| Parser | A mano: descenso recursivo + Pratt |
| Meta | Aprendizaje + serio a largo plazo; self-hosting al final |

## Sintaxis (borrador aprobado)

```
pack demo.zoo                      // paquete

kind Habla {                       // interfaz
    fn sonido() -> Str
}

thing Animal(fix nombre: Str) {    // clase; el constructor va en la cabecera
    mut edad = 0                   // campo mutable
    fn cumple() { edad += 1 }
}

thing Perro(nombre: Str) : Animal(nombre), Habla {
    fn sonido() -> Str = "guau"    // cuerpo de una expresión con "="
}

fn main() {
    fix p = Perro("Rex")           // fix = inmutable, mut = mutable
    p.cumple()
    show("${p.nombre} dice ${p.sonido()}")

    fix dueño: Perro? = nil        // nil y T? en vez de null
    dueño?.cumple()                // acceso seguro

    attempt { riesgo() }           // try/catch
    rescue e: Fallo { show(e.msg) }
}
```

Palabras propias: `thing` (clase), `kind` (interfaz), `fix`/`mut` (inmutable/mutable), `show` (imprimir),
`attempt`/`rescue` (excepciones), `nil`. Sin `;` obligatorios.

Principios heredados de Kotlin y de lo que Java lamenta: null-safety (`T?`), inmutable por defecto,
igualdad estructural, strings UTF-8, sin boxing visible al usuario.

## Arquitectura

Workspace de Rust en `crates/`:

- `and23-syntax`: lexer, AST, parser y diagnósticos (spans con línea y columna).
- `and23-types`: verificador de tipos (`Checker`); corre entre el parser y el compilador.
- `and23-bytecode`: `Op`, `Module`/`Function`, compilador AST → bytecode y desensamblador.
- `and23-vm`: VM de pila (`Vm`) y `Session`, que une compilador y VM conservando el estado del REPL.
- `and23-eval`: intérprete de árbol (M0/M1a). Ya no es el camino de ejecución; se conserva como oráculo en los tests diferenciales y para `--tree`.
- `and23-cli`: binario `and23` (ejecuta archivos `.jb` y ofrece un REPL).


## Hitos

| Hito | Contenido | Criterio de éxito |
|---|---|---|
| M0 | Lexer, parser y expresiones aritméticas | `1 + 2 * 3` da 7, con tests |
| M1 | Variables, `if`/`while`, funciones, bytecode y VM de pila | fib(30) y factorial |
| M2 | Tipos estáticos y errores con línea y columna | Un programa mal tipado se rechaza con mensaje claro |
| M3 | Strings, arrays y GC mark-sweep | 10M de asignaciones sin agotar memoria; pasa en modo stress |
| M4 | Clases, campos, métodos, constructores | Programa OO de ~200 líneas |
| M5 | Herencia, interfaces, vtables, `instanceof` | Ejemplo polimórfico |
| M6 | Excepciones y null-safety | `attempt`/`rescue`; el compilador rechaza deref de `T?` |
| M7 | Genéricos con erasure | `List<T>` y `Map<K,V>` propios |
| M8 | Closures y lambdas | `lista.map { ... }` |
| M9 | Módulos y paquetes | Proyecto de 3+ archivos |
| M10 | Stdlib inicial (IO, strings, math) | Leer y procesar un archivo |
| M11 | Concurrencia | Productor/consumidor |
| M12 | Herramientas (REPL, desensamblador, benchmarks) | `and23 --disasm` |
| Mn | Auto-compilación | El compilador escrito en And23 se compila a sí mismo |

## Estado

- **M0** (hecho): lexer, parser Pratt y expresiones aritméticas. `examples/aritmetica.jb`.
- **M1a** (hecho): variables, booleanos, `if`/`while`, funciones y recursión sobre el intérprete de árbol.
  `examples/funciones.jb` y `examples/control.jb`. Referencia para M1b: `fib(30)` = 832040 en ~1,7 s (release).
- **M1b** (hecho): compilador a bytecode y VM de pila. `fib(30)` = 832040 en ~0,17 s (release) contra ~1,6 s del
  árbol (≈9×). Tests diferenciales: la VM da la misma salida y los mismos errores que el intérprete de árbol.

- **M2** (hecho): tipos estáticos (`Int`, `Float`, `Bool`, `Unit`) con errores de línea y columna; ver "Tipos (M2)".

### Diseño de la VM (M1b)

- Instrucciones `Op` (enum `Copy`, destinos de salto absolutos) y una tabla de `Span` por instrucción para los errores.
- Los locales son las primeras posiciones del marco en la pila; cada bloque saca los suyos al salir (`Pop`/`PopN`),
  y `break`/`continue` sacan los que declaró el cuerpo del bucle antes de saltar.
- Las `fix`/`mut` del nivel superior son globales (ranuras); las funciones y globales del archivo se registran antes de
  compilar, así que una función puede usar una global declarada más abajo (falla al ejecutar si aún no tiene valor).
- Las llamadas no usan la pila nativa de Rust: un marco por llamada en un `Vec`, con límite de 10 000.
- Variable no definida, asignación a `fix`, aridad incorrecta y función desconocida se detectan **al compilar**
  (antes de ejecutar nada); los errores de tipo, desbordamiento y división entre cero, al ejecutar.
- Si la compilación falla, el compilador queda intacto (REPL). `and23 --disasm archivo.jb` imprime el bytecode.

### Semántica fijada en M1a

- Valores: `Int` (i64 con desbordamiento verificado), `Float`, `Bool` y `Unit`. Los strings llegan en M3.
- `fix` es inmutable y `mut` mutable. Los parámetros de función son inmutables.
- `Int / Int` es división entera; si cualquier lado es `Float`, la operación es decimal. `1 == 1.0` es verdadero.
- `&&` y `||` hacen cortocircuito. Las condiciones de `if` y `while` deben ser `Bool`.
- `if` y `while` son sentencias (no expresiones). `else` va en la misma línea que la `}` anterior.
- Las funciones se declaran solo al nivel superior, pueden usarse antes de su definición y ven las variables
  globales, no las locales del llamador. Si el archivo define `main()`, se llama tras el código del nivel superior.
- Las anotaciones de tipo (`n: Int`, `-> Int`) son obligatorias en los parámetros y desde M2 se verifican.
- Recursión limitada a 10 000 llamadas anidadas: más allá es un error con posición, no un cierre del proceso.
- Desde M1b esos errores de nombres y aridad se informan al compilar, no al ejecutar.
- Palabras clave: `fix mut fn if else while return break continue true false show`.

### Tipos (M2)

- Tipos: `Int`, `Float`, `Bool`, `Unit`. Un tipo desconocido es un error. Un `fix`/`mut` no puede guardar `Unit`.
- Variables: el tipo sale de la anotación o, si falta, del valor inicial; una anotación distinta del valor es un error.
- **Sin conversión implícita** `Int` → `Float` al asignar, pasar argumentos o devolver. En aritmética y comparaciones,
  mezclar `Int` y `Float` es válido y da `Float`/`Bool`. Con `==` solo se pueden comparar tipos iguales (o dos numéricos).
- Funciones: sin `->` y con cuerpo `{ }` devuelven `Unit`; con cuerpo `= expr` el tipo se infiere. Una función
  con `= expr` que se llama a sí misma antes de conocer su tipo debe anotar `-> Tipo`. Una función con tipo de retorno
  no `Unit` debe devolver en todos los caminos (`return` final o `if`/`else` que devuelven los dos).
- Las funciones se registran antes (pueden usarse antes de definirse); el nivel superior se verifica en orden,
  así que una global debe declararse antes de usarse desde el nivel superior. Los cuerpos de función se verifican
  después y ven todas las globales del archivo.
- El verificador devuelve un solo error, el primero que encuentra: primero el nivel superior y después los cuerpos
  de función, no necesariamente en orden de líneas.
- Si falla, no cambia su estado (REPL).

## Fuentes de referencia

- Crafting Interpreters (VM de bytecode, GC): https://craftinginterpreters.com
- Writing a Compiler in Go (AST, compilador, VM): https://compilerbook.com
- Especificación de la JVM (class file, despacho): https://docs.oracle.com/javase/specs/jvms/se21/html/
- Dora VM (lenguaje con VM en Rust, MIT): https://github.com/dinfuehr/dora
- Wren (VM pequeña con clases): https://wren.io

Los datos de crates (versiones, licencias) se verificaron en crates.io el 2026-11-05:
`gc-arena` 0.7.0 (MIT), `logos` 0.16.1, `ariadne` 0.6.0. Los libros y la especificación de la JVM no se verificaron.
