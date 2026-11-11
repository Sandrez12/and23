//! Funciones nativas de la biblioteca estándar (M10). Los índices de texto son de caracteres,
//! igual que `len`. Los ids siguen el orden de `and23_types::NATIVES`.

use crate::heap::{Heap, Object};
use crate::value::Value;
use std::io::{BufRead, Write};
use std::time::{SystemTime, UNIX_EPOCH};

/// Estado que las nativas conservan entre llamadas.
pub struct NativeState {
    /// Mensaje del último error de archivos (lo lee `_errorES`).
    pub io_error: String,
    /// Argumentos del programa (los que siguen al archivo en la línea de comandos).
    pub args: Vec<String>,
    /// Estado del generador de números aleatorios (xorshift64*; nunca 0).
    pub rng: u64,
}

impl NativeState {
    pub fn new() -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0x9e37_79b9_7f4a_7c15, |d| d.as_nanos() as u64);
        NativeState {
            io_error: String::new(),
            args: Vec::new(),
            rng: seed(nanos),
        }
    }

    fn next_random(&mut self) -> u64 {
        let mut x = self.rng;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.rng = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
}

impl Default for NativeState {
    fn default() -> Self {
        Self::new()
    }
}

/// Mezcla `n` para que semillas parecidas den secuencias distintas; el resultado nunca es 0.
fn seed(n: u64) -> u64 {
    let mut z = n.wrapping_add(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^= z >> 31;
    if z == 0 {
        0x9e37_79b9_7f4a_7c15
    } else {
        z
    }
}

fn text(heap: &Heap, v: Value) -> &str {
    match v {
        Value::Obj(r) => heap.str(r),
        _ => unreachable!("el verificador garantiza una cadena"),
    }
}

fn int(v: Value) -> i64 {
    match v {
        Value::Int(n) => n,
        _ => unreachable!("el verificador garantiza un Int"),
    }
}

fn float(v: Value) -> f64 {
    match v {
        Value::Float(x) => x,
        _ => unreachable!("el verificador garantiza un Float"),
    }
}

/// Posición en bytes del carácter número `index` (recortada a `[0, largo]`).
fn byte_of(s: &str, index: i64) -> usize {
    if index <= 0 {
        return 0;
    }
    let index = index as usize;
    if s.is_ascii() {
        return index.min(s.len());
    }
    s.char_indices().nth(index).map_or(s.len(), |(b, _)| b)
}

/// Número de carácter de la posición en bytes `byte`.
fn char_of(s: &str, byte: usize) -> i64 {
    if s.is_ascii() {
        byte as i64
    } else {
        s[..byte].chars().count() as i64
    }
}

fn to_int(x: f64) -> Result<Value, String> {
    // 2^63 como f64 es exacto: el rango válido es [-2^63, 2^63).
    if x.is_finite() && (-9_223_372_036_854_775_808.0..9_223_372_036_854_775_808.0).contains(&x) {
        Ok(Value::Int(x as i64))
    } else {
        Err(format!("no se puede convertir {x} a Int"))
    }
}

/// Ejecuta la nativa `id` con `args`. `state` guarda el último error de archivos (`_errorES`), los
/// argumentos del programa y el generador de azar.
pub fn call(
    id: u32,
    heap: &mut Heap,
    args: &[Value],
    state: &mut NativeState,
) -> Result<Value, String> {
    let a = |i: usize| args[i];
    let result = match id {
        // _trozo(s, desde, hasta)
        0 => {
            let s = text(heap, a(0));
            let from = byte_of(s, int(a(1)));
            let to = byte_of(s, int(a(2))).max(from);
            s[from..to].to_string()
        }
        // _busca(s, t, desde)
        1 => {
            let s = text(heap, a(0));
            let start = byte_of(s, int(a(2)));
            return Ok(Value::Int(
                s[start..]
                    .find(text(heap, a(1)))
                    .map_or(-1, |i| char_of(s, start + i)),
            ));
        }
        2 => text(heap, a(0)).to_uppercase(),
        3 => text(heap, a(0)).to_lowercase(),
        4 => text(heap, a(0)).trim().to_string(),
        // _reemplaza(s, de, por)
        5 => {
            let (s, from, to) = (text(heap, a(0)), text(heap, a(1)), text(heap, a(2)));
            if from.is_empty() {
                s.to_string()
            } else {
                s.replace(from, to)
            }
        }
        6 => return Ok(Value::Bool(text(heap, a(0)).starts_with(text(heap, a(1))))),
        7 => return Ok(Value::Bool(text(heap, a(0)).ends_with(text(heap, a(1))))),
        8 => {
            return Ok(match text(heap, a(0)).parse::<i64>() {
                Ok(n) => Value::Int(n),
                Err(_) => Value::Nil,
            })
        }
        9 => {
            return Ok(match text(heap, a(0)).parse::<f64>() {
                Ok(x) if x.is_finite() => Value::Float(x),
                _ => Value::Nil,
            })
        }
        10 => return Ok(Value::Float(float(a(0)).sqrt())),
        11 => return Ok(Value::Float(float(a(0)).powf(float(a(1))))),
        12 => return to_int(float(a(0)).floor()),
        13 => return to_int(float(a(0)).ceil()),
        14 => return to_int(float(a(0)).round()),
        15 => return Ok(Value::Float(int(a(0)) as f64)),
        // _leeArchivo(ruta)
        16 => {
            let path = text(heap, a(0)).to_string();
            match std::fs::read_to_string(&path) {
                Ok(content) => content
                    .strip_prefix('\u{feff}')
                    .unwrap_or(&content)
                    .to_string(),
                Err(e) => {
                    state.io_error = format!("no se pudo leer `{path}`: {e}");
                    return Ok(Value::Nil);
                }
            }
        }
        // _escribeArchivo(ruta, texto) y _agregaArchivo(ruta, texto)
        17 | 18 => {
            let path = text(heap, a(0)).to_string();
            let content = text(heap, a(1));
            let written = if id == 17 {
                std::fs::write(&path, content)
            } else {
                std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&path)
                    .and_then(|mut f| f.write_all(content.as_bytes()))
            };
            return Ok(match written {
                Ok(()) => Value::Bool(true),
                Err(e) => {
                    state.io_error = format!("no se pudo escribir `{path}`: {e}");
                    Value::Bool(false)
                }
            });
        }
        19 => return Ok(Value::Bool(std::path::Path::new(text(heap, a(0))).exists())),
        // leeLinea()
        20 => {
            let mut line = String::new();
            match std::io::stdin().lock().read_line(&mut line) {
                Ok(0) | Err(_) => return Ok(Value::Nil),
                Ok(_) => {
                    while line.ends_with('\n') || line.ends_with('\r') {
                        line.pop();
                    }
                    line
                }
            }
        }
        21 => state.io_error.clone(),
        // _borraArchivo(ruta)
        22 => {
            let path = text(heap, a(0)).to_string();
            return Ok(match std::fs::remove_file(&path) {
                Ok(()) => Value::Bool(true),
                Err(e) => {
                    state.io_error = format!("no se pudo borrar `{path}`: {e}");
                    Value::Bool(false)
                }
            });
        }
        // _args()
        23 => {
            let items: Vec<Value> = state
                .args
                .clone()
                .into_iter()
                .map(|a| heap.alloc_str(a))
                .collect();
            return Ok(heap.alloc_array(items));
        }
        // entorno(nombre)
        24 => {
            return Ok(match std::env::var(text(heap, a(0))) {
                Ok(v) => heap.alloc_str(v),
                Err(_) => Value::Nil,
            })
        }
        // ahora(): milisegundos desde 1970-01-01
        25 => {
            let ms = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |d| d.as_millis() as i64);
            return Ok(Value::Int(ms));
        }
        // _pausa(ms)
        26 => {
            let ms = int(a(0)).max(0) as u64;
            std::thread::sleep(std::time::Duration::from_millis(ms));
            return Ok(Value::Unit);
        }
        // azar(n): entero en [0, n)
        27 => {
            let n = int(a(0));
            if n <= 0 {
                return Err(format!("`azar` necesita un límite positivo, no {n}"));
            }
            let r = state.next_random();
            return Ok(Value::Int(((u128::from(r) * n as u128) >> 64) as i64));
        }
        // azarF(): decimal en [0, 1)
        28 => {
            let r = state.next_random() >> 11;
            return Ok(Value::Float(r as f64 / (1u64 << 53) as f64));
        }
        // semilla(n)
        29 => {
            state.rng = seed(int(a(0)) as u64);
            return Ok(Value::Unit);
        }
        // _listaDir(ruta): nombres, en orden alfabético
        30 => {
            let path = text(heap, a(0)).to_string();
            return Ok(match std::fs::read_dir(&path) {
                Ok(entries) => {
                    let mut names: Vec<String> = entries
                        .filter_map(Result::ok)
                        .map(|e| e.file_name().to_string_lossy().into_owned())
                        .collect();
                    names.sort();
                    let items: Vec<Value> = names.into_iter().map(|n| heap.alloc_str(n)).collect();
                    heap.alloc_array(items)
                }
                Err(e) => {
                    state.io_error = format!("no se pudo listar `{path}`: {e}");
                    Value::Nil
                }
            });
        }
        // _creaDir(ruta): crea también los padres que falten
        31 => {
            let path = text(heap, a(0)).to_string();
            return Ok(match std::fs::create_dir_all(&path) {
                Ok(()) => Value::Bool(true),
                Err(e) => {
                    state.io_error = format!("no se pudo crear `{path}`: {e}");
                    Value::Bool(false)
                }
            });
        }
        32 => return Ok(Value::Bool(std::path::Path::new(text(heap, a(0))).is_dir())),
        // _esLetra(s), _esAlfanum(s): no vacía y todos sus caracteres son letras (o letras y dígitos Unicode)
        33 => {
            let s = text(heap, a(0));
            return Ok(Value::Bool(
                !s.is_empty() && s.chars().all(char::is_alphabetic),
            ));
        }
        34 => {
            let s = text(heap, a(0));
            return Ok(Value::Bool(
                !s.is_empty() && s.chars().all(char::is_alphanumeric),
            ));
        }
        // _esDigito(s): no vacía y todos sus caracteres son dígitos ASCII
        35 => {
            let s = text(heap, a(0));
            return Ok(Value::Bool(
                !s.is_empty() && s.chars().all(|c| c.is_ascii_digit()),
            ));
        }
        // _caracteres(s): un arreglo con cada carácter como cadena
        36 => {
            let chars: Vec<String> = text(heap, a(0)).chars().map(String::from).collect();
            let items: Vec<Value> = chars.into_iter().map(|c| heap.alloc_str(c)).collect();
            return Ok(heap.alloc_array(items));
        }
        // _desdeCodigo(n): el carácter con ese punto de código, o nil si no es válido
        37 => {
            return Ok(
                match u32::try_from(int(a(0))).ok().and_then(char::from_u32) {
                    Some(c) => heap.alloc_str(c.to_string()),
                    None => Value::Nil,
                },
            );
        }
        // _une(partes, sep): une las cadenas de un arreglo en O(n)
        38 => {
            let Value::Obj(r) = a(0) else {
                unreachable!("el verificador garantiza un arreglo")
            };
            let sep = text(heap, a(1)).to_string();
            let Object::Array(items) = heap.get(r) else {
                unreachable!("el verificador garantiza un arreglo")
            };
            let joined = items
                .iter()
                .map(|v| text(heap, *v))
                .collect::<Vec<&str>>()
                .join(&sep);
            joined
        }
        // _parte(s, sep): los trozos de `s` separados por `sep` (uno por carácter si `sep` es vacío), en una sola pasada.
        // Hacerlo con `_busca` y `_trozo` costaba O(n) por trozo (convertir posiciones carácter-byte), es decir O(n·k).
        39 => {
            let s = text(heap, a(0)).to_string();
            let sep = text(heap, a(1)).to_string();
            let parts: Vec<String> = if sep.is_empty() {
                s.chars().map(String::from).collect()
            } else {
                s.split(sep.as_str()).map(String::from).collect()
            };
            let items: Vec<Value> = parts.into_iter().map(|p| heap.alloc_str(p)).collect();
            return Ok(heap.alloc_array(items));
        }
        // _formato(x, decimales): `x` con exactamente ese número de decimales (0 a 100). Redondea el valor binario exacto
        // como `printf` (2.675 da "2.67" porque el Float guarda 2.67499...; los empates exactos, como 2.5 con 0 decimales,
        // van al par: "2"). Un Float que no es finito (nan, inf) se escribe como lo escribe `show`.
        40 => {
            let (x, decimals) = (float(a(0)), int(a(1)));
            if !(0..=100).contains(&decimals) {
                return Err(format!(
                    "formato: los decimales deben estar entre 0 y 100, pero son {decimals}"
                ));
            }
            if x.is_finite() {
                format!("{x:.*}", decimals as usize)
            } else {
                format!("{x:?}")
            }
        }
        _ => return Err(format!("función nativa desconocida: {id}")),
    };
    Ok(heap.alloc_str(result))
}
