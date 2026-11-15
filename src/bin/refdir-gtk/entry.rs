//! `EntryPane`: an entry, its tags, note, and files, and what the user does with it: read its PDF or
//! get it, copy or edit its BibTeX, get it again from INSPIRE, or move it to the trash

use crate::{boxed_list, heading, open_file};
use adw::prelude::*;
use gtk::{gdk, gio};
use refdir::{Bib, Entry, fetch};
use relm4::actions::{RelmAction, RelmActionGroup};
use relm4::factory::{DynamicIndex, FactoryComponent, FactorySender, FactoryVecDeque};
use relm4::prelude::*;
use std::path::PathBuf;

relm4::new_action_group!(EntryActions, "entry");
relm4::new_stateless_action!(EditAction, EntryActions, "edit");
relm4::new_stateless_action!(AgainAction, EntryActions, "again");
relm4::new_stateless_action!(ShowInFilesAction, EntryActions, "show-in-files");
relm4::new_stateless_action!(TrashAction, EntryActions, "trash");

/// A tag of the entry, with a button that removes it. The row opens the tag's place
pub struct TagRow {
    tag: String,
}

#[derive(Debug)]
pub enum TagOutput {
    Open(String),
    Remove(String),
}

#[relm4::factory(pub)]
impl FactoryComponent for TagRow {
    type Init = String;
    type Input = ();
    type Output = TagOutput;
    type CommandOutput = ();
    type ParentWidget = gtk::ListBox;

    view! {
        adw::ActionRow {
            set_title: &self.tag,
            set_use_markup: false,
            set_activatable: true,
            connect_activated[sender, tag = self.tag.clone()] => move |_| drop(sender.output(TagOutput::Open(tag.clone()))),
            add_suffix = &gtk::Button {
                set_icon_name: "edit-delete-symbolic",
                set_tooltip_text: Some("Remove Tag"),
                set_valign: gtk::Align::Center,
                add_css_class: "flat",
                connect_clicked[sender, tag = self.tag.clone()] => move |_| drop(sender.output(TagOutput::Remove(tag.clone()))),
            },
        }
    }

    fn init_model(tag: String, _: &DynamicIndex, _: FactorySender<Self>) -> Self {
        TagRow { tag }
    }
}

/// A file of the entry, with its kind's icon
pub struct FileRow {
    file: PathBuf,
}

#[relm4::factory(pub)]
impl FactoryComponent for FileRow {
    type Init = PathBuf;
    type Input = ();
    type Output = ();
    type CommandOutput = ();
    type ParentWidget = gtk::ListBox;

    view! {
        adw::ActionRow {
            set_title: &self.file.to_string_lossy(),
            set_use_markup: false,
            set_activatable: true,
            add_prefix = &gtk::Image::from_gicon(&gio::content_type_get_symbolic_icon(&gio::content_type_guess(Some(&self.file), None).0)),
        }
    }

    fn init_model(file: PathBuf, _: &DynamicIndex, _: FactorySender<Self>) -> Self {
        FileRow { file }
    }
}

pub struct EntryPane {
    entry: Entry,
    /// The note as last read or written, so that only a changed one is written
    note: String,
    getting_pdf: bool,
    tags: FactoryVecDeque<TagRow>,
    files: FactoryVecDeque<FileRow>,
}

#[derive(Debug)]
pub enum Msg {
    CopyKey,
    /// The entry, read again
    Update(Box<Entry>),
    /// Read, or Get PDF
    Main,
    CopyBibtex,
    OpenInspire,
    AddTag(String),
    RemoveTag(String),
    OpenTag(String),
    SaveNote,
    OpenFile(usize),
    AddFiles,
    Files(Vec<PathBuf>),
    Edit,
    /// The edited BibTeX
    Edited(String),
    Again,
    ShowInFiles,
    Trash,
}

#[derive(Debug)]
pub enum Output {
    Toast(String),
    /// The entry changed, or was moved to the trash
    Changed,
    OpenTag(String),
}

#[derive(Debug)]
pub enum Cmd {
    Pdf(Result<PathBuf, String>),
    Again(Result<Box<Bib>, String>),
}

