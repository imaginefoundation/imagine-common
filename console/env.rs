//! Tabela de variáveis com escopos aninhados.
//!
//! Suporta `.var()` (declara no escopo atual), `.setvar()` (atualiza no escopo
//! mais próximo onde existe), `.scope()` (abre novo nível) e `.endscope()`
//! (fecha o nível). A resolução busca do escopo mais interno para o externo,
//! então escopos internos sombreiam variáveis externas com o mesmo nome.

use crate::lexer::Word;

/// Número máximo de escopos simultâneos (pilha de escopo).
pub const MAX_SCOPES: usize = 8;
/// Número máximo de variáveis por escopo.
pub const MAX_VARS_PER_SCOPE: usize = 16;
/// Tamanho máximo do nome de uma variável.
pub const NAME_SIZE: usize = 24;
/// Tamanho máximo do valor de uma variável.
pub const VALUE_SIZE: usize = 96;

#[derive(Clone, Copy)]
pub struct Variable {
    pub name: [u8; NAME_SIZE],
    pub name_length: usize,
    pub value: [u8; VALUE_SIZE],
    pub value_length: usize,
    pub active: bool,
}

impl Variable {
    pub const EMPTY: Self = Self {
        name: [0; NAME_SIZE],
        name_length: 0,
        value: [0; VALUE_SIZE],
        value_length: 0,
        active: false,
    };
}

/// Um escopo: array fixo de variáveis.
#[derive(Clone, Copy)]
pub struct Scope {
    variables: [Variable; MAX_VARS_PER_SCOPE],
}

impl Scope {
    pub const fn new() -> Self {
        Self {
            variables: [Variable::EMPTY; MAX_VARS_PER_SCOPE],
        }
    }

    pub fn get(&self, name: &[u8]) -> Option<&[u8]> {
        self.variables
            .iter()
            .find(|variable| variable.active && variable.name[..variable.name_length] == *name)
            .map(|variable| &variable.value[..variable.value_length])
    }

    /// Define ou cria a variável neste escopo. Devolve falso se não houver
    /// espaço ou o nome/valor for inválido ou muito longo.
    pub fn set(&mut self, name: &[u8], value: &[u8]) -> bool {
        if !valid_name(name) || name.is_empty() || name.len() > NAME_SIZE || value.len() > VALUE_SIZE {
            return false;
        }
        let slot = self
            .variables
            .iter()
            .position(|variable| variable.active && variable.name[..variable.name_length] == *name)
            .or_else(|| self.variables.iter().position(|variable| !variable.active));
        let Some(slot) = slot else {
            return false;
        };
        let variable = &mut self.variables[slot];
        variable.name[..name.len()].copy_from_slice(name);
        variable.value[..value.len()].copy_from_slice(value);
        variable.name_length = name.len();
        variable.value_length = value.len();
        variable.active = true;
        true
    }

    pub fn unset(&mut self, name: &[u8]) {
        if let Some(variable) = self
            .variables
            .iter_mut()
            .find(|variable| variable.active && variable.name[..variable.name_length] == *name)
        {
            *variable = Variable::EMPTY;
        }
    }
}

/// Ambiente com pilha de escopos. O escopo 0 é o global (sempre presente).
pub struct Environment {
    scopes: [Scope; MAX_SCOPES],
    scope_count: usize,
}

impl Environment {
    pub fn new() -> Self {
        let mut environment = Self {
            scopes: [Scope::new(); MAX_SCOPES],
            scope_count: 1,
        };
        // Variáveis globais iniciais (implícitas do DSH).
        environment.scopes[0].set(b"HOME", b"/", true);
        environment.scopes[0].set(b"PATH", b"/bin", true);
        environment.scopes[0].set(b"PWD", b"/", true);
        environment.scopes[0].set(b"SHELL", b"/bin/shell", true);
        environment.scopes[0].set(b"?", b"0", true);
        environment.scopes[0].set(b"stdout", b"", true);
        environment
    }

    /// Número de escopos ativos.
    pub fn depth(&self) -> usize {
        self.scope_count
    }

    /// Abre um novo escopo. Devolve falso se já estiver no limite.
    pub fn push_scope(&mut self) -> bool {
        if self.scope_count == MAX_SCOPES {
            return false;
        }
        self.scope_count += 1;
        true
    }

    /// Fecha o escopo mais interno (não fecha o global). Devolve falso se não
    /// houver escopo para fechar.
    pub fn pop_scope(&mut self) -> bool {
        if self.scope_count <= 1 {
            return false;
        }
        self.scope_count -= 1;
        true
    }

    /// Resolve uma variável buscando do escopo mais interno para o externo.
    pub fn get(&self, name: &[u8]) -> Option<&[u8]> {
        let mut index = self.scope_count;
        while index > 0 {
            index -= 1;
            if let Some(value) = self.scopes[index].get(name) {
                return Some(value);
            }
        }
        None
    }

