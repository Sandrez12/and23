# And23

And23 es un lenguaje de programación estático y orientado a objetos, de nivel similar a Java, con compilador y máquina virtual de bytecode escritos en Rust. Es un proyecto de aprendizaje y a largo plazo: crece por hitos hasta tener clases, interfaces, genéricos, excepciones y GC, y la meta final es que el compilador se compile a sí mismo.

Los archivos de código usan la extensión `.jb`. Los identificadores admiten Unicode (`dueño`, `ñandú`).

> **Estado: M11.** Hoy verifica tipos y compila a bytecode programas con variables, `if`/`while`, funciones recursivas, cadenas con interpolación, arreglos, clases, herencia, interfaces, excepciones, tipos opcionales, genéricos, lambdas con closures, proyectos de varios archivos una biblioteca estándar inicial (texto, números y archivos) y tareas con canales, y los ejecuta en una VM de pila con recolector de basura (`fib(30)` en ~0,13 s, unas 9 veces más rápido que el intérprete de árbol anterior). Un programa mal tipado se rechaza con su línea y columna antes de ejecutar nada. El resto llega en los siguientes hitos (ver [`DESIGN.md`](DESIGN.md)).

**Primeros pasos:** [`docs/tutorial.md`](docs/tutorial.md) recorre el lenguaje con ejemplos que se ejecutan tal cual. Los cambios de cada versión están en [`CHANGELOG.md`](CHANGELOG.md).

## Cómo se ve

Clases con el constructor en la cabecera, herencia e interfaces (funciona desde M5):

```
thing Animal(fix nombre: Str) {
    mut edad = 0
    fn cumple() { edad += 1 }
}

fn main() {
    fix p = Animal("Rex")
    p.cumple()
    show("${p.nombre} tiene ${p.edad}")   // Rex tiene 1
}
```

El borrador completo, con herencia e interfaces:

```
kind Habla {                                   // interfaz: solo firmas
    fn sonido() -> Str
}

thing Perro(nombre: Str) : Animal(nombre), Habla {   // padre con argumentos, e interfaces
    fn sonido() -> Str = "guau"
}

fn main() {
    fix animales: [Animal] = [Perro("Rex"), Animal("Bicho")]
    fix h: Habla = Perro("Toby")
    show(h.sonido())                           // guau: se elige por la clase real
    show(animales[0] is Perro)                 // true
}
```

Excepciones y valores opcionales (funciona desde M6):

```
thing SaldoInsuficiente(fix pedido: Int) : Fallo("saldo insuficiente: pides ${pedido}")

fn retira(saldo: Int, monto: Int) -> Int {
    if monto > saldo { fail SaldoInsuficiente(monto) }
    return saldo - monto
}

attempt { show(retira(10, 50)) }
rescue e: SaldoInsuficiente { show(e.msg) }     // saldo insuficiente: pides 50

fix dueño: Perro? = nil
show(dueño?.nombre ?: "sin nombre")             // sin nombre
fix p: Perro? = Perro("Rex")
if p != nil { show(p.nombre) }                  // Rex: dentro del if, p ya no es opcional
```

Genéricos, con `Lista<T>` y `Mapa<K, V>` escritas en el propio lenguaje (funciona desde M7):

```
fix nombres = Lista<Str>()
nombres.agrega("Ana")
nombres.agrega("Luis")
show(nombres.tam())                             // 2

fix edades = Mapa<Str, Int>()
edades.pone("Ana", 30)
show(edades.obtiene("Ana") ?: 0)                // 30
show(edades.obtiene("Zoe") ?: 0)                // 0: obtiene devuelve Int?

thing Caja<T>(fix valor: T)
fix c = Caja(5)                                 // T se deduce: Caja<Int>
fn primero<T>(xs: [T]) -> T = xs[0]
show(primero(["a", "b"]))                       // a
```

