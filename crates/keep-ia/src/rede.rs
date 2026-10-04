//! Perguntas HTTP, curtas e sem segredo nos registros.

use std::time::Duration;

/// O que um servidor respondeu.
pub struct Resposta {
    pub status: u16,
    /// O `Retry-After` de um 429, como veio.
    pub retry_after: Option<String>,
    pub corpo: Vec<u8>,
}

/// GET com cabeçalhos. `Err` só quando não houve resposta nenhuma (rede, DNS,
/// prazo); um 4xx ou 5xx é uma resposta e volta em `Ok`.
pub fn get(url: &str, cabecalhos: &[(&str, String)], prazo: Duration) -> Result<Resposta, String> {
    // O TLS do sistema (Security.framework, SChannel, OpenSSL) com as raízes
    // do sistema: um proxy corporativo confiável para o sistema vale aqui.
    let tls = ureq::tls::TlsConfig::builder()
        .provider(ureq::tls::TlsProvider::NativeTls)
        .root_certs(ureq::tls::RootCerts::PlatformVerifier)
        .build();
    let agente: ureq::Agent = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_global(Some(prazo))
        .tls_config(tls)
        .build()
        .into();
    let mut pedido = agente.get(url);
    for (nome, valor) in cabecalhos {
        pedido = pedido.header(*nome, valor.as_str());
    }
    let mut resposta = pedido.call().map_err(|e| e.to_string())?;
    let status = resposta.status().as_u16();
    let retry_after = resposta
        .headers()
        .get("retry-after")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let corpo = resposta.body_mut().read_to_vec().unwrap_or_default();
    Ok(Resposta { status, retry_after, corpo })
}

/// O endereço de um servidor de teste, se alguma das variáveis nomear um —
/// e só `http://127.0.0.1`: um valor esquecido no ambiente não pode mandar
/// um token de verdade para outro lugar.
pub fn url_de_teste(variaveis: &[&str], real: &str) -> String {
    for nome in variaveis {
        if let Ok(valor) = std::env::var(nome) {
            if let Some(resto) = valor.strip_prefix("http://127.0.0.1") {
                if resto.is_empty() || resto.starts_with(':') || resto.starts_with('/') {
                    return valor;
                }
            }
        }
    }
    real.to_string()
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn so_um_servidor_desta_maquina_substitui_o_real() {
        // SAFETY: o teste é o único a mexer nestas variáveis.
        unsafe {
            std::env::set_var("KEEP_IA_TESTE_URL_A", "http://127.0.0.1:9/x");
            std::env::set_var("KEEP_IA_TESTE_URL_B", "https://outro.lugar/x");
            std::env::set_var("KEEP_IA_TESTE_URL_C", "http://127.0.0.1.outro.lugar/x");
        }
        assert_eq!(url_de_teste(&["KEEP_IA_TESTE_URL_A"], "https://real"), "http://127.0.0.1:9/x");
        assert_eq!(url_de_teste(&["KEEP_IA_TESTE_URL_B"], "https://real"), "https://real");
        assert_eq!(url_de_teste(&["KEEP_IA_TESTE_URL_C"], "https://real"), "https://real");
        assert_eq!(url_de_teste(&["NAO_EXISTE"], "https://real"), "https://real");
    }
}
