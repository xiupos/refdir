//! `refdir-gtk`: a library of references for GNOME, in three panes as GNOME's mail apps have them:
//! the places (all entries, and each tag), the place's entries, and the entry chosen

mod entry;

use adw::prelude::*;
use entry::EntryPane;
use gtk::gio;
use refdir::{Bib, Entry, Library, fetch};
use relm4::actions::{AccelsPlus, RelmAction, RelmActionGroup};
use relm4::factory::{DynamicIndex, FactoryComponent, FactorySender, FactoryVecDeque};
use relm4::prelude::*;
use relm4::typed_view::list::{RelmListItem, TypedListView};
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

const APP_ID: &str = "net.xiupos.Refdir";

relm4::new_action_group!(WindowActions, "win");
relm4::new_stateless_action!(AddAction, WindowActions, "add");
relm4::new_stateless_action!(SearchAction, WindowActions, "search");
relm4::new_stateless_action!(ChooseFolderAction, WindowActions, "choose-folder");
relm4::new_stateless_action!(ExportAction, WindowActions, "export");
relm4::new_stateless_action!(AboutAction, WindowActions, "about");
relm4::new_stateless_action!(QuitAction, WindowActions, "quit");

pub fn boxed_list() -> gtk::ListBox {
    gtk::ListBox::builder().selection_mode(gtk::SelectionMode::None).css_classes(["boxed-list"]).build()
}

/// A section's heading
pub fn heading(title: &str) -> gtk::Label {
    gtk::Label::builder().label(title).xalign(0.0).css_classes(["heading"]).build()
}

/// Opens a file in the app the user has for it, or shows it in its folder
pub fn open_file(path: &Path, in_folder: bool, parent: &impl IsA<gtk::Widget>) {
    let window = parent.root().and_downcast::<gtk::Window>();
    let launcher = gtk::FileLauncher::new(Some(&gio::File::for_path(path)));
    if in_folder {
        launcher.open_containing_folder(window.as_ref(), gio::Cancellable::NONE, |_| {});
    } else {
        launcher.launch(window.as_ref(), gio::Cancellable::NONE, |_| {});
    }
}

/// The app's settings: its installed schema, or else the one `build.rs` compiles, so that the app
/// runs from `cargo run` too
fn settings() -> gio::Settings {
    let installed = gio::SettingsSchemaSource::default();
    let schema = installed.as_ref().and_then(|s| s.lookup(APP_ID, true)).or_else(|| {
        let built = gio::SettingsSchemaSource::from_directory(env!("SCHEMA_DIR"), installed.as_ref(), false);
        built.ok()?.lookup(APP_ID, false)
    });
    gio::Settings::new_full(&schema.expect("the app's settings schema"), None::<&gio::SettingsBackend>, None)
}

/// The library's folder: the setting, or else `$REFDIR`, as the command reads it
fn folder(settings: &gio::Settings) -> Option<PathBuf> {
    let set = PathBuf::from(settings.string("folder").as_str());
    Some(set).filter(|f| !f.as_os_str().is_empty()).or_else(|| std::env::var_os("REFDIR").map(PathBuf::from))
}

/// A place of the sidebar: all entries, or a tag's, with how many entries it has
struct PlaceRow {
    tag: Option<String>,
    count: usize,
}

#[relm4::factory]
impl FactoryComponent for PlaceRow {
    type Init = (Option<String>, usize);
    type Input = ();
    type Output = ();
    type CommandOutput = ();
    type ParentWidget = gtk::ListBox;

    view! {
        gtk::ListBoxRow {
            // The kind of row, for the sidebar's heading
            set_widget_name: if self.tag.is_some() { "tag" } else { "all" },
            gtk::Box {
                set_spacing: 12,
                gtk::Image {
                    set_icon_name: Some("user-bookmarks-symbolic"),
                    set_visible: self.tag.is_none(),
                },
                gtk::Label {
                    set_label: self.tag.as_deref().unwrap_or("All Entries"),
                    set_xalign: 0.0,
                    set_hexpand: true,
                    set_ellipsize: gtk::pango::EllipsizeMode::End,
                },
                gtk::Label {
                    set_label: &self.count.to_string(),
                    add_css_class: "dim-label",
                    add_css_class: "numeric",
                },
            },
        }
    }

