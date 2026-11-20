//! Parser da sintaxe DSH.
//!
//! Transforma os tokens do lexer em uma sequência de comandos executáveis,
//! respeitando a cadeia ponto-notação (`.run("x").args(...)`), blocos de função
//! (`{ ... }`) e estruturas de controle (`.if`/`.else`/`.endif`, `.loop`,
//! `.scope`/`.endscope`, `.function`).

use crate::lexer::{LexError, Token, TokenKind};

// ---------------------------------------------------------------------------
// Nós da árvore de execução linear
// ---------------------------------------------------------------------------

/// Um comando DSH: um encadeamento de chamadas `.nome(...)` seguido de seus
/// argumentos. Cada item da cadeia é um `ChainItem`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChainItemKind {
    Run,      // .run("programa")
    From,     // .from("comando")
    Args,     // .args(chave="valor", ...)
    Flags,    // .flags(-chave="valor", ...)
    Loop,     // .loop(n)
    If,       // .if(condicao)
    Else,     // .else()
    EndIf,    // .endif()
    Scope,    // .scope()
    EndScope, // .endscope()
    Function, // .function(nome)
    Import,   // .import("modulo")
    Pipe,     // .pipe("programa")
    Var,      // .var(nome = valor)
    SetVar,   // .setvar(nome = valor)
}

/// Um item de cadeia `.nome(...)` com seus argumentos brutos (textos).
#[derive(Clone, Copy, Debug)]
pub struct ChainItem {
    pub kind: ChainItemKind,
    /// Posição do primeiro argumento dentro de `args_buffer` (índice em args).
    pub arg_start: usize,
    /// Quantidade de argumentos deste item.
    pub arg_count: usize,
}

/// Um argumento é um literal (string/int/bool/ident) ou um par nomeado
/// (`nome=valor`) ou uma flag (`-nome=valor`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Arg {
    Positional { text: [u8; crate::lexer::TOKEN_TEXT], len: usize },
    Named { name: [u8; crate::lexer::TOKEN_TEXT], name_len: usize, value: [u8; crate::lexer::TOKEN_TEXT], value_len: usize },
    Flag { name: [u8; crate::lexer::TOKEN_TEXT], name_len: usize, value: [u8; crate::lexer::TOKEN_TEXT], value_len: usize },
}

/// Um statement (uma linha ou bloco completo) do programa.
#[derive(Clone, Copy, Debug)]
pub enum Stmt {
    /// Um encadeamento `.run("x").args(...)` (ou outros built-ins).
    Chain { items: [ChainItem; 8], item_count: usize },
    /// Bloco de função `.function(nome) { ... }` seguido do corpo (índices no
    /// programa). Não usado diretamente aqui; mantido para expansão futura.
    Function { name: [u8; crate::lexer::TOKEN_TEXT], name_len: usize, body_start: usize, body_end: usize },
}

/// Programa completo: sequência de statements.
pub struct Program {
    pub stmts: [Stmt; 64],
    pub stmt_count: usize,
    /// Buffer de argumentos (textos) compartilhado por todos os itens.
    pub args: [[u8; crate::lexer::TOKEN_TEXT]; 128],
    pub args_len: [usize; 128],
    pub arg_count: usize,
}

impl Program {
    pub const fn new() -> Self {
        Self {
            stmts: [Stmt::Chain { items: [ChainItem { kind: ChainItemKind::Run, arg_start: 0, arg_count: 0 }; 8], item_count: 0 }; 64],
            stmt_count: 0,
            args: [[0; crate::lexer::TOKEN_TEXT]; 128],
            args_len: [0; 128],
            arg_count: 0,
        }
    }
}

/// Erros de parse.
#[derive(Debug, PartialEq, Eq)]
pub enum ParseError {
    UnexpectedToken,
    MissingLParen,
    MissingRParen,
    MissingComma,
    MissingEq,
    TooManyStatements,
    TooManyChainItems,
    TooManyArgs,
    MissingBlockBody,
    UnterminatedBlock,
    UnknownCommand,
}

