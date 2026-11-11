# Referencia del lenguaje And23

Qué significa cada construcción del lenguaje y qué reglas aplica el verificador de tipos. Para aprender el lenguaje paso a paso
está el [tutorial](tutorial.md); para las funciones y clases que vienen incluidas, la [biblioteca](biblioteca.md); para los mensajes de
error, la [guía de errores](errores.md). Los bloques `jb` de este documento son programas completos que el verificador acepta; los
`jb-error` son programas que rechaza (un test lo comprueba).

1. [Estructura de un programa](#1-estructura-de-un-programa) · 2. [Léxico](#2-léxico) · 3. [Tipos](#3-tipos) · 4. [Variables](#4-variables)
5. [Expresiones](#5-expresiones) · 6. [Sentencias](#6-sentencias) · 7. [Funciones](#7-funciones) · 8. [Lambdas](#8-lambdas)
9. [Clases e interfaces](#9-clases-e-interfaces) · 10. [Genéricos](#10-genéricos) · 11. [Opcionales](#11-opcionales)
12. [Excepciones](#12-excepciones) · 13. [Enumerados y match](#13-enumerados-y-match) · 14. [Módulos](#14-módulos)
15. [Pruebas](#15-pruebas) · 16. [Ejecución y errores](#16-ejecución-y-errores) · 17. [Límites](#17-límites)

## 1. Estructura de un programa

Un archivo `.jb` es una lista de **declaraciones** (`fn`, `thing`, `kind`, `enum`, `use`, `prueba`) y **sentencias** al nivel superior.
Las sentencias se ejecutan en orden al cargar el archivo; después, si existe, se llama a `main()` (sin parámetros).

```jb
show("primero")

fn main() {
    show("después")
}
```

Una declaración puede usarse antes de aparecer en el archivo (las funciones y clases son visibles en todo el programa); una variable
del nivel superior, no. Las sentencias se separan con saltos de línea (o `;`). Los comentarios son `// hasta el final de la línea`.

## 2. Léxico

- **Identificadores:** letras Unicode, dígitos y `_`, sin empezar por dígito (`edad`, `dueño`, `_x1`).
- **Palabras reservadas (24):** `fix mut fn thing this kind super is as nil attempt rescue fail if else while return break continue true false show enum match`.
  No pueden usarse como nombres. Esta lista es cerrada en toda la serie 1.x. `use`, `open`, `como` y `prueba` solo son especiales en su sitio.
- **Enteros:** `42`, `1_000_000`. Un `Int` es de 64 bits; escribir un literal fuera de rango es un error.
- **Decimales:** `3.14`, `1e3`, `2.5E-4`. `1.` no es un decimal (`1.metodo` sería un acceso).
- **Cadenas:** `"texto"` con los escapes `\n \t \r \0 \\ \" \$ \u{1F600}` y la interpolación `${expresión}`, que puede contener otras cadenas.
- **Operadores y signos:** `+ - * / % == != < <= > >= && || ! = += -= *= /= %= -> ?. ?: !! ? . , : ; ( ) { } [ ]`.

## 3. Tipos

| Tipo | Valores |
|---|---|
| `Int` | enteros de 64 bits; desbordar es un error de ejecución |
| `Float` | decimales de 64 bits |
| `Bool` | `true` y `false` |
| `Str` | texto inmutable (Unicode); se compara por contenido |
| `Unit` | «nada»: lo que devuelven las funciones sin valor; no se puede guardar en una variable |
| `[T]` | arreglo de `T`; se crea con `[1, 2, 3]`; se compara por referencia |
| `T?` | `T` o `nil` (sección 11) |
| `(A, B) -> R` | función o lambda |
| `Clase`, `Interfaz`, `Clase<T>` | objetos (secciones 9 y 10) |

No hay conversión implícita entre `Int` y `Float` al asignar o pasar argumentos; la aritmética y las comparaciones que mezclan los
dos dan `Float`. `x as T` comprueba en ejecución que un objeto es de la clase `T` y `x is T` pregunta si lo es.

```jb
fix a = 7
fix b = 2.5
fix c = a * b            // Float: 17.5
fix d: Float = aFloat(a) // la conversión es explícita
show(c + d)
```

```jb-error
fix a: Float = 3
```

## 4. Variables

`fix` declara una variable que no se reasigna; `mut`, una que sí. El tipo se deduce del valor o se anota con `:`.
Un nombre no puede declararse dos veces en el mismo bloque, pero un bloque interior puede volver a declararlo.

```jb
fix nombre = "Ana"
mut edad = 30
edad += 1
fix grande: Float = 1e3
mut lista: [Int] = []
push(lista, edad)
show("${nombre} ${edad} ${grande} ${len(lista)}")
```

```jb-error
fix x = 1
x = 2
```

Una variable `fix` **de un objeto** no cambia de objeto, pero el objeto puede cambiar si sus campos son `mut`.

## 5. Expresiones

### Operadores (de menor a mayor precedencia)

| Nivel | Operadores | Asociatividad |
|---|---|---|
| 0 | `a ?: b` | izquierda |
| 1 | `\|\|` | izquierda |
| 2 | `&&` | izquierda |
| 3 | `==` `!=` | izquierda |
| 4 | `<` `<=` `>` `>=` `is` | izquierda |
| 5 | `+` `-` | izquierda |
| 6 | `*` `/` `%` | izquierda |
| 7 | `x as T` | izquierda |
| 8 | `-x` `!x` (prefijos) | |
| 9 | `x.campo` `x.metodo()` `x?.campo` `x[i]` `f(x)` `x!!` (sufijos) | izquierda |

`&&` y `||` exigen `Bool` en los dos lados y no evalúan el derecho si no hace falta. `+` también une dos `Str`; para unir un texto con otros
valores se usa la interpolación. `/` entre enteros es división entera; dividir entre cero es un error de ejecución. `==` compara
números por valor, `Str` por contenido y objetos y arreglos por referencia (no hay sobrecarga de operadores).

### `if` como valor

```jb
fix n = 5
fix texto = if n > 3 { "grande" } else { "chico" }
show(texto)
```

Un `if` usado como valor necesita `else`. Cada rama es **una** expresión, o un `fail Fallo("...")` si ese caso no puede darse (`if b == 0 { fail Fallo("división") } else { a / b }`); para varias sentencias se usa una función o lambda (un `return` ahí no se admite). Si el contexto conoce el tipo (un `return`, `-> Tipo = ...`, `fix x: T = ...`), cada rama se comprueba contra él; si no, el tipo lo fija la primera rama.

### Acceso, índice y llamada

`a[i]` indexa un arreglo (o da el carácter de un `Str`); un índice fuera de rango es un error de ejecución. `obj.campo` lee un campo y
`obj.metodo(args)` llama a un método. `f(x)(y)` llama al resultado de `f(x)` si es una función.

## 6. Sentencias

```jb
mut i = 0
while i < 10 {
    i += 1
    if i % 2 == 0 { continue }
    if i > 7 { break }
    show(i)
}

if i > 5 {
    show("grande")
} else if i > 2 {
    show("mediano")
} else {
    show("chico")
}
```

- `if cond { ... } else { ... }`: la condición debe ser `Bool`. El `else` va en la misma línea que la `}` anterior.
- `while cond { ... }`, con `break` y `continue`.
- `return` y `return valor` (solo dentro de una función o lambda).
- `show(x)` o `show x`: escribe un valor y un salto de línea.
- Asignaciones `x = v`, `x += v` (también `-= *= /= %=`), `obj.campo = v` y `a[i] = v`.
- Una expresión suelta (una llamada, normalmente) también es una sentencia.

## 7. Funciones

```jb
fn fib(n: Int) -> Int {
    if n < 2 { return n }
    return fib(n - 1) + fib(n - 2)
}

fn doble(x: Int) = x * 2              // el tipo de retorno se deduce

fn saluda(nombre: Str) {              // sin `->` y sin valor: devuelve Unit
    show("hola ${nombre}")
}

fn main() {
    saluda("mundo")
    show(fib(10) + doble(4))
}
```

- Los parámetros llevan siempre su tipo y son inmutables. Un parámetro `x` puede volver a declararse con `fix x = ...` en el cuerpo.
- El tipo de retorno se anota con `-> Tipo`. Sin anotar, se deduce de los `return` (y de la expresión de `= expr`). Una función
  que se llama a sí misma antes de conocer su tipo (recursión) debe anotarlo.
- Si el tipo es distinto de `Unit`, todos los caminos deben devolver un valor.
- Una función puede devolver `T` en unos caminos y `nil` en otros: el tipo deducido es `T?`.
- Una función con nombre se puede usar como valor (`fix f = doble`), si no es genérica.
- Solo se pueden declarar funciones al nivel superior (no dentro de otras funciones; para eso hay lambdas).
- Una función del programa puede tener el mismo nombre que una del preludio: gana la del programa.

```jb-error
fn f(n: Int) -> Int {
    if n > 0 { return 1 }
}
```

## 8. Lambdas

Una lambda es `{ parámetros -> cuerpo }`. El valor de la lambda es el de su **última sentencia**, si es una expresión o un
`if`/`else` cuyas ramas terminan en una expresión; si no, devuelve `Unit`. Los tipos de los parámetros se dejan implícitos cuando el
contexto los conoce.

```jb
fix doble = { x: Int -> x * 2 }
fix suma: (Int, Int) -> Int = { a, b -> a + b }
fix signo = { n: Int -> if n < 0 { "negativo" } else if n == 0 { "cero" } else { "positivo" } }

fn aplica(f: (Int) -> Int, v: Int) -> Int = f(v)

show(aplica(doble, 4) + suma(1, 2))
show(signo(-3))
show(aplica({ n -> n + 100 }, 1))
```

**Lambda final:** si el último argumento de una llamada es una lambda, puede ir fuera de los paréntesis: `lista.paraCada { x -> show(x) }`.

**Capturas:** una lambda ve las variables del ámbito donde se crea. Una variable `mut` capturada es compartida: los cambios se ven por los dos lados.

```jb
mut contador = 0
fix incrementa = { -> contador += 1 }
incrementa()
incrementa()
show(contador)
```

**Recursión:** una lambda local que se llama a sí misma necesita el tipo de función anotado en su variable:

```jb
fn main() {
    fix fact: (Int) -> Int = { n -> if n <= 1 { 1 } else { n * fact(n - 1) } }
    show(fact(6))
}
```

```jb-error
fn main() {
    fix fact = { n: Int -> if n <= 1 { 1 } else { n * fact(n - 1) } }
    show(fact(6))
}
```

**Funciones, constructores y métodos como valores:** una función con nombre (`doble`), un constructor (`Persona`, si la clase no es
genérica) y un método atado a un objeto (`cuenta.suma`, sin paréntesis) se usan como cualquier lambda.

```jb
thing Persona(fix nombre: Str, fix edad: Int)

thing Cuenta(mut saldo: Int) {
    fn suma(n: Int) { saldo += n }
}

fn crea(f: (Str, Int) -> Persona) -> Persona = f("Ana", 30)

fn main() {
    show(crea(Persona).nombre)
    fix c = Cuenta(0)
    fix abona = c.suma
    abona(5)
    show(c.saldo)
}
```

## 9. Clases e interfaces

```jb
kind Figura {
    fn area() -> Float
    fn nombre() -> Str
}

thing Circulo(fix radio: Float) : Figura {
    fn area() -> Float = 3.14159 * radio * radio
    fn nombre() -> Str = "círculo"
}

thing Cuadrado(fix lado: Float) : Figura {
    fn area() -> Float = lado * lado
    fn nombre() -> Str = "cuadrado"
}

fn describe(f: Figura) -> Str = "${f.nombre()}: ${f.area().formato(2)}"

fn main() {
    show(describe(Circulo(2.0)))
    show(describe(Cuadrado(3.0)))
}
```

- `thing Nombre(parámetros) { ... }` declara una clase. Los **parámetros del constructor** con `fix` o `mut` son campos;
  los que no llevan ninguno solo existen durante la construcción (para pasarlos al padre o inicializar otros campos).
- Dentro del cuerpo: campos (`fix x = 0`, `mut y: Int = 0`) y métodos (`fn`). Un campo `fix` no se reasigna ni desde los métodos.
- Dentro de un método, los campos y los otros métodos se nombran sin `this.`; `this` es el objeto.
- **Herencia:** `thing Hijo(n: Str) : Padre(n)`. Una sola clase padre (con sus argumentos) y las interfaces que quiera, separadas por comas.
  Un método del hijo con el mismo nombre que uno del padre lo **sobrescribe** (misma firma); `super.metodo(...)` llama al del padre.
  Toda clase se puede heredar.
- **Interfaces:** `kind` solo declara firmas de métodos (sin cuerpo). Una clase que declara una interfaz debe implementar todos sus métodos.
- **Despacho:** la llamada a un método se resuelve por la clase real del objeto, no por el tipo de la variable.
- `obj is Clase` y `obj as Clase` preguntan y comprueban la clase en ejecución (también con interfaces).
- `==` entre objetos compara por referencia. `show` de un objeto escribe `Clase(campo=valor, ...)`.

```jb
thing Animal(fix nombre: Str) {
    fn habla() -> Str = "..."
    fn presenta() -> Str = "${nombre} dice ${habla()}"
}

thing Perro(nombre: Str) : Animal(nombre) {
    fn habla() -> Str = "guau"
}

fn main() {
    fix a: Animal = Perro("Rex")
    show(a.presenta())
    show(a is Perro)
}
```

```jb-error
kind Volador {
    fn vuela() -> Str
}

thing Piedra() : Volador {
}
```

## 10. Genéricos

Las clases, interfaces, funciones y métodos pueden tener parámetros de tipo entre `<>`. Los argumentos de tipo se deducen de los valores
o del contexto, o se escriben (`Lista<Int>()`, `identidad<Str>("a")`). Un parámetro de tipo puede tener una **cota** (`T: Interfaz`), que
permite llamar los métodos de esa interfaz sobre un `T`.

```jb
kind Comparable<T> {
    fn menorQue(otro: T) -> Bool
}

thing Edad(fix valor: Int) : Comparable<Edad> {
    fn menorQue(otro: Edad) -> Bool = valor < otro.valor
}

fn menor<T: Comparable<T>>(a: T, b: T) -> T = if a.menorQue(b) { a } else { b }

thing Caja<T>(fix contenido: T) {
    fn mapea<R>(f: (T) -> R) -> Caja<R> = Caja(f(contenido))
}

fn identidad<T>(x: T) -> T = x

fn main() {
    show(menor(Edad(30), Edad(25)).valor)
    show(Caja(20).mapea { n -> "valor ${n}" }.contenido)
    show(identidad<Str>("hola"))
}
```

Los genéricos solo existen en el verificador: en ejecución una `Lista<Int>` y una `Lista<Str>` son lo mismo. No hay varianza, comodines,
cotas múltiples ni tipos sin argumentos (`Lista` a secas es un error). Un constructor de clase genérica o un método genérico no se pueden
usar como valor sin envolverlos en una lambda.

## 11. Opcionales

`T?` admite un `T` o `nil`; un `T` común nunca es `nil`. Usar un `T?` donde se pide un `T` es un error de tipos: hay que comprobarlo.

| Forma | Significado |
|---|---|
| `x != nil`, `x == nil` | Comprobación. Dentro del `if` (o `&&`, `\|\|`, `!`, `while`) el verificador sabe que `x` no es `nil`. |
| `if x == nil { return }` | Después del `if`, `x` no es `nil` para el resto del bloque. |
| `x?.campo`, `x?.metodo(a)` | `nil` si `x` lo es (sin evaluar los argumentos); si no, el campo o el resultado, como opcional. Vale también con `Str?` y `Float?`. |
| `x ?: valor` | `x` si no es `nil`; si lo es, `valor`. |
| `x ?: return`, `x ?: return v`, `x ?: fail Fallo("...")` | Si `x` es `nil`, sale de la función o lanza; si no, es el valor de `x`. |
| `x!!` | El valor de `x`, o un `Fallo` si es `nil`. |

```jb
thing Persona(fix nombre: Str, fix jefe: Persona?)

fn nombreDelJefe(p: Persona) -> Str = p.jefe?.nombre ?: "sin jefe"

fn primero(l: Lista<Int>) -> Int {
    fix v = l.busca { n -> n > 10 } ?: return -1
    return v
}

fn main() {
    fix ana = Persona("Ana", nil)
    fix beto = Persona("Beto", ana)
    show(nombreDelJefe(beto))
    show(nombreDelJefe(ana))
    show(primero(listaDe([1, 20, 3])))
    fix p: Persona? = beto
    if p != nil { show(p.nombre) }
}
```

Se estrechan las variables locales, los parámetros, los campos `fix` de `this` y las globales; **no** los campos `mut` (cualquier llamada podría
cambiarlos: cópialos a un `fix` antes de comprobar) ni las variables `mut` capturadas por una lambda. `nil` solo se asigna a un opcional.
`break` y `continue` no pueden ir a la derecha de `?:` (se usa un `if`).

## 12. Excepciones

`fail valor` lanza un objeto de una clase que herede de `Fallo`; `attempt { ... } rescue e: Clase { ... }` lo captura. Se pueden poner
varios `rescue` (gana el primero cuya clase coincide, subclases incluidas). Una excepción que nadie captura termina el programa.

```jb
thing ErrorDeSaldo(m: Str, fix faltan: Int) : Fallo(m)

fn retira(saldo: Int, monto: Int) -> Int {
    if monto > saldo { fail ErrorDeSaldo("saldo insuficiente", monto - saldo) }
    return saldo - monto
}

fn main() {
    attempt {
        show(retira(100, 30))
        show(retira(100, 130))
    } rescue e: ErrorDeSaldo {
        show("${e.msg}: faltan ${e.faltan}")
    } rescue e: Fallo {
        show("otro error: ${e.msg}")
    }
}
```

Los errores de ejecución de la VM (división entre cero, índice fuera de rango, desbordamiento, `as` inválido, pila agotada...) **no** son
excepciones: terminan el programa con un mensaje y la traza de llamadas.

## 13. Enumerados y match

```jb
enum Color { Rojo, Verde, Azul }

fn nombre(c: Color) -> Str = match c {
    Color.Rojo -> "rojo"
    Color.Verde -> "verde"
    Color.Azul -> "azul"
}

kind Forma {
    fn id() -> Int
}
thing A() : Forma {
    fn id() -> Int = 1
}
thing B() : Forma {
    fn id() -> Int = 2
}

fn clase(f: Forma) -> Str = match f {
    a: A -> "es una A"
    b: B -> "es una B"
}

fn main() {
    show(nombre(Color.Verde))
    show(Color.Azul.orden)
    show(Color.todos.tam())
    show(clase(B()))
    fix n = 7
    match n {
        1 -> show("uno")
        7 -> show("siete")
        _ -> show("otro")
    }
}
```

- `enum Color { Rojo, Verde }` declara un tipo con valores constantes. Cada valor tiene `nombre` (`Str`) y `orden` (`Int`, desde 0), y
  `Color.todos` es la lista de todos ellos.
- `match valor { caso -> resultado ... }` compara el valor con cada caso en orden: un literal, una variante de enumerado, un tipo
  (`c: Circulo -> ...`, que además da nombre al valor con ese tipo) o `_` (cualquier cosa). Los casos pueden ir separados por saltos de línea.
- Como **sentencia** cada caso es una sentencia; como **expresión** cada caso es una expresión (o un `fail ...`) y el `match` es un valor, con el tipo esperado como en el `if`.
- Un `match` sobre un enumerado del mismo archivo debe cubrir todas las variantes (o tener `_`). Sin caso que coincida y sin `_`, falla
  con un `Fallo`.
- No hay guardas, desestructuración ni variantes con datos: para eso se usan clases con una interfaz común.

## 14. Módulos

`use "ruta"` importa lo que otro archivo marcó con `open`. La ruta es relativa al archivo que la escribe y no lleva la extensión `.jb`.
`use "ruta" como alias` lo importa bajo un alias (`alias.nombre`). Lo que no es `open` es privado del archivo.

```text
// util/mate.jb
open fn cuadrado(x: Int) -> Int = x * x
open fix PI = 3.14159
fn interno() = 0

// main.jb
use "util/mate" como mate
show(mate.cuadrado(4))
```

- Cada archivo se carga una sola vez, aunque varios lo importen. Las dependencias circulares son un error.
- Un nombre importado sin alias no puede coincidir con uno declarado en el archivo ni con otro importado.
- No hay reexportación ni importación de nombres sueltos.
- En el REPL, `use` carga relativo a la carpeta actual y se recuerda en las entradas siguientes.
- Los errores de un módulo dicen su archivo; usar algo privado avisa «es privado en ...».

## 15. Pruebas

```text
fn suma(a: Int, b: Int) -> Int = a + b

prueba "la suma suma" {
    afirmaIgual(suma(1, 2), 3)
    afirma(suma(2, 2) == 4)
}
```

`and23 --prueba archivo.jb` ejecuta cada bloque `prueba` y resume el resultado; una prueba falla si lanza un `Fallo` (las afirmaciones lanzan
`FalloPrueba`) o si una tarea que nadie esperó falló. El código de salida es 1 si alguna falla. Las pruebas de los módulos importados no se ejecutan.

## 16. Ejecución y errores

| Código de salida | Significado |
|---|---|
| 0 | El programa terminó bien. |
| 1 | Error de sintaxis o de tipos (no se ejecuta nada), error de ejecución, excepción sin capturar, o una tarea que falló sin que nadie la esperara. |
| 2 | Uso incorrecto de la línea de comandos. |

- Los errores de sintaxis y de tipos se detectan **antes** de ejecutar nada, con la línea y la columna.
- Un error de ejecución escribe el mensaje, la línea del código y la cadena de llamadas pendientes (`llamado desde ...`).
- Hay un límite de 100 000 llamadas anidadas (`--max-depth N` lo cambia); pasarlo es un error de ejecución, no un cierre abrupto.
- Las tareas son cooperativas (ver la [biblioteca](biblioteca.md#tareas-y-canales)).
- Línea de comandos: `and23 programa.jb [args]`, `and23` (REPL), `and23 fmt [--check] archivos`, `and23 --prueba archivo`, `and23 --check archivo`,
  `and23 --disasm archivo`, `and23 --gc-stress ...`, `and23 --version`.

## 17. Límites

Cosas que el lenguaje no hace (y que, de añadirse en 1.x, serían ampliaciones compatibles):

- Una sola clase padre; las interfaces no heredan de otras ni traen métodos por defecto.
- Sin cotas múltiples, varianza ni comodines en los genéricos.
- Sin guardas ni desestructuración en `match`.
- Sin `it` implícito, sin `::nombre` y sin lambdas genéricas. La recursión mutua entre dos lambdas locales no es posible (entre globales, sí).
- Sin reexportar ni importar nombres sueltos de un módulo; sin manifiesto de proyecto.
- Solo `Str`, `Float` y los objetos tienen métodos: `Int` y `Bool` no (`n.formato(2)` no existe; `Int.parse(s)` y `Float.parse(s)` se escriben sobre el nombre del tipo).
- Las tareas no se interrumpen por tiempo: una que no cede ocupa la VM; `leeLinea` bloquea todas.
- Los métodos de texto en cadenas no ASCII cuestan O(n) por posición (para recorrer un texto largo, `s.caracteres()`).
