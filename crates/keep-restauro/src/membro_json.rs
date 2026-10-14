//! Um membro da raiz de um JSON de configuração (o `settings.json` do Claude Code), posto, trocado ou tirado NO
//! TEXTO: o resto do arquivo fica byte a byte como a pessoa o deixou (ordem das chaves, indentação, fim de linha, BOM).
//! Reescrever pelo `serde_json` reordenaria tudo.
//!
//! Toda edição é conferida depois: o texto novo tem de ler como o antigo com só aquele membro diferente. Se não der
//! para garantir, devolve erro e nada é gravado.

use serde_json::{Map, Value};

const BOM: char = '\u{feff}';

#[derive(Debug)]
struct Membro {
    chave: String,
    /// Onde a aspa da chave abre.
    inicio: usize,
    valor_ini: usize,
    valor_fim: usize,
}

fn espaco(b: &[u8], mut i: usize) -> usize {
    while i < b.len() && matches!(b[i], b' ' | b'\t' | b'\n' | b'\r') {
        i += 1;
    }
    i
}

/// Depois da string que abre em `i`.
fn fim_da_string(b: &[u8], i: usize) -> usize {
    let mut j = i + 1;
    while j < b.len() {
        match b[j] {
            b'\\' => j += 2,
            b'"' => return j + 1,
            _ => j += 1,
        }
    }
    b.len()
}

/// Depois do valor que começa em `i` (o texto já foi lido como JSON: aqui só se acha onde as coisas acabam).
fn fim_do_valor(b: &[u8], i: usize) -> usize {
    match b[i] {
        b'"' => fim_da_string(b, i),
        b'{' | b'[' => {
            let mut fundo = 0usize;
            let mut j = i;
            while j < b.len() {
                match b[j] {
                    b'"' => {
                        j = fim_da_string(b, j);
                        continue;
                    }
                    b'{' | b'[' => fundo += 1,
                    b'}' | b']' => {
                        fundo -= 1;
                        if fundo == 0 {
                            return j + 1;
                        }
                    }
                    _ => {}
                }
                j += 1;
            }
            b.len()
        }
        _ => {
            let mut j = i;
            while j < b.len() && !matches!(b[j], b',' | b'}' | b']' | b' ' | b'\t' | b'\n' | b'\r') {
                j += 1;
            }
            j
        }
    }
}

/// Os membros da raiz e onde ficam a `{` e a `}` dela. O texto tem de ser um objeto JSON (sem o BOM).
fn varre(texto: &str) -> Result<(usize, usize, Vec<Membro>), String> {
    if !matches!(serde_json::from_str::<Value>(texto), Ok(Value::Object(_))) {
        return Err("não é um objeto JSON".into());
    }
    let b = texto.as_bytes();
    let abre = espaco(b, 0);
    let mut i = espaco(b, abre + 1);
    let mut membros = Vec::new();
    while b.get(i) == Some(&b'"') {
        let fim_chave = fim_da_string(b, i);
        let chave: String = serde_json::from_str(&texto[i..fim_chave]).map_err(|e| e.to_string())?;
        let valor_ini = espaco(b, espaco(b, fim_chave) + 1);
        let valor_fim = fim_do_valor(b, valor_ini);
        membros.push(Membro { chave, inicio: i, valor_ini, valor_fim });
        i = espaco(b, valor_fim);
        if b.get(i) != Some(&b',') {
            break;
        }
        i = espaco(b, i + 1);
    }
    Ok((abre, i, membros))
}

fn objeto(texto: &str) -> Result<Map<String, Value>, String> {
    match serde_json::from_str::<Value>(texto) {
        Ok(Value::Object(m)) => Ok(m),
        _ => Err("não é um objeto JSON".into()),
    }
}

fn sem_bom(texto: &str) -> (&str, &str) {
    match texto.strip_prefix(BOM) {
        Some(resto) => ("\u{feff}", resto),
        None => ("", texto),
    }
}

/// Só um membro por nome: com repetição, qual vale depende de quem lê.
fn acha<'a>(membros: &'a [Membro], chave: &str) -> Result<Option<&'a Membro>, String> {
    let mut iguais = membros.iter().filter(|m| m.chave == chave);
    let primeiro = iguais.next();
    if iguais.next().is_some() {
        return Err(format!("a chave {chave:?} aparece mais de uma vez"));
    }
    Ok(primeiro)
}

/// O texto do valor do membro, como está no arquivo; `None` se não há o membro.
pub fn valor_bruto(texto: &str, chave: &str) -> Result<Option<String>, String> {
    let (_, texto) = sem_bom(texto);
    let (_, _, membros) = varre(texto)?;
    Ok(acha(&membros, chave)?.map(|m| texto[m.valor_ini..m.valor_fim].to_string()))
}

