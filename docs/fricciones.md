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

Lo que funcionó bien al primer intento: el marco de pruebas (`prueba`, `afirmaIgual`), `use` entre archivos con `open`, las excepciones propias
(`ErrorUso`), `x ?: fail ...`, la recursión con una lambda local, `Mapa` con orden de inserción para el resumen, y los archivos y carpetas
(`listaDir`, `esDir`, `unirRuta`, `leeLineas`).