/// Estado do parser durante a leitura de uma linha.
struct Parser<'a> {
    tokens: &'a [Token],
    pos: usize,
    program: &'a mut Program,
    /// Conta chaves abertas para detectar blocos `{ ... }`.
    brace_depth: usize,
    /// Índice do statement corrente que estamos preenchendo.
    current_stmt: Option<Stmt>,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<Token> {
        self.tokens.get(self.pos).copied()
    }

    fn next(&mut self) -> Option<Token> {
        let token = self.tokens.get(self.pos).copied();
        if token.is_some() {
            self.pos += 1;
        }
        token
    }

    fn expect(&mut self, kind: TokenKind) -> Result<Token, ParseError> {
        match self.next() {
            Some(token) if token.kind == kind => Ok(token),
            Some(_) => Err(ParseError::UnexpectedToken),
            None => Err(ParseError::UnexpectedToken),
        }
    }

    /// Lê um statement completo (cadeia ponto-notação opcionalmente seguida de
    /// bloco `{ ... }`).
    fn parse_stmt(&mut self) -> Result<(), ParseError> {
        if self.program.stmt_count == self.program.stmts.len() {
            return Err(ParseError::TooManyStatements);
        }

        // Espera `.` (ou Newline ignorado).
        match self.peek() {
            Some(Token { kind: TokenKind::Newline, .. }) => {
                self.next();
                return Ok(());
            }
            Some(Token { kind: TokenKind::Eof, .. }) => return Ok(()),
            Some(Token { kind: TokenKind::Dot, .. }) => {}
            _ => return Err(ParseError::UnexpectedToken),
        }

        let mut items = [ChainItem { kind: ChainItemKind::Run, arg_start: 0, arg_count: 0 }; 8];
        let mut item_count = 0usize;

        loop {
            // Consome o `.`.
            self.expect(TokenKind::Dot)?;
            let name_token = self.expect_ident()?;
            let kind = match name_token.as_str() {
                "run" => ChainItemKind::Run,
                "from" => ChainItemKind::From,
                "args" => ChainItemKind::Args,
                "flags" => ChainItemKind::Flags,
                "loop" => ChainItemKind::Loop,
                "if" => ChainItemKind::If,
                "else" => ChainItemKind::Else,
                "endif" => ChainItemKind::EndIf,
                "scope" => ChainItemKind::Scope,
                "endscope" => ChainItemKind::EndScope,
                "function" => ChainItemKind::Function,
                "import" => ChainItemKind::Import,
                "pipe" => ChainItemKind::Pipe,
                "var" => ChainItemKind::Var,
                "setvar" => ChainItemKind::SetVar,
                _ => return Err(ParseError::UnknownCommand),
            };
            self.expect(TokenKind::LParen)?;
            let (arg_start, arg_count) = self.parse_args()?;
            self.expect(TokenKind::RParen)?;

            if item_count == items.len() {
                return Err(ParseError::TooManyChainItems);
            }
            items[item_count] = ChainItem { kind, arg_start, arg_count };
            item_count += 1;

            // Se o próximo for `.`, continua a cadeia; senão, termina o statement.
            match self.peek() {
                Some(Token { kind: TokenKind::Dot, .. }) => continue,
                _ => break,
            }
        }

        // Se houver um bloco `{ ... }` logo após, anexa ao último item (ex. .function).
        if let Some(Token { kind: TokenKind::LBrace, .. }) = self.peek() {
            self.next();
            let body_start = self.program.stmt_count + 1;
            self.parse_until_rbrace()?;
            let body_end = self.program.stmt_count;
            // Adiciona um statement Function para capturar o bloco (simplificado:
            // o executor trata o bloco como statements sequenciais entre body_start..body_end).
            let _ = body_start;
            let _ = body_end;
            // Por enquanto, ignoramos o bloco no statement; o executor lê os
            // statements seguintes até .endif/.endscope quando aplicável.
        }

        if item_count == 0 {
            return Err(ParseError::UnexpectedToken);
        }

        self.program.stmts[self.program.stmt_count] = Stmt::Chain { items, item_count };
        self.program.stmt_count += 1;
        Ok(())
    }

    /// Lê argumentos dentro dos parênteses (já consumido o `(`).
    fn parse_args(&mut self) -> Result<(usize, usize), ParseError> {
        let arg_start = self.program.arg_count;
        let mut arg_count = 0usize;

        loop {
            match self.peek() {
                Some(Token { kind: TokenKind::RParen, .. }) => break,
                Some(Token { kind: TokenKind::Comma, .. }) => {
                    self.next();
                    continue;
                }
                Some(Token { kind: TokenKind::Newline, .. }) | Some(Token { kind: TokenKind::Eof, .. }) => {
                    return Err(ParseError::MissingRParen);
                }
                _ => {}
            }

            if arg_count == 128 {
                return Err(ParseError::TooManyArgs);
            }
            let arg = self.parse_one_arg()?;
            self.store_arg(arg);
            arg_count += 1;
        }

        Ok((arg_start, arg_count))
    }

    /// Lê um único argumento: `ident=valor`, `-ident=valor` ou literal.
    fn parse_one_arg(&mut self) -> Result<Arg, ParseError> {
        // Flag: `-nome=valor`
        if let Some(Token { kind: TokenKind::Minus, .. }) = self.peek() {
            self.next();
            let name_tok = self.expect_ident()?;
            self.expect(TokenKind::Eq)?;
            let value_tok = self.expect_arg_value()?;
            let mut name = [0u8; crate::lexer::TOKEN_TEXT];
            let mut value = [0u8; crate::lexer::TOKEN_TEXT];
            name[..name_tok.text_len].copy_from_slice(name_tok.as_bytes());
            value[..value_tok.text_len].copy_from_slice(value_tok.as_bytes());
            return Ok(Arg::Flag { name, name_len: name_tok.text_len, value, value_len: value_tok.text_len });
        }

        let first = self.expect_arg_value()?;
        // Verifica se vem `=` (argumento nomeado `nome=valor`).
        if let Some(Token { kind: TokenKind::Eq, .. }) = self.peek() {
            self.next();
            let name_tok = first;
            let value_tok = self.expect_arg_value()?;
            let mut name = [0u8; crate::lexer::TOKEN_TEXT];
            let mut value = [0u8; crate::lexer::TOKEN_TEXT];
            name[..name_tok.text_len].copy_from_slice(name_tok.as_bytes());
            value[..value_tok.text_len].copy_from_slice(value_tok.as_bytes());
            return Ok(Arg::Named { name, name_len: name_tok.text_len, value, value_len: value_tok.text_len });
        }

        // Positional literal.
        let mut text = [0u8; crate::lexer::TOKEN_TEXT];
        text[..first.text_len].copy_from_slice(first.as_bytes());
        Ok(Arg::Positional { text, len: first.text_len })
    }

    fn expect_ident(&mut self) -> Result<Token, ParseError> {
        match self.next() {
            Some(token) if matches!(token.kind, TokenKind::Ident | TokenKind::String | TokenKind::Int | TokenKind::Bool) => Ok(token),
            _ => Err(ParseError::UnexpectedToken),
        }
    }

    fn expect_arg_value(&mut self) -> Result<Token, ParseError> {
        match self.next() {
            Some(token) if matches!(
                token.kind,
                TokenKind::String | TokenKind::Int | TokenKind::Bool | TokenKind::Ident
            ) => Ok(token),
            _ => Err(ParseError::UnexpectedToken),
        }
    }

    /// Guarda um argumento no buffer compartilhado (índices são referenciados
    /// pelos ChainItems).
    fn store_arg(&mut self, arg: Arg) {
        match arg {
            Arg::Positional { text, len } => {
                self.program.args[self.program.arg_count][..len].copy_from_slice(&text[..len]);
                self.program.args_len[self.program.arg_count] = len;
                self.program.arg_count += 1;
            }
            Arg::Named { name, name_len, value, value_len } => {
                // Armazena "nome=valor" como um único texto para simplificar o
                // executor (ele redivide em '=').
                let mut combined = [0u8; crate::lexer::TOKEN_TEXT];
                let mut total = 0usize;
                combined[..name_len].copy_from_slice(&name[..name_len]);
                total += name_len;
                combined[total] = b'=';
                total += 1;
                combined[total..total + value_len].copy_from_slice(&value[..value_len]);
                total += value_len;
                self.program.args[self.program.arg_count][..total].copy_from_slice(&combined[..total]);
                self.program.args_len[self.program.arg_count] = total;
                self.program.arg_count += 1;
            }
            Arg::Flag { name, name_len, value, value_len } => {
                let mut combined = [0u8; crate::lexer::TOKEN_TEXT];
                let mut total = 0usize;
                combined[total] = b'-';
                total += 1;
                combined[total..total + name_len].copy_from_slice(&name[..name_len]);
                total += name_len;
                combined[total] = b'=';
                total += 1;
                combined[total..total + value_len].copy_from_slice(&value[..value_len]);
                total += value_len;
                self.program.args[self.program.arg_count][..total].copy_from_slice(&combined[..total]);
                self.program.args_len[self.program.arg_count] = total;
                self.program.arg_count += 1;
            }
        }
    }

    /// Lê statements até encontrar `}` (não incluso), para blocos de função.
    fn parse_until_rbrace(&mut self) -> Result<(), ParseError> {
        let mut depth = 1usize;
        loop {
            match self.peek() {
                Some(Token { kind: TokenKind::LBrace, .. }) => {
                    depth += 1;
                    self.next();
                }
                Some(Token { kind: TokenKind::RBrace, .. }) => {
                    depth -= 1;
                    if depth == 0 {
                        self.next();
                        return Ok(());
                    }
                    self.next();
                }
                Some(Token { kind: TokenKind::Newline, .. }) | Some(Token { kind: TokenKind::Eof, .. }) => {
                    self.next();
                }
                _ => {
                    if self.program.stmt_count == self.program.stmts.len() {
                        return Err(ParseError::TooManyStatements);
                    }
                    self.parse_stmt()?;
                }
            }
        }
    }
}

