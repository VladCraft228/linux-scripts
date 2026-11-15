#include "applicationmodel.h"
#include "launcheripc.h"
#include "platform/x11/windowmodel.h"
#include <QFileInfo>
#include <QUrl>

#include <QCommandLineParser>
#include <QDir>
#include <QGuiApplication>
#include <QIcon>
#include <QLockFile>
#include <QProcess>
#include <QQmlApplicationEngine>
#include <QQmlContext>
#include <QQuickImageProvider>
#include <QScreen>
#include <QStandardPaths>
#include <cstdio>

class TaskIcons final : public QQuickImageProvider {
public:
  explicit TaskIcons(WindowModel *model)
      : QQuickImageProvider(Image), model_(model) {}
  QImage requestImage(const QString &id, QSize *size,
                      const QSize &requested) override {
    auto image = model_->icon(id.section('/', 0, 0).toUInt());
    if (size)
      *size = image.size();
    if (requested.isValid())
      image = image.scaled(requested, Qt::KeepAspectRatio,
                           Qt::SmoothTransformation);
    return image;
  }

private:
  WindowModel *model_;
};

class ApplicationIcons final : public QQuickImageProvider {
public:
  explicit ApplicationIcons(ApplicationModel *model)
      : QQuickImageProvider(Image), model_(model) {}
  QImage requestImage(const QString &key, QSize *size, const QSize &) override {
    auto image = model_->icon(key);
    if (size)
      *size = image.size();
    return image;
  }

private:
  ApplicationModel *model_;
};

class CatalogIcons final : public QQuickImageProvider {
public:
  explicit CatalogIcons(Runtime *runtime)
      : QQuickImageProvider(Image), runtime_(runtime) {}
  QImage requestImage(const QString &id, QSize *size, const QSize &) override {
    auto image = runtime_->catalogIcon(QUrl::fromPercentEncoding(id.toUtf8()));
    if (size)
      *size = image.size();
    return image;
  }

private:
  Runtime *runtime_;
};

