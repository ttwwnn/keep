//! As contas de IA: quais existem, de quem são, em que ordem, e o que uma aba
//! precisa no ambiente para rodar em cada uma. Ver `docs/ia.md` ("Contas").

pub mod credencial;
pub mod donos;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::caminhos;
use credencial::{Lido, Oauth};

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

    /// O motor de uma chave (`claude:…`, `gpt:…`).
    pub fn da_chave(chave: &str) -> Option<Motor> {
        match chave.split_once(':')?.0 {
            "claude" => Some(Motor::Claude),
            "gpt" => Some(Motor::Codex),
            _ => None,
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
    /// O token que mede o consumo dela. Nunca sai em JSON.
    #[serde(skip)]
    pub token: Option<Token>,
    /// Se há um armazém em que uma aba pode rodar nela agora.
    pub roda: bool,
}

impl Conta {
    /// Todas as chaves que a nomeiam, uma por apelido.
    pub fn chaves(&self) -> Vec<String> {
        self.aliases.iter().map(|a| self.engine.chave(a)).collect()
    }

    /// Se `chave` nomeia esta conta.
    pub fn e(&self, chave: &str) -> bool {
        self.chaves().iter().any(|c| c == chave)
    }

    /// A pasta de login próprio, se ela tem uma com login.
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
#[derive(Clone, Debug)]
pub struct Token {
    pub acesso: String,
    /// Unix ms; `None` quando o armazém não diz.
    pub expira_em: Option<u64>,
    /// GPT: o `ChatGPT-Account-Id`.
    pub conta_chatgpt: Option<String>,
}

/// Um armazém achado, antes de ser juntado aos outros da mesma conta.
struct Achado {
    motor: Motor,
    armazem: Armazem,
    apelidos: Vec<String>,
    dono: Option<String>,
    email: Option<String>,
    plano: Option<String>,
    token: Option<Token>,
    aviso: Option<String>,
    /// Pode rodar uma aba (tem login com refresh).
    roda: bool,
}

fn texto(v: &Value, chave: &str) -> Option<String> {
    v.get(chave).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()).map(str::to_string)
}

/// "default_claude_max_20x" se lê "Max 20x"; o resto, pelo nome da assinatura.
pub fn plano_claude(nivel: Option<&str>, assinatura: Option<&str>) -> Option<String> {
    if let Some(n) = nivel {
        if let Some(i) = n.find("max_") {
            let resto = &n[i + 4..];
            let fim = resto.find(|c: char| !(c.is_ascii_alphanumeric())).unwrap_or(resto.len());
            if fim > 0 {
                return Some(format!("Max {}", &resto[..fim]));
            }
        }
    }
    assinatura.filter(|s| !s.is_empty()).map(maiuscula)
}

fn maiuscula(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(p) => p.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

/// Um apelido aceito em chave e em nome de pasta.
pub fn apelido_valido(a: &str) -> bool {
    !a.is_empty()
        && a.len() <= 40
        && a.chars().next().is_some_and(|c| c.is_ascii_alphanumeric())
        && a.chars().all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))
}

/// Um apelido a partir de um e-mail: a parte antes do `@`, limpa.
pub fn apelido_de_email(email: &str) -> String {
    let local = email.split('@').next().unwrap_or("");
    let limpo: String = local
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || "._-".contains(c) { c } else { '-' })
        .collect::<String>()
        .trim_matches(|c: char| !c.is_ascii_alphanumeric())
        .chars()
        .take(40)
        .collect();
    if apelido_valido(&limpo) { limpo } else { "conta".into() }
}

// ------------------------------------------------------------------ Claude

/// Os metadados que o Keep grava numa pasta fixa criada por ele.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Metadados {
    pub versao: u32,
    #[serde(default)]
    pub apelido: String,
    #[serde(default)]
    pub email: String,
    #[serde(default)]
    pub uuid: String,
    #[serde(default, rename = "criadaEm")]
    pub criada_em: u64,
}

pub fn ler_metadados(pasta: &Path) -> Option<Metadados> {
    serde_json::from_str(&std::fs::read_to_string(pasta.join("keep.json")).ok()?).ok()
}

