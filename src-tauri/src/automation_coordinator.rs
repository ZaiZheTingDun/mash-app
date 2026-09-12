//! Process-wide ownership for mutually exclusive device automations.
//!
//! Individual runners still expose their detailed progress state. This small
//! coordinator answers a different question atomically: which workflow owns
//! the device from the instant a start command is accepted until its worker
//! exits?

use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AutomationKind {
    Battle,
    ServantEnhancement,
    CraftEssenceEnhancement,
    FriendPointSummon,
}

impl AutomationKind {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Battle => "战斗自动化",
            Self::ServantEnhancement => "从者强化自动化",
            Self::CraftEssenceEnhancement => "概念礼装强化自动化",
            Self::FriendPointSummon => "友情点抽取自动化",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct AutomationCoordinator {
    active: Arc<Mutex<Option<AutomationKind>>>,
}

impl AutomationCoordinator {
    pub(crate) fn reserve(&self, kind: AutomationKind) -> Result<AutomationLease, String> {
        let mut active = self.active.lock().unwrap();
        if let Some(current) = *active {
            return Err(format!("{}正在运行中，请先停止", current.label()));
        }
        *active = Some(kind);
        Ok(AutomationLease {
            coordinator: self.clone(),
            kind,
        })
    }

    pub(crate) fn require_idle(&self) -> Result<(), String> {
        match *self.active.lock().unwrap() {
            Some(kind) => Err(format!("{}正在运行中，请先停止", kind.label())),
            None => Ok(()),
        }
    }

    #[cfg(test)]
    fn active(&self) -> Option<AutomationKind> {
        *self.active.lock().unwrap()
    }
}

#[derive(Debug)]
pub(crate) struct AutomationLease {
    coordinator: AutomationCoordinator,
    kind: AutomationKind,
}

impl Drop for AutomationLease {
    fn drop(&mut self) {
        let mut active = self.coordinator.active.lock().unwrap();
        if *active == Some(self.kind) {
            *active = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reservation_is_exclusive_and_released_on_drop() {
        let coordinator = AutomationCoordinator::default();
        let lease = coordinator.reserve(AutomationKind::Battle).unwrap();

        assert_eq!(coordinator.active(), Some(AutomationKind::Battle));
        assert_eq!(
            coordinator
                .reserve(AutomationKind::ServantEnhancement)
                .unwrap_err(),
            "战斗自动化正在运行中，请先停止"
        );

        drop(lease);
        assert!(coordinator
            .reserve(AutomationKind::ServantEnhancement)
            .is_ok());
    }
}
