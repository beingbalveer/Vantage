//! UI-agnostic event sink. A front end can wire it to its own state updates;
//! the engine only knows this trait.

use std::sync::Arc;

pub type EventFn = Arc<dyn Fn(&str, serde_json::Value) + Send + Sync>;

#[derive(Clone, Default)]
pub struct Sink {
    inner: Option<EventFn>,
}

impl Sink {
    pub fn new(handler: impl Fn(&str, serde_json::Value) + Send + Sync + 'static) -> Self {
        Self {
            inner: Some(Arc::new(handler)),
        }
    }

    /// Emit a serializable payload; serialization failures are dropped because
    /// events are best-effort by design.
    pub fn emit(&self, event: &str, payload: impl serde::Serialize) {
        let Some(handler) = &self.inner else {
            return;
        };
        if let Ok(value) = serde_json::to_value(payload) {
            handler(event, value);
        }
    }
}