    fn init_model((tag, count): Self::Init, _: &DynamicIndex, _: FactorySender<Self>) -> Self {
        PlaceRow { tag, count }
    }
}

/// An entry of the list, with what its row shows and what finding looks in, made once
struct EntryItem {
    entry: Entry,
    title: String,
    subtitle: String,
    pdf: bool,
    text: String,
}

impl EntryItem {
    fn new(entry: &Entry) -> Self {
        let subtitle = [Some(entry.byline()), entry.year()].into_iter().flatten().filter(|s| !s.is_empty());
        EntryItem {
            entry: entry.clone(),
            title: entry.title(),
            subtitle: subtitle.collect::<Vec<_>>().join(" · "),
            pdf: entry.pdf().is_some(),
            text: entry.search_text(),
        }
    }
}

struct EntryWidgets {
    title: gtk::Label,
    subtitle: gtk::Label,
    pdf: gtk::Image,
}

impl RelmListItem for EntryItem {
    type Root = gtk::Box;
    type Widgets = EntryWidgets;

    fn setup(_: &gtk::ListItem) -> (gtk::Box, EntryWidgets) {
        relm4::view! {
            root = gtk::Box {
                set_spacing: 12,
                set_margin_top: 8,
                set_margin_bottom: 8,
                set_margin_start: 6,
                set_margin_end: 6,
                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 2,
                    set_hexpand: true,
                    #[name = "title"]
                    gtk::Label {
                        set_xalign: 0.0,
                        set_wrap: true,
                        set_lines: 3,
                        set_ellipsize: gtk::pango::EllipsizeMode::End,
                    },
                    #[name = "subtitle"]
                    gtk::Label {
                        set_xalign: 0.0,
                        set_ellipsize: gtk::pango::EllipsizeMode::End,
                        add_css_class: "dim-label",
                        add_css_class: "caption",
                    },
                },
                #[name = "pdf"]
                gtk::Image {
                    set_icon_name: Some("x-office-document-symbolic"),
                    set_tooltip_text: Some("Has a PDF"),
                },
            }
        }
        (root, EntryWidgets { title, subtitle, pdf })
    }

    fn bind(&mut self, w: &mut EntryWidgets, _: &mut gtk::Box) {
        w.title.set_label(&self.title);
        w.subtitle.set_label(&self.subtitle);
        w.pdf.set_visible(self.pdf);
    }
}

/// What the list shows: the place's entries that the field finds
#[derive(Default)]
struct Find {
    tag: Option<String>,
    search: String,
}

impl Find {
    fn in_place(&self, entry: &Entry) -> bool {
        self.tag.as_ref().is_none_or(|t| entry.tags().contains(t))
    }
}

struct App {
    settings: gio::Settings,
    folder: Option<PathBuf>,
    /// The entries, with their files, as last read: a reload rebuilds the lists only if they changed
    read: Vec<(Entry, Vec<PathBuf>)>,
    tags: Vec<(String, usize)>,
    find: Rc<RefCell<Find>>,
    places: FactoryVecDeque<PlaceRow>,
    list: TypedListView<EntryItem, gtk::SingleSelection>,
    /// The entry pane's entry, by its folder
    shown: Option<(Controller<EntryPane>, PathBuf)>,
    /// The folders watched, and their monitors: a change in one reloads the library
    watched: Vec<PathBuf>,
    monitors: Vec<gio::FileMonitor>,
}

#[derive(Debug)]
enum Msg {
    Reload,
    ChooseFolder,
    /// The folder in the settings changed
    Folder,
    About,
    Toast(String),
    /// A row of the sidebar: all entries, then the tags
    Place(usize),
    OpenTag(String),
    /// The field's text
    Filter(String),
    /// A row of the list, by its place in what is shown
    Open(u32),
    Export,
    Exported(PathBuf),
    FocusSearch,
    Add,
    /// What the user pasted to add
    Fetch(String),
}