/// Converte uma lista de tokens (uma linha) em um Program.
pub fn parse(tokens: &[Token]) -> Result<Program, ParseError> {
    let mut program = Program::new();
    let mut parser = Parser {
        tokens,
        pos: 0,
        program: &mut program,
        brace_depth: 0,
        current_stmt: None,
    };
    loop {
        match parser.peek() {
            None | Some(Token { kind: TokenKind::Eof, .. }) => break,
            Some(Token { kind: TokenKind::Newline, .. }) => {
                parser.next();
            }
            _ => parser.parse_stmt()?,
        }
    }
    Ok(program)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::{tokenize, TokenKind, MAX_TOKENS};

    fn parse_line(input: &str) -> Program {
        let mut tokens = [Token { kind: TokenKind::Eof, ..Default::default() }; MAX_TOKENS];
        let count = tokenize(input.as_bytes(), &mut tokens).expect("tokenize");
        parse(&tokens[..count]).expect("parse")
    }

    #[test]
    fn simple_run() {
        let prog = parse_line(r#".run("ls")"#);
        assert_eq!(prog.stmt_count, 1);
        match prog.stmts[0] {
            Stmt::Chain { items, item_count } => {
                assert_eq!(item_count, 1);
                assert_eq!(items[0].kind, ChainItemKind::Run);
                assert_eq!(items[0].arg_count, 1);
            }
            _ => panic!("esperado Chain"),
        }
    }

    #[test]
    fn chained_command() {
        let prog = parse_line(r#".from("cow").run("moo")"#);
        match prog.stmts[0] {
            Stmt::Chain { items, item_count } => {
                assert_eq!(item_count, 2);
                assert_eq!(items[0].kind, ChainItemKind::From);
                assert_eq!(items[1].kind, ChainItemKind::Run);
            }
            _ => panic!("esperado Chain"),
        }
    }

    #[test]
    fn named_args_and_flags() {
        let prog = parse_line(r#".run("cow").args(owner="root").flags(-sound="moo")"#);
        match prog.stmts[0] {
            Stmt::Chain { items, item_count } => {
                assert_eq!(item_count, 3);
                assert_eq!(items[0].kind, ChainItemKind::Run);
                assert_eq!(items[1].kind, ChainItemKind::Args);
                assert_eq!(items[2].kind, ChainItemKind::Flags);
            }
            _ => panic!("esperado Chain"),
        }
    }

    #[test]
    fn loop_and_if() {
        let prog = parse_line(r#".loop(3).run("echo")"#);
        match prog.stmts[0] {
            Stmt::Chain { items, item_count } => {
                assert_eq!(items[0].kind, ChainItemKind::Loop);
                assert_eq!(items[0].arg_count, 1);
            }
            _ => panic!("esperado Chain"),
        }
    }

    #[test]
    fn var_declaration() {
        let prog = parse_line(r#".var(x = 10)"#);
        match prog.stmts[0] {
            Stmt::Chain { items, item_count } => {
                assert_eq!(item_count, 1);
                assert_eq!(items[0].kind, ChainItemKind::Var);
            }
            _ => panic!("esperado Chain"),
        }
    }

    #[test]
    fn empty_line() {
        let prog = parse_line("");
        assert_eq!(prog.stmt_count, 0);
    }

    #[test]
    fn comment_line() {
        let prog = parse_line("# so um comentario");
        assert_eq!(prog.stmt_count, 0);
    }
}
