//! Que processos estão por trás de cada aba: o shell dela e o processo da
//! frente (o que segura o terminal agora — o shell, no prompt).
//!
//! O daemon novo diz isso na lista (`List3`): vínculo `exato`. O antigo não
//! diz, e o daemon sobrevive a cada atualização do app; para ele, o
//! vínculo é deduzido dos processos (`provavel`): serve para LER a conta e a
//! conversa de uma aba, nunca para digitar nela.

use std::collections::HashMap;

use crate::daemon::Retrato;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Vinculo {
    pub shell: u32,
    /// O processo da frente; o próprio shell no prompt.
    pub frente: u32,
    pub exato: bool,
}

/// O vínculo de cada aba viva, por (workspace, aba).
pub fn vincular(r: &Retrato) -> HashMap<(String, u32), Vinculo> {
    if r.exato {
        return r
            .abas
            .iter()
            .filter(|a| !a.info.finished && a.info.pid != 0)
            .map(|a| {
                let shell = if a.info.shell_pid != 0 { a.info.shell_pid } else { a.info.pid };
                ((a.ws.clone(), a.info.id), Vinculo { shell, frente: a.info.pid, exato: true })
            })
            .collect();
    }
    provavel(r)
}

/// O vínculo deduzido para um daemon sem `List3`: só os pares únicos.
fn provavel(_r: &Retrato) -> HashMap<(String, u32), Vinculo> {
    HashMap::new()
}
