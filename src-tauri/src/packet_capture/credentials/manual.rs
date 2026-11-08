//! Pending manual authorization is bound to one cancellable start operation.
use super::{candidate, OperationCancellation};
use std::sync::{Arc, Mutex};

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AuthorizationPrompt {
    request_id: String,
    serial: String,
    message: String,
    accepting_input: bool,
}

struct Pending {
    prompt: AuthorizationPrompt,
    cancellation: OperationCancellation,
    key: Option<String>,
}

#[derive(Clone, Default)]
pub(crate) struct AuthorizationStore(Arc<Mutex<Option<Pending>>>);

pub(super) struct AuthorizationSession {
    store: AuthorizationStore,
    id: String,
}

impl AuthorizationStore {
    pub(super) fn begin(
        &self,
        serial: &str,
        cancellation: &OperationCancellation,
    ) -> AuthorizationSession {
        let id = uuid::Uuid::new_v4().to_string();
        *self.0.lock().unwrap() = Some(Pending {
            prompt: AuthorizationPrompt {
                request_id: id.clone(),
                serial: serial.into(),
                message: "请复制或手动输入 Reqable API key".into(),
                accepting_input: true,
            },
            cancellation: cancellation.clone(),
            key: None,
        });
        AuthorizationSession {
            store: self.clone(),
            id,
        }
    }

    pub(crate) fn prompt(&self) -> Option<AuthorizationPrompt> {
        self.0
            .lock()
            .unwrap()
            .as_ref()
            .filter(|pending| !pending.cancellation.is_cancelled())
            .map(|pending| pending.prompt.clone())
    }

    pub(crate) fn submit(&self, request_id: &str, key: &str) -> Result<(), String> {
        let key = candidate(key).ok_or("API key 格式无效，请检查后重试")?;
        let mut pending = self.0.lock().unwrap();
        let pending = pending
            .as_mut()
            .filter(|pending| pending.prompt.request_id == request_id)
            .ok_or("API key 请求已结束，请重新开始抓包")?;
        pending.cancellation.check()?;
        if pending.key.is_some() || !pending.prompt.accepting_input {
            return Err("正在验证 API key，请稍候".into());
        }
        pending.key = Some(key.into());
        pending.prompt.accepting_input = false;
        pending.prompt.message = "已收到 API key，正在验证…".into();
        Ok(())
    }
}

impl AuthorizationSession {
    pub(super) fn verifying(&self) {
        if let Some(pending) = self
            .store
            .0
            .lock()
            .unwrap()
            .as_mut()
            .filter(|pending| pending.prompt.request_id == self.id)
        {
            pending.prompt.message = "正在验证 API key…".into();
            pending.prompt.accepting_input = false;
        }
    }
    pub(super) fn take_key(&self) -> Option<String> {
        self.store
            .0
            .lock()
            .unwrap()
            .as_mut()
            .filter(|pending| pending.prompt.request_id == self.id)
            .and_then(|pending| pending.key.take())
    }
    pub(super) fn feedback(&self, message: &str) {
        if let Some(pending) = self
            .store
            .0
            .lock()
            .unwrap()
            .as_mut()
            .filter(|pending| pending.prompt.request_id == self.id)
        {
            pending.prompt.message = message.into();
            pending.prompt.accepting_input = true;
        }
    }
}

impl Drop for AuthorizationSession {
    fn drop(&mut self) {
        let mut pending = self.store.0.lock().unwrap();
        if pending
            .as_ref()
            .is_some_and(|pending| pending.prompt.request_id == self.id)
        {
            *pending = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pending_keys_are_private_scoped_and_cancelled_with_the_start() {
        let store = AuthorizationStore::default();
        let cancellation = OperationCancellation::default();
        let session = store.begin("device-a", &cancellation);
        let prompt = store.prompt().unwrap();
        assert!(store.submit("stale", "test-secret-key").is_err());
        store.submit(&prompt.request_id, "test-secret-key").unwrap();
        assert!(!serde_json::to_string(&store.prompt())
            .unwrap()
            .contains("test-secret-key"));
        assert_eq!(session.take_key().as_deref(), Some("test-secret-key"));
        cancellation.cancel();
        assert!(store.prompt().is_none());
        assert!(store.submit(&prompt.request_id, "test-secret-key").is_err());
        drop(session);
        assert!(store.prompt().is_none());
    }
    #[test]
    fn stale_session_cleanup_cannot_clear_a_new_request() {
        let store = AuthorizationStore::default();
        let old = store.begin("device-a", &OperationCancellation::default());
        let new = store.begin("device-b", &OperationCancellation::default());
        drop(old);
        assert_eq!(store.prompt().unwrap().serial, "device-b");
        drop(new);
        assert!(store.prompt().is_none());
    }
}
