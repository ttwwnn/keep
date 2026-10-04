//! As contas de IA: quais existem, de quem são, em que ordem, e o que uma aba
//! precisa no ambiente para rodar em cada uma. Ver `docs/ia.md` ("Contas").

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// O serviço de uma conta. Escrito como o app do macOS sempre escreveu
/// (`claude`, `codex`); na ordem e nas chaves, o GPT é `gpt:`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Motor {
    Claude,
    Codex,
}

impl Motor {
    /// O prefixo das chaves: `claude`, `gpt`.
    pub fn prefixo(self) -> &'static str {
        match self {
            Motor::Claude => "claude",
            Motor::Codex => "gpt",
        }
    }

    /// `claude:reserva`, `gpt:principal`.
    pub fn chave(self, apelido: &str) -> String {
        format!("{}:{apelido}", self.prefixo())
    }

    /// O programa que roda a conta.
    pub fn programa(self) -> &'static str {
        match self {
            Motor::Claude => "claude",
            Motor::Codex => "codex",
        }
    }
}

/// Onde mora o login de uma conta.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "tipo", rename_all = "lowercase")]
pub enum Armazem {
    /// O login padrão do Claude Code (Chaveiro no macOS, arquivo nos demais).
    Global,
    /// Uma pasta de login próprio: `CLAUDE_SECURESTORAGE_CONFIG_DIR=<pasta>`.
    Fixa { pasta: PathBuf },
    /// Um slot do cofre do kit: só leitura, não roda aba.
    Cofre { arquivo: PathBuf },
    /// Um `CODEX_HOME`; o principal é `~/.codex`.
    Codex { home: PathBuf, principal: bool },
}

/// Uma conta, como o rodapé e os menus a mostram. Os nomes dos campos são os
/// do app do macOS (`AIAccountSummary`), para ele ler direto.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Conta {
    pub engine: Motor,
    /// A identidade da conta no serviço (uuid do dono, id da conta do
    /// ChatGPT, ou o e-mail): dois armazéns com o mesmo `key` são a mesma conta.
    pub key: String,
    pub alias: String,
    pub aliases: Vec<String>,
    pub email: Option<String>,
    pub plan: Option<String>,
    /// Claude: é a dona do login global. GPT: é a principal.
    pub is_active: bool,
    /// A primeira Claude da ordem.
    pub is_preferred: bool,
    /// Algo errado com o login (sem login, recusado), dito na linha dela.
    pub warning: Option<String>,
    /// A chave que a representa na ordem.
    pub order_key: String,
    pub has_token: bool,
    /// Onde estão os logins dela.
    pub armazens: Vec<Armazem>,
}

impl Conta {
    /// Todas as chaves que a nomeiam, uma por apelido.
    pub fn chaves(&self) -> Vec<String> {
        self.aliases.iter().map(|a| self.engine.chave(a)).collect()
    }

    /// A pasta de login próprio, se ela tem uma.
    pub fn fixa(&self) -> Option<&PathBuf> {
        self.armazens.iter().find_map(|a| match a {
            Armazem::Fixa { pasta } => Some(pasta),
            _ => None,
        })
    }

    /// O `CODEX_HOME` dela.
    pub fn codex_home(&self) -> Option<(&PathBuf, bool)> {
        self.armazens.iter().find_map(|a| match a {
            Armazem::Codex { home, principal } => Some((home, *principal)),
            _ => None,
        })
    }

    pub fn global(&self) -> bool {
        self.armazens.iter().any(|a| matches!(a, Armazem::Global))
    }
}

/// Um token de acesso, só para medir o consumo. Nunca sai em JSON.
#[derive(Clone)]
pub struct Token {
    pub acesso: String,
    /// Unix ms; `None` quando o armazém não diz.
    pub expira_em: Option<u64>,
    /// GPT: o `ChatGPT-Account-Id`.
    pub conta_chatgpt: Option<String>,
}