fn text(view: &gtk::TextView) -> String {
    let buffer = view.buffer();
    buffer.text(&buffer.start_iter(), &buffer.end_iter(), false).into()
}

/// The dialog that edits the entry's BibTeX as text; `save` gets the text
fn ask_edit(bibtex: &str, parent: &impl IsA<gtk::Widget>, save: impl Fn(String) + 'static) {
    relm4::view! {
        dialog = adw::Dialog {
            set_title: "Edit BibTeX",
            set_content_width: 640,
            set_content_height: 560,
            #[wrap(Some)]
            set_child = &adw::ToolbarView {
                add_top_bar = &adw::HeaderBar {
                    set_show_start_title_buttons: false,
                    set_show_end_title_buttons: false,
                    pack_start: cancel = &gtk::Button {
                        set_label: "Cancel",
                    },
                    pack_end: done = &gtk::Button {
                        set_label: "Save",
                        add_css_class: "suggested-action",
                    },
                },
                #[wrap(Some)]
                set_content = &gtk::ScrolledWindow {
                    #[name = "source"]
                    gtk::TextView {
                        set_monospace: true,
                        set_wrap_mode: gtk::WrapMode::WordChar,
                        set_top_margin: 12,
                        set_bottom_margin: 12,
                        set_left_margin: 12,
                        set_right_margin: 12,
                    },
                },
            },
        }
    }
    source.buffer().set_text(bibtex);
    let d = dialog.downgrade();
    cancel.connect_clicked(move |_| {
        if let Some(d) = d.upgrade() {
            d.close();
        }
    });
    let d = dialog.downgrade();
    done.connect_clicked(move |_| {
        save(text(&source));
        if let Some(d) = d.upgrade() {
            d.close();
        }
    });
    dialog.present(Some(parent));
}

impl EntryPane {
    fn toast(&self, sender: &ComponentSender<Self>, text: String) {
        sender.output(Output::Toast(text)).ok();
    }

    /// Writes the entry, and tells the app; or says why it could not
    fn write(&self, sender: &ComponentSender<Self>) {
        match self.entry.write() {
            Ok(()) => drop(sender.output(Output::Changed)),
            Err(e) => self.toast(sender, format!("Could not write {}: {e}", refdir::ENTRY)),
        }
    }

    /// The tags' and files' rows, from the entry
    fn fill(&mut self) {
        let mut tags = self.tags.guard();
        tags.clear();
        self.entry.tags().into_iter().for_each(|t| drop(tags.push_back(t)));
        drop(tags);
        let mut files = self.files.guard();
        files.clear();
        self.entry.files().into_iter().for_each(|f| drop(files.push_back(f)));
    }

    fn info(&self) -> String {
        let b = &self.entry.bib;
        let journal = b.get("journal").or_else(|| b.get("booktitle")).map(|j| refdir::plain(&j));
        let parts = [self.entry.year(), journal, Some(self.entry.key().to_owned())];
        parts.into_iter().flatten().collect::<Vec<_>>().join(" · ")
    }

    fn names(&self) -> String {
        let names = self.entry.names();
        let b = &self.entry.bib;
        let others = b.get("author").or_else(|| b.get("editor")).is_some_and(|l| l.ends_with(" and others"));
        let more = match names.len() {
            n if n > 10 => format!(" and {} more", n - 10),
            _ if others => " et al.".to_owned(),
            _ => String::new(),
        };
        format!("{}{more}", names.iter().take(10).cloned().collect::<Vec<_>>().join(", "))
    }

    /// Read the PDF, get it from arXiv, or neither
    fn main_label(&self) -> Option<&'static str> {
        match (self.entry.pdf(), self.entry.bib.get("eprint")) {
            _ if self.getting_pdf => Some("Downloading…"),
            (Some(_), _) => Some("Read"),
            (None, Some(_)) => Some("Get PDF"),
            (None, None) => None,
        }
    }
}

#[relm4::component(pub)]
impl Component for EntryPane {
    type Init = Entry;
    type Input = Msg;
    type Output = Output;
    type CommandOutput = Cmd;

