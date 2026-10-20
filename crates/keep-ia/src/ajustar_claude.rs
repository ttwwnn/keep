//! `keep ia ajustar-claude`: a statusLine e o gancho da conta da aba no
//! `settings.json` do Claude Code.
//!
//! A barra do Claude no Keep (modelo, esforço, agentes) só sabe o que a
//! statusLine lhe conta (`keep ia statusline`), e o Claude de uma aba trocada
//! a quente só diz a conta certa com o gancho `keep ia conta-da-aba --gancho`
//! (docs/ia.md). No Windows o app grava os dois ao abrir
//! (`apps/windows/src/claudeSettings.ts` e `claudeHooks.ts`); no Mac eles
//! estavam à mão no settings.json do autor, e um Mac novo ficava sem a barra e
//! sem o gancho (20/10/2026). O app do Mac roda este comando uma vez por
//! abertura, com as regras do Windows:
//!
//! - só onde a pasta do Claude Code existe: sem ela, ele ainda não rodou aqui,
//!   e nada é criado;
//! - uma statusLine que já é a deste `keep` fica; outra é da pessoa e não é
//!   tocada;
//! - o gancho entra depois dos que a pessoa tem; um gancho com
//!   `ia conta-da-aba --gancho` de outro lugar é dela e não é tocado;
//! - a statusLine ou o gancho de um Keep que mudou de lugar (o `keep` de
//!   dentro de um `.app` que não existe mais) passa para o caminho de agora;
//!   e nada é gravado quando o app roda de uma cópia temporária do macOS
//!   (App Translocation, o Keep aberto direto da pasta de downloads), cujo
//!   caminho some quando ele fecha;
//! - o resto do arquivo fica como estava, byte a byte, no recuo e nas quebras
//!   de linha dele, e o resultado é conferido: relido, tem de ser o mesmo mais
//!   o que entrou, senão nada é gravado. Arquivo que não é JSON, ou `hooks` de
//!   outro formato, também não é tocado;
//! - gravado só por cima do texto que foi lido (compare-and-swap), com a
//!   permissão que o arquivo tinha.
//!
//! Responde `{"ok": true, "statusLine": "…", "gancho": "…"}`, cada um com o
//! que foi feito: `gravada`/`gravado`, `ja-e-do-keep`, `outra`/`outro` (com o
//! `comando` que está lá), `sem-pasta`, `mudou` ou `nao-tocado` (com o
//! `porque`).

use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};

use crate::caminhos;
use crate::cli::{Args, recusa, responde};

/// O que vem depois do `keep` na statusLine.
pub const STATUSLINE_ARGS: &str = "ia statusline";
/// O que vem depois do `keep` no gancho, e o que diz que um gancho é deste
/// tipo, venha de onde vier.
pub const GANCHO_ARGS: &str = "ia conta-da-aba --gancho";
/// Um settings.json tem alguns kilobytes; passando disto, não é um.
const MAX_BYTES: u64 = 4 * 1024 * 1024;

/// O que fazer com o arquivo.
#[derive(Debug, PartialEq, Eq)]
pub enum Plano {
    /// O arquivo a gravar: o que está lá com o acréscimo, ou um novo só com ele.
    Gravar(String),
    /// Já está, como seria gravado.
    Nosso,
    /// Outro do mesmo tipo já está (o da pessoa, ou de outro lugar): fica.
    Outro(String),
    /// Arquivo que não se toca: não é JSON, não é um objeto, `hooks` de outro
    /// formato, ou um acréscimo que mexeria no resto.
    Ilegivel(String),
}

/// A pasta do Claude Code: `CLAUDE_CONFIG_DIR`, senão `~/.claude`, como o
/// próprio Claude Code a lê. Numa casa de teste (`KEEP_IA_HOME`), sempre a
/// dela.
pub fn pasta_do_claude() -> PathBuf {
    match std::env::var_os("CLAUDE_CONFIG_DIR") {
        Some(d) if !d.is_empty() && std::env::var_os("KEEP_IA_HOME").is_none() => PathBuf::from(d),
        _ => caminhos::claude(),
    }
}

/// O caminho como o shell do Claude Code o lê: como está, quando só tem
/// letras, números e `/._-+@%,:=`; entre aspas simples, senão. `None` para um
/// com aspas simples ou quebra de linha, que não se arrisca.
pub fn no_shell(caminho: &str) -> Option<String> {
    if caminho.is_empty() || caminho.contains(['\'', '\n', '\r']) {
        return None;
    }
    let simples = caminho.chars().all(|c| c.is_ascii_alphanumeric() || "/._-+@%,:=".contains(c));
    Some(if simples { caminho.to_string() } else { format!("'{caminho}'") })
}

