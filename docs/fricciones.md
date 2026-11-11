# Fricciones al usar And23 en programas reales

Registro de lo que estorbó al escribir programas de verdad (los de `proyectos/`) y qué se hizo. Cada hallazgo es una de tres cosas:
**arreglado** (con su paso en el `ROADMAP`), **anotado** (límite conocido, no se toca antes de 1.0) o **descartado** (es una decisión de diseño).

## Proyecto 1: `proyectos/buscador` (una herramienta de línea de comandos)

Un `grep` sencillo con varios archivos (`opciones.jb`, `busqueda.jb`, `buscador.jb`), pruebas (`buscador_prueba.jb`) y códigos de salida.

| # | Fricción | Estado |
|---|---|---|
| 1 | **No se podían partir en varias líneas** los parámetros de una función o de un constructor (`fn f(\n a: Int,\n b: Int\n)`), los argumentos de una llamada, una expresión tras un operador al final de la línea (`a +\n b`) ni una cadena de métodos con el punto al comienzo de la línea (`l\n .map {...}\n .filter {...}`). El código real las usa a cada paso. | **Arreglado.** El lexer ya no emite salto de línea dentro de `(...)` y `[...]` ni tras un operador que espera su segundo operando, y une la línea siguiente si empieza con `.`, `?.`, `&&`, `\|\|` o `?:`. Mismos tokens en Rust y en Mn. |
| 2 | **No había forma de terminar con un código de salida** (una herramienta de línea de comandos necesita distinguir «sin coincidencias» de «argumentos mal»). | **Arreglado:** `salir(codigo)`. |
| 3 | **No había salida de errores:** todo `show` iba a la salida normal, y los mensajes de uso se mezclaban con los resultados. | **Arreglado:** `muestraError(texto)`. |
| 4 | No hay una función para la carpeta temporal: la prueba usa `entorno("TEMP") ?: entorno("TMPDIR") ?: "/tmp"`. | Anotado: se puede añadir en 1.x (`carpetaTemporal()`) sin romper nada. |
| 5 | `lista.ordena` siempre pide un comparador (`{ a, b -> a < b }`), incluso para textos o números. | Anotado: una `ordenaNatural()` sería una ampliación compatible. |
| 6 | Tras `salir(n)` el verificador sigue creyendo que el código continúa: una función que debe devolver un valor necesita un `return` después. | Anotado: haría falta un tipo «nunca» (fuera del alcance de 1.0). |

## Proyecto 2: `proyectos/calculadora` (un intérprete de expresiones)

Un analizador léxico, uno sintáctico de descenso recursivo, un árbol de nodos con una interfaz (`Nodo`) y clases que la implementan, un entorno con variables y
funciones (`Mapa<Str, (Float) -> Float>`), un bucle que lee de la entrada estándar y pruebas. Seis archivos.

| # | Fricción | Estado |
|---|---|---|
| 7 | **`fail` no se podía usar como rama de un `if` ni de un `match` usado como valor** (`if b == 0.0 { fail ErrorCalculo("...") } else { a / b }`, `_ -> fail ...`): daba «se esperaba una expresión, pero se encontró `fail`». Es lo más natural para los casos imposibles. | **Arreglado:** una rama de `if`/`match` como valor admite `fail` (no `return`, que sería confuso porque la rama es una lambda al instante). |
| 8 | **Un `match` o `if` como valor fijaba su tipo con la primera rama:** `fn primario() -> Nodo = match t.clase { Clase.Numero -> Numero(..) ... Clase.Abre -> grupo() }` daba «se esperaba Numero, pero es Nodo», aunque la función declara `-> Nodo`. Obligaba a usar `fix` intermedios con tipo. | **Arreglado:** cuando el contexto conoce el tipo (un `return`, `-> Tipo = ...`, `fix x: T = ...`, un argumento), cada rama se comprueba contra él. Sin tipo conocido sigue fijándose con la primera rama (se anota el tipo). |
| 9 | No hay un tipo «nunca»: tras `fail` o `salir` el verificador sigue pidiendo un valor. Aquí no molestó porque todo se resolvió con `?: fail` y `if/else`. | Anotado (ver el 6). |
| 10 | `Float.parse("1.2.3")` da `nil` (no falla), y el analizador lo convierte en un error legible. | Sin problema. |

Un fallo **del programa, no del lenguaje**, que las pruebas cazaron: la gramática daba `-2 ^ 2 = 4` en vez de `-4` (el menos unario ataba más que la potencia). Es la
clase de error que justifica tener el marco de pruebas a mano.

Lo que funcionó bien al primer intento en los dos proyectos: el marco de pruebas (`prueba`, `afirmaIgual`), `use` entre archivos con `open`, las excepciones propias
(`ErrorUso`), `x ?: fail ...`, la recursión con una lambda local, `Mapa` con orden de inserción para el resumen, y los archivos y carpetas
(`listaDir`, `esDir`, `unirRuta`, `leeLineas`).