    /// Declara (ou atualiza se já existir no escopo atual) uma variável no
    /// escopo mais interno. Usado por `.var()`.
    pub fn set_current(&mut self, name: &[u8], value: &[u8]) -> bool {
        self.scopes[self.scope_count - 1].set(name, value)
    }

    /// Atualiza uma variável existente no escopo mais próximo onde ela está
    /// definida. Se não existir em nenhum escopo, cria no escopo atual.
    /// Usado por `.setvar()`.
    pub fn setvar(&mut self, name: &[u8], value: &[u8]) -> bool {
        let mut index = self.scope_count;
        while index > 0 {
            index -= 1;
            if self.scopes[index].get(name).is_some() {
                return self.scopes[index].set(name, value);
            }
        }
        self.scopes[self.scope_count - 1].set(name, value)
    }

    /// Remove uma variável do escopo onde ela foi definida primeiro.
    pub fn unset(&mut self, name: &[u8]) {
        let mut index = self.scope_count;
        while index > 0 {
            index -= 1;
            if self.scopes[index].get(name).is_some() {
                self.scopes[index].unset(name);
                return;
            }
        }
    }

    /// Retorna uma referência ao escopo atual (para leitura direta em expande).
    pub fn current_scope(&self) -> &Scope {
        &self.scopes[self.scope_count - 1]
    }
}

/// Valida o nome de uma variável (letra ou _ no início, depois alfanuméricos/_).
pub fn valid_name(name: &[u8]) -> bool {
    match name.first() {
        Some(first) if first.is_ascii_alphabetic() || *first == b'_' => {}
        _ => return false,
    }
    name[1..]
        .iter()
        .all(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
}

/// Expande uma `Word` (token) substituindo variáveis `$nome` e `${nome}`.
/// Devolve o texto expandido ou None se a word não for literal simples.
pub fn expand_word(word: &Word, environment: &Environment, output: &mut [u8; 256]) -> Option<usize> {
    let mut length = 0usize;
    for segment in word.segments[..word.segment_count].iter() {
        let bytes = segment.as_bytes();
        if segment.copies_variable {
            expand_segment(segment, environment, output, &mut length)?;
        } else {
            if length + bytes.len() > output.len() {
                return None;
            }
            output[length..length + bytes.len()].copy_from_slice(bytes);
            length += bytes.len();
        }
    }
    Some(length)
}

fn expand_segment(
    segment: &lexer::Segment,
    environment: &Environment,
    output: &mut [u8; 256],
    length: &mut usize,
) -> Option<()> {
    let bytes = segment.as_bytes();
    let mut index = 0usize;
    while index < bytes.len() {
        if bytes[index] != b'$' {
            if *length >= output.len() {
                return None;
            }
            output[*length] = bytes[index];
            *length += 1;
            index += 1;
            continue;
        }
        let (name, next) = parse_reference(bytes, index + 1);
        if name.is_empty() {
            if *length >= output.len() {
                return None;
            }
            output[*length] = b'$';
            *length += 1;
            index += 1;
            continue;
        }
        let value = environment.get(name).unwrap_or(b"");
        for &byte in value {
            if *length >= output.len() {
                return None;
            }
            output[*length] = byte;
            *length += 1;
        }
        index = next;
    }
    Some(())
}

/// Lê o nome da variável após `$`, aceitando `${nome}`.
fn parse_reference(bytes: &[u8], start: usize) -> (&[u8], usize) {
    if bytes.get(start) == Some(&b'{') {
        let name_start = start + 1;
        if let Some(end) = bytes[name_start..].iter().position(|byte| *byte == b'}') {
            let end = name_start + end;
            return (&bytes[name_start..end], end + 1);
        }
        return (&[], start);
    }
    if matches!(bytes.get(start), Some(b'?') | Some(b'$')) {
        return (&bytes[start..start + 1], start + 1);
    }
    let mut end = start;
    while end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
        end += 1;
    }
    (&bytes[start..end], end)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scoped_lookup_shadows_outer() {
        let mut env = Environment::new();
        env.scopes[0].set(b"x", b"1").unwrap();
        assert_eq!(env.get(b"x"), Some(&b"1"[..]));
        env.push_scope().unwrap();
        env.scopes[1].set(b"x", b"2").unwrap();
        assert_eq!(env.get(b"x"), Some(&b"2"[..]));
        env.pop_scope().unwrap();
        assert_eq!(env.get(b"x"), Some(&b"1"[..]));
    }

    #[test]
    fn setvar_updates_nearest_scope() {
        let mut env = Environment::new();
        env.push_scope().unwrap();
        env.scopes[1].set(b"x", b"1").unwrap();
        assert!(env.setvar(b"x", b"9"));
        assert_eq!(env.scopes[1].get(b"x"), Some(&b"9"[..]));
        env.pop_scope().unwrap();
    }

    #[test]
    fn cannot_pop_global_scope() {
        let mut env = Environment::new();
        assert!(!env.pop_scope());
        assert_eq!(env.depth(), 1);
    }
}