Lambdas y closures (desde M8): `{ x -> ... }` es una función sin nombre que ve las variables de su alrededor:

```
fix numeros = Lista<Int>()
numeros.agrega(1)
numeros.agrega(2)
numeros.agrega(3)
show(numeros.map { n -> n * n }.texto())        // [1, 4, 9]
show(numeros.reduce(0, { acc, n -> acc + n }))  // 6

mut visitados = 0
numeros.paraCada { n -> visitados += n }        // la lambda modifica la variable de fuera
show(visitados)                                 // 6

fn contador() -> () -> Int {
    mut n = 0
    return {
        n += 1
        n
    }
}
fix c = contador()
show("${c()} ${c()}")                           // 1 2: `n` sigue vivo aunque contador() ya terminó
```

Varios archivos (desde M9): `use "ruta"` importa lo que otro archivo marcó con `open`; el resto es privado:

```
// util/mate.jb
open fn cuadrado(x: Int) -> Int = x * x
fn auxiliar() { ... }                 // privada: nadie fuera de este archivo la ve

// main.jb
use "util/mate"                       // los nombres abiertos entran directamente
use "formas" como f                   // o con alias: f.Circulo(2.0)
show(cuadrado(7))                     // 49
```

Biblioteca estándar (desde M10): métodos de texto, conversiones y archivos:

