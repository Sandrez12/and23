# Guía de errores y de migración

Los errores de And23 llevan el archivo, la línea, la columna y el trozo de código subrayado. Hay tres clases:

- **De sintaxis y de tipos:** se detectan **antes** de ejecutar nada; si hay uno, no se ejecuta nada y el programa termina con código 1.
- **De ejecución:** ocurren mientras corre el programa; escriben el mensaje y la cadena de llamadas pendientes (`llamado desde ...`).
- **Excepciones sin capturar:** un `fail` que nadie rescata termina el programa con `excepción sin capturar `Clase`: mensaje`.

Los bloques `jb-error` de este documento son programas que el verificador rechaza (un test lo comprueba).

## Errores de sintaxis

| Mensaje | Qué pasó | Cómo arreglarlo |
|---|---|---|
| `se esperaba una expresión, pero se encontró `*`` | Falta un valor donde el lenguaje esperaba uno (`fix x = * 3`). | Escribe la expresión que falta. |
| `esta cadena no se cierra: falta `"`` | Una cadena abierta sin la comilla final. | Cierra la cadena; para una comilla dentro usa `\"`. |
| `` `else` debe ir en la misma línea que la `}` del `if` `` | El `else` está en una línea nueva. | Escribe `} else {` en una sola línea. |
| `` `return` solo se puede usar dentro de una función `` | Un `return` al nivel superior. | Ponlo dentro de una función o lambda. |
| `entero fuera de rango (i64)` | Un literal como `9223372036854775808`. | Un `Int` llega hasta 9223372036854775807; usa un `Float` si necesitas más. |

## Errores de tipos

```jb-error
fix n: Int = "hola"
```

`se esperaba Int, pero es Str`: el valor no tiene el tipo que pide el sitio donde se usa. No hay conversiones implícitas (tampoco entre
`Int` y `Float`: usa `aFloat`, `piso`, `techo` o `redondea`).

```jb-error
show(total)
```

`la variable `total` no está definida`: el nombre no existe a la vista (revisa la ortografía y que no esté declarado dentro de otro bloque).
Una función o clase del mismo archivo sí es visible desde cualquier punto, pero una variable del nivel superior solo después de declararla.

```jb-error
fix x = 1
x = 2
```

`no se puede asignar a `x` porque es inmutable; decláralo con `mut``: una variable `fix` no se reasigna. Cámbiala a `mut`.

```jb-error
fix x = 1
fix x = 2
```

`` `x` ya está declarada en este ámbito ``: dos variables con el mismo nombre en el mismo bloque. Reasigna con una variable `mut` o usa otro nombre
(un bloque interior sí puede volver a declararlo).

```jb-error
fn f(s: Str?) -> Int = s.largo()
```

`este valor es Str? y puede ser nil: usa `?.`, `?:` o comprueba `if x != nil` antes de acceder a los métodos de Str`: un opcional hay que
resolverlo antes de usarlo. Opciones: `s?.largo() ?: 0` (valor por defecto), `if s != nil { s.largo() }` (comprobar), `s ?: return 0` (salir) o `s!!.largo()`
(lanza un `Fallo` si es nil).

```jb-error
fn f(a: Int) = a
show(f(1, 2))
```

`la función `f` espera 1 argumento(s), pero recibió 2`: sobra o falta un argumento.

```jb-error
fn f(n: Int) -> Int {
    if n > 0 { return 1 }
}
```

`la función `f` puede terminar sin devolver un valor de tipo Int`: algún camino llega al final sin `return`. Añade un `return` final o un `else`.

```jb-error
fn f(n: Int) = if n == 0 { 0 } else { f(n - 1) }
```

`` `f` se llama a sí misma antes de conocer su tipo; añade `-> Tipo` a su declaración ``: una función recursiva debe escribir su tipo de retorno.
Lo mismo pasa con una lambda local recursiva (`fix f: (Int) -> Int = { ... f(x) ... }`: hay que anotar el tipo de la variable).

```jb-error
fix n = 1
if n { show(1) }
```

`la condición debe ser Bool, pero es Int`: no hay valores «verdaderos» o «falsos» de otros tipos. Compara explícitamente (`n != 0`).

```jb-error
enum C { A, B }
fn f(c: C) -> Int = match c {
    C.A -> 1
}
```

`este `match` no cubre `C.B`: añade esos casos o un caso `_``: un `match` sobre un enumerado debe cubrir todas las variantes.

```jb-error
fail "hola"
```