    view! {
        adw::ToolbarView {
            // The title is the pane's heading: in the header bar it would be cut short
            add_top_bar = &adw::HeaderBar {
                set_show_title: false,
                pack_end = &gtk::MenuButton {
                    set_icon_name: "view-more-symbolic",
                    set_tooltip_text: Some("Entry Menu"),
                    set_menu_model: Some(&entry_menu),
                },
            },
            #[wrap(Some)]
            set_content = &gtk::ScrolledWindow {
                set_hscrollbar_policy: gtk::PolicyType::Never,
                adw::Clamp {
                    set_maximum_size: 720,
                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 24,
                        set_margin_top: 24,
                        set_margin_bottom: 24,
                        set_margin_start: 12,
                        set_margin_end: 12,
                        gtk::Box {
                            set_orientation: gtk::Orientation::Vertical,
                            set_spacing: 6,
                            gtk::Label {
                                set_wrap: true,
                                set_xalign: 0.0,
                                add_css_class: "title-2",
                                #[watch]
                                set_label: &model.entry.title(),
                            },
                            gtk::Label {
                                set_wrap: true,
                                set_xalign: 0.0,
                                #[watch]
                                set_label: &model.names(),
                            },
                            gtk::Label {
                                set_wrap: true,
                                set_xalign: 0.0,
                                add_css_class: "dim-label",
                                #[watch]
                                set_label: &model.info(),
                            },
                            // Wrapped in a narrow pane
                            adw::WrapBox {
                                set_child_spacing: 12,
                                set_line_spacing: 12,
                                set_margin_top: 12,
                                append = &gtk::Button {
                                    add_css_class: "pill",
                                    add_css_class: "suggested-action",
                                    #[watch]
                                    set_label: model.main_label().unwrap_or_default(),
                                    #[watch]
                                    set_visible: model.main_label().is_some(),
                                    #[watch]
                                    set_sensitive: !model.getting_pdf,
                                    connect_clicked => Msg::Main,
                                },
                                append = &gtk::Button {
                                    set_label: "Copy Key",
                                    set_tooltip_text: Some("Copy the citation key"),
                                    add_css_class: "pill",
                                    connect_clicked => Msg::CopyKey,
                                },
                                append = &gtk::Button {
                                    set_label: "Copy BibTeX",
                                    add_css_class: "pill",
                                    connect_clicked => Msg::CopyBibtex,
                                },
                                append = &gtk::Button {
                                    set_label: "INSPIRE",
                                    add_css_class: "pill",
                                    #[watch]
                                    set_visible: fetch::inspire_url(&model.entry.bib).is_some(),
                                    connect_clicked => Msg::OpenInspire,
                                },
                            },
                        },
                        gtk::Box {
                            set_orientation: gtk::Orientation::Vertical,
                            set_spacing: 12,
                            heading("Tags"),
                            #[local_ref]
                            tags -> gtk::ListBox {
                                #[watch]
                                set_visible: !model.tags.is_empty(),
                            },
                            // Apart from the tags' rows, which are made again: what is typed stays
                            boxed_list() {
                                adw::EntryRow {
                                    set_title: "Add Tag",
                                    connect_entry_activated[sender] => move |e| {
                                        sender.input(Msg::AddTag(e.text().into()));
                                        e.set_text("");
                                    },
                                },
                            },
                        },
                        gtk::Box {
                            set_orientation: gtk::Orientation::Vertical,
                            set_spacing: 12,
                            heading("Note"),
                            // Written when it loses the focus, and when the pane closes
                            #[name = "note"]
                            gtk::TextView {
                                set_wrap_mode: gtk::WrapMode::WordChar,
                                set_height_request: 100,
                                set_left_margin: 12,
                                set_top_margin: 12,
                                add_css_class: "card",
                                add_controller = gtk::EventControllerFocus {
                                    connect_leave => Msg::SaveNote,
                                },
                            },
                        },
                        gtk::Box {
                            set_orientation: gtk::Orientation::Vertical,
                            set_spacing: 12,
                            heading("Files"),
                            #[local_ref]
                            files -> gtk::ListBox {
                                #[watch]
                                set_visible: !model.files.is_empty(),
                                connect_row_activated[sender] => move |_, row| sender.input(Msg::OpenFile(row.index() as usize)),
                            },
                            boxed_list() {
                                adw::ButtonRow {
                                    set_title: "Add File…",
                                    set_start_icon_name: Some("list-add-symbolic"),
                                    connect_activated => Msg::AddFiles,
                                },
                            },
                        },
                    },
                },
                // Files dropped on the pane are added to the entry
                add_controller = gtk::DropTarget::new(gdk::FileList::static_type(), gdk::DragAction::COPY) {
                    connect_drop[sender] => move |_, value, _, _| {
                        let Ok(list) = value.get::<gdk::FileList>() else { return false };
                        sender.input(Msg::Files(list.files().iter().filter_map(|f| f.path()).collect()));
                        true
                    },
                },
            },
        }
    }

    menu! {
        entry_menu: {
            "Edit BibTeX…" => EditAction,
            "Get Again from INSPIRE" => AgainAction,
            "Show in Files" => ShowInFilesAction,
            "Move to Trash" => TrashAction,
        }
    }

    fn init(entry: Entry, root: Self::Root, sender: ComponentSender<Self>) -> ComponentParts<Self> {
        let tags =
            FactoryVecDeque::builder().launch(boxed_list()).forward(sender.input_sender(), |o| match o {
                TagOutput::Open(tag) => Msg::OpenTag(tag),
                TagOutput::Remove(tag) => Msg::RemoveTag(tag),
            });
        let files = FactoryVecDeque::builder().launch(boxed_list()).detach();
        let mut model = EntryPane { note: entry.note(), entry, getting_pdf: false, tags, files };
        model.fill();
        let (tags, files) = (model.tags.widget(), model.files.widget());
        let widgets = view_output!();
        widgets.note.buffer().set_text(&model.note);

        let mut actions = RelmActionGroup::<EntryActions>::new();
        let action = |msg: fn() -> Msg| {
            let s = sender.clone();
            move |_: &gio::SimpleAction| s.input(msg())
        };
        actions.add_action(RelmAction::<EditAction>::new_stateless(action(|| Msg::Edit)));
        actions.add_action(RelmAction::<AgainAction>::new_stateless(action(|| Msg::Again)));
        actions.add_action(RelmAction::<ShowInFilesAction>::new_stateless(action(|| Msg::ShowInFiles)));
        actions.add_action(RelmAction::<TrashAction>::new_stateless(action(|| Msg::Trash)));
        actions.register_for_widget(&root);
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
            Msg::Update(entry) => {
                self.entry = *entry;
                self.fill();
                // A note changed on disk does not replace the one being typed
                if !widgets.note.has_focus() && self.entry.note() != self.note {
                    self.note = self.entry.note();
                    widgets.note.buffer().set_text(&self.note);
                }
            }
            Msg::Main => match self.entry.pdf() {
                Some(pdf) => open_file(&pdf, false, root),
                None => {
                    self.getting_pdf = true;
                    let entry = self.entry.clone();
                    sender.spawn_oneshot_command(move || Cmd::Pdf(fetch::pdf(&entry)));
                }
            },
            Msg::CopyKey => {
                root.display().clipboard().set_text(self.entry.key());
                self.toast(&sender, format!("{} copied", self.entry.key()));
            }
            Msg::CopyBibtex => {
                root.display().clipboard().set_text(&self.entry.bib.to_bibtex());
                self.toast(&sender, "BibTeX copied".into());
            }
            Msg::OpenInspire => {
                if let Some(url) = fetch::inspire_url(&self.entry.bib) {
                    let window = root.root().and_downcast::<gtk::Window>();
                    gtk::UriLauncher::new(&url).launch(window.as_ref(), gio::Cancellable::NONE, |_| {});
                }
            }
            Msg::AddTag(tag) => {
                let mut tags = self.entry.tags();
                let tag = tag.trim().to_owned();
                if !tag.is_empty() && !tags.contains(&tag) {
                    tags.push(tag);
                    self.entry.set_tags(&tags);
                    self.write(&sender);
                }
            }
            Msg::RemoveTag(tag) => {
                let tags: Vec<String> = self.entry.tags().into_iter().filter(|t| *t != tag).collect();
                self.entry.set_tags(&tags);
                self.write(&sender);
            }
            Msg::OpenTag(tag) => drop(sender.output(Output::OpenTag(tag))),
            Msg::SaveNote => {
                let note = text(&widgets.note);
                if note != self.note {
                    self.entry.set_note(&note);
                    self.note = note;
                    self.write(&sender);
                }
            }
            Msg::OpenFile(i) => {
                if let Some(file) = self.entry.files().get(i) {
                    open_file(&self.entry.dir.join(file), false, root);
                }
            }
            Msg::AddFiles => {
                let window = root.root().and_downcast::<gtk::Window>();
                let dialog = gtk::FileDialog::builder().title("Add Files").modal(true).build();
                let s = sender.clone();
                dialog.open_multiple(window.as_ref(), gio::Cancellable::NONE, move |result| {
                    let Ok(list) = result else { return };
                    s.input(Msg::Files(
                        list.iter::<gio::File>().flatten().filter_map(|f| f.path()).collect(),
                    ));
                });
            }
            Msg::Files(paths) => {
                for path in paths {
                    let name = path.file_name().unwrap_or_default().to_string_lossy().into_owned();
                    let added = std::fs::read(&path).and_then(|data| self.entry.add_file(&name, &data));
                    match added {
                        Ok(_) => drop(sender.output(Output::Changed)),
                        Err(e) => self.toast(&sender, format!("Could not add {name}: {e}")),
                    }
                }
            }
            Msg::Edit => {
                let s = sender.clone();
                ask_edit(&self.entry.bib.to_bibtex(), root, move |text| s.input(Msg::Edited(text)));
            }
            // The key stays the folder's name
            Msg::Edited(text) => match Bib::parse(&text) {
                Ok(bibs) if bibs.len() == 1 => {
                    let mut bib = bibs.into_iter().next().unwrap();
                    bib.key = self.entry.key().to_owned();
                    self.entry.bib = bib;
                    self.note = self.entry.note();
                    widgets.note.buffer().set_text(&self.note);
                    self.write(&sender);
                }
                Ok(bibs) => self.toast(&sender, format!("Not saved: {} entries, not one", bibs.len())),
                Err(e) => self.toast(&sender, format!("Not saved: {e}")),
            },
            Msg::Again => {
                let entry = self.entry.clone();
                sender.spawn_oneshot_command(move || Cmd::Again(fetch::again(&entry).map(Box::new)));
            }
            Msg::ShowInFiles => open_file(&self.entry.dir.join(refdir::ENTRY), true, root),
            // The trash is the undo
            Msg::Trash => match gio::File::for_path(&self.entry.dir).trash(gio::Cancellable::NONE) {
                Ok(()) => {
                    self.toast(&sender, "Moved to the trash".into());
                    sender.output(Output::Changed).ok();
                }
                Err(e) => self.toast(&sender, format!("Could not move to the trash: {e}")),
            },
        }
        self.update_view(widgets, sender);
    }

    fn update_cmd(&mut self, cmd: Cmd, sender: ComponentSender<Self>, _: &Self::Root) {
        match cmd {
            Cmd::Pdf(result) => {
                self.getting_pdf = false;
                match result {
                    Ok(_) => drop(sender.output(Output::Changed)),
                    Err(e) => self.toast(&sender, format!("No PDF: {e}")),
                }
            }
            Cmd::Again(Ok(bib)) if *bib == self.entry.bib => {
                self.toast(&sender, "INSPIRE has nothing new".into())
            }
            Cmd::Again(Ok(bib)) => {
                self.entry.bib = *bib;
                self.write(&sender);
                self.toast(&sender, "Got again from INSPIRE".into());
            }
            Cmd::Again(Err(e)) => self.toast(&sender, e),
        }
    }

    fn shutdown(&mut self, widgets: &mut Self::Widgets, output: relm4::Sender<Output>) {
        let note = text(&widgets.note);
        if note != self.note {
            self.entry.set_note(&note);
            if self.entry.write().is_err() {
                output.send(Output::Toast("Could not write the note".into())).ok();
            }
        }
    }
}
