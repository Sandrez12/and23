# Cambios

Formato basado en [Keep a Changelog](https://keepachangelog.com/es-ES/1.1.0/). Las versiones siguen
[SemVer](https://semver.org/lang/es/); mientras sea `0.x`, el lenguaje puede cambiar entre versiones.

## Sin publicar

### Añadido
- **Línea de referencia de rendimiento:** `scripts/rendimiento.sh [--escribe]` mide (mediana de 5) un bucle numérico, `Lista`/`Mapa`, texto,
  tareas y canales, y las cargas de Rust y de Mn (incluido el punto fijo); `docs/rendimiento.md` guarda los tiempos de esta máquina y
  `benchmarks/` las cargas.
- **El GC no cambia ningún programa:** `crates/and23-vm/tests/gc_corpus.rs` ejecuta cada programa ejecutable del repositorio (ejemplos y
  casos) con la recolección normal y con `--gc-stress` y exige la misma salida y el mismo resultado; otro test hace lo mismo con el lexer y
  el parser de Mn. Como son tests normales, el CI de los tres sistemas los corre.
- **Fuzzing del verificador, el compilador y la VM** (`crates/and23-vm/tests/fuzz.rs`): programas del repositorio mutados (con cambios que
  conservan los tipos y otros que no) se analizan, verifican, compilan y ejecutan; nada puede entrar en pánico ni colgarse, y una de cada
  ocho ejecuciones va con `--gc-stress`. El corpus excluye todo lo que toque disco, entorno o entrada. 3 000 programas en cada `cargo test`;
  `AND23_FUZZ_ITERS=200000` para una sesión larga (no encontró fallos).
- `Vm::with_step_limit(n)` / `Session::with_step_limit(n)`: tope de instrucciones que puede ejecutar una VM; al agotarse, el error es
  «se superó el límite de instrucciones». Sin tope por defecto (sin diferencia medible de velocidad).
- **Fuzzing del lexer y el parser** (`crates/and23-syntax/tests/fuzz.rs`): un generador determinista muta los programas reales del repositorio
  y exige que no haya pánicos ni cuelgues, que cada error apunte dentro del texto, y que `format` deje válido e idempotente todo programa
  válido; más casos de anidamiento profundo. 20 000 entradas en cada `cargo test`; `AND23_FUZZ_ITERS=1000000` para una sesión larga.
- **Enumerados, `match` e `if` como valor.** `enum Color { Rojo, Verde }` (con `nombre`, `orden` y `Color.todos`), `match` por valor,
  variante o tipo (`c: Circulo -> ...`) como sentencia y como expresión, con comprobación de que cubre todas las variantes de un enumerado
  del mismo archivo, e `if c { a } else { b }` como valor. Nuevas palabras clave: `enum` y `match`. Ver `examples/enumerados.jb`.
- `and23 --tokens` muestra la posición de cada token (`@desde-hasta`) y la del error; `and23 --ast` hace lo mismo con los nodos.
- **Mn, etapa 5.** `and23 --carga preludio.txt programa.txt` ejecuta en la VM los módulos que escribe `mn/compilador.jb`. El compilador de
  And23, compilado por sí mismo, produce exactamente el mismo bytecode que compilado por Rust (punto fijo), y también el del preludio.
- **Mn, etapa 4.** `mn/compilador.jb`, el compilador a bytecode de And23 escrito en And23 (port de `compiler.rs`), comparado con el de Rust
  (`and23 --dump`) sobre el preludio, los ejemplos, los casos del verificador y de los módulos y los propios archivos de Mn, él mismo incluido
  (unas 70 000 líneas de bytecode idénticas, con la posición de cada instrucción).
- **Mn, etapa 3.** `mn/checker.jb`, el verificador de tipos de And23 escrito en And23, comparado con el de Rust (`and23 --check`) sobre el
  preludio, los ejemplos, los archivos de `mn`, 297 programas de los tests del verificador y 108 casos propios; y se verifica a sí mismo.
- **Mn, etapa 2.** `mn/parser.jb`, el parser de And23 escrito en And23, comparado árbol por árbol con el de Rust (`and23 --ast`) sobre el
  preludio, los ejemplos, el propio parser y 84 casos propios (válidos y cada mensaje de error), con las posiciones de cada nodo y de cada error.
- **Mn, etapa 1.** `mn/lexer.jb`, el lexer de And23 escrito en And23, comparado token por token con el de Rust (`and23 --tokens`).
- Texto: `c.esLetra()`, `c.esAlfanumerico()`, `c.esDigito()`, `s.caracteres()` y `desdeCodigo(n)`.

- **Mn, pasos hacia el compilador.** `mn/modulos.jb` carga `use` como `modules.rs` (nombres con su ruta, `open`, alias, avisos de «es privado»,
  dependencias circulares) y se compara con `and23 --unido`; el verificador de `mn/checker.jb` calcula las resoluciones del compilador (índice de
  campo, llamada virtual, `super`, llamada a un valor, descartes, redirecciones de `Str`) y se compara con `and23 --resuelve`; y `and23 --dump`
  / `--dump-preludio` vuelcan el módulo compilado (tablas, funciones, instrucciones y posiciones en caracteres) para comparar el compilador
  de la etapa 4.
- `listaDe(arreglo)` (una `Lista` desde un arreglo literal) y `Dupla<A, B>` (dos valores juntos) en la biblioteca base.

### Cambiado (lenguaje)
- **`?.` sobre `Str?` y `Float?`:** `s?.largo()`, `x?.formato(2)` y cualquier método de texto o de `Float` valen con `?.` (nil si el valor es nil; el
  resultado es opcional). Antes `?.` solo servía con objetos.
- **`x.formato(decimales)`:** un `Float` como texto con ese número de decimales (0 a 100): `3.14159.formato(2)` es `"3.14"`. Redondea el valor
  binario exacto como `printf` (`2.675.formato(2)` da `"2.67"`; los empates exactos van al par). Es el primer método de `Float`.
- **`Mapa` recuerda el orden de inserción:** `claves()`, `valores()`, `paraCada` y `show` lo recorren en el orden en que se pusieron las claves (poner
  una clave existente conserva su lugar; quitarla y volver a ponerla la deja al final). Antes el orden era el de las cubetas del hash.
- **Una lambda local puede llamarse a sí misma:** `fix f: (Int) -> Int = { n -> ... f(n - 1) ... }` ya vale dentro de funciones, métodos y bloques
  (antes solo en el nivel superior). Hace falta anotar el tipo de función; sin él, el error explica cómo. Ver `examples/lambdas.jb`.

### Eliminado
- El intérprete de árbol (`and23 --tree` y el crate `and23-eval`): era la referencia de M1a y no conocía la biblioteca, las tareas ni las cadenas. Sus
  salidas esperadas quedaron fijas en `crates/and23-vm/tests/semantica.rs` (antes `diferencial.rs`).

### Cambiado
- `Lista.ordena` es una mezcla estable O(n log n) (antes una inserción O(n²): 100 000 elementos tardaban minutos, ahora menos de un segundo). Mismo
  orden para elementos iguales; si el comparador falla, la lista queda como estaba.
- `s.parte(sep)` es lineal: pasó a una nativa (`_parte`) en vez de repetir `_busca` y `_trozo`, que costaba O(n) por trozo. Partir 1,4 MB en
  100 000 trozos tardaba 24 s; ahora 0,1 s.
- Cadenas con interpolaciones muy anidadas (`"${"${"${...`) ya no cuestan el cuadrado de la profundidad al analizarlas: el código de cada
  interpolación se comparte con `Arc` en vez de copiarse al consumir cada token, y los enumerados del parser también.
- Un `Fallo` de una tarea que nadie espera (`espera()`) ya no se pierde: al terminar el programa se avisa (`error: N tarea(s) fallaron y
  nadie las esperó: ...`) y el código de salida es 1; con `--prueba`, la prueba falla.
- Los ids de método del compilador ya no dependen del orden de un `HashMap`: se asignan por posición en el fuente (antes el volcado del
  bytecode cambiaba de una ejecución a otra).
- Un valor devuelto que es a veces `T` y a veces `nil` en una función o lambda sin tipo de retorno (y en un `if` como valor) da `T?`, en vez
  de pedir anotar el tipo.
- Los campos `fix` de `this` se estrechan tras comprobar `!= nil` (los `mut` siguen sin estrecharse).
- `mn/checker.jb` acepta varios archivos (`and23 mn/checker.jb a.jb b.jb`): comprueba el preludio una vez y vuelve a su estado entre archivo y
  archivo, también tras un error. El test que lo compara con Rust pasó de 11 s a 3 s.
- La gramática de VS Code conoce `absolutoF`, `minimoF`, `maximoF`, `desdeCodigo` y `listaDe`; un test avisa si falta alguna función predefinida.
- Un test comprueba que la tabla de nativas de `mn/checker.jb` coincide con `natives.rs`.
- La release prueba el binario de cada sistema (`--version`, un ejemplo y los dos verificadores) antes de empaquetarlo.
- `une(partes, sep)` pasó de cuadrática a lineal.

## 0.3.0 - 2026-11-07

### Añadido
- **Marco de pruebas.** `prueba "nombre" { ... }`, `afirma`, `afirmaIgual` y `and23 --prueba archivo.jb`.
- **Formateador.** `and23 fmt [--check] archivos...`.
- **Extensión de VS Code** con resaltado de sintaxis (`editors/vscode`).

## 0.2.0 - 2026-11-07

### Añadido
- **Biblioteca estándar 2.** `argumentos()`, `entorno()`, `ahora()`, `duerme()`, `azar()`, `azarF()`, `semilla()`, `unirRuta`,
  `nombreDe`, `carpetaDe`, `extension`, `esDir`, `listaDir`, `creaDir`; literales `Float` con exponente (`1e3`).
- **Trazas de pila** en los errores de ejecución (`llamado desde ...`).
- **M11, concurrencia.** Tareas cooperativas (`lanza`, `lanzaDe`) y canales (`Canal<T>`), con detección de interbloqueo.
- **M10, biblioteca estándar inicial.** Texto, números, archivos y entrada estándar.
- **M9, módulos.** `use "ruta"`, `use "ruta" como a`, `open`.
- **M8, lambdas y closures**, tipos de función y métodos genéricos.
- **M7, genéricos** con `Lista<T>` y `Mapa<K, V>` escritos en el propio lenguaje (preludio).
- **M6, excepciones** (`attempt`/`rescue`/`fail`) y tipos opcionales (`T?`).
- **M5, herencia e interfaces.** M4 clases (`thing`). M3 cadenas, arreglos y recolector de basura. M2 verificador de tipos estáticos.
  M1 variables, funciones, bytecode y VM de pila. M0 lexer, parser y evaluador.

### Cambiado
- `show` de `Lista` y `Mapa` los imprime como `[1, 2]` y `{"a": 1}`.
- Las variables `mut` globales se estrechan (`x != nil`) en el nivel superior si ninguna función las asigna.
- El hash de un objeto sale de su número de creación, no de la casilla del heap.
- Límite de recursión por defecto de 100000 llamadas (`--max-depth N`).

### Corregido
- Un error dentro de una función del preludio señalaba una posición sin sentido; ahora apunta a la llamada del usuario.

### Límites conocidos
Ver «Límites» de cada sección en [`DESIGN.md`](DESIGN.md): sin `it`, sin `::nombre`, sin reexportar ni importar nombres sueltos,
sin expulsión de tareas por tiempo, `Mapa` sin orden de inserción, el intérprete de árbol (`--tree`) no conoce la biblioteca.
