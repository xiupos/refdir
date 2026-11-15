//! GNOME Shell's search of the library (`refdir-gtk --search-provider`): a service of its own, which
//! D-Bus starts when Shell searches and which ends when unused, so that searching never makes the
//! window. A result is an entry, by its key. Choosing it shows it in the app, and "more" searches
//! there: the app's actions `show-entry` and `search`, which D-Bus starts the app for

use gtk::{gio, glib, prelude::*};
use refdir::{Entry, Library};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

const BUS: &str = "net.xiupos.Refdir.SearchProvider";
const PATH: &str = "/net/xiupos/Refdir/SearchProvider";
const INTERFACE: &str = r#"<node><interface name="org.gnome.Shell.SearchProvider2">
<method name="GetInitialResultSet"><arg type="as" direction="in"/><arg type="as" direction="out"/></method>
<method name="GetSubsearchResultSet"><arg type="as" direction="in"/><arg type="as" direction="in"/><arg type="as" direction="out"/></method>
<method name="GetResultMetas"><arg type="as" direction="in"/><arg type="aa{sv}" direction="out"/></method>
<method name="ActivateResult"><arg type="s" direction="in"/><arg type="as" direction="in"/><arg type="u" direction="in"/></method>
<method name="LaunchSearch"><arg type="as" direction="in"/><arg type="u" direction="in"/></method>
</interface></node>"#;

pub fn run() -> glib::ExitCode {
    let app = gio::Application::new(Some(BUS), gio::ApplicationFlags::IS_SERVICE);
    app.set_inactivity_timeout(10_000);
    app.connect_startup(|app| {
        let info = gio::DBusNodeInfo::for_xml(INTERFACE)
            .unwrap()
            .lookup_interface("org.gnome.Shell.SearchProvider2");
        let connection = app.dbus_connection().expect("on the session bus");
        // The library as read when a search begins: narrowing it searches the same
        let library = Rc::new(RefCell::new(Library::default()));
        let app = app.downgrade();
        let call = move |connection: gio::DBusConnection,
                         _: Option<&str>,
                         _: &str,
                         _: Option<&str>,
                         method: &str,
                         params: glib::Variant,
                         invocation: gio::DBusMethodInvocation| {
            let _hold = app.upgrade().map(|a| a.hold());
            let strings = |i: usize| params.child_value(i).get::<Vec<String>>().unwrap_or_default();
            let reply = match method {
                "GetInitialResultSet" => {
                    *library.borrow_mut() = read();
                    Some(found(&library.borrow(), None, &strings(0)).to_variant())
                }
                "GetSubsearchResultSet" => {
                    Some(found(&library.borrow(), Some(&strings(0)), &strings(1)).to_variant())
                }
                "GetResultMetas" => Some(metas(&library.borrow(), &strings(0)).to_variant()),
                "ActivateResult" => {
                    app_action(
                        &connection,
                        "show-entry",
                        &params.child_value(0).get::<String>().unwrap_or_default(),
                    );
                    None
                }
                _ => {
                    app_action(&connection, "search", &strings(0).join(" "));
                    None
                }
            };
            invocation.return_value(reply.map(|r| glib::Variant::tuple_from_iter([r])).as_ref());
        };
        connection
            .register_object(PATH, &info.unwrap())
            .method_call(call)
            .build()
            .expect("the search's object");
    });
    app.run_with_args(&["refdir-gtk"])
}

/// Activates an action of the app, with a text
fn app_action(connection: &gio::DBusConnection, action: &str, text: &str) {
    let params = (action, vec![text.to_variant()], HashMap::<String, glib::Variant>::new()).to_variant();
    let (bus, path, interface) = (super::APP_ID, "/net/xiupos/Refdir", "org.freedesktop.Application");
    let none = gio::Cancellable::NONE;
    connection.call(
        Some(bus),
        path,
        interface,
        "ActivateAction",
        Some(&params),
        None,
        gio::DBusCallFlags::NONE,
        -1,
        none,
        |r| {
            if let Err(e) = r {
                eprintln!("refdir-gtk: cannot reach the app: {e}");
            }
        },
    );
}

/// The library in the app's folder; none if it has none, or cannot be read
fn read() -> Library {
    super::folder(&super::settings()).and_then(|f| Library::read(&f).ok()).unwrap_or_default()
}

/// The keys of the entries that have every term, among `among` if given
fn found(library: &Library, among: Option<&[String]>, terms: &[String]) -> Vec<String> {
    let search = terms.join(" ");
    let entries =
        library.entries.iter().filter(|e| among.is_none_or(|keys| keys.iter().any(|k| k == e.key())));
    entries.filter(|e| e.matches(&search)).map(|e| e.key().to_owned()).collect()
}

/// What Shell shows of each result: the title, and the first author, the year, and the key
fn metas(library: &Library, keys: &[String]) -> Vec<HashMap<String, glib::Variant>> {
    let meta = |e: &Entry| {
        let name = Some(e.title()).filter(|t| !t.is_empty()).unwrap_or(e.key().to_owned());
        let about: Vec<String> =
            [e.byline()].into_iter().chain(e.year()).chain([e.key().to_owned()]).collect();
        HashMap::from([
            ("id".to_owned(), e.key().to_variant()),
            ("name".to_owned(), name.to_variant()),
            (
                "description".to_owned(),
                about.into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(", ").to_variant(),
            ),
        ])
    };
    keys.iter().filter_map(|k| library.find(k)).map(meta).collect()
}
