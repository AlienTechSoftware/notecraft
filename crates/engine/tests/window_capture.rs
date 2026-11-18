use notecraft_engine::{Output, Params, Session};
use notecraft_store::{Settings, testing::TempDir};

#[test]
fn quick_notes_are_unique_durable_pages_in_the_configured_format() {
    for format in ["html", "cmgfm"] {
        let t = TempDir::new("quick-notes");
        let settings = Settings::at(t.path().join("settings.toml"));
        settings.set("default_format", format).expect("format");
        let mut s = Session::new(&t.path().join("notebooks"), settings).expect("session");
        let mut pages = Vec::new();
        for _ in 0..2 {
            let Output::Path(path) = s.run("window.quick-note", &Params::new()).expect("quick note") else { panic!("page path") };
            let page = notecraft_store::page::load_page(&path).expect("durable page");
            assert_eq!(page.format.id(), format);
            assert_eq!(notecraft_store::tree::read_node(path.parent().expect("section")).expect("section metadata").title, "Quick Notes");
            pages.push(path);
        }
        assert_ne!(pages[0], pages[1]);
        assert!(pages.iter().all(|p| p.is_dir()));
        let before = std::fs::read(notecraft_store::tree::page_file(&pages[0]).expect("file").1).expect("bytes");
        let p = Params::from([("path".into(), pages[0].to_string_lossy().into_owned())]);
        assert!(matches!(s.run("window.new", &p).expect("new window target"), Output::Path(path) if path == pages[0]));
        assert_eq!(std::fs::read(notecraft_store::tree::page_file(&pages[0]).expect("file").1).expect("bytes"), before);
    }
}

#[test]
fn invalid_quick_note_destination_does_not_fall_back_silently() {
    let t = TempDir::new("quick-note-invalid");
    let settings = Settings::at(t.path().join("settings.toml"));
    settings.set("quick_notes_section", "missing/section").expect("destination");
    let mut s = Session::new(&t.path().join("notebooks"), settings).expect("session");
    assert!(s.run("window.quick-note", &Params::new()).is_err());
    assert!(s.tree().is_empty());
}

#[test]
fn stale_window_save_preserves_the_newer_page() {
    let t = TempDir::new("window-save-conflict");
    let mut s = Session::new(&t.path().join("notebooks"), Settings::at(t.path().join("settings.toml"))).expect("session");
    let sample = notecraft_engine::sample::create_sample(&mut s, None).expect("sample");
    for path in [&sample.html_page, &sample.md_page] {
        let original = notecraft_store::page::load_page(path).expect("load").body;
        notecraft_store::page::save_body(path, "newer window").expect("newer edit");
        let p = Params::from([
            ("path".into(), path.to_string_lossy().into_owned()),
            ("body".into(), "stale window".into()),
            ("expected_body".into(), original),
        ]);
        assert!(s.run("page.save", &p).is_err(), "a stale editor cannot replace newer content");
        assert_eq!(notecraft_store::page::load_page(path).expect("kept").body.trim(), "newer window");
    }
}

#[test]
fn a_page_used_as_session_root_has_a_distinct_body_write_lock() {
    let t = TempDir::new("window-root-page");
    let settings = Settings::at(t.path().join("settings.toml"));
    let mut s = Session::new(&t.path().join("notebooks"), settings.clone()).expect("session");
    let sample = notecraft_engine::sample::create_sample(&mut s, None).expect("sample");
    let mut page_session = Session::new(&sample.html_page, settings).expect("page root");
    let (sent, received) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        let result = page_session.run("page.save", &Params::from([("path".into(), ".".into()), ("body".into(), "<p>saved at the root</p>".into())]));
        sent.send(result.is_ok()).expect("result receiver");
    });
    assert!(received.recv_timeout(std::time::Duration::from_secs(5)).expect("root and page locks must not reenter"));
    worker.join().expect("worker");
}