```
fix lineas = leeLineas("notas.csv")            // Lista<Str>; lanza ErrorES si no se puede leer
fix campos = lineas.obtiene(1).parte(",")      // "ana,4.5" -> ["ana", "4.5"]
fix nota = Float.parse(campos.obtiene(1))      // Float?: nil si no es un número
show(campos.obtiene(0).mayus())                // ANA
show(nota ?: 0.0)                              // 4.5
escribeArchivo("salida.txt", "listo
")
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
- Clases con `thing`: el constructor va en la cabecera y los parámetros con `fix`/`mut` se vuelven campos (los demás solo existen en el constructor). El cuerpo declara más campos (`mut edad = 0`, que ven los parámetros y los campos anteriores) y métodos (`fn`). Dentro de un método los campos se usan por su nombre (`edad += 1`) y `this` los desambigua (`this.edad`, o `this` solo, que es el propio objeto). Fuera: `p.nombre`, `p.edad = 5`, `p.cumple()`. Los objetos son referencias, como los arreglos; `fix` en un campo impide reasignarlo, y `==` los compara por referencia. `show(p)` imprime `Animal(nombre="Rex", edad=1)`.
- Herencia e interfaces: `thing Perro(nombre: Str) : Animal(nombre), Habla` hereda de una clase (con los argumentos de su constructor) e implementa interfaces `kind` (solo firmas). Un hijo puede usarse donde se pide el padre o la interfaz (`fix a: Animal = Perro("Rex")`, `[Animal]`, parámetros y retornos). Un método con el mismo nombre y la misma firma que uno heredado lo sobrescribe, y las llamadas son virtuales: se elige según la clase real del objeto, también las hechas con `this`. `super.metodo()` llama a la versión del padre. `x is T` pregunta por la clase real, un ancestro o una interfaz; `x as T` convierte y falla al ejecutar si no corresponde. Una clase solo hereda de una; implementa las interfaces que quiera.
- Excepciones: `fail unError` lanza un objeto de una clase que herede de `Fallo` (predefinida, con el campo `msg`), y `attempt { ... } rescue e: Tipo { ... }` lo captura; puede haber varios `rescue` (gana el primero que coincide, y un `rescue` de una clase base captura a sus hijas). Si ninguno coincide la excepción sigue subiendo, y sin capturar es un error de ejecución con el nombre, el mensaje y la posición del `fail`. `fail e` dentro de un `rescue` la relanza. Los errores de ejecución de la VM (división entre cero, índice fuera de rango, `as` inválido) siguen siendo fatales: no se pueden capturar. En el REPL el `rescue` va en la misma línea que el `attempt`.
- Tipos opcionales: `T?` admite un `T` o `nil`; un `T` común nunca es `nil` (`fix x: Perro = nil` es un error). Usar un `T?` como `T` (`p.nombre`, `p.m()`, pasarlo donde se pide `T`) se rechaza al compilar. Se trabaja con `p?.campo` y `p?.metodo()` (dan `nil` si `p` lo es y no evalúan los argumentos), `p ?: valor` (valor por defecto) y comprobando: tras `if p != nil { ... }` (o `&&`, `||`, `!`, `while p != nil`, y `if p == nil { return }` para el resto del bloque) el compilador sabe que `p` no es `nil`. Solo se estrechan variables locales, parámetros y globales `fix`; no los campos ni los `mut` globales.
- Genéricos con *erasure*: `thing Caja<T>(...)`, `kind Fuente<T>` y `fn primero<T>(xs: [T]) -> T` (los métodos usan los parámetros de su clase, pero no tienen los suyos). Los argumentos de tipo se escriben `Caja<Int>` o se deducen de los argumentos (`Caja(5)`), del tipo esperado (`fix l: Lista<Int> = Lista()`) o se dan explícitos (`Lista<Int>()`, `primero<Int>(xs)`). `<T: Ordenable<T>>` pone una cota: solo se aceptan tipos que la cumplan y dentro se pueden usar sus métodos y campos. Son invariantes (`Caja<Perro>` no es `Caja<Animal>`). Como en ejecución `T` no existe, los valores no se encajonan; por lo mismo `x is Lista` comprueba solo la clase y `as Lista<Int>` no existe.
- Biblioteca base (`crates/and23-syntax/prelude.jb`, escrita en And23 y cargada en cada sesión): `Lista<T>` (`agrega`, `obtiene`, `pone`, `tam`, `vacia`, `contiene`, `indiceDe`, `inserta`, `quita`, `quitaEn`, `copia`, `texto`) y `Mapa<K, V>`, una tabla hash (`pone`, `obtiene` que da `V?`, `contiene`, `quita`, `tam`, `vacio`, `claves`, `valores`). Sus nombres, `Fallo` y `Entrada` quedan reservados. Para escribirlas se añadieron `pop(arreglo)` y `hash(valor)`.
- Módulos: `use "ruta"` (sin `.jb`, relativa a la carpeta del archivo que importa) trae las declaraciones `open` de ese archivo (`open fn`, `open thing`, `open kind`, `open fix`); `use "ruta" como a` las deja bajo el alias (`a.f(1)`, `a.Clase(..)`, `x: a.Clase`, `: a.Padre`). Sin `open` una declaración es privada, y dos archivos pueden tener funciones privadas del mismo nombre. Cada archivo se carga una sola vez y se ejecuta antes que quien lo importa; una dependencia circular es un error. Los errores indican el archivo, la línea y la columna correctos. `and23 main.jb` ejecuta el proyecto desde el archivo principal (su `main()` es el punto de entrada). El REPL no admite `use`.
- `show` de una `Lista` imprime `[1, 2, 3]` y el de un `Mapa` imprime `{"a": 1}` (el orden del `Mapa` no es el de inserción). Una variable `mut` del nivel superior se estrecha como una local mientras ninguna función o lambda la asigne: `mut l = leeLinea()` y `while l != nil { ... l = leeLinea() }` funciona.
- Biblioteca estándar (M10). **Texto**: `s.largo()`, `vacia()`, `recorta()`, `mayus()`, `minus()`, `contiene(t)`, `indiceDe(t)` (-1 si no está), `empiezaCon(t)`, `terminaCon(t)`, `reemplaza(de, por)`, `trozo(desde, hasta)`, `parte(sep)` y `lineas()` (ambas devuelven `Lista<Str>`), `s[i]` (la cadena de un carácter) y `une(partes, sep)`. Las posiciones cuentan caracteres, igual que `len`; las cadenas son inmutables. **Números**: `Int.parse(s)` y `Float.parse(s)` devuelven `Int?`/`Float?` (nil si no es un número; ignoran los espacios de los extremos), `absoluto`, `minimo`, `maximo` (y `absolutoF`, `minimoF`, `maximoF` para Float), `raiz`, `potencia`, `piso`, `techo`, `redondea` (Float a Int), `aFloat`. **Archivos y consola**: `leeArchivo(ruta)`, `leeLineas(ruta)`, `escribeArchivo(ruta, texto)`, `agregaArchivo`, `borraArchivo`, `existe(ruta)` y `leeLinea()` (una línea de la entrada estándar, nil al final). Los fallos de archivos lanzan `ErrorES`, que hereda de `Fallo`. Un programa puede definir una función con el mismo nombre que una del preludio y la suya gana.
- Tareas y canales (M11). Las tareas son cooperativas: se ejecutan en la misma VM y solo se alternan cuando una llama a `cede()`, espera o termina, así que no hay carreras de datos y el resultado es siempre el mismo. `lanza { ... }` crea una `Tarea` (sin resultado) y `lanzaDe<T> { ... }` una `TareaDe<T>`; `t.espera()` bloquea hasta que termine (y relanza su `Fallo`, si lo hubo) y `t.terminada()` la consulta. `Canal<T>(capacidad)` es una cola entre tareas: `envia(x)` espera si está llena, `recibe()` espera si está vacía y devuelve `nil` cuando el canal está cerrado y vacío, `cierra()` lo cierra y enviar a uno cerrado lanza `ErrorCanal`. Si todas las tareas esperan, el programa falla con «interbloqueo». Al terminar la tarea principal, las demás se descartan. Ver `examples/tareas.jb`.
- Biblioteca estándar 2. `argumentos()` (lo que sigue al archivo en `and23 programa.jb a b`), `entorno(nombre)` (`Str?`), `ahora()` (milisegundos desde 1970), `duerme(ms)` (deja avanzar a las otras tareas), `azar(n)` (entero en `[0, n)`), `azarF()`, `semilla(n)`, `unirRuta`, `nombreDe`, `carpetaDe`, `extension`, `esDir`, `listaDir` y `creaDir` (los fallos lanzan `ErrorES`). Los decimales aceptan exponente: `1e3`, `2.5E-4`. Un error de ejecución muestra la cadena de llamadas (``llamado desde `f` (archivo:línea:columna)``).
- Lambdas y closures: `{ a, b -> cuerpo }` (los tipos de los parámetros se deducen del contexto o se escriben `{ a: Int -> ... }`; `{ cuerpo }` sin `->` no tiene parámetros). La última expresión es el valor y `return` sale de la lambda. Si la lambda es el último argumento va fuera del paréntesis (`lista.map { x -> x }`, `f(1) { ... }`, y `f { ... }` si es el único). Los tipos de función se escriben `(Int, Str) -> Bool` (`() -> Unit` sin parámetros), y `((Int) -> Int)?` es opcional. Valen para variables, parámetros, retornos, arreglos y campos (`b.alClic(x)` llama al campo); `f(1)(2)` llama a lo que devuelve `f(1)`, y una función con nombre se pasa como valor (`aplica(triple, 4)`). Captura: lo `fix` se copia y lo `mut` se comparte por referencia (la lambda y la función de fuera ven los mismos cambios, también después de que esa función termine); `this` y los campos se capturan igual. Las funciones genéricas y los métodos admiten parámetros de tipo propios que se deducen de la lambda (`fn mapea<R>(f: (T) -> R) -> Caja<R>`; explícito: `c.mapea<Float> { ... }`). `Lista` ganó `paraCada`, `map`, `filter`, `reduce`, `busca`, `alguno`, `todos` y `ordena`, y `Mapa` ganó `paraCada`.
- Igualdad: `==` y `!=` valen entre valores del mismo tipo: los números por valor, las cadenas por contenido y los objetos y arreglos por referencia (`[1] == [1]` es falso). Un `nil` solo se compara con un opcional.
- Recolector de basura mark-sweep propio: 10 millones de asignaciones corren con el heap acotado. `--gc-stress` recolecta en cada punto seguro para probarlo.
- Los enteros se verifican: el desbordamiento y la división entre cero son errores con su posición. La recursión
  infinita da un error en vez de cerrar el proceso.
- El REPL recuerda variables y funciones entre líneas y acepta bloques en varias líneas.

Más ejemplos en [`examples/`](examples/): `aritmetica.jb`, `funciones.jb`, `control.jb`, `cadenas.jb`, `clases.jb` (un programa orientado a objetos de ~220 líneas), `polimorfismo.jb` (herencia, interfaces, `is`/`as`/`super`), `excepciones.jb` (excepciones propias y opcionales) `genericos.jb` (`Lista`, `Mapa`, clases, interfaces y funciones genéricas) `lambdas.jb` (lambdas, closures, callbacks y `Lista` con `map`/`filter`/`ordena`) `proyecto/` (un proyecto de tres archivos con `use`/`open`), `palabras.jb` (lee un texto y cuenta palabras con un `Mapa`) y `notas.jb` (lee un CSV, convierte texto a números y escribe un resumen); ambos se ejecutan desde la raíz del repositorio.

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
cargo run -p and23-cli -- --tree examples/funciones.jb     # intérprete de árbol (referencia, sin cadenas ni clases)
cargo run -p and23-cli -- --gc-stress examples/cadenas.jb  # GC en cada punto seguro
cargo run -p and23-cli -- examples/clases.jb               # programa orientado a objetos
cargo run -p and23-cli -- examples/polimorfismo.jb         # herencia e interfaces
cargo run -p and23-cli -- examples/excepciones.jb          # excepciones y opcionales
cargo run -p and23-cli -- examples/genericos.jb            # genéricos, Lista y Mapa
cargo run -p and23-cli -- examples/lambdas.jb              # lambdas y closures
cargo run -p and23-cli -- examples/proyecto/main.jb        # proyecto de varios archivos
cargo run -p and23-cli -- examples/palabras.jb             # leer un archivo y contar palabras
cargo run -p and23-cli -- examples/notas.jb                # leer un CSV, convertir y escribir
cargo test --workspace                             # tests
```