#[derive(Debug)]
enum Cmd {
    Fetched(Result<Box<Bib>, String>),
}

impl App {
    fn entries(&self) -> impl Iterator<Item = &Entry> {
        self.read.iter().map(|(e, _)| e)
    }

    /// The list's page: no folder, no entries, none found, or the list
    fn page(&self) -> &'static str {
        let find = self.find.borrow();
        match () {
            _ if self.folder.is_none() => "none",
            _ if !self.entries().any(|e| find.in_place(e)) => "empty",
            _ if self.list.selection_model.n_items() == 0 => "no-results",
            _ => "list",
        }
    }

    /// Where an entry is in what the list shows
    fn shown_at(&self, dir: &Path) -> u32 {
        let shown = self.list.selection_model.n_items();
        let at = (0..shown).find(|&p| self.list.get_visible(p).is_some_and(|e| e.borrow().entry.dir == dir));
        at.unwrap_or(gtk::INVALID_LIST_POSITION)
    }

    /// Reads the folder, and rebuilds the lists only if it changed, or if `force`
    fn reload(&mut self, widgets: &AppWidgets, sender: &ComponentSender<Self>, force: bool) {
        self.watch(sender);
        let library = self.folder.as_deref().and_then(|f| Library::read(f).ok()).unwrap_or_default();
        let read: Vec<(Entry, Vec<PathBuf>)> = library
            .entries
            .into_iter()
            .map(|e| {
                let files = e.files();
                (e, files)
            })
            .collect();
        if !force && read == self.read {
            return;
        }
        self.read = read;
        self.tags = refdir::tags(self.entries());
        let mut find = self.find.borrow_mut();
        if find.tag.as_ref().is_some_and(|t| !self.tags.iter().any(|(tag, _)| tag == t)) {
            find.tag = None;
        }
        let place = find.tag.as_ref().and_then(|t| self.tags.iter().position(|(tag, _)| tag == t));
        drop(find);
        let mut places = self.places.guard();
        places.clear();
        places.push_back((None, self.read.len()));
        self.tags.iter().for_each(|(t, n)| drop(places.push_back((Some(t.clone()), *n))));
        drop(places);
        // The factory names a row after inserting it: the headings are made again from the names
        widgets.places.invalidate_headers();
        widgets.places.select_row(widgets.places.row_at_index(place.map_or(0, |i| i as i32 + 1)).as_ref());
        self.list.clear();
        self.list.extend_from_iter(self.read.iter().map(|(e, _)| EntryItem::new(e)));
        // The entry shown is updated, and stays selected; one moved to the trash is closed
        let shown = self.shown.as_ref().map(|(_, dir)| dir.clone());
        match shown.and_then(|dir| self.entries().find(|e| e.dir == dir).cloned()) {
            Some(entry) => {
                self.list.selection_model.set_selected(self.shown_at(&entry.dir));
                if let Some((pane, _)) = &self.shown {
                    pane.emit(entry::Msg::Update(Box::new(entry)));
                }
            }
            None if self.shown.is_some() => {
                self.shown = None;
                widgets.entry_page.set_child(Some(&placeholder()));
            }
            None => {}
        }
    }

    /// Watches the library's folder and every folder in it, if they are not watched yet: an app
    /// makes an entry's folder before its `entry.bib`, which the folder's monitor then sees
    fn watch(&mut self, sender: &ComponentSender<Self>) {
        let Some(folder) = &self.folder else { return self.monitors.clear() };
        let mut dirs = vec![folder.clone()];
        let subfolders = std::fs::read_dir(folder).into_iter().flatten().flatten().map(|e| e.path());
        dirs.extend(subfolders.filter(|p| p.is_dir()));
        dirs.sort();
        if dirs == self.watched {
            return;
        }
        let monitor = |dir: &PathBuf| {
            let file = gio::File::for_path(dir);
            let monitor = file.monitor_directory(gio::FileMonitorFlags::NONE, gio::Cancellable::NONE).ok()?;
            let s = sender.clone();
            monitor.connect_changed(move |_, _, _, _| s.input(Msg::Reload));
            Some(monitor)
        };
        self.monitors = dirs.iter().filter_map(monitor).collect();
        self.watched = dirs;
    }

    /// Shows an entry in the entry pane, unless it shows it already
    fn open(&mut self, widgets: &AppWidgets, dir: &Path, sender: &ComponentSender<Self>) {
        let Some(entry) = self.entries().find(|e| e.dir == dir).cloned() else { return };
        if self.shown.as_ref().is_none_or(|(_, shown)| shown != dir) {
            let pane =
                EntryPane::builder().launch(entry).forward(sender.input_sender(), |output| match output {
                    entry::Output::Toast(text) => Msg::Toast(text),
                    entry::Output::Changed => Msg::Reload,
                    entry::Output::OpenTag(tag) => Msg::OpenTag(tag),
                });
            widgets.entry_page.set_child(Some(pane.widget()));
            // The entry shown before is dropped, which writes its note
            self.shown = Some((pane, dir.to_owned()));
        }
        self.list.selection_model.set_selected(self.shown_at(dir));
        widgets.inner.set_show_content(true);
    }

    fn show_list(&self, widgets: &AppWidgets) {
        widgets.outer.set_show_content(true);
        widgets.inner.set_show_content(false);
    }
}