pub fn gravar_metadados(pasta: &Path, m: &Metadados) -> std::io::Result<()> {
    std::fs::create_dir_all(pasta)?;
    let tmp = pasta.join("keep.json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(m)?)?;
    std::fs::rename(&tmp, pasta.join("keep.json"))
}

fn token_claude(o: &Oauth) -> Option<Token> {
    Some(Token { acesso: o.acesso.clone()?, expira_em: o.expira_em, conta_chatgpt: None })
}

/// Os slots do cofre do kit, se houver: o apelido de cada dono, e um token
/// para medir quem não tem outro armazém legível.
fn slots_do_kit() -> Vec<Achado> {
    let mut saida = Vec::new();
    let Ok(dir) = std::fs::read_dir(caminhos::contas()) else { return saida };
    let mut arquivos: Vec<PathBuf> = dir
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.extension().is_some_and(|e| e == "json")
                && p.file_name().is_some_and(|n| !n.to_string_lossy().starts_with('.'))
        })
        .collect();
    arquivos.sort();
    for arquivo in arquivos {
        let Some(slot) = std::fs::read_to_string(&arquivo).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok())
        else {
            continue;
        };
        let nome = arquivo.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let apelido = texto(&slot, "apelido").unwrap_or(nome);
        let oauth = slot.get("credenciais").and_then(Oauth::de);
        let conta = slot.get("oauthAccount");
        let email = texto(&slot, "email").or_else(|| conta.and_then(|c| texto(c, "emailAddress")));
        let uuid = texto(&slot, "accountUuid").or_else(|| conta.and_then(|c| texto(c, "accountUuid")));
        let morto = texto(&slot, "refreshMorto").is_some();
        saida.push(Achado {
            motor: Motor::Claude,
            armazem: Armazem::Cofre { arquivo: arquivo.clone() },
            apelidos: vec![apelido],
            dono: uuid,
            email,
            plano: oauth.as_ref().and_then(|o| plano_claude(o.nivel.as_deref(), o.assinatura.as_deref())),
            token: oauth.as_ref().and_then(token_claude),
            aviso: morto.then(|| "login recusado: entre de novo com /login".to_string()),
            // Um slot é uma cópia: nenhuma aba roda nele.
            roda: false,
        });
    }
    saida
}

/// O slot que o kit diz estar no login global (`.ativa`), se houver kit.
fn ativa_do_kit() -> Option<String> {
    std::fs::read_to_string(caminhos::contas().join(".ativa"))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn achado_de_login(lido: Lido, armazem: Armazem, perguntar: bool, quem: &str) -> Option<Achado> {
    match lido {
        Lido::Ausente => None,
        Lido::Ilegivel(por) => Some(Achado {
            motor: Motor::Claude,
            armazem,
            apelidos: vec![],
            dono: None,
            email: None,
            plano: None,
            token: None,
            aviso: Some(format!("não deu para ler o login {quem} ({por})")),
            roda: false,
        }),
        Lido::Ok(valor) => {
            let oauth = Oauth::de(&valor);
            let token = oauth.as_ref().and_then(token_claude);
            let dono = token.as_ref().and_then(|t| donos::dono(&t.acesso, perguntar));
            let recusado = oauth.as_ref().is_none_or(|o| o.recusado());
            Some(Achado {
                motor: Motor::Claude,
                armazem,
                apelidos: vec![],
                dono: dono.as_ref().map(|d| d.uuid.clone()),
                email: dono.map(|d| d.email).filter(|e| !e.is_empty()),
                plano: oauth.as_ref().and_then(|o| plano_claude(o.nivel.as_deref(), o.assinatura.as_deref())),
                token,
                aviso: recusado.then(|| format!("login {quem} recusado: entre de novo com /login")),
                roda: !recusado,
            })
        }
    }
}

/// O login global e as pastas fixas.
fn armazens_claude(perguntar: bool) -> Vec<Achado> {
    let mut saida = Vec::new();
    if let Some(mut a) = achado_de_login(credencial::global(), Armazem::Global, perguntar, "global") {
        // Dono que o servidor não confirmou (token vencido, sem rede): o kit
        // sabe qual slot está no global.
        if a.dono.is_none() {
            if let Some(ativa) = ativa_do_kit() {
                a.apelidos.push(ativa);
            }
        }
        saida.push(a);
    }
    let Ok(dir) = std::fs::read_dir(caminhos::fixas()) else { return saida };
    let mut pastas: Vec<PathBuf> = dir
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_dir() && p.file_name().is_some_and(|n| !n.to_string_lossy().starts_with('.')))
        .collect();
    pastas.sort();
    for pasta in pastas {
        let meta = ler_metadados(&pasta);
        let armazem = Armazem::Fixa { pasta: pasta.clone() };
        let achado = achado_de_login(credencial::fixa(&pasta), armazem.clone(), perguntar, "próprio");
        let mut a = match achado {
            Some(a) => a,
            // Pasta sem login: só vale se o Keep sabe de quem ela é (um login
            // a fazer, ou desfeito).
            None => match &meta {
                Some(m) if !m.uuid.is_empty() || !m.email.is_empty() => Achado {
                    motor: Motor::Claude,
                    armazem,
                    apelidos: vec![],
                    dono: None,
                    email: None,
                    plano: None,
                    token: None,
                    aviso: Some("sem login próprio: entre pelo menu da aba".into()),
                    roda: false,
                },
                _ => continue,
            },
        };
        if let Some(m) = &meta {
            if a.dono.is_none() && !m.uuid.is_empty() {
                a.dono = Some(m.uuid.clone());
            }
            if a.email.is_none() && !m.email.is_empty() {
                a.email = Some(m.email.clone());
            }
            if apelido_valido(&m.apelido) {
                a.apelidos.push(m.apelido.clone());
            }
        }
        // As pastas do kit têm o nome dos 8 primeiros do uuid do dono.
        if a.dono.is_none() {
            a.dono = pasta.file_name().map(|n| format!("prefixo:{}", n.to_string_lossy()));
        }
        saida.push(a);
    }
    saida
}