## Estructura

| Crate | Contenido |
|---|---|
| `and23-syntax` | lexer, AST, parser Pratt, diagnósticos con línea y columna y el cargador de módulos (`use`/`open`) |
| `and23-types` | verificador de tipos estáticos (con estado para el REPL) |
| `and23-bytecode` | instrucciones, compilador del AST a bytecode y desensamblador |
| `and23-vm` | VM de pila, heap con recolector mark-sweep y sesión (verificador + compilador + VM con estado para el REPL) |
| `and23-eval` | intérprete de árbol de M0/M1a; queda como referencia para comparar comportamiento y velocidad (no soporta cadenas, arreglos, clases, excepciones, genéricos ni lambdas) |
| `and23-cli` | binario `and23`: ejecuta archivos `.jb` y ofrece un REPL |

## Hoja de ruta

M0 expresiones, M1 variables y funciones con bytecode y VM, M2 tipos estáticos, M3 strings y GC, M4 clases, M5 herencia e interfaces (hecho), M6 excepciones y null-safety (hecho), M7 genéricos (hecho), M8 closures y lambdas (hecho), M9 módulos (hecho), M10 stdlib inicial (hecho), M11 concurrencia (hecho), y más adelante auto-compilación. El detalle y los criterios de éxito están en [`DESIGN.md`](DESIGN.md).

## Licencia

[MIT](LICENSE)