fn placeholder() -> adw::ToolbarView {
    relm4::view! {
        view = adw::ToolbarView {
            add_top_bar = &adw::HeaderBar {
                set_show_title: false,
            },
            #[wrap(Some)]
            set_content = &adw::StatusPage {
                set_icon_name: Some("x-office-document-symbolic"),
                set_title: "No Entry Selected",
            },
        }
    }
    view
}

fn toast(overlay: &adw::ToastOverlay, text: &str) {
    overlay.add_toast(adw::Toast::builder().title(text).priority(adw::ToastPriority::High).build());
}

#[relm4::component]
impl Component for App {
    type Init = ();
    type Input = Msg;
    type Output = ();
    type CommandOutput = Cmd;

    view! {
        #[root]
        window = adw::ApplicationWindow {
            set_title: Some("refdir"),
            set_default_size: (1280, 800),
            #[name = "toasts"]
            adw::ToastOverlay {
                #[name = "outer"]
                adw::NavigationSplitView {
                    set_min_sidebar_width: 200.0,
                    set_max_sidebar_width: 260.0,
                    set_show_content: true,
                    #[wrap(Some)]
                    set_sidebar = &adw::NavigationPage {
                        set_title: "refdir",
                        #[wrap(Some)]
                        set_child = &adw::ToolbarView {
                            add_top_bar = &adw::HeaderBar {
                                pack_end = &gtk::MenuButton {
                                    set_icon_name: "open-menu-symbolic",
                                    set_tooltip_text: Some("Main Menu"),
                                    set_menu_model: Some(&main_menu),
                                },
                            },
                            #[wrap(Some)]
                            set_content = &gtk::ScrolledWindow {
                                set_hscrollbar_policy: gtk::PolicyType::Never,
                                #[local_ref]
                                places -> gtk::ListBox {
                                    add_css_class: "navigation-sidebar",
                                    // A heading over the first tag
                                    set_header_func: |row, before| {
                                        let first = row.widget_name() == "tag"
                                            && before.is_none_or(|b| b.widget_name() != "tag");
                                        let label = gtk::Label::builder().label("Tags").xalign(0.0).margin_top(12).margin_start(12);
                                        row.set_header(first.then(|| label.css_classes(["heading", "dim-label"]).build()).as_ref());
                                    },
                                    connect_row_activated[sender] => move |_, row| sender.input(Msg::Place(row.index() as usize)),
                                },
                            },
                        },
                    },
                    #[wrap(Some)]
                    set_content = &adw::NavigationPage {
                        set_title: "refdir",
                        #[wrap(Some)]
                        set_child: inner = &adw::NavigationSplitView {
                            set_min_sidebar_width: 300.0,
                            set_max_sidebar_width: 440.0,
                            set_sidebar_width_fraction: 0.4,
                            #[wrap(Some)]
                            set_sidebar = &adw::NavigationPage {
                                #[watch]
                                set_title: model.find.borrow().tag.as_deref().unwrap_or("All Entries"),
                                #[wrap(Some)]
                                set_child = &adw::ToolbarView {
                                    add_top_bar = &adw::HeaderBar {
                                        pack_start = &gtk::Button {
                                            set_icon_name: "list-add-symbolic",
                                            set_tooltip_text: Some("Add Paper"),
                                            connect_clicked => Msg::Add,
                                        },
                                        pack_end = &gtk::Button {
                                            set_icon_name: "document-save-symbolic",
                                            set_tooltip_text: Some("Export BibTeX"),
                                            connect_clicked => Msg::Export,
                                        },
                                    },
                                    add_top_bar: field = &gtk::SearchEntry {
                                        set_placeholder_text: Some("Search"),
                                        set_search_delay: 0,
                                        set_margin_start: 12,
                                        set_margin_end: 12,
                                        set_margin_bottom: 6,
                                        connect_search_changed[sender] => move |f| sender.input(Msg::Filter(f.text().into())),
                                        connect_stop_search => |f| f.set_text(""),
                                    },
                                    #[wrap(Some)]
                                    set_content = &gtk::Stack {
                                        add_named[Some("list")] = &gtk::ScrolledWindow {
                                            set_hscrollbar_policy: gtk::PolicyType::Never,
                                            #[local_ref]
                                            list -> gtk::ListView {
                                                add_css_class: "navigation-sidebar",
                                                set_single_click_activate: true,
                                                connect_activate[sender] => move |_, position| sender.input(Msg::Open(position)),
                                            },
                                        },
                                        add_named[Some("none")] = &adw::StatusPage {
                                            set_icon_name: Some("folder-symbolic"),
                                            set_title: "No Library Folder",
                                            set_description: Some("Choose a folder for your references."),
                                            #[wrap(Some)]
                                            set_child = &gtk::Button {
                                                set_label: "Choose Folder",
                                                set_halign: gtk::Align::Center,
                                                add_css_class: "pill",
                                                add_css_class: "suggested-action",
                                                connect_clicked => Msg::ChooseFolder,
                                            },
                                        },
                                        add_named[Some("empty")] = &adw::StatusPage {
                                            set_icon_name: Some("user-bookmarks-symbolic"),
                                            set_title: "No Entries",
                                            set_description: Some("Add a paper with +, by its arXiv number, DOI, or texkey."),
                                        },
                                        add_named[Some("no-results")] = &adw::StatusPage {
                                            set_icon_name: Some("edit-find-symbolic"),
                                            set_title: "No Results",
                                        },
                                        // After the pages, which must be there to be shown
                                        #[watch]
                                        set_visible_child_name: model.page(),
                                    },
                                },
                            },
                            #[wrap(Some)]
                            set_content: entry_page = &adw::NavigationPage {
                                set_title: "Entry",
                                set_child: Some(&placeholder()),
                            },
                        },
                    },
                },
            },
        }
    }