// --------------------------------------------------------------------- GPT

/// As declarações de um JWT, sem conferir a assinatura: só decidem o que
/// mostrar; o servidor confere o token a cada pedido.
pub fn jwt(token: &str) -> Option<Value> {
    use base64::Engine;
    let parte = token.split('.').nth(1)?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(parte.trim_end_matches('='))
        .ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn achado_codex(home: &Path, apelido: &str, principal: bool) -> Option<Achado> {
    let arquivo: Value = serde_json::from_str(&std::fs::read_to_string(home.join("auth.json")).ok()?).ok()?;
    let tokens = arquivo.get("tokens")?;
    let acesso = texto(tokens, "access_token")?;
    let declaracoes = jwt(&acesso);
    let identidade = texto(tokens, "id_token").and_then(|t| jwt(&t));
    let perfil = declaracoes.as_ref().and_then(|c| c.get("https://api.openai.com/profile").cloned());
    let auth = declaracoes
        .as_ref()
        .and_then(|c| c.get("https://api.openai.com/auth").cloned())
        .or_else(|| identidade.as_ref().and_then(|c| c.get("https://api.openai.com/auth").cloned()));
    let email = perfil
        .as_ref()
        .and_then(|p| texto(p, "email"))
        .or_else(|| identidade.as_ref().and_then(|i| texto(i, "email")));
    let conta = texto(tokens, "account_id").or_else(|| auth.as_ref().and_then(|a| texto(a, "chatgpt_account_id")));
    let expira = declaracoes.as_ref().and_then(|c| c.get("exp")).and_then(Value::as_u64).map(|s| s * 1000);
    let plano = auth.as_ref().and_then(|a| texto(a, "chatgpt_plan_type")).map(|p| maiuscula(&p));
    Some(Achado {
        motor: Motor::Codex,
        armazem: Armazem::Codex { home: home.to_path_buf(), principal },
        apelidos: vec![apelido.to_string()],
        dono: conta.clone().or_else(|| email.clone()),
        email,
        plano,
        token: Some(Token { acesso, expira_em: expira, conta_chatgpt: conta }),
        aviso: None,
        roda: true,
    })
}

fn armazens_codex() -> Vec<Achado> {
    let mut saida: Vec<Achado> = achado_codex(&caminhos::codex(), "principal", true).into_iter().collect();
    let Ok(dir) = std::fs::read_dir(caminhos::codex_contas()) else { return saida };
    let mut nomes: Vec<String> = dir
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| !n.starts_with('.') && apelido_valido(n))
        .collect();
    nomes.sort();
    for nome in nomes {
        if let Some(a) = achado_codex(&caminhos::codex_contas().join(&nome), &nome, false) {
            saida.push(a);
        }
    }
    saida
}

