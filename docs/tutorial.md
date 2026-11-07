# Tutorial de And23

And23 es un lenguaje de tipos estáticos con sintaxis parecida a Java y Kotlin, que se compila a bytecode y corre
en una máquina virtual de pila con recolector de basura. Este tutorial recorre el lenguaje de principio a fin; cada
ejemplo es un programa completo que puedes guardar como `algo.jb` y ejecutar con `and23 algo.jb`.

## 1. Instalación y primer programa

Descarga el binario de tu sistema desde la página de *Releases* del repositorio, o compila desde el código fuente
(necesitas [Rust](https://rustup.rs)):

```
cargo build --release
./target/release/and23 --version
```

El primer programa:

```jb
show("Hola, mundo")
```

`and23 programa.jb` ejecuta un archivo; `and23` a secas abre el REPL, donde cada línea se evalúa y se imprime su
valor. Dos opciones útiles van antes del archivo: `--disasm` muestra el bytecode en vez de ejecutar y `--max-depth N`
cambia el límite de llamadas anidadas (100000 por defecto). Lo que sigue al archivo lo recibe el programa
(sección 11).

## 2. Variables y tipos

`fix` declara una variable que no cambia y `mut` una que sí. El tipo se deduce del valor; se puede escribir con `:`.

```jb
fix nombre = "Ana"          // Str
mut edad = 30               // Int (64 bits)
fix altura: Float = 1.68
fix activa = true           // Bool
edad += 1
show("${nombre} tiene ${edad} años y mide ${altura}")
```

Los tipos son `Int`, `Float`, `Bool`, `Str`, `Unit` y los arreglos `[T]`. No hay conversión implícita entre `Int` y
`Float`: usa `aFloat(n)`, `piso(x)`, `techo(x)` o `redondea(x)`. Los decimales aceptan exponente (`1e3`, `2.5E-4`).
Las cadenas llevan interpolación `"${expresión}"` y los escapes `\n \t \" \\ \$ \u{1F600}`.

## 3. Control de flujo y funciones

```jb
fn fib(n: Int) -> Int {
    if n < 2 { return n }
    return fib(n - 1) + fib(n - 2)
}

fn doble(x: Int) = x * 2            // el tipo de retorno se deduce de la expresión

mut i = 0
while i < 10 {
    i += 1
    if i % 2 == 0 { continue }
    if i > 7 { break }
    show("${i}: fib=${fib(i)}, doble=${doble(i)}")
}
```

Hay `if` / `else if` / `else`, `while`, `break`, `continue` y `return`. Si el archivo define `main()`, se llama al
final. `&&` y `||` hacen cortocircuito.

## 4. Arreglos, listas y mapas

Un arreglo es de tamaño variable y se pasa por referencia. Para el día a día están `Lista<T>` y `Mapa<K, V>`.

```jb
fix xs = [3, 1, 2]
xs[0] = 10
push(xs, 4)
show(len(xs))                        // 4

fix nombres = Lista<Str>()
nombres.agrega("Ana")
nombres.agrega("Luis")
show(nombres)                        // ["Ana", "Luis"]

fix edades = Mapa<Str, Int>()
edades.pone("Ana", 30)
show(edades.obtiene("Ana") ?: 0)     // 30
show(edades.obtiene("Zoe") ?: 0)     // 0: obtiene devuelve Int?
```

Un `Mapa` no recuerda el orden de inserción. Las claves de texto se comparan por contenido; las de objetos, por
identidad.

## 5. Clases, herencia e interfaces

El constructor va en la cabecera: los parámetros con `fix` o `mut` se vuelven campos.

```jb
kind Habla {
    fn sonido() -> Str
}

thing Animal(fix nombre: Str) {
    mut edad = 0
    fn cumple() { edad += 1 }
    fn describe() -> Str = "${nombre}, ${edad}"
}

thing Perro(nombre: Str) : Animal(nombre), Habla {
    fn sonido() -> Str = "guau"
}

fix p = Perro("Rex")
p.cumple()
show(p.describe())                   // Rex, 1
fix h: Habla = p
show(h.sonido())                     // guau (se elige por la clase real)
fix a: Animal = p
show(a is Perro)                     // true
```

Las clases heredan de una sola; las interfaces (`kind`) solo llevan firmas. `is` pregunta por el tipo, `as` convierte
y `super` llama al método del padre.

## 6. Valores opcionales y errores

`T?` admite un `T` o `nil`. El compilador rechaza usar un `T?` como `T` hasta que compruebes que no es `nil`.

```jb
thing Persona(fix nombre: Str)
fix quien: Persona? = nil
show(quien?.nombre ?: "nadie")       // nadie
fix otra: Persona? = Persona("Rex")
if otra != nil { show(otra.nombre) } // dentro del if, ya no es opcional
```

Las excepciones son objetos de una clase que hereda de `Fallo`:

```jb
thing SaldoInsuficiente(fix pedido: Int) : Fallo("saldo insuficiente: pides ${pedido}")

fn retira(saldo: Int, monto: Int) -> Int {
    if monto > saldo { fail SaldoInsuficiente(monto) }
    return saldo - monto
}

attempt {
    show(retira(100, 30))
    show(retira(10, 50))
} rescue e: SaldoInsuficiente {
    show(e.msg)                      // saldo insuficiente: pides 50
}
```

Un error de ejecución (división entre cero, índice fuera de rango...) detiene el programa y muestra la línea, la
columna y la cadena de llamadas.

## 7. Genéricos

```jb
thing Caja<T>(fix valor: T)
fn primero<T>(xs: [T]) -> T = xs[0]

fix c = Caja(5)                      // T se deduce: Caja<Int>
show(c.valor)
show(primero(["a", "b"]))
```

## 8. Lambdas y closures

`{ x -> ... }` es una función sin nombre que ve las variables de su alrededor y puede modificarlas.

```jb
fix numeros = Lista<Int>()
numeros.agrega(1)
numeros.agrega(2)
numeros.agrega(3)
show(numeros.map { n -> n * n })               // [1, 4, 9]
show(numeros.reduce(0, { acc, n -> acc + n })) // 6

mut visitados = 0
numeros.paraCada { n -> visitados += n }
show(visitados)                                // 6

fn contador() -> () -> Int {
    mut n = 0
    return {
        n += 1
        n
    }
}
fix c = contador()
show("${c()} ${c()}")                          // 1 2
```

## 9. Varios archivos

`open` hace pública una declaración; `use` la importa. Las rutas son relativas al archivo que escribe el `use` y se
escriben sin `.jb`.

```
// util/mate.jb
open fn cuadrado(x: Int) -> Int = x * x

// main.jb
use "util/mate"
use "formas" como f          // con alias: f.Circulo(2.0)
show(cuadrado(7))            // 49
```

Cada archivo se carga una sola vez y los ciclos entre archivos son un error. Hay un proyecto de ejemplo completo en
`examples/proyecto`.

## 10. Texto, números y archivos

```jb
fix linea = "  Ana, 4.5 , Luis,3 "
fix campos = linea.recorta().parte(",")
show(campos.obtiene(0).mayus())                // ANA
show(Float.parse(campos.obtiene(1).recorta()) ?: 0.0)  // 4.5
show(Int.parse("abc"))                         // nil
show("hola mundo".contiene("mundo"))           // true
show("hola mundo".reemplaza("o", "0"))         // h0la mund0
show("hola"[1])                                // o
show(une("a b c".parte(" "), "-"))             // a-b-c
show(raiz(2.0))
show(maximo(3, 9))

escribeArchivo("saludo.txt", "hola\nmundo\n")
show(leeLineas("saludo.txt"))                  // ["hola", "mundo"]
borraArchivo("saludo.txt")
attempt {
    leeArchivo("no_existe.txt")
} rescue e: ErrorES {
    show("no pude leer: ${e.msg}")
}
```

Las posiciones de texto cuentan caracteres, no bytes. Los métodos de texto devuelven cadenas nuevas (las cadenas no
cambian). `leeLinea()` lee una línea de la entrada estándar y devuelve `nil` al terminar.

## 11. Entorno, tiempo, azar y carpetas

```jb
show(argumentos())                   // lo que sigue al archivo: and23 programa.jb uno dos
show(entorno("NO_EXISTE_JAMAS"))     // nil
fix inicio = ahora()                 // milisegundos desde 1970
duerme(20)
show(ahora() - inicio >= 20)         // true

semilla(42)                          // misma semilla, misma secuencia
show(azar(6) < 6)                    // un entero en [0, 6)
show(azarF() < 1.0)                  // un decimal en [0, 1)

show(unirRuta("datos", "notas.csv")) // datos/notas.csv
show(extension("a/b/c.tar.gz"))      // gz
creaDir("tmp_tutorial/sub")
show(listaDir("tmp_tutorial"))       // ["sub"]
```

## 12. Tareas y canales

Las tareas son cooperativas: viven en la misma máquina virtual y solo se alternan cuando una espera, llama a `cede()` o
termina. Por eso no hay carreras de datos y el resultado es siempre el mismo.

```jb
fix canal = Canal<Int>(2)               // cola con capacidad 2

fix productor = lanza {
    mut i = 1
    while i <= 5 {
        canal.envia(i)                  // espera si el canal está lleno
        i += 1
    }
    canal.cierra()
}

fix suma = lanzaDe<Int> {               // una tarea que devuelve un valor
    mut total = 0
    mut x = canal.recibe()              // espera; nil cuando está cerrado y vacío
    while x != nil {
        total += x
        x = canal.recibe()
    }
    return total
}

show(suma.espera())                     // 15
productor.espera()
```

`t.espera()` bloquea hasta que la tarea termine y relanza su `Fallo` si lo hubo. Si todas las tareas esperan, el
programa falla con «interbloqueo». Cuando termina la tarea principal, las demás se descartan. Una tarea que nunca cede
(un bucle sin fin) bloquea a las demás. Hay un ejemplo completo en `examples/tareas.jb`.

## 13. Pruebas y formato

Un bloque `prueba "nombre" { ... }` define una prueba. Al ejecutar el archivo normalmente no hacen nada; se corren con
`and23 --prueba archivo.jb`, que ejecuta el nivel superior del archivo (pero no `main`), luego cada prueba, e informa de
las que fallan sin detenerse. El código de salida es 1 si alguna falla, lo que sirve para el CI.

```jb
fn doble(x: Int) -> Int = x * 2

prueba "doble de un numero" {
    afirmaIgual(doble(4), 8)
    afirma(doble(0) == 0)
}

prueba "una lista recuerda lo que se le agrega" {
    fix l = Lista<Int>()
    l.agrega(7)
    afirmaIgual(l.tam(), 1)
}
```

`afirma(condicion)` y `afirmaIgual(real, esperado)` lanzan un `FalloPrueba` (que hereda de `Fallo`) cuando no se cumplen,
y el mensaje de `afirmaIgual` dice qué se esperaba y qué fue. Cualquier otro error de ejecución dentro de una prueba también
la hace fallar. Las variables globales se comparten entre pruebas. Las pruebas de los archivos importados con `use` no se
ejecutan: solo las del archivo que le pasas a `--prueba`.

`and23 fmt archivo.jb` da formato a los archivos: la sangría (4 espacios por nivel, y 4 más en las líneas que continúan una
expresión), espacios sobrantes al final de línea y líneas en blanco repetidas. No cambia los saltos de línea ni mueve
comentarios. `and23 fmt --check archivo.jb` no escribe nada y termina con error si el archivo cambiaría.

Para VS Code hay una extensión con resaltado de sintaxis en `editors/vscode`.

## 14. Dónde seguir

- [`README.md`](../README.md): estado del proyecto y la lista de lo que funciona.
- [`DESIGN.md`](../DESIGN.md): decisiones de diseño y límites conocidos de cada parte.
- `examples/`: programas completos (`palabras.jb`, `notas.jb`, `tareas.jb`, `proyecto/`...).
- `and23 --disasm archivo.jb`: el bytecode que genera un programa.
