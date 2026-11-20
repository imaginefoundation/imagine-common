//! Tokenizador da sintaxe DSH (DotShell).
//!
//! Gramática simplificada (por linha):
//!   stmt := '.' ident '(' args? ')' | '.' ident '(' args? ')' chain* | block
//!   chain := '.' ident '(' args? ')'
//!   args := arg (',' arg)*   onde arg := string | int | bool | ident | '-ident=arg' | 'ident=arg'
//!   block := '{' stmt* '}'
//!
//! Comentários começam em `#` e vão até o fim da linha.

use core::fmt;

/// Número máximo de tokens por linha.
pub const MAX_TOKENS: usize = 96;
/// Tamanho máximo do texto de um token (strings, identificadores).
pub const TOKEN_TEXT: usize = 64;

/// Tipo de token da linguagem DSH.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TokenKind {
    /// `.` prefixo de comando.
    Dot,
    /// Identificador (nome de comando, variável, módulo).
    Ident,
    /// Literal de string `"..."`.
    String,
    /// Inteiro.
    Int,
    /// Booleano `true` / `false`.
    Bool,
    /// `(`
    LParen,
    /// `)`
    RParen,
    /// `,` separador de argumentos.
    Comma,
    /// `=` (atribuição / argumento nomeado).
    Eq,
    /// `-` prefixo de flag.
    Minus,
    /// `{` abertura de bloco (função / corpo).
    LBrace,
    /// `}` fechamento de bloco.
    RBrace,
    /// Fim de linha / fim da entrada.
    Newline,
    /// Fim do arquivo/entrada.
    Eof,
    /// Erro léxico.
    Error,
}

/// Um único token com seu texto (quando aplicável) e tipo.
#[derive(Clone, Copy, Debug)]
pub struct Token {
    pub kind: TokenKind,
    /// Texto do token (para Ident, String, Int, Bool). Sem as aspas das strings.
    pub text: [u8; TOKEN_TEXT],
    pub text_len: usize,
    /// Valor inteiro quando kind == Int.
    pub int_value: i64,
    /// Quando kind == Bool, o valor booleano.
    pub bool_value: bool,
}

impl Token {
    pub const fn empty() -> Self {
        Self {
            kind: TokenKind::Eof,
            text: [0; TOKEN_TEXT],
            text_len: 0,
            int_value: 0,
            bool_value: false,
        }
    }

    pub fn as_str(&self) -> &str {
        core::str::from_utf8(&self.text[..self.text_len]).unwrap_or("")
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.text[..self.text_len]
    }
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.kind)
    }
}

/// Resultado da tokenização de uma linha.
pub struct LexResult {
    pub count: usize,
}