fn confere(novo: &str, esperado: Map<String, Value>) -> Result<(), String> {
    match objeto(novo) {
        Ok(lido) if lido == esperado => Ok(()),
        _ => Err("não deu para editar sem mexer no resto do arquivo".into()),
    }
}

fn quebra(texto: &str) -> &'static str {
    if texto.contains("\r\n") { "\r\n" } else { "\n" }
}

/// O arquivo com o membro `chave` valendo `bruto` (um valor JSON): trocado se existe, acrescentado ao fim se não.
/// `texto` `None`: não há arquivo, e sai um novo só com o membro.
pub fn define(texto: Option<&str>, chave: &str, bruto: &str) -> Result<String, String> {
    let valor: Value = serde_json::from_str(bruto).map_err(|e| format!("valor inválido: {e}"))?;
    let chave_json = serde_json::to_string(chave).map_err(|e| e.to_string())?;
    let Some(inteiro) = texto.filter(|t| !t.trim_matches(|c: char| c.is_whitespace() || c == BOM).is_empty()) else {
        return Ok(format!("{{\n  {chave_json}: {bruto}\n}}\n"));
    };
    let (bom, texto) = sem_bom(inteiro);
    let (abre, fecha, membros) = varre(texto)?;
    let mut esperado = objeto(texto)?;
    esperado.insert(chave.to_string(), valor);
    let novo = match acha(&membros, chave)? {
        Some(m) => format!("{}{bruto}{}", &texto[..m.valor_ini], &texto[m.valor_fim..]),
        None => {
            let nl = quebra(texto);
            match membros.last() {
                None => format!("{}{nl}  {chave_json}: {bruto}{nl}{}", &texto[..=abre], &texto[fecha..]),
                Some(ultimo) => {
                    // Na indentação do primeiro membro, se cada um está numa linha; senão, na mesma linha.
                    let primeiro = &membros[0];
                    let antes = &texto[abre + 1..primeiro.inicio];
                    let separador = match antes.rfind('\n') {
                        Some(i) => format!("{nl}{}", &antes[i + 1..]),
                        None => String::new(),
                    };
                    let espaco_depois_dos_dois_pontos = if texto[primeiro.inicio..primeiro.valor_ini].contains(": ") { " " } else { "" };
                    format!(
                        "{},{separador}{chave_json}:{espaco_depois_dos_dois_pontos}{bruto}{}",
                        &texto[..ultimo.valor_fim],
                        &texto[ultimo.valor_fim..]
                    )
                }
            }
        }
    };
    confere(&novo, esperado)?;
    Ok(format!("{bom}{novo}"))
}

