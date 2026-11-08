# Cambios

Formato basado en [Keep a Changelog](https://keepachangelog.com/es-ES/1.1.0/). Las versiones siguen
[SemVer](https://semver.org/lang/es/); mientras sea `0.x`, el lenguaje puede cambiar entre versiones.

## Sin publicar

### Añadido
- **Enumerados, `match` e `if` como valor.** `enum Color { Rojo, Verde }` (con `nombre`, `orden` y `Color.todos`), `match` por valor,
  variante o tipo (`c: Circulo -> ...`) como sentencia y como expresión, con comprobación de que cubre todas las variantes de un enumerado
  del mismo archivo, e `if c { a } else { b }` como valor. Nuevas palabras clave: `enum` y `match`. Ver `examples/enumerados.jb`.
- `and23 --tokens` muestra la posición de cada token (`@desde-hasta`) y la del error; `and23 --ast` hace lo mismo con los nodos.
- **Mn, etapa 2.** `mn/parser.jb`, el parser de And23 escrito en And23, comparado árbol por árbol con el de Rust (`and23 --ast`) sobre el
  preludio, los ejemplos, el propio parser y 84 casos propios (válidos y cada mensaje de error), con las posiciones de cada nodo y de cada error.
- **Mn, etapa 1.** `mn/lexer.jb`, el lexer de And23 escrito en And23, comparado token por token con el de Rust (`and23 --tokens`).
- Texto: `c.esLetra()`, `c.esAlfanumerico()`, `c.esDigito()`, `s.caracteres()` y `desdeCodigo(n)`.

### Cambiado
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
