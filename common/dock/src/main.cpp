#include "platform/x11/windowmodel.h"

#include <QCommandLineParser>
#include <QDir>
#include <QGuiApplication>
#include <QIcon>
#include <QLockFile>
#include <QQmlApplicationEngine>
#include <QQmlContext>
#include <QQuickImageProvider>
#include <QScreen>
#include <QStandardPaths>
#include <cstdio>

class TaskIcons final : public QQuickImageProvider {
public:
    explicit TaskIcons(WindowModel *model) : QQuickImageProvider(Image), model_(model) {}
    QImage requestImage(const QString &id, QSize *size, const QSize &requested) override {
        auto image = model_->icon(id.section('/', 0, 0).toUInt());
        if (size) *size = image.size();
        if (requested.isValid()) image = image.scaled(requested, Qt::KeepAspectRatio, Qt::SmoothTransformation);
        return image;
    }
private:
    WindowModel *model_;
};

int main(int argc, char **argv) {
    if (qEnvironmentVariable("XDG_SESSION_TYPE") == "wayland"
        || !qEnvironmentVariableIsEmpty("WAYLAND_DISPLAY")) {
        std::fprintf(stderr, "This prototype supports X11 only. Plasma Wayland is planned.\n");
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
    QCoreApplication::setApplicationVersion("0.1.0-prototype");
    QCommandLineParser parser;
    parser.setApplicationDescription("Experimental XFCE/X11 dock. Right-click for Quit; Ctrl+C in the launching terminal also exits.");
    parser.addHelpOption(); parser.addVersionOption();
    parser.addOption({"bottom-offset", "Distance above the screen bottom in logical pixels (default leaves room for Plank).", "pixels", "96"});
    parser.addOption({"icon-theme", "Explicit freedesktop icon theme, for example WhiteSur-light or Papirus.", "name"});
    parser.process(app);
    bool offsetOk = false;
    const int offset = parser.value("bottom-offset").toInt(&offsetOk);
    if (!offsetOk || offset < 0 || offset > 1000) {
        std::fprintf(stderr, "--bottom-offset must be an integer between 0 and 1000.\n");
        return 1;
    }
    if (parser.isSet("icon-theme")) QIcon::setThemeName(parser.value("icon-theme"));
    QIcon::setFallbackThemeName("hicolor");
    auto lockDir = QStandardPaths::writableLocation(QStandardPaths::RuntimeLocation);
    if (lockDir.isEmpty()) lockDir = QStandardPaths::writableLocation(QStandardPaths::CacheLocation);
    if (!QDir().mkpath(lockDir)) return 1;
    QLockFile lock(lockDir + "/linux-scripts-dock.lock");
    if (!lock.tryLock(0)) {
        std::fprintf(stderr, "The dock is already running, or its lock is unavailable.\n");
        return 1;
    }
    WindowModel model;
    if (!model.valid()) {
        std::fprintf(stderr, "Cannot connect to the X11 server.\n");
        return 1;
    }
    QQmlApplicationEngine engine;
    engine.rootContext()->setContextProperty("windowModel", &model);
    engine.addImageProvider("task", new TaskIcons(&model));
    engine.load(QUrl(QStringLiteral("qrc:/qml/Main.qml")));
    if (engine.rootObjects().isEmpty()) return 1;
    auto *window = qobject_cast<QQuickWindow *>(engine.rootObjects().first());
    if (!window) return 1;
    window->create();
    model.attach(window);
    const auto position = [window, offset] {
        const auto *screen = window->screen();
        if (!screen) return;
        const auto rect = screen->geometry();
        window->setWidth(qMin(1200, qMax(320, rect.width() - 32)));
        window->setPosition(rect.x() + (rect.width() - window->width()) / 2,
                            qMax(rect.y(), rect.bottom() + 1 - window->height() - offset));
    };
    QObject::connect(window, &QWindow::screenChanged, &app, [position] { position(); });
    for (auto *screen : app.screens())
        QObject::connect(screen, &QScreen::geometryChanged, &app, [position] { position(); });
    position();
    window->show();
    std::fprintf(stderr, "Dock prototype running. Right-click the dock for Quit, or press Ctrl+C here.\n");
    return app.exec();
}
