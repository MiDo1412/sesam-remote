//! Austauschbarer Ereigniskanal: eine laufende Sitzung kann in ein anderes Fenster wechseln
//! (Tab abkoppeln/andocken), ohne die Verbindung neu aufzubauen.

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};
use tauri::ipc::{Channel, InvokeResponseBody, IpcResponse};

pub struct Relay<T: IpcResponse>(Arc<Mutex<Channel<T>>>);

impl<T: IpcResponse> Clone for Relay<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T: IpcResponse> Relay<T> {
    pub fn new(c: Channel<T>) -> Self {
        Self(Arc::new(Mutex::new(c)))
    }

    pub fn send(&self, v: T) -> tauri::Result<()> {
        let c = self.0.lock().unwrap().clone();
        c.send(v)
    }

    /// Ab jetzt an diesen Kanal senden (neues Fenster).
    pub fn swap(&self, c: Channel<T>) {
        *self.0.lock().unwrap() = c;
    }
}

/// Kanäle der offenen Desktop-Sitzungen (VNC und RDP) und Shells, nach Sitzungs-ID.
pub static DESKTOPS: LazyLock<Mutex<HashMap<String, Relay<InvokeResponseBody>>>> = LazyLock::new(Default::default);
pub static SHELLS: LazyLock<Mutex<HashMap<String, Relay<crate::ssh::ShellEvent>>>> = LazyLock::new(Default::default);

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Abkoppeln/Andocken: nach dem Tausch kommen die Ereignisse nur noch im neuen Fenster an.
    #[test]
    fn swap_moves_events_to_new_window() {
        let (a, b) = (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
        let ch = |n: Arc<AtomicUsize>| {
            Channel::<InvokeResponseBody>::new(move |_| {
                n.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })
        };
        let r = Relay::new(ch(a.clone()));
        let copy = r.clone(); // so hält sie die laufende Sitzung
        copy.send(InvokeResponseBody::Raw(vec![1])).unwrap();
        r.swap(ch(b.clone()));
        copy.send(InvokeResponseBody::Raw(vec![2])).unwrap();
        copy.send(InvokeResponseBody::Raw(vec![3])).unwrap();
        assert_eq!((a.load(Ordering::SeqCst), b.load(Ordering::SeqCst)), (1, 2));
    }
}