/// O arquivo sem o membro `chave` (igual a ele se não há).
pub fn tira(texto: &str, chave: &str) -> Result<String, String> {
    let (bom, corpo) = sem_bom(texto);
    let (abre, fecha, membros) = varre(corpo)?;
    let Some(m) = acha(&membros, chave)? else { return Ok(texto.to_string()) };
    let mut esperado = objeto(corpo)?;
    esperado.remove(chave);
    let pos = membros.iter().position(|x| std::ptr::eq(x, m)).unwrap_or(0);
    let (de, ate) = if membros.len() == 1 {
        (abre + 1, fecha)
    } else if let Some(proximo) = membros.get(pos + 1) {
        (m.inicio, proximo.inicio)
    } else {
        (membros[pos - 1].valor_fim, m.valor_fim)
    };
    let novo = format!("{}{}", &corpo[..de], &corpo[ate..]);
    confere(&novo, esperado)?;
    Ok(format!("{bom}{novo}"))
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn arquivo_novo() {
        assert_eq!(define(None, "k", "\"disable\"").unwrap(), "{\n  \"k\": \"disable\"\n}\n");
        assert_eq!(define(Some("  \n"), "k", "1").unwrap(), "{\n  \"k\": 1\n}\n");
    }

    #[test]
    fn acrescenta_no_estilo_do_arquivo() {
        let multi = "{\n    \"a\": 1,\n    \"b\": {\n        \"c\": [1, 2]\n    }\n}\n";
        let novo = define(Some(multi), "k", "\"v\"").unwrap();
        assert_eq!(novo, "{\n    \"a\": 1,\n    \"b\": {\n        \"c\": [1, 2]\n    },\n    \"k\": \"v\"\n}\n");
        let uma = "{\"a\":1}";
        assert_eq!(define(Some(uma), "k", "\"v\"").unwrap(), "{\"a\":1,\"k\":\"v\"}");
        let uma_com_espaco = "{ \"a\": 1 }";
        assert_eq!(define(Some(uma_com_espaco), "k", "true").unwrap(), "{ \"a\": 1,\"k\": true }");
        assert_eq!(define(Some("{}"), "k", "1").unwrap(), "{\n  \"k\": 1\n}");
    }

    #[test]
    fn fim_de_linha_e_bom_do_arquivo_ficam() {
        let crlf = "\u{feff}{\r\n  \"a\": 1\r\n}\r\n";
        let novo = define(Some(crlf), "k", "2").unwrap();
        assert_eq!(novo, "\u{feff}{\r\n  \"a\": 1,\r\n  \"k\": 2\r\n}\r\n");
        assert_eq!(tira(&novo, "k").unwrap(), crlf);
    }

    #[test]
    fn troca_o_valor_e_devolve_o_antigo_por_valor_bruto() {
        let t = "{\n  \"a\": 1,\n  \"k\": [1,\n 2],\n  \"z\": \"fim\"\n}\n";
        assert_eq!(valor_bruto(t, "k").unwrap().as_deref(), Some("[1,\n 2]"));
        assert_eq!(valor_bruto(t, "nao").unwrap(), None);
        assert_eq!(define(Some(t), "k", "\"disable\"").unwrap(), "{\n  \"a\": 1,\n  \"k\": \"disable\",\n  \"z\": \"fim\"\n}\n");
        // Voltar ao valor de antes devolve o arquivo como era.
        let trocado = define(Some(t), "k", "\"disable\"").unwrap();
        assert_eq!(define(Some(&trocado), "k", "[1,\n 2]").unwrap(), t);
    }

    #[test]
    fn tira_em_cada_posicao() {
        let t = "{\n  \"a\": 1,\n  \"k\": \"v\",\n  \"z\": 2\n}\n";
        assert_eq!(tira(t, "k").unwrap(), "{\n  \"a\": 1,\n  \"z\": 2\n}\n");
        assert_eq!(tira(t, "a").unwrap(), "{\n  \"k\": \"v\",\n  \"z\": 2\n}\n");
        assert_eq!(tira(t, "z").unwrap(), "{\n  \"a\": 1,\n  \"k\": \"v\"\n}\n");
        assert_eq!(tira("{\"k\": 1}", "k").unwrap(), "{}");
        assert_eq!(tira("{\n  \"k\": 1\n}\n", "k").unwrap(), "{}\n");
        assert_eq!(tira(t, "nao-tem").unwrap(), t);
    }

    #[test]
    fn acrescentar_e_tirar_devolve_o_arquivo() {
        for t in [
            "{\n  \"a\": 1\n}\n",
            "{\"a\":{\"b\":[1,2,{\"c\":\"}\"}]},\"s\":\"a\\\"b,}\"}",
            "{ \"env\": { \"X\": \"1\" }, \"hooks\": {} }",
            "{\n\t\"a\": null,\n\t\"b\": true\n}",
        ] {
            let com = define(Some(t), "k", "\"disable\"").unwrap();
            assert_ne!(com, t);
            assert_eq!(tira(&com, "k").unwrap(), t, "{t}");
        }
    }

    #[test]
    fn chaves_e_texto_com_chaves_dentro_de_string_nao_enganam() {
        let t = "{\"a\": \"k\", \"b\": \"{\\\"k\\\": 1}\", \"k\": 5}";
        assert_eq!(valor_bruto(t, "k").unwrap().as_deref(), Some("5"));
        assert_eq!(valor_bruto(t, "a").unwrap().as_deref(), Some("\"k\""));
        assert_eq!(tira(t, "k").unwrap(), "{\"a\": \"k\", \"b\": \"{\\\"k\\\": 1}\"}");
    }

    #[test]
    fn o_que_nao_e_objeto_ou_tem_chave_repetida_nao_e_editado() {
        for t in ["[1]", "\"x\"", "{", "{\"a\":}", "nada", "{\"a\":1,}"] {
            assert!(define(Some(t), "k", "1").is_err(), "{t}");
            assert!(tira(t, "k").is_err(), "{t}");
        }
        assert!(define(Some("{\"k\":1,\"k\":2}"), "k", "3").is_err());
        assert!(tira("{\"k\":1,\"k\":2}", "k").is_err());
        assert!(define(Some("{}"), "k", "{").is_err(), "valor inválido");
    }

    #[test]
    fn nada_acontece_com_o_resto() {
        let t = "{\n  // não é JSON\n}";
        assert!(define(Some(t), "k", "1").is_err());
    }
}