// ------------------------------------------------------------------- juntar

/// Junta os armazéns de cada dono numa conta.
fn juntar(achados: Vec<Achado>, ativa: Option<&str>) -> Vec<Conta> {
    // Donos conhecidos só pelo prefixo (pasta do kit sem login legível)
    // ficam com o dono inteiro de quem tiver o mesmo começo.
    let inteiros: Vec<String> =
        achados.iter().filter_map(|a| a.dono.clone()).filter(|d| !d.starts_with("prefixo:")).collect();
    let mut grupos: Vec<(String, Vec<Achado>)> = Vec::new();
    for (n, mut a) in achados.into_iter().enumerate() {
        if let Some(prefixo) = a.dono.as_deref().and_then(|d| d.strip_prefix("prefixo:")) {
            a.dono = inteiros.iter().find(|u| u.starts_with(prefixo)).cloned();
        }
        let chave = match (&a.dono, &a.email) {
            (Some(d), _) => d.clone(),
            (None, Some(e)) => e.clone(),
            (None, None) => format!("sem-dono-{n}"),
        };
        let motor = a.motor;
        match grupos.iter_mut().find(|(k, g)| *k == chave && g[0].motor == motor) {
            Some((_, g)) => g.push(a),
            None => grupos.push((chave, vec![a])),
        }
    }

    let mut contas = Vec::new();
    let mut usados: BTreeSet<String> = BTreeSet::new();
    for (key, grupo) in grupos {
        let motor = grupo[0].motor;
        let email = grupo.iter().find_map(|a| a.email.clone());
        // Apelidos: os do cofre do kit primeiro (são os da ordem dele), depois
        // os dados pelo Keep, depois o do e-mail.
        let mut aliases: Vec<String> = Vec::new();
        for a in grupo.iter().filter(|a| matches!(a.armazem, Armazem::Cofre { .. } | Armazem::Codex { .. })) {
            for ap in &a.apelidos {
                if !aliases.contains(ap) {
                    aliases.push(ap.clone());
                }
            }
        }
        for a in &grupo {
            for ap in &a.apelidos {
                if !aliases.contains(ap) {
                    aliases.push(ap.clone());
                }
            }
        }
        if aliases.is_empty() {
            let mut base = email.as_deref().map(apelido_de_email).unwrap_or_else(|| {
                if grupo.iter().any(|a| matches!(a.armazem, Armazem::Global)) { "global".into() } else { "conta".into() }
            });
            if usados.contains(&motor.chave(&base)) {
                let mut n = 2;
                while usados.contains(&motor.chave(&format!("{base}-{n}"))) {
                    n += 1;
                }
                base = format!("{base}-{n}");
            }
            aliases.push(base);
        }
        for a in &aliases {
            usados.insert(motor.chave(a));
        }
        let is_active = match motor {
            Motor::Claude => grupo.iter().any(|a| matches!(a.armazem, Armazem::Global)),
            Motor::Codex => grupo.iter().any(|a| matches!(a.armazem, Armazem::Codex { principal: true, .. })),
        };
        // Mostrada pelo nome que as abas conhecem: o slot ativo do kit, ou o
        // principal do GPT.
        let alias = match (motor, ativa) {
            (Motor::Claude, Some(at)) if is_active && aliases.iter().any(|a| a == at) => at.to_string(),
            (Motor::Codex, _) if is_active => "principal".to_string(),
            _ => aliases[0].clone(),
        };
        // O token mais novo mede; empate: global, fixa, cofre.
        let peso = |a: &Achado| match a.armazem {
            Armazem::Global | Armazem::Codex { principal: true, .. } => 3,
            Armazem::Fixa { .. } | Armazem::Codex { .. } => 2,
            Armazem::Cofre { .. } => 1,
        };
        let token = grupo
            .iter()
            .filter(|a| a.token.is_some() && a.aviso.is_none())
            .max_by_key(|a| (a.token.as_ref().and_then(|t| t.expira_em).unwrap_or(0), peso(a)))
            .or_else(|| grupo.iter().filter(|a| a.token.is_some()).max_by_key(|a| peso(a)))
            .and_then(|a| a.token.clone());
        let roda = grupo.iter().any(|a| a.roda);
        // Aviso só quando nenhum armazém dela serve: um login recusado num
        // e bom noutro é uma conta que funciona.
        let aceitavel = grupo.iter().any(|a| a.aviso.is_none() && (a.roda || a.token.is_some()));
        let warning = if aceitavel { None } else { grupo.iter().find_map(|a| a.aviso.clone()) };
        let plan = grupo.iter().find_map(|a| a.plano.clone());
        let armazens = grupo.iter().map(|a| a.armazem.clone()).collect();
        contas.push(Conta {
            engine: motor,
            key,
            order_key: motor.chave(&alias),
            alias,
            aliases,
            email,
            plan,
            is_active,
            is_preferred: false,
            warning,
            has_token: token.is_some(),
            armazens,
            token,
            roda,
        });
    }
    contas
}