/// Um comando deste tipo (`<keep> <args>`) cujo `keep`, o de dentro de um app
/// do Keep, não existe mais: o app mudou de lugar. É do Keep, e vai para o
/// caminho de agora.
fn de_um_keep_que_sumiu(comando: &str, args: &str) -> bool {
    let Some(programa) = comando.strip_suffix(args).map(str::trim_end) else { return false };
    let programa = programa.strip_prefix('\'').and_then(|p| p.strip_suffix('\'')).unwrap_or(programa);
    programa.ends_with(".app/Contents/Resources/keep") && !Path::new(programa).exists()
}

/// Um app aberto de uma cópia temporária do macOS (App Translocation): o
/// caminho dele some quando ele fecha.
fn copia_temporaria(keep: &str) -> bool {
    keep.contains("/AppTranslocation/")
}

// ---------------------------------------------------------------- o JSON do arquivo

fn sem_bom(texto: &str) -> &str {
    texto.strip_prefix('\u{feff}').unwrap_or(texto)
}

/// O objeto que o arquivo guarda; `None` para um que não é.
fn objeto(texto: &str) -> Option<Map<String, Value>> {
    match serde_json::from_str::<Value>(sem_bom(texto)) {
        Ok(Value::Object(m)) => Some(m),
        _ => None,
    }
}

/// Um valor como texto em que dois valores iguais se leem iguais: as chaves
/// de todo objeto em ordem.
fn canonico(v: &Value) -> String {
    fn ordena(v: &Value) -> Value {
        match v {
            Value::Object(m) => {
                let mut chaves: Vec<&String> = m.keys().collect();
                chaves.sort();
                Value::Object(chaves.into_iter().map(|k| (k.clone(), ordena(&m[k]))).collect())
            }
            Value::Array(a) => Value::Array(a.iter().map(ordena).collect()),
            _ => v.clone(),
        }
    }
    serde_json::to_string(&ordena(v)).unwrap_or_default()
}

