//! Process-wide ownership shared by device automation and maintenance.
//! Each operation holds a lease from acceptance until its worker exits.

use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AutomationKind {
    Battle,
    ServantEnhancement,
    CraftEssenceEnhancement,
    FriendPointSummon,
}

impl AutomationKind {
    fn label(self) -> &'static str {
        match self {
            Self::Battle => "战斗自动化",
            Self::ServantEnhancement => "从者强化自动化",
            Self::CraftEssenceEnhancement => "概念礼装强化自动化",
            Self::FriendPointSummon => "友情点抽取自动化",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DeviceOperation {
    Automation(AutomationKind),
    ResetResolution,
}

impl DeviceOperation {
    fn busy_message(self) -> String {
        match self {
            Self::Automation(kind) => format!("{}正在运行中，请先停止", kind.label()),
            Self::ResetResolution => "设备分辨率正在重置，请稍后重试".into(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct DeviceOperationCoordinator {
    active: Arc<Mutex<Option<DeviceOperation>>>,
}

impl DeviceOperationCoordinator {
    pub(crate) fn reserve(
        &self,
        operation: DeviceOperation,
    ) -> Result<DeviceOperationLease, String> {
        let mut active = self.active.lock().unwrap();
        if let Some(current) = *active {
            return Err(current.busy_message());
        }
        *active = Some(operation);
        Ok(DeviceOperationLease {
            coordinator: self.clone(),
            operation,
        })
    }

    pub(crate) fn require_idle(&self) -> Result<(), String> {
        match *self.active.lock().unwrap() {
            Some(operation) => Err(operation.busy_message()),
            None => Ok(()),
        }
    }

    #[cfg(test)]
    fn active(&self) -> Option<DeviceOperation> {
        *self.active.lock().unwrap()
    }
}

#[derive(Debug)]
pub(crate) struct DeviceOperationLease {
    coordinator: DeviceOperationCoordinator,
    operation: DeviceOperation,
}

impl Drop for DeviceOperationLease {
    fn drop(&mut self) {
        let mut active = self.coordinator.active.lock().unwrap();
        if *active == Some(self.operation) {
            *active = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reservation_is_exclusive_and_released_on_drop() {
        let coordinator = DeviceOperationCoordinator::default();
        let lease = coordinator
            .reserve(DeviceOperation::Automation(AutomationKind::Battle))
            .unwrap();

        assert_eq!(
            coordinator.active(),
            Some(DeviceOperation::Automation(AutomationKind::Battle))
        );
        assert_eq!(
            coordinator
                .reserve(DeviceOperation::Automation(
                    AutomationKind::ServantEnhancement
                ))
                .unwrap_err(),
            "战斗自动化正在运行中，请先停止"
        );

        drop(lease);
        assert!(coordinator
            .reserve(DeviceOperation::Automation(
                AutomationKind::ServantEnhancement
            ))
            .is_ok());
    }

    #[test]
    fn resolution_reset_and_automation_cannot_run_together() {
        let coordinator = DeviceOperationCoordinator::default();
        let other_caller = coordinator.clone();
        let reset = coordinator
            .reserve(DeviceOperation::ResetResolution)
            .unwrap();
        assert_eq!(
            other_caller.require_idle().unwrap_err(),
            "设备分辨率正在重置，请稍后重试"
        );
        for kind in [
            AutomationKind::Battle,
            AutomationKind::ServantEnhancement,
            AutomationKind::CraftEssenceEnhancement,
            AutomationKind::FriendPointSummon,
        ] {
            assert_eq!(
                other_caller
                    .reserve(DeviceOperation::Automation(kind))
                    .unwrap_err(),
                "设备分辨率正在重置，请稍后重试"
            );
        }
        assert!(coordinator
            .reserve(DeviceOperation::ResetResolution)
            .is_err());
        drop(reset);
        assert!(other_caller.require_idle().is_ok());
        let battle = other_caller
            .reserve(DeviceOperation::Automation(AutomationKind::Battle))
            .unwrap();
        assert!(coordinator
            .reserve(DeviceOperation::ResetResolution)
            .is_err());
        drop(battle);
        assert!(coordinator
            .reserve(DeviceOperation::ResetResolution)
            .is_ok());
    }
}
