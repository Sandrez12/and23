//! Funciones nativas de la biblioteca estándar (M10). Los índices de texto son de caracteres,
//! igual que `len`. Los ids siguen el orden de `and23_types::NATIVES`.

use crate::heap::Heap;
use crate::value::Value;
use std::io::{BufRead, Write};

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

/// Ejecuta la nativa `id` con `args`. `io_error` guarda el último error de archivos (`_errorES`).
pub fn call(
    id: u32,
    heap: &mut Heap,
    args: &[Value],
    io_error: &mut String,
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
                    *io_error = format!("no se pudo leer `{path}`: {e}");
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
                    *io_error = format!("no se pudo escribir `{path}`: {e}");
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
        21 => io_error.clone(),
        // _borraArchivo(ruta)
        22 => {
            let path = text(heap, a(0)).to_string();
            return Ok(match std::fs::remove_file(&path) {
                Ok(()) => Value::Bool(true),
                Err(e) => {
                    *io_error = format!("no se pudo borrar `{path}`: {e}");
                    Value::Bool(false)
                }
            });
        }
        _ => return Err(format!("función nativa desconocida: {id}")),
    };
    Ok(heap.alloc_str(result))
}
