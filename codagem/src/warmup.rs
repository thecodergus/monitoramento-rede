use std::collections::HashMap;

/// Estado de warmup de cada target.
/// Garante que só consideramos o target "up" após N ciclos de sucesso.
#[derive(Debug, Clone)]
pub struct TargetWarmupState {
    /// Contador de ciclos de sucesso por target_id
    success_streak: HashMap<i32, usize>,
    /// Número mínimo de ciclos de sucesso para considerar "up"
    required_streak: usize,
}

impl TargetWarmupState {
    /// Cria um novo estado de warmup com o streak mínimo desejado
    pub fn new(required_streak: usize) -> Self {
        Self {
            success_streak: HashMap::new(),
            required_streak,
        }
    }

    /// Atualiza o streak de sucesso para um target.
    /// Retorna true se o target já está "aquecido" (pronto para registrar outages).
    pub fn update(&mut self, target_id: i32, is_success: bool) -> bool {
        if is_success {
            let streak = self.success_streak.entry(target_id).or_insert(0);
            *streak += 1;
        } else {
            self.success_streak.insert(target_id, 0);
        }
        self.success_streak[&target_id] >= self.required_streak
    }

    /// Reseta o streak de um target (opcional, para controle fino)
    pub fn reset(&mut self, target_id: i32) {
        self.success_streak.insert(target_id, 0);
    }
}
