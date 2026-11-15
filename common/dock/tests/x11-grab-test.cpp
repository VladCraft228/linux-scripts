#include "applicationmodel.h"
#include "platform/x11/windowmodel.h"
#include <QElapsedTimer>
#include <QGuiApplication>
#include <QSettings>
#include <QTemporaryDir>
#include <QThread>
#include <array>
#include <cstring>
#include <memory>
#include <xcb/xcb.h>

// Run only in a dedicated Xvfb display, never against a user's live desktop.
#define CHECK(value)                                                           \
  do {                                                                         \
    if (!(value)) {                                                            \
      qCritical("X11 grab test failed at line %d: %s", __LINE__, #value);      \
      return 1;                                                                \
    }                                                                          \
  } while (false)
template <class Predicate> bool waitFor(Predicate predicate) {
  QElapsedTimer time;
  time.start();
  while (!predicate() && time.elapsed() < 1500) {
    QCoreApplication::processEvents();
    QThread::msleep(5);
  }
  return predicate();
}
int main(int argc, char **argv) {
  QGuiApplication app(argc, argv);
  QCoreApplication::setOrganizationName("LinuxScriptsTest");
  QCoreApplication::setApplicationName("dock-x11-rust-test");
  QTemporaryDir configDirectory;
  CHECK(configDirectory.isValid());
  QSettings::setPath(QSettings::NativeFormat, QSettings::UserScope,
                     configDirectory.path());
  QSettings before;
  before.setValue("dock/pinnedDesktopIds", QStringList{"missing-test.desktop"});
  before.setValue(
      "dock/importedDesktopFiles",
      QVariantMap{{"missing-test.desktop", "/nonexistent/test.desktop"}});
  before.setValue("dock/appearance/iconSize", QString("60"));
  before.setValue("dock/appearance/animationDuration", QString("150"));
  before.sync();
  Runtime runtime;
  ApplicationModel applications(&runtime);
  CHECK(applications.rowCount() == 1);
  CHECK(applications.preferences().value("iconSize").toInt() == 60);
  CHECK(applications.preferences().value("animationDuration").toInt() == 150);
  applications.setPreference("iconSize", 64);
  QSettings after;
  CHECK(after.value("dock/appearance/iconSize").toInt() == 64);
  CHECK(after.value("dock/pinnedDesktopIds").toStringList() ==
        QStringList{"missing-test.desktop"});
  CHECK(after.value("dock/importedDesktopFiles")
            .toMap()
            .value("missing-test.desktop")
            .toString() == "/nonexistent/test.desktop");
  applications.setAutoHide(false);
  CHECK(QMetaObject::invokeMethod(&applications, "resetPreferences"));
  CHECK(applications.preferences().value("iconSize").toInt() == 48);
  CHECK(applications.preferences().value("indicatorStyle").toInt() == 0);
  CHECK(applications.autoHide());
  CHECK(applications.rowCount() == 1);
  after.sync();
  CHECK(after.value("dock/pinnedDesktopIds").toStringList() ==
        QStringList{"missing-test.desktop"});
  CHECK(after.value("dock/importedDesktopFiles")
            .toMap()
            .contains("missing-test.desktop"));
  qInfo("PASS: original QSettings list/map encoding and preferences survive "
        "the Rust boundary.");
  WindowModel model(&runtime);
  CHECK(model.valid());
  QQuickWindow window;
  window.setFlags(Qt::FramelessWindowHint | Qt::BypassWindowManagerHint |
                  Qt::WindowDoesNotAcceptFocus);
  window.setGeometry(100, 100, 400, 180);
  window.create();
  model.attach(&window);
  window.show();
  app.processEvents();
  model.setInputRegion(
      {QVariantMap{{"x", 0}, {"y", 177}, {"width", 400}, {"height", 3}}});
  std::unique_ptr<xcb_connection_t, decltype(&xcb_disconnect)> connection(
      xcb_connect(nullptr, nullptr), &xcb_disconnect);
  auto *c = connection.get();
  CHECK(c && !xcb_connection_has_error(c));
  const auto root = xcb_setup_roots_iterator(xcb_get_setup(c)).data->root;
  const auto grabWindow = xcb_generate_id(c);
  const uint32_t overrideRedirect = 1;
  xcb_create_window(c, XCB_COPY_FROM_PARENT, grabWindow, root, 0, 0, 1, 1, 0,
                    XCB_WINDOW_CLASS_INPUT_OUTPUT, XCB_COPY_FROM_PARENT,
                    XCB_CW_OVERRIDE_REDIRECT, &overrideRedirect);
  xcb_map_window(c, grabWindow);
  xcb_warp_pointer(c, XCB_WINDOW_NONE, root, 0, 0, 0, 0, 20, 20);
  auto *grab = xcb_grab_pointer_reply(
      c,
      xcb_grab_pointer(c, false, grabWindow, XCB_EVENT_MASK_POINTER_MOTION,
                       XCB_GRAB_MODE_ASYNC, XCB_GRAB_MODE_ASYNC,
                       XCB_WINDOW_NONE, XCB_CURSOR_NONE, XCB_CURRENT_TIME),
      nullptr);
  const bool grabbed = grab && grab->status == XCB_GRAB_STATUS_SUCCESS;
  free(grab);
  CHECK(grabbed);
  model.setDockHidden(true);
  xcb_warp_pointer(c, XCB_WINDOW_NONE, root, 0, 0, 0, 0, 300, 278);
  xcb_flush(c);
  CHECK(waitFor([&] { return model.edgeHovered(); }));
  model.setDockHidden(false);
  xcb_warp_pointer(c, XCB_WINDOW_NONE, root, 0, 0, 0, 0, 20, 20);
  xcb_flush(c);
  CHECK(waitFor([&] { return !model.pointerInside(); }));
  xcb_ungrab_pointer(c, XCB_CURRENT_TIME);
  xcb_flush(c);
  const auto atom = [c](const char *name) {
    auto *reply = xcb_intern_atom_reply(
        c, xcb_intern_atom(c, false, uint16_t(strlen(name)), name), nullptr);
    const auto id = reply ? reply->atom : xcb_atom_t(XCB_ATOM_NONE);
    free(reply);
    return id;
  };
  const auto set = [c, &atom](xcb_window_t target, const char *name,
                              xcb_atom_t type, uint32_t value) {
    xcb_change_property(c, XCB_PROP_MODE_REPLACE, target, atom(name), type, 32,
                        1, &value);
    xcb_flush(c);
  };
  set(root, "_NET_CLIENT_LIST", XCB_ATOM_WINDOW, grabWindow);
  set(root, "_NET_CURRENT_DESKTOP", XCB_ATOM_CARDINAL, 0);
  set(grabWindow, "_NET_WM_DESKTOP", XCB_ATOM_CARDINAL, 0);
  set(grabWindow, "_NET_WM_WINDOW_TYPE", XCB_ATOM_ATOM,
      atom("_NET_WM_WINDOW_TYPE_NORMAL"));
  CHECK(waitFor([&] { return !model.desktopVisible(); }));
  model.setDockArea(-100, -100, 10,
                    10); // Root origin, where the test client lives.
  CHECK(waitFor([&] { return model.dockObstructed(); }));
  model.setDockArea(200, 100, 30, 30);
  CHECK(waitFor([&] { return !model.dockObstructed(); }));
  xcb_configure_window(c, grabWindow, XCB_CONFIG_WINDOW_X | XCB_CONFIG_WINDOW_Y,
                       std::array<uint32_t, 2>{300, 200}.data());
  xcb_flush(c);
  CHECK(waitFor([&] { return model.dockObstructed(); }));
  set(grabWindow, "_NET_WM_STATE", XCB_ATOM_ATOM, atom("_NET_WM_STATE_HIDDEN"));
  CHECK(waitFor(
      [&] { return model.desktopVisible() && !model.dockObstructed(); }));
  xcb_delete_property(c, grabWindow, atom("_NET_WM_STATE"));
  xcb_flush(c);
  CHECK(waitFor([&] { return !model.desktopVisible(); }));
  set(grabWindow, "WM_STATE", atom("WM_STATE"), 3);
  CHECK(waitFor([&] { return model.desktopVisible(); }));
  set(grabWindow, "WM_STATE", atom("WM_STATE"), 1);
  CHECK(waitFor([&] { return !model.desktopVisible(); }));
  set(grabWindow, "_NET_WM_STATE", XCB_ATOM_ATOM,
      atom("_NET_WM_STATE_SKIP_TASKBAR"));
  CHECK(waitFor([&] { return model.desktopVisible(); }));
  xcb_delete_property(c, grabWindow, atom("_NET_WM_STATE"));
  xcb_flush(c);
  CHECK(waitFor([&] { return !model.desktopVisible(); }));
  set(root, "_NET_SHOWING_DESKTOP", XCB_ATOM_CARDINAL, 1);
  CHECK(waitFor([&] { return model.desktopVisible(); }));
  set(root, "_NET_SHOWING_DESKTOP", XCB_ATOM_CARDINAL, 0);
  CHECK(waitFor([&] { return !model.desktopVisible(); }));
  set(root, "_NET_CURRENT_DESKTOP", XCB_ATOM_CARDINAL, 1);
  CHECK(waitFor([&] { return model.desktopVisible(); }));
  set(root, "_NET_CURRENT_DESKTOP", XCB_ATOM_CARDINAL, 0);
  CHECK(waitFor([&] { return !model.desktopVisible(); }));
  set(grabWindow, "_NET_WM_PID", XCB_ATOM_CARDINAL,
      QCoreApplication::applicationPid());
  CHECK(waitFor([&] { return model.rowCount() == 0; }));
  xcb_delete_property(c, grabWindow, atom("_NET_WM_PID"));
  xcb_flush(c);
  CHECK(waitFor([&] { return model.rowCount() == 1; }));
  qInfo("PASS: own-process windows are excluded from the task inventory.");
  qInfo("PASS: desktop visibility tracks minimized windows, Show Desktop and "
        "workspace changes.");
  qInfo("PASS: edge reveal and leave detection during another client's pointer "
        "grab.");
  return 0;
}