/// Um valor a escrever, com as chaves na ordem dada (`type`, `command`,
/// `timeout`, como o Claude Code escreve).
enum J {
    Texto(String),
    Numero(u64),
    Objeto(Vec<(&'static str, J)>),
    Lista(Vec<J>),
}

impl J {
    fn valor(&self) -> Value {
        match self {
            J::Texto(t) => json!(t),
            J::Numero(n) => json!(n),
            J::Objeto(campos) => Value::Object(campos.iter().map(|(k, v)| (k.to_string(), v.valor())).collect()),
            J::Lista(itens) => Value::Array(itens.iter().map(J::valor).collect()),
        }
    }

    /// Numa linha só, sem espaços.
    fn junto(&self) -> String {
        match self {
            J::Texto(t) => json!(t).to_string(),
            J::Numero(n) => n.to_string(),
            J::Objeto(campos) => {
                let dentro: Vec<String> = campos.iter().map(|(k, v)| format!("{}:{}", json!(k), v.junto())).collect();
                format!("{{{}}}", dentro.join(","))
            }
            J::Lista(itens) => format!("[{}]", itens.iter().map(J::junto).collect::<Vec<_>>().join(",")),
        }
    }

    /// Um item por linha, recuado `unidade` a cada nível, as linhas de
    /// continuação debaixo de `base` (como o `JSON.stringify(v, null, unidade)`
    /// do Windows).
    fn bonito(&self, unidade: &str, base: &str, quebra: &str) -> String {
        let dentro = format!("{base}{unidade}");
        match self {
            J::Objeto(campos) if !campos.is_empty() => {
                let linhas: Vec<String> = campos
                    .iter()
                    .map(|(k, v)| format!("{dentro}{}: {}", json!(k), v.bonito(unidade, &dentro, quebra)))
                    .collect();
                format!("{{{quebra}{}{quebra}{base}}}", linhas.join(&format!(",{quebra}")))
            }
            J::Lista(itens) if !itens.is_empty() => {
                let linhas: Vec<String> =
                    itens.iter().map(|v| format!("{dentro}{}", v.bonito(unidade, &dentro, quebra))).collect();
                format!("[{quebra}{}{quebra}{base}]", linhas.join(&format!(",{quebra}")))
            }
            _ => self.junto(),
        }
    }
}

// ---------------------------------------------------------------- onde as coisas estão no texto

fn espaco(b: &[u8], de: usize) -> usize {
    let mut i = de;
    loop {
        match b.get(i) {
            Some(b' ' | b'\t' | b'\n' | b'\r') => i += 1,
            Some(0xEF) if b.get(i + 1) == Some(&0xBB) && b.get(i + 2) == Some(&0xBF) => i += 3,
            _ => return i,
        }
    }
}

/// Depois da string que abre em `de`.
fn fim_da_string(b: &[u8], de: usize) -> usize {
    let mut i = de + 1;
    while i < b.len() {
        match b[i] {
            b'\\' => i += 2,
            b'"' => return i + 1,
            _ => i += 1,
        }
    }
    b.len()
}

/// Depois do valor que começa em `de`. O texto é um que o JSON aceitou, e não
/// é conferido de novo: aqui só importa onde as coisas acabam.
fn fim_do_valor(b: &[u8], de: usize) -> usize {
    match b.get(de) {
        Some(b'"') => fim_da_string(b, de),
        Some(&abre @ (b'{' | b'[')) => {
            let fecha = if abre == b'{' { b'}' } else { b']' };
            let mut i = espaco(b, de + 1);
            if b.get(i) == Some(&fecha) {
                return i + 1;
            }
            loop {
                if abre == b'{' {
                    i = espaco(b, fim_da_string(b, i));
                    i = espaco(b, i + 1); // os dois-pontos
                }
                i = espaco(b, fim_do_valor(b, i));
                if b.get(i) != Some(&b',') {
                    return i + 1;
                }
                i = espaco(b, i + 1);
            }
        }
        _ => {
            let mut i = de;
            while i < b.len() && !b",}] \t\r\n".contains(&b[i]) {
                i += 1;
            }
            i
        }
    }
}

/// O valor raiz: o objeto que o arquivo inteiro é (início, fim exclusivo).
fn raiz(texto: &str) -> (usize, usize) {
    let b = texto.as_bytes();
    let inicio = espaco(b, 0);
    (inicio, fim_do_valor(b, inicio))
}

/// O valor de um membro do objeto em `obj`; `None` sem um com esse nome.
fn membro(texto: &str, obj: (usize, usize), chave: &str) -> Option<(usize, usize)> {
    let b = texto.as_bytes();
    let mut i = espaco(b, obj.0 + 1);
    while b.get(i) == Some(&b'"') {
        let fim_chave = fim_da_string(b, i);
        let inicio = espaco(b, espaco(b, fim_chave) + 1); // depois dos dois-pontos
        let fim = fim_do_valor(b, inicio);
        if serde_json::from_str::<String>(&texto[i..fim_chave]).ok().as_deref() == Some(chave) {
            return Some((inicio, fim));
        }
        i = espaco(b, fim);
        if b.get(i) != Some(&b',') {
            break;
        }
        i = espaco(b, i + 1);
    }
    None
}

// ---------------------------------------------------------------- acrescentar a um objeto ou lista

/// As quebras de linha do arquivo, e quanto ele recua.
fn estilo(texto: &str) -> (&'static str, String) {
    let quebra = if texto.contains("\r\n") { "\r\n" } else { "\n" };
    let unidade = regex::Regex::new(r#"\n([ \t]+)["\[{]"#)
        .ok()
        .and_then(|r| r.captures(texto))
        .map_or("  ".to_string(), |c| c[1].to_string());
    (quebra, unidade)
}

/// O texto com `item` (`"chave": valor`, ou só o valor, numa lista) como o
/// último do objeto ou lista em `onde`: depois do último que há, no recuo que
/// os outros têm, e todo o resto como estava. Numa linha só quando o objeto
/// ou lista está numa linha só.
fn acrescenta(texto: &str, onde: (usize, usize), item: impl Fn(bool, &str) -> String) -> String {
    let fecha = onde.1 - 1;
    let dentro = &texto[onde.0 + 1..fecha];
    let vazio = dentro.trim().is_empty();
    let cabeca = format!("{}{}", &texto[..onde.0 + 1], if vazio { "" } else { dentro.trim_end() });
    let virgula = if vazio { "" } else { "," };
    let cauda = &texto[fecha..];
    if !dentro.contains(['\r', '\n']) {
        return format!("{cabeca}{virgula}{}{cauda}", item(true, ""));
    }
    let (quebra, unidade) = estilo(texto);
    // A linha do fecha está no recuo do objeto; os itens, um passo adiante —
    // ou no dos que já estão lá.
    let fechando = regex::Regex::new(r"\n([ \t]*)$")
        .ok()
        .and_then(|r| r.captures(dentro))
        .map_or(String::new(), |c| c[1].to_string());
    let recuo = regex::Regex::new(r"\n([ \t]+)\S")
        .ok()
        .and_then(|r| r.captures(dentro))
        .map_or(format!("{fechando}{unidade}"), |c| c[1].to_string());
    format!("{cabeca}{virgula}{quebra}{recuo}{}{quebra}{fechando}{cauda}", item(false, &recuo))
}

fn como_membro<'a>(chave: &'a str, valor: &'a J, texto: &'a str) -> impl Fn(bool, &str) -> String + 'a {
    move |junto, base| {
        if junto {
            format!("{}:{}", json!(chave), valor.junto())
        } else {
            let (quebra, unidade) = estilo(texto);
            format!("{}: {}", json!(chave), valor.bonito(&unidade, base, quebra))
        }
    }
}

fn como_item<'a>(valor: &'a J, texto: &'a str) -> impl Fn(bool, &str) -> String + 'a {
    move |junto, base| {
        if junto {
            valor.junto()
        } else {
            let (quebra, unidade) = estilo(texto);
            valor.bonito(&unidade, base, quebra)
        }
    }
}

/// O acréscimo vale só se o arquivo relido é o `esperado`: o que havia, mais
/// o que entrou.
fn conferido(novo: String, esperado: &Value) -> Plano {
    match objeto(&novo).map(Value::Object) {
        Some(relido) if canonico(&relido) == canonico(esperado) => Plano::Gravar(novo),
        _ => Plano::Ilegivel("não deu para acrescentar sem mexer no resto".into()),
    }
}

/// O texto com o comando `velho` trocado pelo `novo`, onde o texto o escreve
/// (uma vez só, senão nada), e o resto como estava.
fn trocado(texto: &str, velho: &str, novo: &str, esperado: &Value) -> Plano {
    let antes = json!(velho).to_string();
    if texto.matches(&antes).count() != 1 {
        return Plano::Ilegivel(format!("o comando antigo ({velho}) não está uma vez só no texto"));
    }
    conferido(texto.replacen(&antes, &json!(novo).to_string(), 1), esperado)
}

// ---------------------------------------------------------------- os planos

/// O texto de um arquivo novo, só com `chave`.
fn arquivo_novo(chave: &str, valor: &J) -> String {
    format!("{{\n  {}: {}\n}}\n", json!(chave), valor.bonito("  ", "  ", "\n"))
}

/// O que fazer com um settings.json (o texto, ou `None` sem arquivo) para o
/// Claude Code rodar `comando` como statusLine: nada se ele já roda, ou se
/// roda outra; senão o texto com a `statusLine` acrescentada.
pub fn plano_statusline(texto: Option<&str>, comando: &str) -> Plano {
    let valor = J::Objeto(vec![("type", J::Texto("command".into())), ("command", J::Texto(comando.into()))]);
    let Some(texto) = texto.filter(|t| !sem_bom(t).trim().is_empty()) else {
        return Plano::Gravar(arquivo_novo("statusLine", &valor));
    };
    let Some(settings) = objeto(texto) else {
        return Plano::Ilegivel("não é um objeto JSON".into());
    };
    if let Some(atual) = settings.get("statusLine") {
        let dela = atual.get("command").and_then(Value::as_str);
        if dela == Some(comando) {
            return Plano::Nosso;
        }
        if let Some(velho) = dela.filter(|c| de_um_keep_que_sumiu(c, STATUSLINE_ARGS)) {
            let mut esperado = settings.clone();
            if let Some(o) = esperado.get_mut("statusLine").and_then(Value::as_object_mut) {
                o.insert("command".into(), json!(comando));
            }
            return trocado(texto, velho, comando, &Value::Object(esperado));
        }
        return Plano::Outro(dela.map_or_else(|| canonico(atual), String::from));
    }
    let novo = acrescenta(texto, raiz(texto), como_membro("statusLine", &valor, texto));
    let mut esperado = settings;
    esperado.insert("statusLine".into(), valor.valor());
    conferido(novo, &Value::Object(esperado))
}

/// Os comandos de todo gancho de um evento, onde quer que estejam nos grupos.
fn comandos(grupos: &[Value]) -> Vec<String> {
    grupos
        .iter()
        .filter_map(|g| g.get("hooks")?.as_array())
        .flatten()
        .filter_map(|h| h.get("command")?.as_str().map(String::from))
        .collect()
}

/// O que fazer com um settings.json para o gancho `comando` rodar no `evento`:
/// nada se ele já roda, ou se um com `assinatura` no comando já está; senão
/// um grupo com ele acrescentado à lista do evento — a lista criada se não
/// houver, o `hooks` criado se não houver —, depois do que a pessoa tem.
pub fn plano_gancho(texto: Option<&str>, evento: &'static str, comando: &str, prazo: u64, assinatura: &str) -> Plano {
    let grupo = J::Objeto(vec![(
        "hooks",
        J::Lista(vec![J::Objeto(vec![
            ("type", J::Texto("command".into())),
            ("command", J::Texto(comando.into())),
            ("timeout", J::Numero(prazo)),
        ])]),
    )]);
    let Some(texto) = texto.filter(|t| !sem_bom(t).trim().is_empty()) else {
        return Plano::Gravar(arquivo_novo("hooks", &J::Objeto(vec![(evento, J::Lista(vec![grupo]))])));
    };
    let Some(settings) = objeto(texto) else {
        return Plano::Ilegivel("não é um objeto JSON".into());
    };
    let mut esperado = settings.clone();
    let novo = match settings.get("hooks") {
        None => {
            let valor = J::Objeto(vec![(evento, J::Lista(vec![grupo]))]);
            esperado.insert("hooks".into(), valor.valor());
            acrescenta(texto, raiz(texto), como_membro("hooks", &valor, texto))
        }
        Some(Value::Object(ganchos)) => match ganchos.get(evento) {
            None => {
                let Some(onde) = membro(texto, raiz(texto), "hooks") else {
                    return Plano::Ilegivel("não achei hooks no arquivo".into());
                };
                let lista = J::Lista(vec![grupo]);
                let mut novos = ganchos.clone();
                novos.insert(evento.into(), lista.valor());
                esperado.insert("hooks".into(), Value::Object(novos));
                acrescenta(texto, onde, como_membro(evento, &lista, texto))
            }
            Some(Value::Array(grupos)) => {
                let ja = comandos(grupos);
                if ja.iter().any(|c| c == comando) {
                    return Plano::Nosso;
                }
                if let Some(velho) = ja.iter().find(|c| de_um_keep_que_sumiu(c, assinatura)) {
                    let mut lista = grupos.clone();
                    for h in lista.iter_mut().filter_map(|g| g.get_mut("hooks")?.as_array_mut()).flatten() {
                        if h.get("command").and_then(Value::as_str) == Some(velho.as_str()) {
                            h["command"] = json!(comando);
                        }
                    }
                    let mut novos = ganchos.clone();
                    novos.insert(evento.into(), Value::Array(lista));
                    esperado.insert("hooks".into(), Value::Object(novos));
                    return trocado(texto, velho, comando, &Value::Object(esperado));
                }
                if let Some(parecido) = ja.into_iter().find(|c| c.contains(assinatura)) {
                    return Plano::Outro(parecido);
                }
                let Some(onde) = membro(texto, raiz(texto), "hooks").and_then(|h| membro(texto, h, evento)) else {
                    return Plano::Ilegivel(format!("não achei hooks.{evento} no arquivo"));
                };
                let mut lista = grupos.clone();
                lista.push(grupo.valor());
                let mut novos = ganchos.clone();
                novos.insert(evento.into(), Value::Array(lista));
                esperado.insert("hooks".into(), Value::Object(novos));
                acrescenta(texto, onde, como_item(&grupo, texto))
            }
            Some(_) => return Plano::Ilegivel(format!("hooks.{evento} não é uma lista")),
        },
        Some(_) => return Plano::Ilegivel("hooks não é um objeto".into()),
    };
    conferido(novo, &Value::Object(esperado))
}

// ---------------------------------------------------------------- o arquivo

/// O texto do arquivo; `None` sem arquivo.
fn ler(arquivo: &Path) -> Result<Option<String>, String> {
    match std::fs::metadata(arquivo) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("não li {}: {e}", arquivo.display())),
        Ok(m) if m.len() > MAX_BYTES => {
            return Err(format!("{} é grande demais para ser um settings.json", arquivo.display()))
        }
        Ok(_) => {}
    }
    std::fs::read_to_string(arquivo).map(Some).map_err(|e| format!("não li {}: {e}", arquivo.display()))
}

