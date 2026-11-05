use crate::error::Diagnostic;
use crate::span::Span;

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    Int(i64),
    Float(f64),
    Ident(String),
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    LParen,
    RParen,
    /// Salto de línea o `;`.
    Newline,
    Eof,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

/// Convierte el código fuente en tokens. Los identificadores admiten Unicode.
pub fn lex(src: &str) -> Result<Vec<Token>, Diagnostic> {
    let chars: Vec<(usize, char)> = src.char_indices().collect();
    let byte_at = |idx: usize| chars.get(idx).map_or(src.len(), |&(b, _)| b);
    let is_digit_at = |idx: usize| chars.get(idx).is_some_and(|&(_, c)| c.is_ascii_digit());

    let mut tokens = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let (start, c) = chars[i];
        let single = |kind: TokenKind| Token {
            kind,
            span: Span::new(start, start + c.len_utf8()),
        };
        match c {
            ' ' | '\t' | '\r' => i += 1,
            // BOM UTF-8 al inicio del archivo (lo agregan algunos editores de Windows).
            '\u{feff}' if i == 0 => i += 1,
            '\n' | ';' => {
                tokens.push(single(TokenKind::Newline));
                i += 1;
            }
            '/' if chars.get(i + 1).is_some_and(|&(_, n)| n == '/') => {
                while i < chars.len() && chars[i].1 != '\n' {
                    i += 1;
                }
            }
            '+' => {
                tokens.push(single(TokenKind::Plus));
                i += 1;
            }
            '-' => {
                tokens.push(single(TokenKind::Minus));
                i += 1;
            }
            '*' => {
                tokens.push(single(TokenKind::Star));
                i += 1;
            }
            '/' => {
                tokens.push(single(TokenKind::Slash));
                i += 1;
            }
            '%' => {
                tokens.push(single(TokenKind::Percent));
                i += 1;
            }
            '(' => {
                tokens.push(single(TokenKind::LParen));
                i += 1;
            }
            ')' => {
                tokens.push(single(TokenKind::RParen));
                i += 1;
            }
            c if c.is_ascii_digit() => {
                let mut j = i;
                while j < chars.len() && (is_digit_at(j) || chars[j].1 == '_') {
                    j += 1;
                }
                let mut is_float = false;
                if chars.get(j).is_some_and(|&(_, c)| c == '.') && is_digit_at(j + 1) {
                    is_float = true;
                    j += 1;
                    while j < chars.len() && (is_digit_at(j) || chars[j].1 == '_') {
                        j += 1;
                    }
                }
                let span = Span::new(start, byte_at(j));
                let text: String = chars[i..j]
                    .iter()
                    .map(|&(_, c)| c)
                    .filter(|&c| c != '_')
                    .collect();
                let kind = if is_float {
                    TokenKind::Float(text.parse().expect("literal flotante válido"))
                } else {
                    let value = text
                        .parse::<i64>()
                        .map_err(|_| Diagnostic::new(span, "entero fuera de rango (i64)"))?;
                    TokenKind::Int(value)
                };
                tokens.push(Token { kind, span });
                i = j;
            }
            c if c.is_alphabetic() || c == '_' => {
                let mut j = i;
                while j < chars.len() && (chars[j].1.is_alphanumeric() || chars[j].1 == '_') {
                    j += 1;
                }
                let name: String = chars[i..j].iter().map(|&(_, c)| c).collect();
                tokens.push(Token {
                    kind: TokenKind::Ident(name),
                    span: Span::new(start, byte_at(j)),
                });
                i = j;
            }
            other => {
                return Err(Diagnostic::new(
                    Span::new(start, start + other.len_utf8()),
                    format!("carácter inesperado `{other}`"),
                ));
            }
        }
    }
    tokens.push(Token {
        kind: TokenKind::Eof,
        span: Span::new(src.len(), src.len()),
    });
    Ok(tokens)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(src: &str) -> Vec<TokenKind> {
        lex(src).unwrap().into_iter().map(|t| t.kind).collect()
    }

    #[test]
    fn numeros_y_operadores() {
        assert_eq!(
            kinds("1 + 2.5 * (3)"),
            vec![
                TokenKind::Int(1),
                TokenKind::Plus,
                TokenKind::Float(2.5),
                TokenKind::Star,
                TokenKind::LParen,
                TokenKind::Int(3),
                TokenKind::RParen,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn separadores_de_miles() {
        assert_eq!(kinds("1_000_000")[0], TokenKind::Int(1_000_000));
    }

    #[test]
    fn identificadores_unicode() {
        assert_eq!(
            kinds("dueño ñandú _x1"),
            vec![
                TokenKind::Ident("dueño".into()),
                TokenKind::Ident("ñandú".into()),
                TokenKind::Ident("_x1".into()),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn comentarios_y_saltos_de_linea() {
        assert_eq!(
            kinds("1 // comentario con ñ\n2;3"),
            vec![
                TokenKind::Int(1),
                TokenKind::Newline,
                TokenKind::Int(2),
                TokenKind::Newline,
                TokenKind::Int(3),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn ignora_el_bom_inicial_pero_no_uno_en_medio() {
        assert_eq!(kinds("\u{feff}1"), vec![TokenKind::Int(1), TokenKind::Eof]);
        assert!(lex("1 \u{feff}2").is_err());
    }

    #[test]
    fn punto_sin_digito_no_es_flotante() {
        let toks = lex("1.").unwrap_err();
        assert!(toks.message.contains("inesperado"));
    }

    #[test]
    fn entero_fuera_de_rango() {
        let err = lex("99999999999999999999").unwrap_err();
        assert!(err.message.contains("fuera de rango"));
    }

    #[test]
    fn caracter_inesperado_con_posicion() {
        let err = lex("1 + @").unwrap_err();
        assert_eq!(err.span, Span::new(4, 5));
    }
}
