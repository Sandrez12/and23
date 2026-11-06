use crate::error::Diagnostic;
use crate::span::Span;

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    Int(i64),
    Float(f64),
    Ident(String),
    // Palabras clave.
    Fix,
    Mut,
    Fn,
    If,
    Else,
    While,
    Return,
    Break,
    Continue,
    True,
    False,
    Show,
    // Operadores.
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    EqEq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
    AndAnd,
    OrOr,
    Bang,
    Assign,
    PlusEq,
    MinusEq,
    StarEq,
    SlashEq,
    PercentEq,
    Arrow,
    // Puntuación.
    LParen,
    RParen,
    LBrace,
    RBrace,
    Comma,
    Colon,
    /// Salto de línea o `;`.
    Newline,
    Eof,
}

impl TokenKind {
    /// Descripción para mensajes de error.
    pub fn describe(&self) -> String {
        use TokenKind as T;
        match self {
            T::Int(v) => format!("el entero `{v}`"),
            T::Float(v) => format!("el decimal `{v}`"),
            T::Ident(name) => format!("el identificador `{name}`"),
            T::Newline => "el fin de la línea".into(),
            T::Eof => "el fin del programa".into(),
            other => format!("`{}`", other.text()),
        }
    }

    /// Texto fuente de palabras clave, operadores y puntuación.
    fn text(&self) -> &'static str {
        use TokenKind as T;
        match self {
            T::Fix => "fix",
            T::Mut => "mut",
            T::Fn => "fn",
            T::If => "if",
            T::Else => "else",
            T::While => "while",
            T::Return => "return",
            T::Break => "break",
            T::Continue => "continue",
            T::True => "true",
            T::False => "false",
            T::Show => "show",
            T::Plus => "+",
            T::Minus => "-",
            T::Star => "*",
            T::Slash => "/",
            T::Percent => "%",
            T::EqEq => "==",
            T::NotEq => "!=",
            T::Lt => "<",
            T::LtEq => "<=",
            T::Gt => ">",
            T::GtEq => ">=",
            T::AndAnd => "&&",
            T::OrOr => "||",
            T::Bang => "!",
            T::Assign => "=",
            T::PlusEq => "+=",
            T::MinusEq => "-=",
            T::StarEq => "*=",
            T::SlashEq => "/=",
            T::PercentEq => "%=",
            T::Arrow => "->",
            T::LParen => "(",
            T::RParen => ")",
            T::LBrace => "{",
            T::RBrace => "}",
            T::Comma => ",",
            T::Colon => ":",
            T::Int(_) | T::Float(_) | T::Ident(_) | T::Newline | T::Eof => "",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

fn keyword(name: &str) -> Option<TokenKind> {
    use TokenKind as T;
    Some(match name {
        "fix" => T::Fix,
        "mut" => T::Mut,
        "fn" => T::Fn,
        "if" => T::If,
        "else" => T::Else,
        "while" => T::While,
        "return" => T::Return,
        "break" => T::Break,
        "continue" => T::Continue,
        "true" => T::True,
        "false" => T::False,
        "show" => T::Show,
        _ => return None,
    })
}

/// Operador o signo de puntuación que empieza en `c`, y cuántos caracteres ocupa.
fn operator(c: char, next: Option<char>) -> Option<(TokenKind, usize)> {
    use TokenKind as T;
    let two = match (c, next) {
        ('=', Some('=')) => Some(T::EqEq),
        ('!', Some('=')) => Some(T::NotEq),
        ('<', Some('=')) => Some(T::LtEq),
        ('>', Some('=')) => Some(T::GtEq),
        ('&', Some('&')) => Some(T::AndAnd),
        ('|', Some('|')) => Some(T::OrOr),
        ('-', Some('>')) => Some(T::Arrow),
        ('+', Some('=')) => Some(T::PlusEq),
        ('-', Some('=')) => Some(T::MinusEq),
        ('*', Some('=')) => Some(T::StarEq),
        ('/', Some('=')) => Some(T::SlashEq),
        ('%', Some('=')) => Some(T::PercentEq),
        _ => None,
    };
    if let Some(kind) = two {
        return Some((kind, 2));
    }
    let one = match c {
        '+' => T::Plus,
        '-' => T::Minus,
        '*' => T::Star,
        '/' => T::Slash,
        '%' => T::Percent,
        '=' => T::Assign,
        '!' => T::Bang,
        '<' => T::Lt,
        '>' => T::Gt,
        '(' => T::LParen,
        ')' => T::RParen,
        '{' => T::LBrace,
        '}' => T::RBrace,
        ',' => T::Comma,
        ':' => T::Colon,
        _ => return None,
    };
    Some((one, 1))
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
        let next = chars.get(i + 1).map(|&(_, n)| n);
        match c {
            ' ' | '\t' | '\r' => i += 1,
            // BOM UTF-8 al inicio del archivo (lo agregan algunos editores de Windows).
            '\u{feff}' if i == 0 => i += 1,
            '\n' | ';' => {
                tokens.push(Token {
                    kind: TokenKind::Newline,
                    span: Span::new(start, start + c.len_utf8()),
                });
                i += 1;
            }
            '/' if next == Some('/') => {
                while i < chars.len() && chars[i].1 != '\n' {
                    i += 1;
                }
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
                let kind = keyword(&name).unwrap_or(TokenKind::Ident(name));
                tokens.push(Token {
                    kind,
                    span: Span::new(start, byte_at(j)),
                });
                i = j;
            }
            other => match operator(other, next) {
                Some((kind, len)) => {
                    tokens.push(Token {
                        kind,
                        span: Span::new(start, byte_at(i + len)),
                    });
                    i += len;
                }
                None => {
                    let hint = match other {
                        '&' => " (¿quisiste escribir `&&`?)",
                        '|' => " (¿quisiste escribir `||`?)",
                        _ => "",
                    };
                    return Err(Diagnostic::new(
                        Span::new(start, start + other.len_utf8()),
                        format!("carácter inesperado `{other}`{hint}"),
                    ));
                }
            },
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
    fn operadores_de_dos_caracteres() {
        use TokenKind as T;
        assert_eq!(
            kinds("== != <= >= && || -> += -= *= /= %= = < > !"),
            vec![
                T::EqEq,
                T::NotEq,
                T::LtEq,
                T::GtEq,
                T::AndAnd,
                T::OrOr,
                T::Arrow,
                T::PlusEq,
                T::MinusEq,
                T::StarEq,
                T::SlashEq,
                T::PercentEq,
                T::Assign,
                T::Lt,
                T::Gt,
                T::Bang,
                T::Eof,
            ]
        );
    }

    #[test]
    fn palabras_clave_frente_a_identificadores() {
        use TokenKind as T;
        assert_eq!(
            kinds("fix mut fn if else while return break continue true false show fixx _fn"),
            vec![
                T::Fix,
                T::Mut,
                T::Fn,
                T::If,
                T::Else,
                T::While,
                T::Return,
                T::Break,
                T::Continue,
                T::True,
                T::False,
                T::Show,
                T::Ident("fixx".into()),
                T::Ident("_fn".into()),
                T::Eof,
            ]
        );
    }

    #[test]
    fn llaves_comas_y_dos_puntos() {
        use TokenKind as T;
        assert_eq!(
            kinds("{ a: Int, b }"),
            vec![
                T::LBrace,
                T::Ident("a".into()),
                T::Colon,
                T::Ident("Int".into()),
                T::Comma,
                T::Ident("b".into()),
                T::RBrace,
                T::Eof,
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
        let err = lex("1.").unwrap_err();
        assert!(err.message.contains("inesperado"));
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

    #[test]
    fn ampersand_suelto_sugiere_el_doble() {
        let err = lex("a & b").unwrap_err();
        assert!(err.message.contains("`&&`"), "{}", err.message);
    }
}