/// Grava `texto` por cima do arquivo se ele ainda tem o `lido` (`None`: não
/// havia arquivo). `false`, e nada gravado, se ele mudou nesse meio-tempo. O
/// texto vai antes para um arquivo ao lado e é movido por cima, para o Claude
/// Code nunca ler metade dele; um link é seguido, não trocado; a permissão é
/// a que o arquivo tinha.
fn trocar(arquivo: &Path, lido: Option<&str>, texto: &str) -> Result<bool, String> {
    if ler(arquivo)?.as_deref() != lido {
        return Ok(false);
    }
    let alvo = std::fs::canonicalize(arquivo).unwrap_or_else(|_| arquivo.to_path_buf());
    let pasta = alvo.parent().ok_or("o settings.json não tem pasta")?;
    let lado = pasta.join(format!(".settings.json.keep-{}", std::process::id()));
    std::fs::write(&lado, texto).map_err(|e| format!("não gravei {}: {e}", lado.display()))?;
    if let Ok(m) = std::fs::metadata(&alvo) {
        let _ = std::fs::set_permissions(&lado, m.permissions());
    }
    std::fs::rename(&lado, &alvo).map_err(|e| {
        let _ = std::fs::remove_file(&lado);
        format!("não gravei {}: {e}", alvo.display())
    })?;
    Ok(true)
}