/// Tokeniza `line` para `tokens`. Devolve o número de tokens produzidos.
///
/// Uma linha pode conter múltiplos statements separados por ponto-e-vírgula ou
/// simplesmente por quebra de linha (o chamador alimenta uma linha por vez).
pub fn tokenize(line: &[u8], tokens: &mut [Token; MAX_TOKENS]) -> Result<usize, LexError> {
    let mut count = 0usize;
    let mut position = 0usize;

    while position < line.len() {
        let byte = line[position];

        // Espaços em branco e tab: ignorados.
        if byte == b' ' || byte == b'\t' || byte == b'\r' {
            position += 1;
            continue;
        }

        // Fim de linha: encerra o statement corrente.
        if byte == b'\n' {
            if count == tokens.len() {
                return Err(LexError::TooManyTokens);
            }
            tokens[count] = Token { kind: TokenKind::Newline, ..Token::empty() };
            count += 1;
            position += 1;
            continue;
        }

        // Comentário até o fim da linha.
        if byte == b'#' {
            break;
        }

        if count == tokens.len() {
            return Err(LexError::TooManyTokens);
        }

        match byte {
            b'.' => {
                tokens[count] = Token { kind: TokenKind::Dot, ..Token::empty() };
                count += 1;
                position += 1;
            }
            b'(' => {
                tokens[count] = Token { kind: TokenKind::LParen, ..Token::empty() };
                count += 1;
                position += 1;
            }
            b')' => {
                tokens[count] = Token { kind: TokenKind::RParen, ..Token::empty() };
                count += 1;
                position += 1;
            }
            b',' => {
                tokens[count] = Token { kind: TokenKind::Comma, ..Token::empty() };
                count += 1;
                position += 1;
            }
            b'=' => {
                tokens[count] = Token { kind: TokenKind::Eq, ..Token::empty() };
                count += 1;
                position += 1;
            }
            b'-' => {
                tokens[count] = Token { kind: TokenKind::Minus, ..Token::empty() };
                count += 1;
                position += 1;
            }
            b'{' => {
                tokens[count] = Token { kind: TokenKind::LBrace, ..Token::empty() };
                count += 1;
                position += 1;
            }
            b'}' => {
                tokens[count] = Token { kind: TokenKind::RBrace, ..Token::empty() };
                count += 1;
                position += 1;
            }
            b'"' => {
                // String literal com sequências de escape.
                let (text, len, next) = read_string(line, position + 1)?;
                if len > TOKEN_TEXT {
                    return Err(LexError::StringTooLong);
                }
                let mut token = Token::empty();
                token.kind = TokenKind::String;
                token.text[..len].copy_from_slice(&text[..len]);
                token.text_len = len;
                tokens[count] = token;
                count += 1;
                position = next;
            }
            b'0'..=b'9' | b'+' | b'-' => {
                // Inteiro (o '-' isolado já é Minus; aqui só aparece seguido de dígito).
                let (digits, len, next, is_neg) = read_int(line, position);
                if len > TOKEN_TEXT {
                    return Err(LexError::NumberTooLong);
                }
                let mut token = Token::empty();
                token.kind = TokenKind::Int;
                token.text[..len].copy_from_slice(&digits[..len]);
                token.text_len = len;
                token.int_value = if is_neg { -(digits.iter().fold(0i64, |a, b| a * 10 + (*b - b'0') as i64)) } else {
                    digits.iter().fold(0i64, |a, b| a * 10 + (*b - b'0') as i64)
                };
                tokens[count] = token;
                count += 1;
                position = next;
            }
            _ => {
                // Identificador, palavra reservada, literal true/false, ou $var.
                let (text, len, next) = read_ident(line, position);
                if len > TOKEN_TEXT {
                    return Err(LexError::IdentTooLong);
                }
                let mut token = Token::empty();
                let lower = text.iter().take(len).collect::<Vec<_>>();
                let is_bool = len == 4 && text[..4] == *b"true" || len == 5 && text[..5] == *b"false";
                if is_bool {
                    token.kind = TokenKind::Bool;
                    token.bool_value = len == 4;
                    token.text[..len].copy_from_slice(&text[..len]);
                    token.text_len = len;
                } else {
                    token.kind = TokenKind::Ident;
                    token.text[..len].copy_from_slice(&text[..len]);
                    token.text_len = len;
                }
                tokens[count] = token;
                count += 1;
                position = next;
            }
        }
    }

    Ok(count)
}

/// Lê uma string entre aspas (posição já após o `"` abertura). Devolve
/// (texto_expandido, comprimento, posição_após_aspas_fechamento).
fn read_string(line: &[u8], mut position: usize) -> Result<([u8; TOKEN_TEXT], usize, usize), LexError> {
    let mut text = [0u8; TOKEN_TEXT];
    let mut length = 0usize;
    while position < line.len() {
        let byte = line[position];
        if byte == b'"' {
            return Ok((text, length, position + 1));
        }
        if byte == b'\\' && position + 1 < line.len() {
            let escaped = match line[position + 1] {
                b'n' => b'\n',
                b't' => b'\t',
                b'\\' => b'\\',
                b'"' => b'"',
                other => other,
            };
            if length < TOKEN_TEXT {
                text[length] = escaped;
                length += 1;
            }
            position += 2;
            continue;
        }
        if length < TOKEN_TEXT {
            text[length] = byte;
            length += 1;
        }
        position += 1;
    }
    Err(LexError::UnterminatedString)
}