    menu! {
        main_menu: {
            "Choose Library Folder…" => ChooseFolderAction,
            "About refdir" => AboutAction,
        }
    }

    fn init(_: (), root: Self::Root, sender: ComponentSender<Self>) -> ComponentParts<Self> {
        // The app's icon, where the app is not installed
        #[cfg(debug_assertions)]
        gtk::IconTheme::for_display(&WidgetExt::display(&root))
            .add_search_path(concat!(env!("CARGO_MANIFEST_DIR"), "/data/icons"));

        let settings = settings();
        let s = sender.clone();
        settings.connect_changed(Some("folder"), move |_, _| s.input(Msg::Folder));
        let find: Rc<RefCell<Find>> = Rc::default();
        let mut list: TypedListView<EntryItem, gtk::SingleSelection> = TypedListView::new();
        list.selection_model.set_autoselect(false);
        list.selection_model.set_can_unselect(true);
        let f = find.clone();
        list.add_filter(move |item: &EntryItem| {
            let find = f.borrow();
            find.in_place(&item.entry)
                && refdir::fold(&find.search).split_whitespace().all(|w| item.text.contains(w))
        });
        let mut model = App {
            folder: folder(&settings),
            settings,
            read: Vec::new(),
            tags: Vec::new(),
            find,
            places: FactoryVecDeque::builder().launch(gtk::ListBox::new()).detach(),
            list,
            shown: None,
            watched: Vec::new(),
            monitors: Vec::new(),
        };
        let (places, list) = (model.places.widget(), &model.list.view);
        let widgets = view_output!();

        // The entry goes over the list in a smaller window, and the list over the places in a
        // narrow one; the header bars' back buttons then walk back
        let breakpoint = |width: &str, collapsed: &[&adw::NavigationSplitView]| {
            let b = adw::Breakpoint::new(adw::BreakpointCondition::parse(width).unwrap());
            collapsed.iter().for_each(|v| b.add_setter(*v, "collapsed", Some(&true.to_value())));
            b
        };
        root.add_breakpoint(breakpoint("max-width: 860sp", &[&widgets.inner]));
        root.add_breakpoint(breakpoint("max-width: 560sp", &[&widgets.inner, &widgets.outer]));

        let mut actions = RelmActionGroup::<WindowActions>::new();
        let action = |msg: fn() -> Msg| {
            let s = sender.clone();
            move |_: &gio::SimpleAction| s.input(msg())
        };
        actions.add_action(RelmAction::<AddAction>::new_stateless(action(|| Msg::Add)));
        actions.add_action(RelmAction::<SearchAction>::new_stateless(action(|| Msg::FocusSearch)));
        actions.add_action(RelmAction::<ChooseFolderAction>::new_stateless(action(|| Msg::ChooseFolder)));
        actions.add_action(RelmAction::<ExportAction>::new_stateless(action(|| Msg::Export)));
        actions.add_action(RelmAction::<AboutAction>::new_stateless(action(|| Msg::About)));
        actions.add_action(RelmAction::<QuitAction>::new_stateless(|_| relm4::main_application().quit()));
        actions.register_for_widget(&root);
        let app = relm4::main_application();
        app.set_accelerators_for_action::<AddAction>(&["<Control>n"]);
        app.set_accelerators_for_action::<SearchAction>(&["<Control>f"]);
        app.set_accelerators_for_action::<ChooseFolderAction>(&["<Control>o"]);
        app.set_accelerators_for_action::<ExportAction>(&["<Control>e"]);
        app.set_accelerators_for_action::<QuitAction>(&["<Control>q"]);

        model.reload(&widgets, &sender, true);
        ComponentParts { model, widgets }
    }