// -------------------------------------------------------------------- ordem

/// A ordem como está no arquivo: uma chave por linha.
pub fn ler_ordem() -> Vec<String> {
    let Ok(texto) = std::fs::read_to_string(caminhos::ordem()) else { return Vec::new() };
    let mut vistos = BTreeSet::new();
    texto
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter(|l| vistos.insert(l.to_string()))
        .map(str::to_string)
        .collect()
}

/// As contas na ordem: a de cada uma é a menor posição entre os apelidos
/// dela; as que o arquivo não cita vêm depois, Claude antes de GPT, a global
/// e a principal primeiro.
pub fn ordenar(mut contas: Vec<Conta>, ordem: &[String]) -> Vec<Conta> {
    contas.sort_by(|a, b| {
        let motor = |c: &Conta| if c.engine == Motor::Claude { 0 } else { 1 };
        (motor(a), !a.is_active, &a.alias).cmp(&(motor(b), !b.is_active, &b.alias))
    });
    let posicao = |chave: &str| ordem.iter().position(|k| k == chave);
    let mut listadas: Vec<(usize, usize, Conta)> = Vec::new();
    let mut resto = Vec::new();
    for (n, mut conta) in contas.into_iter().enumerate() {
        let melhor = conta.chaves().into_iter().filter_map(|k| posicao(&k).map(|p| (p, k))).min();
        match melhor {
            Some((p, k)) => {
                conta.order_key = k;
                listadas.push((p, n, conta));
            }
            None => {
                conta.order_key = conta.engine.chave(&conta.alias);
                resto.push(conta);
            }
        }
    }
    listadas.sort_by_key(|(p, n, _)| (*p, *n));
    let mut saida: Vec<Conta> = listadas.into_iter().map(|(_, _, c)| c).chain(resto).collect();
    if let Some(primeira) = saida.iter_mut().find(|c| c.engine == Motor::Claude) {
        primeira.is_preferred = true;
    }
    saida
}

/// Todas as contas, na ordem de prioridade. `perguntar` deixa ir à rede para
/// saber de quem é um login ainda não visto (o resultado fica em cache).
pub fn listar(perguntar: bool) -> Vec<Conta> {
    let mut achados = slots_do_kit();
    achados.extend(armazens_claude(perguntar));
    achados.extend(armazens_codex());
    let ativa = ativa_do_kit();
    ordenar(juntar(achados, ativa.as_deref()), &ler_ordem())
}

/// A conta que uma chave nomeia.
pub fn achar<'a>(contas: &'a [Conta], chave: &str) -> Option<&'a Conta> {
    contas.iter().find(|c| c.e(chave))
}

/// A ordem efetiva: a chave de cada conta, da primeira à última.
pub fn ordem_efetiva(contas: &[Conta]) -> Vec<String> {
    contas.iter().map(|c| c.order_key.clone()).collect()
}

fn gravar_ordem(chaves: &[String]) -> std::io::Result<()> {
    let caminho = caminhos::ordem();
    if let Some(dir) = caminho.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut texto = String::from("# fila de prioridade das contas de IA (Keep); a 1a disponivel atende\n");
    for c in chaves {
        texto.push_str(c);
        texto.push('\n');
    }
    let tmp = caminho.with_extension("tmp");
    std::fs::write(&tmp, texto)?;
    std::fs::rename(&tmp, &caminho)
}