/// Lê um inteiro começando em `position`. Devolve (dígitos, comprimento, próxima_posição, é_negativo).
fn read_int(line: &[u8], position: usize) -> ([u8; TOKEN_TEXT], usize, usize, bool) {
    let mut text = [0u8; TOKEN_TEXT];
    let mut length = 0usize;
    let mut start = position;
    let mut is_neg = false;
    if line[position] == b'-' {
        is_neg = true;
        start = position + 1;
    } else if line[position] == b'+' {
        start = position + 1;
    }
    let mut position = start;
    while position < line.len() && line[position].is_ascii_digit() {
        if length < TOKEN_TEXT {
            text[length] = line[position];
            length += 1;
        }
        position += 1;
    }
    (text, length, position, is_neg)
}

/// Lê um identificador (ou palavra-chave/true/false/$var) começando em `position`.
fn read_ident(line: &[u8], position: usize) -> (&[u8], usize, usize) {
    let mut start = position;
    // Se começa com $, inclui o $ no identificador (ex.: $stdout, $?).
    if line[position] == b'$' {
        start = position;
        position += 1;
    }
    while position < line.len() {
        let byte = line[position];
        if byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'$' {
            position += 1;
        } else {
            break;
        }
    }
    (&line[start..position], position - start, position)
}

/// Erros léxicos.
#[derive(Debug, PartialEq, Eq)]
pub enum LexError {
    TooManyTokens,
    UnterminatedString,
    StringTooLong,
    NumberTooLong,
    IdentTooLong,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lex(input: &str) -> Vec<Token> {
        let mut tokens = [Token::empty(); MAX_TOKENS];
        let count = tokenize(input.as_bytes(), &mut tokens).expect("tokenize");
        tokens[..count].to_vec()
    }

    #[test]
    fn simple_command() {
        let tokens = lex(r#".run("ls")"#);
        assert_eq!(tokens[0].kind, TokenKind::Dot);
        assert_eq!(tokens[1].as_str(), "run");
        assert_eq!(tokens[2].kind, TokenKind::LParen);
        assert_eq!(tokens[3].as_str(), "ls");
        assert_eq!(tokens[4].kind, TokenKind::RParen);
    }

    #[test]
    fn chained_commands() {
        let tokens = lex(r#".from("cow").run("moo")"#);
        assert_eq!(tokens[0].kind, TokenKind::Dot);
        assert_eq!(tokens[1].as_str(), "from");
        assert_eq!(tokens[3].as_str(), "cow");
        assert_eq!(tokens[5].kind, TokenKind::Dot);
        assert_eq!(tokens[6].as_str(), "run");
        assert_eq!(tokens[8].as_str(), "moo");
    }

    #[test]
    fn named_args_and_flags() {
        let tokens = lex(r#".args(owner="root").flags(-sound="moo")"#);
        assert_eq!(tokens[1].as_str(), "args");
        assert_eq!(tokens[3].as_str(), "owner");
        assert_eq!(tokens[4].kind, TokenKind::Eq);
        assert_eq!(tokens[5].as_str(), "root");
        assert_eq!(tokens[7].as_str(), "flags");
        assert_eq!(tokens[9].kind, TokenKind::Minus);
        assert_eq!(tokens[10].as_str(), "sound");
        assert_eq!(tokens[12].as_str(), "moo");
    }

    #[test]
    fn string_escapes() {
        let tokens = lex(r#"."a\nb\tc""#);
        assert_eq!(tokens[1].as_str(), "run");
        let str_tok = &tokens[3];
        assert_eq!(str_tok.kind, TokenKind::String);
        assert_eq!(str_tok.as_str(), "a\nb\tc");
    }

    #[test]
    fn booleans_and_ints() {
        let tokens = lex(".loop(3).if(true)");
        assert_eq!(tokens[3].kind, TokenKind::Int);
        assert_eq!(tokens[3].int_value, 3);
        assert_eq!(tokens[7].kind, TokenKind::Bool);
        assert!(tokens[7].bool_value);
    }

    #[test]
    fn comments_are_ignored() {
        let tokens = lex("# comentario\n.run(\"x\")");
        // A linha do comentário não produz tokens; a segunda linha sim.
        let mut found_run = false;
        for token in &tokens {
            if token.as_str() == "run" {
                found_run = true;
            }
        }
        assert!(found_run);
    }
}
