//! Run only on a disposable X server: this test temporarily acts as a window manager.
use std::{
    fs,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};
use x11rb::{
    connection::Connection,
    protocol::{xproto::*, Event},
    wrapper::ConnectionExt as _,
};
struct Guard(Child);
impl Drop for Guard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
#[test]
#[ignore = "requires disposable Xvfb and DOCK_ISOLATED_X11_TEST=1"]
fn window_tracking_and_actions() {
    assert_eq!(std::env::var("DOCK_ISOLATED_X11_TEST").as_deref(), Ok("1"));
    let (c, sn) = x11rb::connect(None).unwrap();
    let s = &c.setup().roots[sn];
    assert_eq!(s.width_in_pixels, 1280, "test fixture expects 1280px Xvfb");
    let root = s.root;
    let atom = |name: &str| {
        c.intern_atom(false, name.as_bytes())
            .unwrap()
            .reply()
            .unwrap()
            .atom
    };
    let cm = atom(&format!("_NET_WM_CM_S{sn}"));
    assert_eq!(
        c.get_selection_owner(cm).unwrap().reply().unwrap().owner,
        0,
        "must not run under a real compositor"
    );
    assert!(
        c.get_property(
            false,
            root,
            atom("_NET_SUPPORTING_WM_CHECK"),
            AtomEnum::WINDOW,
            0,
            1
        )
        .unwrap()
        .reply()
        .unwrap()
        .value
        .is_empty(),
        "must not run under a real WM"
    );
    c.change_window_attributes(
        root,
        &ChangeWindowAttributesAux::new()
            .event_mask(EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY),
    )
    .unwrap()
    .check()
    .unwrap();
    let create = |class: &[u8]| {
        let w = c.generate_id().unwrap();
        c.create_window(
            s.root_depth,
            w,
            root,
            20,
            20,
            80,
            60,
            0,
            WindowClass::INPUT_OUTPUT,
            s.root_visual,
            &CreateWindowAux::new().override_redirect(1),
        )
        .unwrap()
        .check()
        .unwrap();
        c.change_property8(
            PropMode::REPLACE,
            w,
            AtomEnum::WM_CLASS,
            AtomEnum::STRING,
            class,
        )
        .unwrap();
        c.change_property32(
            PropMode::REPLACE,
            w,
            atom("_NET_WM_WINDOW_TYPE"),
            AtomEnum::ATOM,
            &[atom("_NET_WM_WINDOW_TYPE_NORMAL")],
        )
        .unwrap();
        c.change_property32(
            PropMode::REPLACE,
            w,
            atom("_NET_WM_DESKTOP"),
            AtomEnum::CARDINAL,
            &[3],
        )
        .unwrap();
        c.map_window(w).unwrap();
        w
    };
    let thunar = create(b"thunar\0Thunar\0");
    let editor = create(b"editor\0TestEditor\0");
    let editor2 = create(b"editor\0TestEditor\0");
    c.set_selection_owner(thunar, cm, 0u32)
        .unwrap()
        .check()
        .unwrap();
    let clients = atom("_NET_CLIENT_LIST");
    let active = atom("_NET_ACTIVE_WINDOW");
    let hidden = atom("_NET_WM_STATE_HIDDEN");
    let state = atom("_NET_WM_STATE");
    c.change_property32(
        PropMode::REPLACE,
        root,
        clients,
        AtomEnum::WINDOW,
        &[thunar, editor, editor2],
    )
    .unwrap();
    c.change_property32(PropMode::REPLACE, root, active, AtomEnum::WINDOW, &[thunar])
        .unwrap();
    c.flush().unwrap();
    let temp = std::env::temp_dir().join(format!("dock-model-x11-{}", std::process::id()));
    fs::create_dir_all(temp.join("applications")).unwrap();
    fs::create_dir_all(temp.join("gtk-3.0")).unwrap();
    fs::write(
        temp.join("gtk-3.0/settings.ini"),
        "[Settings]\ngtk-icon-theme-name=DockTest\n",
    )
    .unwrap();
    fs::create_dir_all(temp.join("icons/DockTest/96x96/apps")).unwrap();
    fs::write(temp.join("icons/DockTest/index.theme"),"[Icon Theme]\nName=DockTest\nDirectories=96x96/apps\n[96x96/apps]\nSize=96\nType=Fixed\nContext=Applications\n").unwrap();
    fs::write(temp.join("icons/DockTest/96x96/apps/dock-test.svg"),r#"<svg xmlns="http://www.w3.org/2000/svg" width="96" height="96"><rect width="96" height="96" fill="red"/></svg>"#).unwrap();
    fs::write(temp.join("applications/thunar.desktop"),"[Desktop Entry]\nType=Application\nName=Files\nStartupWMClass=Thunar\nIcon=dock-test\nExec=thunar\n").unwrap();
    fs::write(temp.join("applications/editor.desktop"),"[Desktop Entry]\nType=Application\nName=Editor\nStartupWMClass=TestEditor\nIcon=dock-test\nExec=editor\n").unwrap();
    let mut child = Guard(
        Command::new(env!("CARGO_BIN_EXE_prychal-dock"))
            .args(["--no-autohide", "--duration", "20"])
            .env("XDG_SESSION_TYPE", "x11")
            .env("XDG_DATA_HOME", &temp)
            .env("XDG_DATA_DIRS", &temp)
            .env("XDG_CONFIG_HOME", &temp)
            .env("PATH", "/nonexistent")
            .stdout(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let until = Instant::now() + Duration::from_secs(8);
    let dock = loop {
        assert!(Instant::now() < until, "dock did not start");
        assert!(child.0.try_wait().unwrap().is_none(), "dock exited");
        let found = c
            .query_tree(root)
            .unwrap()
            .reply()
            .unwrap()
            .children
            .into_iter()
            .find(|&w| {
                c.get_property(false, w, atom("_NET_WM_PID"), AtomEnum::CARDINAL, 0, 1)
                    .unwrap()
                    .reply()
                    .ok()
                    .and_then(|r| r.value32().and_then(|mut v| v.next()))
                    == Some(child.0.id())
            });
        if let Some(w) = found {
            break w;
        }
        thread::sleep(Duration::from_millis(10));
    };
    thread::sleep(Duration::from_millis(200));
    let g = c.get_geometry(dock).unwrap().reply().unwrap();
    assert_eq!(g.width, 478, "five pins + one grouped editor");
    // Themed SVG is loaded by name from an XDG user theme, not a built-in fixture.
    let red = c
        .get_image(ImageFormat::Z_PIXMAP, dock, 152, 110, 1, 1, u32::MAX)
        .unwrap()
        .reply()
        .unwrap()
        .data;
    let value = if c.setup().image_byte_order == ImageOrder::LSB_FIRST {
        u32::from_le_bytes(red[..4].try_into().unwrap())
    } else {
        u32::from_be_bytes(red[..4].try_into().unwrap())
    };
    assert_eq!(value, 0xffff0000);
    let click = |x: i16, time: u32| {
        let geometry = c.get_geometry(dock).unwrap().reply().unwrap();
        for (response_type, mask) in [
            (BUTTON_PRESS_EVENT, EventMask::BUTTON_PRESS),
            (BUTTON_RELEASE_EVENT, EventMask::BUTTON_RELEASE),
        ] {
            c.send_event(
                false,
                dock,
                mask,
                ButtonPressEvent {
                    response_type,
                    detail: 1,
                    sequence: 0,
                    time,
                    root,
                    event: dock,
                    child: 0,
                    root_x: geometry.x + x,
                    root_y: geometry.y + 110,
                    event_x: x,
                    event_y: 110,
                    state: KeyButMask::default(),
                    same_screen: true,
                },
            )
            .unwrap()
            .check()
            .unwrap();
        }
        c.flush().unwrap();
    };
    let message = |kind: u32| {
        let until = Instant::now() + Duration::from_secs(3);
        loop {
            assert!(Instant::now() < until, "missing window action");
            while let Some(e) = c.poll_for_event().unwrap() {
                if let Event::ClientMessage(e) = e {
                    if e.type_ == kind {
                        return e;
                    }
                }
            }
            thread::sleep(Duration::from_millis(5));
        }
    };
    click(152, 123);
    let minimize = message(atom("WM_CHANGE_STATE"));
    assert_eq!(minimize.window, thunar);
    assert_eq!(minimize.data.as_data32()[0], 3);
    c.change_property32(PropMode::REPLACE, root, active, AtomEnum::WINDOW, &[0])
        .unwrap();
    c.change_property32(PropMode::REPLACE, thunar, state, AtomEnum::ATOM, &[hidden])
        .unwrap();
    c.flush().unwrap();
    thread::sleep(Duration::from_millis(150));
    click(152, 456);
    let desktop = message(atom("_NET_CURRENT_DESKTOP"));
    assert_eq!(desktop.data.as_data32()[..2], [3, 456]);
    let activation = message(active);
    assert_eq!(activation.window, thunar);
    assert_eq!(activation.data.as_data32()[..2], [2, 456]);
    c.change_property32(
        PropMode::REPLACE,
        root,
        active,
        AtomEnum::WINDOW,
        &[editor2],
    )
    .unwrap();
    c.flush().unwrap();
    thread::sleep(Duration::from_millis(150));
    click(384, 789);
    assert_eq!(
        message(atom("WM_CHANGE_STATE")).window,
        editor2,
        "group click minimizes its active window"
    );
    c.change_property32(
        PropMode::REPLACE,
        root,
        clients,
        AtomEnum::WINDOW,
        &[thunar],
    )
    .unwrap();
    c.destroy_window(editor).unwrap();
    c.destroy_window(editor2).unwrap();
    c.flush().unwrap();
    thread::sleep(Duration::from_millis(200));
    assert_eq!(
        c.get_geometry(dock).unwrap().reply().unwrap().width,
        420,
        "closed group removed; pinned app retained"
    );
    // New groups expand only up to screen capacity; wheel reveals overflow apps.
    let extras: Vec<_> = (0..20)
        .map(|i| create(format!("extra-{i}\0Extra-{i}\0").as_bytes()))
        .collect();
    let mut list = vec![thunar];
    list.extend_from_slice(&extras);
    c.change_property32(PropMode::REPLACE, root, clients, AtomEnum::WINDOW, &list)
        .unwrap();
    c.flush().unwrap();
    thread::sleep(Duration::from_millis(300));
    let expanded = c.get_geometry(dock).unwrap().reply().unwrap();
    assert!(expanded.width <= s.width_in_pixels);
    let capacity = (s.width_in_pixels.saturating_sub(130) as usize / 58).max(1);
    assert_eq!(expanded.width as usize, capacity * 58 + 130);
    c.send_event(
        false,
        dock,
        EventMask::BUTTON_PRESS,
        ButtonPressEvent {
            response_type: BUTTON_PRESS_EVENT,
            detail: 5,
            sequence: 0,
            time: 900,
            root,
            event: dock,
            child: 0,
            root_x: 0,
            root_y: 0,
            event_x: 150,
            event_y: 110,
            state: KeyButMask::default(),
            same_screen: true,
        },
    )
    .unwrap()
    .check()
    .unwrap();
    c.flush().unwrap();
    thread::sleep(Duration::from_millis(100));
    let last_x = expanded.width as i16 / 2 + ((capacity - 1) as f32 / 2. * 58.) as i16;
    click(last_x, 901);
    let _ = message(atom("_NET_CURRENT_DESKTOP"));
    assert_eq!(
        message(active).window,
        extras[capacity - 5],
        "wheel scroll reaches the next offscreen app"
    );
    child.0.kill().unwrap();
    child.0.wait().unwrap();
    fs::remove_dir_all(temp).unwrap();
}
