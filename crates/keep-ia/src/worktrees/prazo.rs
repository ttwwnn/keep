//! O prazo de uma resposta: o `listar` responde antes dele de qualquer jeito,
//! e o que não coube vira aviso.

use std::time::{Duration, Instant};

/// O prazo acabou: quem chamou transforma o que faltou em aviso.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Estourou;

/// `fim`: o prazo duro (`Estourou`). `rapido`: o alvo de resposta — o git que
/// passa dele sai do registro que o `indexar` grava.
#[derive(Clone, Copy, Debug)]
pub struct Prazo {
    fim: Option<Instant>,
    rapido: Option<Instant>,
}

fn daqui_a(seg: f64) -> Instant {
    Instant::now() + Duration::from_secs_f64(seg.clamp(0.0, 1e9))
}

impl Prazo {
    pub fn de(seg: f64) -> Prazo {
        Prazo { fim: Some(daqui_a(seg)), rapido: None }
    }

    pub fn com_alvo(seg: f64, rapido: f64) -> Prazo {
        Prazo { fim: Some(daqui_a(seg)), rapido: Some(daqui_a(rapido)) }
    }

    pub fn sem() -> Prazo {
        Prazo { fim: None, rapido: None }
    }

    /// Segundos até o prazo duro (infinito sem prazo; negativo depois dele).
    pub fn restante(&self) -> f64 {
        match self.fim {
            None => f64::INFINITY,
            Some(f) => segundos_ate(f),
        }
    }

    /// Segundos até o alvo rápido (ou até o prazo, sem alvo).
    pub fn ate_o_alvo(&self) -> f64 {
        let r = self.restante();
        match self.rapido {
            None => r,
            Some(a) => r.min(segundos_ate(a)),
        }
    }

    pub fn checa(&self) -> Result<(), Estourou> {
        if self.restante() <= 0.0 {
            Err(Estourou)
        } else {
            Ok(())
        }
    }
}

fn segundos_ate(t: Instant) -> f64 {
    let agora = Instant::now();
    if t >= agora {
        (t - agora).as_secs_f64()
    } else {
        -(agora - t).as_secs_f64()
    }
}
