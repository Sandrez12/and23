//! Formateador (`and23 fmt`). Es deliberadamente conservador: no cambia los saltos de línea ni el
//! espaciado dentro de una línea, así que nunca altera lo que el programa significa ni mueve un
//! comentario. Solo normaliza lo que es puramente de aspecto:
//!
//! - la sangría de cada línea (4 espacios por nivel de `{}`, `()` o `[]` abierto, con las líneas que
//!   empiezan cerrando un nivel un paso atrás y 4 espacios más para las líneas de continuación);
//! - los espacios y tabuladores al final de las líneas;
//! - las líneas en blanco repetidas (como mucho una) y las del principio y el final del archivo;
//! - el salto de línea final (exactamente uno).
//!
//! El texto de las cadenas que ocupan varias líneas no se toca. El estilo de fin de línea (`\n` o
//! `\r\n`) se conserva.

const INDENT: usize = 4;

/// Contexto léxico en el que está el recorrido.
#[derive(Clone, Copy, PartialEq)]
enum Mode {
    /// Dentro de una cadena.
    Str,
    /// Dentro de un `${ ... }` de una cadena; guarda cuántas llaves nuevas hay abiertas dentro.
    Interp(u32),
}

/// Formatea el programa `src`.
pub fn format(src: &str) -> String {
    let crlf = src.contains("\r\n");
    let mut stack: Vec<Mode> = Vec::new();
    // Línea en que se abrió cada paréntesis, llave o corchete que sigue abierto.
    let mut open: Vec<usize> = Vec::new();
    // Cómo terminó la última línea con código (sin el comentario), para detectar continuaciones.
    let mut prev_end = String::new();
    let mut out: Vec<String> = Vec::new();
    let mut blank_pending = false;

    for (line_no, raw) in src.lines().enumerate() {
        let line = raw.trim_end_matches('\r');
        let starts_in_string = stack.last() == Some(&Mode::Str);
        if starts_in_string {
            // Contenido de una cadena de varias líneas: se deja tal cual.
            flush_blank(&mut out, &mut blank_pending);
            out.push(line.to_string());
            scan(line, line_no, &mut stack, &mut open);
            // Una cadena que sigue abierta o que acaba de cerrarse no es una continuación.
            prev_end.clear();
            continue;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            blank_pending = true;
            continue;
        }

        let leading_closers = trimmed
            .chars()
            .take_while(|c| matches!(c, '}' | ')' | ']'))
            .count();
        let continuation = is_continuation_start(trimmed) || ends_with_operator(&prev_end);
        let mut indent = levels(&open[..open.len().saturating_sub(leading_closers)]) * INDENT;
        if continuation && leading_closers == 0 {
            indent += INDENT;
        }
        flush_blank(&mut out, &mut blank_pending);
        out.push(format!("{}{}", " ".repeat(indent), trimmed));

        let code_len = scan(trimmed, line_no, &mut stack, &mut open);
        let code = trimmed[..code_len].trim_end();
        if !code.is_empty() {
            prev_end = code.to_string();
        }
    }

    // Sin líneas en blanco al principio (flush_blank no las añade antes de la primera línea) ni al final.
    let eol = if crlf { "\r\n" } else { "\n" };
    if out.is_empty() {
        return String::new();
    }
    let mut text = out.join(eol);
    text.push_str(eol);
    text
}

/// Niveles de sangría que dan los delimitadores abiertos: uno por cada línea distinta en que se abrió
/// alguno (así `f(g {` en una sola línea sangra el cuerpo un nivel, no dos).
fn levels(open: &[usize]) -> usize {
    let mut count = 0;
    let mut last = None;
    for &line in open {
        if last != Some(line) {
            count += 1;
            last = Some(line);
        }
    }
    count
}

/// Añade una línea en blanco si hubo una o más desde la última línea escrita (y ya hay algo escrito).
fn flush_blank(out: &mut Vec<String>, pending: &mut bool) {
    if *pending && !out.is_empty() {
        out.push(String::new());
    }
    *pending = false;
}

/// ¿La línea empieza continuando la expresión de la anterior (`.metodo`, `&&`, `||`, `?:`)?
fn is_continuation_start(trimmed: &str) -> bool {
    (trimmed.starts_with('.') && !trimmed.starts_with(".."))
        || trimmed.starts_with("?.")
        || trimmed.starts_with("&&")
        || trimmed.starts_with("||")
        || trimmed.starts_with("?:")
}

/// ¿La línea de código anterior termina en un operador, de modo que la siguiente la continúa?
fn ends_with_operator(prev: &str) -> bool {
    if prev.ends_with("->") {
        return false;
    }
    ["&&", "||", "?:", "?.", "+", "-", "*", "/", "%", "=", "."]
        .iter()
        .any(|op| prev.ends_with(op))
}