/// Aplica um plano ao arquivo e diz o que foi feito.
fn aplicar(arquivo: &Path, lido: Option<&str>, plano: Plano, feito: &str, outro: &str) -> Value {
    match plano {
        Plano::Gravar(texto) => match trocar(arquivo, lido, &texto) {
            Ok(true) => json!({ "feito": feito }),
            Ok(false) => {
                json!({ "feito": "mudou", "porque": "o arquivo mudou enquanto era lido: fica para a próxima abertura" })
            }
            Err(e) => json!({ "feito": "nao-tocado", "porque": e }),
        },
        Plano::Nosso => json!({ "feito": "ja-e-do-keep" }),
        Plano::Outro(comando) => json!({ "feito": outro, "comando": comando }),
        Plano::Ilegivel(porque) => json!({ "feito": "nao-tocado", "porque": porque }),
    }
}

/// `keep ia ajustar-claude [--keep=<caminho>]`: o `keep` que a statusLine e o
/// gancho rodam é este (o do app), salvo `--keep`.
pub fn cli(a: &Args) -> i32 {
    if cfg!(windows) {
        return responde(recusa(
            "so-no-mac",
            "no Windows o app grava a statusLine e o gancho ao abrir (claudeSettings.ts, claudeHooks.ts)",
        ));
    }
    let keep = match a.opcao("keep") {
        Some(k) if !k.is_empty() => k.to_string(),
        _ => match std::env::current_exe() {
            Ok(p) => p.display().to_string(),
            Err(e) => return responde(recusa("sem-keep", &format!("não sei onde este keep está: {e}"))),
        },
    };
    if copia_temporaria(&keep) {
        let nada = json!({
            "feito": "nao-tocado",
            "porque": format!("o app roda de uma cópia temporária do macOS ({keep}): mova o Keep para Aplicativos"),
        });
        return responde(json!({ "ok": true, "statusLine": nada, "gancho": nada }));
    }
    let Some(keep) = no_shell(&keep) else {
        return responde(recusa("caminho", &format!("o caminho do keep tem aspas ou quebra de linha: {keep}")));
    };
    let pasta = pasta_do_claude();
    let arquivo = pasta.join("settings.json");
    if !pasta.is_dir() {
        let sem = json!({ "feito": "sem-pasta", "porque": format!("sem {}: o Claude Code ainda não rodou aqui", pasta.display()) });
        return responde(json!({ "ok": true, "statusLine": sem, "gancho": sem }));
    }
    // A statusLine primeiro; o gancho sobre o arquivo como ela o deixou.
    let lido = match ler(&arquivo) {
        Ok(t) => t,
        Err(e) => return responde(recusa("leitura", &e)),
    };
    let plano = plano_statusline(lido.as_deref(), &format!("{keep} {STATUSLINE_ARGS}"));
    let statusline = aplicar(&arquivo, lido.as_deref(), plano, "gravada", "outra");
    let lido = match ler(&arquivo) {
        Ok(t) => t,
        Err(e) => return responde(recusa("leitura", &e)),
    };
    let plano = plano_gancho(lido.as_deref(), "UserPromptSubmit", &format!("{keep} {GANCHO_ARGS}"), 10, GANCHO_ARGS);
    let gancho = aplicar(&arquivo, lido.as_deref(), plano, "gravado", "outro");
    responde(
        json!({ "ok": true, "arquivo": arquivo.display().to_string(), "statusLine": statusline, "gancho": gancho }),
    )
}