/// Troca uma conta de lugar com a vizinha e grava a ordem inteira. Subir a
/// primeira (ou descer a última) não muda nada e não é erro.
pub fn mover(chave: &str, para_cima: bool) -> Result<Vec<String>, String> {
    let contas = listar(false);
    let Some(i) = contas.iter().position(|c| c.e(chave)) else {
        return Err(format!("“{chave}” não é uma conta que o Keep conheça."));
    };
    let mut ordem = ordem_efetiva(&contas);
    let j = if para_cima { i.checked_sub(1) } else { Some(i + 1).filter(|j| *j < ordem.len()) };
    if let Some(j) = j {
        ordem.swap(i, j);
    }
    gravar_ordem(&ordem).map_err(|e| e.to_string())?;
    Ok(ordem)
}

/// Põe uma conta nova no fim da ordem (se ela ainda não está).
pub fn acrescentar_na_ordem(chave: &str) -> std::io::Result<()> {
    let contas = listar(false);
    let mut ordem = ordem_efetiva(&contas);
    if !ordem.iter().any(|k| k == chave) {
        ordem.push(chave.to_string());
    }
    gravar_ordem(&ordem)
}

// ----------------------------------------------------------------- rodar

/// Quem troca a conta do login global, quando não é o Keep: o kit do autor.
/// Com ele, "seguir a ordem" é o login global, e o Keep não move abas de um
/// Claude para outro (`docs/ia.md`, "Gerente externo").
pub fn gerente_externo() -> bool {
    match std::env::var("KEEP_IA_GERENTE_EXTERNO").as_deref() {
        Ok("1") => return true,
        Ok("0") => return false,
        _ => {}
    }
    caminhos::contas().join(".ativa").is_file()
        && caminhos::casa().join(".local/bin/claude-melhor-conta").is_file()
}

/// Por que uma conta não pode receber uma aba agora.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Falta {
    /// Nenhuma conta com essa chave.
    SemConta(String),
    /// A conta existe, mas não há login dela que uma aba possa usar: o login
    /// próprio dela precisa ser feito.
    PrecisaLogin { chave: String, apelido: String },
}

/// Se um login guardado pode rodar uma aba: tem o refresh que o CLI zera
/// quando o servidor recusa.
fn login_roda(lido: Lido) -> bool {
    matches!(lido, Lido::Ok(ref v) if Oauth::de(v).is_some_and(|o| !o.recusado()))
}

/// O ambiente de uma aba que roda nesta conta: nada para o login global ou o
/// GPT principal; a pasta fixa ou o `CODEX_HOME` para as outras. Lido na
/// hora, porque é para subir uma conversa agora.
///
/// Com um gerente externo (o kit), o login global troca de dono quando ele
/// quer: uma aba presa numa conta roda sempre na pasta própria dela.
pub fn ambiente(conta: &Conta) -> Result<Vec<(String, String)>, Falta> {
    match conta.engine {
        Motor::Claude => {
            // O global, quando roda, é a casa da conta.
            if conta.global() && !gerente_externo() && login_roda(credencial::global()) {
                return Ok(Vec::new());
            }
            for a in &conta.armazens {
                if let Armazem::Fixa { pasta } = a {
                    if login_roda(credencial::fixa(pasta)) {
                        return Ok(vec![(
                            "CLAUDE_SECURESTORAGE_CONFIG_DIR".into(),
                            pasta.to_string_lossy().into_owned(),
                        )]);
                    }
                }
            }
            Err(Falta::PrecisaLogin { chave: conta.order_key.clone(), apelido: conta.alias.clone() })
        }
        Motor::Codex => match conta.codex_home() {
            Some((_, true)) => Ok(Vec::new()),
            Some((home, false)) => Ok(vec![("CODEX_HOME".into(), home.to_string_lossy().into_owned())]),
            None => Err(Falta::SemConta(conta.order_key.clone())),
        },
    }
}

/// Uma pasta fixa nova, para o login de uma conta ainda sem armazém.
pub fn pasta_fixa_nova() -> PathBuf {
    let mut bytes = [0u8; 4];
    let agora = donos::agora_ms();
    bytes.copy_from_slice(&((agora as u32) ^ std::process::id().rotate_left(16)).to_be_bytes());
    let base = caminhos::fixas();
    let mut n = 0u32;
    loop {
        let nome = format!("k-{:08x}", u32::from_be_bytes(bytes).wrapping_add(n));
        let p = base.join(nome);
        if !p.exists() {
            return p;
        }
        n += 1;
    }
}

#[cfg(test)]
pub(crate) mod testes;