/// Recorre una línea actualizando el contexto y la profundidad de paréntesis. Devuelve dónde empieza
/// el comentario `//` de la línea (o su longitud si no tiene).
fn scan(line: &str, line_no: usize, stack: &mut Vec<Mode>, open: &mut Vec<usize>) -> usize {
    let chars: Vec<(usize, char)> = line.char_indices().collect();
    let mut i = 0;
    while i < chars.len() {
        let (pos, c) = chars[i];
        match stack.last().copied() {
            Some(Mode::Str) => match c {
                '\\' => i += 1,
                '"' => {
                    stack.pop();
                }
                '$' if chars.get(i + 1).is_some_and(|&(_, n)| n == '{') => {
                    stack.push(Mode::Interp(0));
                    i += 1;
                }
                _ => {}
            },
            top => match c {
                '/' if chars.get(i + 1).is_some_and(|&(_, n)| n == '/') => return pos,
                '"' => stack.push(Mode::Str),
                '{' | '(' | '[' => match top {
                    Some(Mode::Interp(n)) => {
                        let last = stack.len() - 1;
                        if c == '{' {
                            stack[last] = Mode::Interp(n + 1);
                        }
                    }
                    _ => open.push(line_no),
                },
                '}' | ')' | ']' => match top {
                    Some(Mode::Interp(n)) => {
                        if c == '}' {
                            if n == 0 {
                                stack.pop();
                            } else {
                                let last = stack.len() - 1;
                                stack[last] = Mode::Interp(n - 1);
                            }
                        }
                    }
                    _ => {
                        open.pop();
                    }
                },
                _ => {}
            },
        }
        i += 1;
    }
    line.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_sangria_sigue_las_llaves() {
        let src = "fn f() {\nif x {\nshow(1)\n} else {\nshow(2)\n}\n}\n";
        assert_eq!(
            format(src),
            "fn f() {\n    if x {\n        show(1)\n    } else {\n        show(2)\n    }\n}\n"
        );
    }

    #[test]
    fn quita_espacios_finales_y_repite_una_sola_linea_en_blanco() {
        assert_eq!(
            format("\n\nshow(1)   \n\n\n\nshow(2)\t\n\n\n"),
            "show(1)\n\nshow(2)\n"
        );
    }

    #[test]
    fn los_tabuladores_de_sangria_se_cambian_por_espacios() {
        assert_eq!(
            format("fn f() {\n\t\tshow(1)\n}\n"),
            "fn f() {\n    show(1)\n}\n"
        );
    }

    #[test]
    fn las_continuaciones_llevan_cuatro_espacios_mas() {
        let src = "fix r = xs\n.map { x -> x }\n.filter { x -> x > 1 }\nfix t = a &&\nb\nshow(r)\n";
        assert_eq!(
            format(src),
            "fix r = xs\n    .map { x -> x }\n    .filter { x -> x > 1 }\nfix t = a &&\n    b\nshow(r)\n"
        );
    }

    #[test]
    fn los_argumentos_en_varias_lineas_se_sangran_por_los_parentesis() {
        let src = "show(\n1,\n2\n)\nfix a = [\n1,\n2,\n]\n";
        assert_eq!(
            format(src),
            "show(\n    1,\n    2\n)\nfix a = [\n    1,\n    2,\n]\n"
        );
    }

    #[test]
    fn los_comentarios_siguen_la_sangria_y_no_cuentan_llaves() {
        let src = "fn f() {\n// abre { sin cerrar\nshow(1) // otra {\n}\n";
        assert_eq!(
            format(src),
            "fn f() {\n    // abre { sin cerrar\n    show(1) // otra {\n}\n"
        );
    }

    #[test]
    fn las_llaves_dentro_de_cadenas_no_cuentan() {
        let src = "fn f() {\nshow(\"{ ( [\")\nshow(\"a ${b + {1}.x} }\")\n}\n";
        assert_eq!(
            format(src),
            "fn f() {\n    show(\"{ ( [\")\n    show(\"a ${b + {1}.x} }\")\n}\n"
        );
    }

    #[test]
    fn una_cadena_de_varias_lineas_no_se_toca() {
        let src = "fn f() {\nshow(\"uno\n   dos  \ntres\")\nshow(2)\n}\n";
        assert_eq!(
            format(src),
            "fn f() {\n    show(\"uno\n   dos  \ntres\")\n    show(2)\n}\n"
        );
    }

    #[test]
    fn un_cuerpo_de_lambda_tras_flecha_no_es_continuacion() {
        let src = "xs.paraCada { x ->\nshow(x)\n}\n";
        assert_eq!(format(src), "xs.paraCada { x ->\n    show(x)\n}\n");
    }

    #[test]
    fn conserva_el_estilo_de_fin_de_linea() {
        assert_eq!(
            format("fn f() {\r\nshow(1)\r\n}\r\n"),
            "fn f() {\r\n    show(1)\r\n}\r\n"
        );
        assert_eq!(format(""), "");
        assert_eq!(format("\n\n"), "");
    }

    #[test]
    fn es_idempotente() {
        let src = "fn f() {\nfix r = xs\n.map { x -> x }\nif a &&\nb {\nshow(\"x ${1}\")\n}\n}\n";
        let once = format(src);
        assert_eq!(format(&once), once);
    }
}