#[cfg(test)]
mod testes {
    use super::*;

    const KEEP: &str = "/Applications/Keep.app/Contents/Resources/keep";

    fn status() -> String {
        format!("{KEEP} {STATUSLINE_ARGS}")
    }

    fn gancho() -> String {
        format!("{KEEP} {GANCHO_ARGS}")
    }

    fn texto(p: Plano) -> String {
        match p {
            Plano::Gravar(t) => t,
            outro => panic!("esperava Gravar, veio {outro:?}"),
        }
    }

    /// O arquivo do autor antes de 20/10 tirando a statusLine e o gancho: o
    /// recuo de 2, os ganchos dele em ordem.
    const DO_AUTOR: &str = r#"{
  "model": "opus[1m]",
  "hooks": {
    "SessionStart": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "/bin/bash /Users/x/.claude/hooks/patch-vscode-plan-bypass.sh",
            "timeout": 15
          }
        ]
      }
    ],
    "UserPromptSubmit": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "/opt/homebrew/bin/python3 /Users/x/.claude/skills/jev/ganchos.py modelo",
            "timeout": 20
          }
        ]
      }
    ]
  },
  "theme": "auto"
}
"#;

    #[test]
    fn statusline_entra_no_fim_e_o_resto_fica_byte_a_byte() {
        let novo = texto(plano_statusline(Some(DO_AUTOR), &status()));
        let fim = DO_AUTOR.rfind("\n}").unwrap();
        assert!(novo.starts_with(&DO_AUTOR[..fim]), "o começo mudou:\n{novo}");
        assert!(novo.ends_with(&format!(
            ",\n  \"statusLine\": {{\n    \"type\": \"command\",\n    \"command\": \"{}\"\n  }}\n}}\n",
            status()
        )));
        assert_eq!(plano_statusline(Some(&novo), &status()), Plano::Nosso);
    }

    #[test]
    fn statusline_de_outro_fica() {
        let dela = r#"{"statusLine": {"type": "command", "command": "~/minha-barra.sh"}}"#;
        assert_eq!(plano_statusline(Some(dela), &status()), Plano::Outro("~/minha-barra.sh".into()));
    }

    #[test]
    fn sem_arquivo_ou_vazio_vira_um_arquivo_so_com_ela() {
        let esperado = format!(
            "{{\n  \"statusLine\": {{\n    \"type\": \"command\",\n    \"command\": \"{}\"\n  }}\n}}\n",
            status()
        );
        assert_eq!(texto(plano_statusline(None, &status())), esperado);
        assert_eq!(texto(plano_statusline(Some(" \n"), &status())), esperado);
        assert_eq!(
            texto(plano_statusline(Some("{}"), &status())),
            format!("{{\"statusLine\":{{\"type\":\"command\",\"command\":\"{}\"}}}}", status())
        );
    }

    #[test]
    fn nao_json_nao_e_tocado() {
        for ruim in ["{\"a\": 1,}", "[1, 2]", "// nada\n{}"] {
            assert!(matches!(plano_statusline(Some(ruim), &status()), Plano::Ilegivel(_)), "{ruim}");
            assert!(
                matches!(plano_gancho(Some(ruim), "UserPromptSubmit", &gancho(), 10, GANCHO_ARGS), Plano::Ilegivel(_)),
                "{ruim}"
            );
        }
    }

    #[test]
    fn gancho_entra_depois_dos_da_pessoa_no_recuo_deles() {
        let novo = texto(plano_gancho(Some(DO_AUTOR), "UserPromptSubmit", &gancho(), 10, GANCHO_ARGS));
        let antes = "/opt/homebrew/bin/python3 /Users/x/.claude/skills/jev/ganchos.py modelo\",\n            \"timeout\": 20\n          }\n        ]\n      }";
        let grupo = format!(
            ",\n      {{\n        \"hooks\": [\n          {{\n            \"type\": \"command\",\n            \"command\": \"{}\",\n            \"timeout\": 10\n          }}\n        ]\n      }}\n    ]",
            gancho()
        );
        assert!(novo.contains(&format!("{antes}{grupo}")), "o grupo não entrou depois do Jev:\n{novo}");
        assert_eq!(novo.replace(&grupo, "\n    ]"), DO_AUTOR, "algo além do grupo mudou");
        assert_eq!(plano_gancho(Some(&novo), "UserPromptSubmit", &gancho(), 10, GANCHO_ARGS), Plano::Nosso);
    }

    #[test]
    fn gancho_de_outro_lugar_fica() {
        let dela = r#"{"hooks":{"UserPromptSubmit":[{"hooks":[{"type":"command","command":"/outro/keep ia conta-da-aba --gancho"}]}]}}"#;
        assert_eq!(
            plano_gancho(Some(dela), "UserPromptSubmit", &gancho(), 10, GANCHO_ARGS),
            Plano::Outro("/outro/keep ia conta-da-aba --gancho".into())
        );
    }

    #[test]
    fn gancho_cria_o_evento_e_o_hooks_que_faltam() {
        let sem_evento = "{\n  \"hooks\": {\n    \"Stop\": []\n  }\n}\n";
        let novo = texto(plano_gancho(Some(sem_evento), "UserPromptSubmit", &gancho(), 10, GANCHO_ARGS));
        let v: Value = serde_json::from_str(&novo).unwrap();
        assert_eq!(v["hooks"]["Stop"], json!([]));
        assert_eq!(v["hooks"]["UserPromptSubmit"][0]["hooks"][0]["command"], json!(gancho()));
        assert!(
            novo.starts_with("{\n  \"hooks\": {\n    \"Stop\": [],\n    \"UserPromptSubmit\": [\n      {\n"),
            "{novo}"
        );

        let sem_hooks = "{\r\n\t\"model\": \"x\"\r\n}";
        let novo = texto(plano_gancho(Some(sem_hooks), "UserPromptSubmit", &gancho(), 10, GANCHO_ARGS));
        assert!(
            novo.starts_with("{\r\n\t\"model\": \"x\",\r\n\t\"hooks\": {\r\n\t\t\"UserPromptSubmit\": [\r\n"),
            "{novo:?}"
        );
        assert!(!novo.replace("\r\n", "").contains('\n'), "quebra de linha fora do estilo do arquivo: {novo:?}");
        assert_eq!(plano_gancho(Some(&novo), "UserPromptSubmit", &gancho(), 10, GANCHO_ARGS), Plano::Nosso);
    }

    #[test]
    fn hooks_de_outro_formato_nao_e_tocado() {
        for ruim in [r#"{"hooks": []}"#, r#"{"hooks": {"UserPromptSubmit": {}}}"#] {
            assert!(
                matches!(plano_gancho(Some(ruim), "UserPromptSubmit", &gancho(), 10, GANCHO_ARGS), Plano::Ilegivel(_)),
                "{ruim}"
            );
        }
    }

    #[test]
    fn acrescimo_que_o_arquivo_relido_nao_confirma_nao_e_gravado() {
        // Chave repetida: o JSON vale a última, o texto acharia a primeira.
        let dobrado = r#"{"hooks": {"UserPromptSubmit": []}, "hooks": {"UserPromptSubmit": []}}"#;
        assert!(matches!(
            plano_gancho(Some(dobrado), "UserPromptSubmit", &gancho(), 10, GANCHO_ARGS),
            Plano::Ilegivel(_)
        ));
    }

    #[test]
    fn a_de_um_keep_que_mudou_de_lugar_vai_para_o_caminho_de_agora() {
        let sumiu = "/Users/x/Downloads/Keep.app/Contents/Resources/keep";
        let antes = DO_AUTOR.replace(
            "  \"theme\": \"auto\"\n}",
            &format!(
                "  \"theme\": \"auto\",\n  \"statusLine\": {{\"type\": \"command\", \"command\": \"{sumiu} {STATUSLINE_ARGS}\"}}\n}}"
            ),
        );
        let novo = texto(plano_statusline(Some(&antes), &status()));
        assert_eq!(novo, antes.replace(&format!("{sumiu} {STATUSLINE_ARGS}"), &status()));

        let com_gancho =
            texto(plano_gancho(Some(DO_AUTOR), "UserPromptSubmit", &format!("{sumiu} {GANCHO_ARGS}"), 10, GANCHO_ARGS));
        let novo = texto(plano_gancho(Some(&com_gancho), "UserPromptSubmit", &gancho(), 10, GANCHO_ARGS));
        assert_eq!(novo, com_gancho.replace(&format!("{sumiu} {GANCHO_ARGS}"), &gancho()));
    }

    #[test]
    fn a_de_um_keep_que_existe_noutro_lugar_fica() {
        let pasta = tempfile::tempdir().unwrap();
        let outro = pasta.path().join("Outro.app/Contents/Resources");
        std::fs::create_dir_all(&outro).unwrap();
        std::fs::write(outro.join("keep"), "").unwrap();
        let dele = format!("{} {STATUSLINE_ARGS}", outro.join("keep").display());
        let arquivo = format!("{{\"statusLine\": {{\"type\": \"command\", \"command\": {}}}}}", json!(dele));
        assert_eq!(plano_statusline(Some(&arquivo), &status()), Plano::Outro(dele));
    }

    #[test]
    fn caminho_no_shell() {
        assert_eq!(no_shell(KEEP).as_deref(), Some(KEEP));
        assert_eq!(no_shell("/Users/x/Aplicações/Keep.app/k").as_deref(), Some("'/Users/x/Aplicações/Keep.app/k'"));
        assert_eq!(no_shell("/a b/keep").as_deref(), Some("'/a b/keep'"));
        assert_eq!(no_shell("/a'b/keep"), None);
        assert_eq!(no_shell(""), None);
    }

    #[test]
    fn grava_por_cima_so_do_que_leu_e_mantem_a_permissao() {
        let pasta = tempfile::tempdir().unwrap();
        let arquivo = pasta.path().join("settings.json");
        std::fs::write(&arquivo, DO_AUTOR).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&arquivo, std::fs::Permissions::from_mode(0o600)).unwrap();
        }
        assert!(!trocar(&arquivo, Some("{}"), "{\"x\": 1}").unwrap(), "gravou por cima de um texto que não leu");
        assert_eq!(std::fs::read_to_string(&arquivo).unwrap(), DO_AUTOR);
        assert!(trocar(&arquivo, Some(DO_AUTOR), "{\"x\": 1}").unwrap());
        assert_eq!(std::fs::read_to_string(&arquivo).unwrap(), "{\"x\": 1}");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&arquivo).unwrap().permissions().mode() & 0o777, 0o600);
        }
        let sobras: Vec<_> = std::fs::read_dir(pasta.path()).unwrap().filter_map(Result::ok).collect();
        assert_eq!(sobras.len(), 1, "sobrou arquivo ao lado");
    }
}