`solo se pueden lanzar objetos de clases que hereden de `Fallo`, pero esto es Str`: `fail Fallo("hola")`, o una clase tuya que herede de `Fallo`.

```jb-error
kind K {
    fn f() -> Int
}
thing T() : K {
}
```

`la clase `T` no implementa el método `f` de la interfaz `K``: una clase que declara una interfaz debe implementar todos sus métodos.

```jb-error
fix l = Lista()
```

`no se puede deducir el tipo `T` de la clase `Lista`; escríbelo, por ejemplo `Lista<Int>(...)``: escribe el tipo, `Lista<Int>()`.

```jb-error
fix x: Persona = nil
```

`el tipo `Persona` no existe ...`: la clase no está declarada (o se escribió mal). Además `nil` solo se asigna a un opcional (`Persona?`).

```jb-error
fix a = 1
fix b = a ?: 0
```

`el operador `?:` necesita un valor opcional (`T?`), pero esto es Int`: `?:` (y `!!`) solo se usan con valores que pueden ser `nil`.

### Módulos

| Mensaje | Cómo arreglarlo |
|---|---|
| `no se encontró el archivo `x.jb` (de `use "x"`)` | La ruta es relativa al archivo que escribe el `use`, sin `.jb`. |
| `dependencia circular entre archivos: a -> b -> a` | Dos archivos se importan mutuamente: saca lo común a un tercero. |
| `la variable `f` no está definida (`f` es privado en `util`: declárala con `open` allí)` | Márcalo con `open` en el módulo. |
| `` `x` viene de dos `use` distintos; usa `como` para distinguirlos `` | Importa uno con `use "ruta" como alias`. |

## Errores de ejecución

| Mensaje | Causa |
|---|---|
| `división entre cero` | `/` o `%` con divisor 0 (también con `Float`). |
| `índice fuera de rango: 5 en un arreglo de 2 elemento(s)` | `a[i]`, `s[i]`, `lista.obtiene(i)`... con un `i` que no existe. |
| `desbordamiento de entero (i64)` | Una operación sale del rango de `Int`. |
| `desbordamiento de pila: más de 100000 llamadas anidadas` | Recursión sin fin (o muy profunda). `--max-depth N` cambia el límite. |
| `interbloqueo: todas las tareas esperan` | Todas las tareas esperan algo que nadie va a dar (un canal vacío, una tarea que no termina). |
| `no se puede convertir ... a Int` | `piso`, `techo` o `redondea` de un `Float` que no cabe (o `nan`/`inf`). |
| `excepción sin capturar `ErrorES`: ...` | Falló una operación de archivos. Capturala con `attempt { ... } rescue e: ErrorES { ... }`. |
| `error: N tarea(s) fallaron y nadie las esperó: ...` | Una tarea terminó con un `Fallo` y ninguna llamó a `espera()`. |

Estos errores terminan el programa; solo las excepciones (`fail`) se capturan con `attempt`/`rescue`.

## Migrar desde 0.x

Lo que cambia respecto a 0.3.0 (el detalle está en el [CHANGELOG](../CHANGELOG.md)). Casi todo es nuevo y no rompe programas; esto sí puede afectarte:

- **`Mapa` se recorre en orden de inserción** (antes salía en el orden del hash). Un programa que dependía del orden anterior ahora lo ve ordenado por inserción.
- **Un valor devuelto que es a veces `T` y a veces `nil`**, en una función o lambda sin tipo anotado, ahora da `T?` (antes pedía anotar el tipo).
- **Una lambda local anotada con un tipo de función puede llamarse a sí misma:** si llamaba a una función exterior con el mismo nombre que su propia variable,
  ahora se llama a sí misma.
- **Un `if`/`else` como último valor de una lambda** devuelve el valor de la rama (antes la lambda devolvía `Unit`). Si sus ramas tienen tipos distintos y la lambda
  no tiene tipo anotado, ahora es un error de tipos.
- **`and23 --tree` desapareció** (el intérprete de árbol): usa `and23 archivo.jb`.
- **Los ids internos del bytecode** (`--dump`, `--disasm`) no son parte de la interfaz estable.

Novedades que puedes empezar a usar: `x.formato(2)`, `x?.metodo()` sobre `Str?`/`Float?`, `x ?: return`, `x ?: fail ...`, `x!!`, constructores y métodos como valores
(`aplica(Persona, "Ana", 30)`, `fix f = obj.metodo`), `use` en el REPL, `listaDe([...])`, `Dupla`, `enum`/`match`, `and23 --prueba` y `and23 fmt`.
