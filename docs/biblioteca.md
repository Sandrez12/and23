# Biblioteca estándar de And23

Todo lo que hay disponible sin importar nada. Casi toda está escrita en el propio lenguaje (`crates/and23-syntax/prelude.jb`);
unas pocas funciones son nativas (las implementa la VM). Un programa puede definir una función o clase con el mismo nombre que
una del preludio: la suya gana. Los nombres que empiezan por `_` son internos y no forman parte de la API.

Las firmas usan `T?` para «`T` o `nil`», `[T]` para un arreglo y `(A, B) -> R` para una función.

- [Integradas](#integradas) · [Texto](#texto-str) · [Números](#números) · [Lista](#listat) · [Mapa](#mapak-v) · [Dupla](#duplaa-b)
- [Excepciones](#excepciones) · [Archivos y carpetas](#archivos-y-carpetas) · [Consola](#consola-y-entorno) · [Tiempo y azar](#tiempo-y-azar)
- [Tareas y canales](#tareas-y-canales) · [Pruebas](#pruebas)

## Integradas

Son parte del lenguaje, no del preludio.

| Nombre | Qué hace |
|---|---|
| `show x` / `show(x)` | Escribe `x` y un salto de línea. Es una sentencia, no una función (no se puede pasar como valor). |
| `len(x)` | Largo de un arreglo o de un texto (en caracteres). |
| `push(arreglo, x)` | Agrega `x` al final de un arreglo. |
| `pop(arreglo)` | Quita y devuelve el último elemento de un arreglo. |
| `hash(x)` | Un entero no negativo que es igual para valores iguales (lo usa `Mapa`). |

`show` escribe los `Float` con su parte decimal (`3.0`), una `Lista` como `[1, 2]`, un `Mapa` como `{"a": 1}` y un objeto como
`Clase(campo=valor)`.

## Texto (`Str`)

Un `Str` es inmutable. Las posiciones cuentan **caracteres**, igual que `len`. Los métodos se llaman con punto: `"hola".mayus()`.
Sobre un `Str?` se llaman con `?.` (`s?.largo()` da `nil` si `s` lo es).

| Método | Resultado |
|---|---|
| `s.largo()` | `Int`: número de caracteres. |
| `s.vacia()` | `Bool`: si no tiene caracteres. |
| `s.recorta()` | `Str` sin los espacios (Unicode) de los extremos. |
| `s.mayus()` / `s.minus()` | `Str` en mayúsculas / minúsculas. |
| `s.contiene(t)` | `Bool`: si `t` aparece en `s`. |
| `s.indiceDe(t)` | `Int`: posición de la primera aparición de `t`, o `-1`. |
| `s.empiezaCon(t)` / `s.terminaCon(t)` | `Bool`. |
| `s.reemplaza(de, por)` | `Str` con todas las apariciones de `de` cambiadas por `por`. |
| `s.trozo(desde, hasta)` | `Str` con los caracteres de `desde` (incluido) a `hasta` (excluido); los límites se recortan al texto. |
| `s.parte(sep)` | `Lista<Str>`: los trozos separados por `sep` (con `sep` vacío, uno por carácter). |
| `s.lineas()` | `Lista<Str>`: las líneas, separadas por `\n` o `\r\n`; un salto final no crea una línea vacía. |
| `s.caracteres()` | `Lista<Str>`: cada carácter como texto. |
| `s.esLetra()` / `s.esDigito()` / `s.esAlfanumerico()` | `Bool`: si el texto no es vacío y todos sus caracteres son letras Unicode / dígitos ASCII / letras o dígitos Unicode. |
| `s[i]` | `Str` de un carácter (error si `i` no está en rango). Cuesta O(i) si hay caracteres no ASCII: para recorrer un texto largo usa `s.caracteres()`. |

Funciones de texto sueltas:

| Función | Resultado |
|---|---|
| `une(partes, sep)` | `Str`: une las cadenas de una `Lista<Str>` poniendo `sep` entre ellas. |
| `desdeCodigo(n)` | `Str?`: el carácter con ese punto de código Unicode, o `nil` si no es uno válido. |

Las cadenas admiten interpolación (`"hola ${nombre}"`) y los escapes `\n \t \r \0 \\ \" \$ \u{...}`.

## Números

`Int` es un entero de 64 bits (desbordar es un error de ejecución) y `Float` un decimal de 64 bits. No hay conversión implícita
entre ellos: se usan `aFloat`, `piso`, `techo` y `redondea`. Dividir entre cero es un error de ejecución (también con `Float`).

| Función | Resultado |
|---|---|
| `Int.parse(s)` | `Int?`: el número de `s` (sin contar los espacios de los extremos), o `nil` si no lo es. |
| `Float.parse(s)` | `Float?`: igual; `nan` e `inf` dan `nil`. |
| `absoluto(n)`, `minimo(a, b)`, `maximo(a, b)` | Sobre `Int`. |
| `absolutoF(x)`, `minimoF(a, b)`, `maximoF(a, b)` | Sobre `Float`. |
| `raiz(x)` | `Float`: raíz cuadrada (`nan` si `x` es negativo). |
| `potencia(base, exponente)` | `Float`. |
| `piso(x)`, `techo(x)`, `redondea(x)` | `Int`: hacia abajo, hacia arriba y al más cercano (los empates hacia fuera). Un `Float` no finito o fuera de rango es un error. |
| `aFloat(n)` | `Float` con el valor de `n`. |
| `x.formato(decimales)` | `Str`: `x` con exactamente ese número de decimales (0 a 100). Redondea el valor binario exacto como `printf`: `2.675.formato(2)` es `"2.67"` y los empates exactos van al par (`2.5.formato(0)` es `"2"`). |

## `Lista<T>`

Una lista que crece. Se crea con `Lista<Int>()` o `listaDe([1, 2, 3])` (que copia los elementos de un arreglo literal).
Los índices empiezan en 0 y un índice fuera de rango es un error de ejecución.

| Método | Resultado |
|---|---|
| `l.agrega(x)` | Agrega `x` al final. |
| `l.tam()` | `Int`: cuántos elementos tiene. |
| `l.vacia()` | `Bool`. |
| `l.obtiene(i)` | `T`: el elemento `i`. |
| `l.pone(i, x)` | Reemplaza el elemento `i`. |
| `l.quita()` | `T`: quita y devuelve el último. |
| `l.quitaEn(i)` | `T`: quita y devuelve el elemento `i`. |
| `l.inserta(i, x)` | Inserta `x` antes de la posición `i` (`i == tam()` agrega al final). |
| `l.indiceDe(x)` | `Int`: posición de la primera aparición de `x` (con `==`), o `-1`. |
| `l.contiene(x)` | `Bool`. |
| `l.copia()` | `Lista<T>`: una copia. |
| `l.paraCada(f)` | Llama a `f` con cada elemento, en orden. |
| `l.map(f)` | `Lista<R>`: `f` aplicada a cada elemento. |
| `l.filter(f)` | `Lista<T>`: los elementos para los que `f` da `true`. |
| `l.reduce(inicial, f)` | `R`: combina de izquierda a derecha, `f(f(inicial, a), b)`. |
| `l.busca(f)` | `T?`: el primer elemento para el que `f` da `true`, o `nil`. |
| `l.alguno(f)` / `l.todos(f)` | `Bool`: si alguno / todos cumplen `f`. |
| `l.ordena(antes)` | Ordena la lista con `antes(a, b)` (`true` si `a` va antes que `b`). Es una mezcla estable de O(n log n): los que no van uno antes que el otro conservan su orden. Si `antes` falla, la lista queda como estaba. |
| `l.texto()` | `Str`: `[1, 2, 3]`. |

## `Mapa<K, V>`

Una tabla hash. Las claves de texto se comparan por contenido y las de objetos, por identidad.
**Se recorre en el orden en que se pusieron las claves**: poner una clave que ya está conserva su lugar, y quitarla y volver a ponerla
la deja al final.

| Método | Resultado |
|---|---|
| `m.pone(clave, valor)` | Asocia `clave` con `valor` (reemplaza el valor si ya estaba). |
| `m.obtiene(clave)` | `V?`: el valor, o `nil` si no está. |
| `m.contiene(clave)` | `Bool`. |
| `m.quita(clave)` | `Bool`: si estaba. |
| `m.tam()` / `m.vacio()` | `Int` / `Bool`. |
| `m.claves()` / `m.valores()` | `Lista<K>` / `Lista<V>`, en orden de inserción. |
| `m.paraCada(f)` | Llama a `f(clave, valor)` con cada entrada, en orden de inserción. Si `f` cambia el mapa, las claves nuevas no se visitan y las quitadas que aún no se visitaron se saltan. |

`Mapa` también tiene métodos internos (`cubetaDe`, `crece`, `compacta`) que no son API.

## `Dupla<A, B>`

Dos valores juntos: `Dupla("edad", 30)`. Campos `primero` y `segundo`; `d.intercambiada()` da una `Dupla<B, A>`.

## Excepciones

Se lanzan con `fail` y se capturan con `attempt { ... } rescue e: Clase { ... }`. Solo se pueden lanzar objetos de clases que
heredan de `Fallo`.

| Clase | Descripción |
|---|---|
| `Fallo(msg)` | La clase base. El texto está en `e.msg`. |
| `ErrorES(m)` | Falla una operación de archivos o carpetas. |
| `ErrorCanal(m)` | Se envía a un canal cerrado. |
| `FalloPrueba(m)` | Una afirmación que no se cumple (ver [Pruebas](#pruebas)). |
| `FalloDeTarea(error)` | Interna: registra el `Fallo` de una tarea para avisar si nadie lo esperó. |

`x ?: fail Fallo("falta")` lanza si `x` es `nil`, y `x!!` da el valor o lanza un `Fallo`.

## Archivos y carpetas

Los fallos lanzan `ErrorES`. Las rutas aceptan `/` y `\`.

| Función | Resultado |
|---|---|
| `leeArchivo(ruta)` | `Str`: el contenido (se ignora un BOM inicial). |
| `leeLineas(ruta)` | `Lista<Str>`: las líneas del archivo. |
| `escribeArchivo(ruta, texto)` | Crea o reemplaza el archivo. |
| `agregaArchivo(ruta, texto)` | Agrega al final (lo crea si no existe). |
| `borraArchivo(ruta)` | Borra un archivo. |
| `existe(ruta)` | `Bool`: si existe un archivo o carpeta. |
| `esDir(ruta)` | `Bool`: si es una carpeta. |
| `listaDir(ruta)` | `Lista<Str>`: nombres del contenido, en orden alfabético. |
| `creaDir(ruta)` | Crea la carpeta y las que falten por encima. |
| `unirRuta(a, b)` | `Str`: une dos partes con `/` (si `b` es absoluta, gana). |
| `nombreDe(ruta)` | `Str`: el último componente. |
| `carpetaDe(ruta)` | `Str`: la ruta sin su último componente. |
| `extension(ruta)` | `Str`: extensión del último componente, sin el punto (`""` si no tiene). |

## Consola y entorno

| Función | Resultado |
|---|---|
| `leeLinea()` | `Str?`: una línea de la entrada estándar sin el salto, o `nil` al terminar. Bloquea toda la VM mientras espera. |
| `argumentos()` | `Lista<Str>`: lo que sigue al archivo en la línea de comandos (`and23 programa.jb uno dos`). |
| `entorno(nombre)` | `Str?`: el valor de una variable de entorno, o `nil`. |

## Tiempo y azar

| Función | Resultado |
|---|---|
| `ahora()` | `Int`: milisegundos desde el 1 de enero de 1970 (UTC). |
| `duerme(ms)` | Espera al menos `ms` milisegundos; mientras, otras tareas avanzan. |
| `azar(n)` | `Int` al azar en `[0, n)` (`n` debe ser positivo). |
| `azarF()` | `Float` al azar en `[0, 1)`. |
| `semilla(n)` | Fija la semilla: la misma semilla da siempre la misma secuencia. No es criptográfico. |

## Tareas y canales

Concurrencia cooperativa dentro de una sola VM: una tarea solo cede el control cuando llama a `cede()`, cuando espera (un canal
vacío o lleno, `espera()`) o cuando termina. Es determinista y no hay carreras. Si todas las tareas esperan, es un error de
interbloqueo. Cuando termina `main`, las tareas pendientes se descartan.

| Función o método | Resultado |
|---|---|
| `lanza(f)` | `Tarea`: crea una tarea que ejecuta `f` y vuelve de inmediato; empieza cuando la actual cede o espera. |
| `lanzaDe(f)` | `TareaDe<T>`: como `lanza`, pero `f` devuelve un valor. |
| `t.espera()` | Bloquea hasta que la tarea termine; entrega el resultado (en `TareaDe<T>`) o relanza el `Fallo` con el que terminó. |
| `t.terminada()` | `Bool`. |
| `cede()` | Pasa al final de la cola de tareas listas. |
| `Canal<T>(capacidad)` | Una cola acotada entre tareas. |
| `c.envia(x)` | Espera si está llena; enviar a un canal cerrado lanza `ErrorCanal`. |
| `c.recibe()` | `T?`: espera si está vacía; da `nil` solo si está cerrado y vacío. |
| `c.cierra()` | Cierra el canal. |
| `c.tam()` / `c.estaCerrado()` | `Int` / `Bool`. |

Si una tarea termina con un `Fallo` y nadie llama a `espera()`, al terminar el programa se avisa
(`error: N tarea(s) fallaron y nadie las esperó: ...`) y el código de salida es 1. `Tarea` y `TareaDe` tienen además un método interno
`corre`.

## Pruebas

Se escriben `prueba "nombre" { ... }` y se ejecutan con `and23 --prueba archivo.jb`. Una prueba falla si lanza un `Fallo`.

| Función | Resultado |
|---|---|
| `afirma(condicion)` | Falla la prueba si `condicion` es falsa. |
| `afirmaIgual(real, esperado)` | Falla la prueba si no son iguales (`==`), con el mensaje «se esperaba X, pero fue Y». |