int main(int argc, char **argv) {
  if (qEnvironmentVariable("XDG_SESSION_TYPE") == "wayland" ||
      !qEnvironmentVariableIsEmpty("WAYLAND_DISPLAY")) {
    std::fprintf(
        stderr,
        "This prototype supports X11 only. Plasma Wayland is planned.\n");
    return 1;
  }
  if (qEnvironmentVariableIsEmpty("DISPLAY")) {
    std::fprintf(stderr, "An X11 display is required.\n");
    return 1;
  }
  qputenv("QT_QPA_PLATFORM", "xcb");
  QQuickWindow::setDefaultAlphaBuffer(true);
  QGuiApplication app(argc, argv);
  QCoreApplication::setOrganizationName("LinuxScripts");
  QCoreApplication::setApplicationName("linux-scripts-dock");
  QGuiApplication::setDesktopFileName("linux-scripts-dock");
  QCoreApplication::setApplicationVersion("0.5.0-rust-preview");
  QCommandLineParser parser;
  parser.setApplicationDescription(
      "Experimental XFCE/X11 application dock. Right-click for Quit; Ctrl+C in "
      "the launching terminal also exits.");
  parser.addHelpOption();
  parser.addVersionOption();
  parser.addOption({"bottom-offset",
                    "Distance above the screen bottom in logical pixels (saved "
                    "between runs; 0–160).",
                    "pixels", "96"});
  parser.addOption({"icon-theme",
                    "Explicit freedesktop icon theme, for example "
                    "WhiteSur-light or Papirus.",
                    "name"});
  parser.addOption(
      {"pin-desktop",
       "Add local .desktop files to the Dock without launching them."});
  parser.addPositionalArgument(
      "files", "Desktop entry paths for --pin-desktop.", "[files…]");
  parser.process(app);
  QStringList pinUrls;
  const bool pinRequested = parser.isSet("pin-desktop");
  if (pinRequested) {
    const auto files = parser.positionalArguments();
    if (files.isEmpty() || files.size() > 32) {
      std::fprintf(stderr,
                   "Use --pin-desktop with 1–32 desktop entry paths.\n");
      return 1;
    }
    for (const auto &file : files)
      pinUrls.append(
          QUrl::fromLocalFile(QFileInfo(file).absoluteFilePath()).toString());
  } else if (!parser.positionalArguments().isEmpty()) {
    parser.showHelp(1);
  }

  bool offsetOk = false;
  const int offset = parser.value("bottom-offset").toInt(&offsetOk);
  if (!offsetOk || offset < 0 || offset > 160) {
    std::fprintf(stderr,
                 "--bottom-offset must be an integer between 0 and 160.\n");
    return 1;
  }
  if (parser.isSet("icon-theme")) {
    QIcon::setThemeName(parser.value("icon-theme"));
  } else if (qEnvironmentVariable("XDG_CURRENT_DESKTOP")
                 .split(':')
                 .contains("XFCE", Qt::CaseInsensitive)) {
    // Query the active XFCE setting once, before loading any icons.
    // Keep Qt's platform theme if xfconf-query is unavailable or fails.
    const auto executable = QStandardPaths::findExecutable("xfconf-query");
    if (!executable.isEmpty()) {
      QProcess query;
      query.start(executable, {"-c", "xsettings", "-p", "/Net/IconTheme"});
      if (query.waitForFinished(1000)) {
        const auto theme =
            QString::fromUtf8(query.readAllStandardOutput()).trimmed();
        if (query.exitStatus() == QProcess::NormalExit &&
            query.exitCode() == 0 && !theme.isEmpty())
          QIcon::setThemeName(theme);
      } else {
        query.kill();
        query.waitForFinished(1000);
      }
    }
  }
  QIcon::setFallbackThemeName("hicolor");
  auto lockDir =
      QStandardPaths::writableLocation(QStandardPaths::RuntimeLocation);
  if (lockDir.isEmpty())
    lockDir = QStandardPaths::writableLocation(QStandardPaths::CacheLocation);
  if (!QDir().mkpath(lockDir))
    return 1;
  const auto ipcName = lockDir + "/linux-scripts-dock.socket";
  QLockFile lock(lockDir + "/linux-scripts-dock.lock");
  if (!lock.tryLock(0)) {
    if (pinRequested)
      return LauncherIpc::submit(ipcName, pinUrls);
    std::fprintf(stderr,
                 "The dock is already running, or its lock is unavailable.\n");
    return 1;
  }
  Runtime runtime;
  WindowModel model(&runtime);
  if (!model.valid()) {
    std::fprintf(
        stderr,
        "The Rust dock engine could not initialize. See the error above.\n");
    return 1;
  }
  ApplicationModel applications(&runtime);
  const auto pin = [&applications](const QStringList &urls) {
    QVariantList list;
    for (const auto &u : urls)
      list.append(u);
    return applications.pinDesktopFiles(list);
  };
  LauncherIpc ipc(pin);
  if (!ipc.listen(ipcName))
    std::fprintf(stderr, "Local launcher integration unavailable; normal Dock "
                         "operation continues.\n");
  if (pinRequested && !pin(pinUrls))
    return 1;
  QQmlApplicationEngine engine;
  engine.rootContext()->setContextProperty("windowModel", &model);
  engine.rootContext()->setContextProperty("applicationModel", &applications);
  engine.addImageProvider("task", new TaskIcons(&model));
  engine.addImageProvider("application", new ApplicationIcons(&applications));
  engine.addImageProvider("catalog", new CatalogIcons(&runtime));
  engine.load(QUrl(QStringLiteral("qrc:/qml/Main.qml")));
  if (engine.rootObjects().isEmpty())
    return 1;
  auto *window = qobject_cast<QQuickWindow *>(engine.rootObjects().first());
  if (!window)
    return 1;
  window->create();
  model.attach(window);
  if (parser.isSet("bottom-offset"))
    applications.setPreference("bottomOffset", offset);
  const auto position = [window, &applications] {
    const int offset = applications.preferences().value("bottomOffset").toInt();
    const auto *screen = window->screen();
    if (!screen)
      return;
    const auto rect = screen->geometry();
    window->setWidth(qMin(1200, qMax(320, rect.width() - 32)));
    window->setHeight(qMin(560 + offset, rect.height()));
    window->setPosition(rect.x() + (rect.width() - window->width()) / 2,
                        qMax(rect.y(), rect.bottom() + 1 - window->height()));
  };
  QObject::connect(window, &QWindow::screenChanged, &app,
                   [position] { position(); });
  for (auto *screen : app.screens())
    QObject::connect(screen, &QScreen::geometryChanged, &app,
                     [position] { position(); });
  QObject::connect(&applications, &ApplicationModel::preferencesChanged, &app,
                   position);
  position();
  window->show();
  std::fprintf(stderr, "Dock preview running. Right-click the dock for Quit, "
                       "or press Ctrl+C here.\n");
  return app.exec();
}
