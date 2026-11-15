//! Previews by GNOME's previewer, Sushi, as Files shows them with the space bar: over the window,
//! its arrow keys moving through the list. Without it, a file opens in its app

use gtk::{gio, glib, prelude::*};
use std::path::Path;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

/// The previewer, and the handle of the window it was last asked to show over
pub struct Previewer {
    proxy: gio::DBusProxy,
    parent: Arc<Mutex<String>>,
}

/// The previewer, which D-Bus starts when first asked. `step` takes its arrow keys: -1 for up or
/// left, 1 for down or right; only while it shows over this window, as it sends them to every
/// program, Files too
pub fn previewer(step: impl Fn(i32) + Send + Sync + 'static) -> Option<Previewer> {
    let flags = gio::DBusProxyFlags::DO_NOT_AUTO_START_AT_CONSTRUCTION;
    let (name, path) = ("org.gnome.NautilusPreviewer", "/org/gnome/NautilusPreviewer");
    let proxy = gio::DBusProxy::for_bus_sync(
        gio::BusType::Session,
        flags,
        None,
        name,
        path,
        "org.gnome.NautilusPreviewer2",
        gio::Cancellable::NONE,
    )
    .ok()?;
    let parent = Arc::new(Mutex::new(String::new()));
    let ours = parent.clone();
    proxy.connect_g_signal(None, move |proxy, _, signal, params| {
        let shown_over = proxy.cached_property("ParentHandle").and_then(|h| h.get::<String>());
        if shown_over.is_none_or(|h| h.is_empty() || h != *ours.lock().unwrap()) {
            return;
        }
        // GTK's directions: up, down, left, right
        match (signal, params.child_value(0).get::<u32>()) {
            ("SelectionEvent", Some(2 | 4)) => step(-1),
            ("SelectionEvent", Some(3 | 5)) => step(1),
            _ => {}
        }
    });
    Some(Previewer { proxy, parent })
}

/// Shows a file over the window; hides it instead if `toggle` and it is shown
pub fn show(previewer: &Previewer, window: &gtk::Window, file: &Path, toggle: bool) {
    let (proxy, parent) = (previewer.proxy.clone(), previewer.parent.clone());
    let (w, file) = (window.clone(), file.to_owned());
    handle(window, move |handle| {
        *parent.lock().unwrap() = handle.to_owned();
        let params = (gio::File::for_path(&file).uri().as_str(), handle, toggle, "").to_variant();
        let (w, file) = (w.clone(), file.clone());
        let none = gio::Cancellable::NONE;
        proxy.call("ShowFile", Some(&params), gio::DBusCallFlags::NONE, -1, none, move |r| {
            if r.is_err() {
                super::open_file(&file, false, &w);
            }
        });
    });
}

/// The window's handle for another program, `x11:<XID>` or `wayland:<handle>`, or else none
fn handle(window: &gtk::Window, then: impl Fn(&str) + 'static) {
    let surface = window.surface();
    if let Some(x11) = surface.as_ref().and_then(|s| s.downcast_ref::<gdk_x11::X11Surface>()) {
        return then(&format!("x11:{:x}", x11.xid()));
    }
    let then = Rc::new(then);
    let t = then.clone();
    let exported = surface.and_then(|s| s.downcast::<gdk_wayland::WaylandToplevel>().ok()).is_some_and(|w| {
        w.export_handle(move |_, h: Result<&str, glib::BoolError>| {
            t(&h.map(|h| format!("wayland:{h}")).unwrap_or_default())
        })
    });
    if !exported {
        then("");
    }
}