    fn update_with_view(
        &mut self,
        widgets: &mut Self::Widgets,
        msg: Msg,
        sender: ComponentSender<Self>,
        root: &Self::Root,
    ) {
        match msg {
            Msg::Reload => self.reload(widgets, &sender, false),
            Msg::ChooseFolder => {
                let dialog = gtk::FileDialog::builder().title("Choose Library Folder").modal(true).build();
                let settings = self.settings.clone();
                dialog.select_folder(Some(root), gio::Cancellable::NONE, move |result| {
                    if let Some(path) = result.ok().and_then(|f| f.path()) {
                        settings.set_string("folder", &path.to_string_lossy()).ok();
                    }
                });
            }
            Msg::Folder => {
                self.folder = folder(&self.settings);
                self.reload(widgets, &sender, true);
            }
            Msg::About => adw::AboutDialog::builder()
                .application_name("refdir")
                .application_icon(APP_ID)
                .version(env!("CARGO_PKG_VERSION"))
                .comments("A library of references: a folder of BibTeX entries")
                .website("https://github.com/xiupos/refdir")
                .license_type(gtk::License::MitX11)
                .build()
                .present(Some(root)),
            Msg::Toast(text) => toast(&widgets.toasts, &text),
            Msg::Place(i) => {
                self.find.borrow_mut().tag = self.places.get(i).and_then(|p| p.tag.clone());
                self.list.notify_filter_changed(0);
                self.show_list(widgets);
            }
            Msg::OpenTag(tag) => {
                let i = self.tags.iter().position(|(t, _)| *t == tag).map_or(0, |i| i + 1);
                widgets.places.select_row(widgets.places.row_at_index(i as i32).as_ref());
                sender.input(Msg::Place(i));
            }
            Msg::Filter(text) => {
                self.find.borrow_mut().search = text;
                self.list.notify_filter_changed(0);
            }
            Msg::Open(position) => {
                if let Some(dir) = self.list.get_visible(position).map(|e| e.borrow().entry.dir.clone()) {
                    self.open(widgets, &dir, &sender);
                }
            }
            Msg::Export => {
                let tag = self.find.borrow().tag.clone();
                let name = tag.as_deref().map_or("library".into(), |t| t.replace(['/', ':'], "-"));
                let dialog = gtk::FileDialog::builder()
                    .title("Export BibTeX")
                    .initial_name(format!("{name}.bib"))
                    .modal(true)
                    .build();
                let s = sender.clone();
                dialog.save(Some(root), gio::Cancellable::NONE, move |result| {
                    if let Some(path) = result.ok().and_then(|f| f.path()) {
                        s.input(Msg::Exported(path));
                    }
                });
            }
            Msg::Exported(path) => {
                let find = self.find.borrow();
                let entries: Vec<&Entry> = self.entries().filter(|e| find.in_place(e)).collect();
                let message = match std::fs::write(&path, refdir::bibtex(entries.iter().copied())) {
                    Ok(()) => format!("Exported {} entries", entries.len()),
                    Err(e) => format!("Could not write the file: {e}"),
                };
                drop(find);
                toast(&widgets.toasts, &message);
            }
            Msg::FocusSearch => {
                widgets.field.grab_focus();
                self.show_list(widgets);
            }
            Msg::Add => {
                if self.folder.is_none() {
                    return toast(&widgets.toasts, "Choose a library folder first");
                }
                relm4::view! {
                    field = gtk::Entry {
                        set_placeholder_text: Some("1207.7214, 10.1103/PhysRevD.49.6587, Maldacena:1997re"),
                        set_activates_default: true,
                    }
                }
                let dialog = adw::AlertDialog::builder()
                    .heading("Add Paper")
                    .body("An arXiv number, a DOI, a texkey, or an INSPIRE, arXiv, or doi.org link")
                    .extra_child(&field)
                    .build();
                dialog.add_responses(&[("cancel", "_Cancel"), ("add", "_Add")]);
                dialog.set_response_appearance("add", adw::ResponseAppearance::Suggested);
                dialog.set_default_response(Some("add"));
                dialog.set_close_response("cancel");
                let (f, s) = (field.clone(), sender.clone());
                dialog.connect_response(Some("add"), move |_, _| s.input(Msg::Fetch(f.text().into())));
                dialog.present(Some(root));
                dialog.set_focus(Some(&field));
            }
            Msg::Fetch(id) => {
                if !id.trim().is_empty() {
                    sender.spawn_oneshot_command(move || Cmd::Fetched(fetch::find(&id).map(Box::new)));
                }
            }
        }
        self.update_view(widgets, sender);
    }

    fn update_cmd_with_view(
        &mut self,
        widgets: &mut Self::Widgets,
        cmd: Cmd,
        sender: ComponentSender<Self>,
        _: &Self::Root,
    ) {
        let Cmd::Fetched(result) = cmd;
        let folder = self.folder.clone().unwrap_or_default();
        match result.and_then(|bib| refdir::add(&folder, *bib).map_err(|e| e.to_string())) {
            Ok(entry) => {
                self.reload(widgets, &sender, false);
                self.open(widgets, &entry.dir, &sender);
            }
            Err(e) => toast(&widgets.toasts, &e),
        }
        self.update_view(widgets, sender);
    }
}

fn main() {
    relm4::RelmApp::new(APP_ID).run::<App>(());
}
