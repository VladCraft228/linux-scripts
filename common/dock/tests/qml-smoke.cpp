#include <QAbstractListModel>
#include <QGuiApplication>
#include <QQmlApplicationEngine>
#include <QQmlContext>
#include <QQuickImageProvider>
#include <QQuickWindow>
#include <QTimer>

// Exercise the actual QML with live model mutations, without a display server.
// This checks the UI/model contract; it does not replace X11 integration testing.
class Fixture final : public QAbstractListModel {
    Q_OBJECT
public:
    int count = 3;
    int maskCalls = 0;
    int rowCount(const QModelIndex &parent = {}) const override { return parent.isValid() ? 0 : count; }
    QVariant data(const QModelIndex &index, int role) const override {
        if (role == Qt::UserRole + 1) return index.row() + 1;
        if (role == Qt::UserRole + 2) return QString("Test window %1").arg(index.row());
        if (role == Qt::UserRole + 3) return QString("image://task/%1/0").arg(index.row());
        if (role == Qt::UserRole + 4) return index.row() == 0;
        return {};
    }
    QHash<int, QByteArray> roleNames() const override {
        return {{Qt::UserRole + 1, "windowId"}, {Qt::UserRole + 2, "windowTitle"},
                {Qt::UserRole + 3, "iconUrl"}, {Qt::UserRole + 4, "windowActive"}};
    }
    Q_INVOKABLE void setInputRegion(const QVariantList &rects) {
        if (!rects.isEmpty()) ++maskCalls;
    }
    Q_INVOKABLE void activate(quint32) {}
    void empty() { beginResetModel(); count = 0; endResetModel(); }
};

class Icons final : public QQuickImageProvider {
public:
    Icons() : QQuickImageProvider(Image) {}
    QImage requestImage(const QString &, QSize *size, const QSize &) override {
        QImage image(128, 128, QImage::Format_ARGB32_Premultiplied);
        image.fill(QColor("#74a5f2"));
        if (size) *size = image.size();
        return image;
    }
};

int main(int argc, char **argv) {
    QGuiApplication app(argc, argv);
    Fixture fixture;
    QQmlApplicationEngine engine;
    int warnings = 0;
    QObject::connect(&engine, &QQmlEngine::warnings, &app,
                     [&warnings](const QList<QQmlError> &errors) { warnings += int(errors.size()); });
    engine.rootContext()->setContextProperty("windowModel", &fixture);
    engine.addImageProvider("task", new Icons);
    engine.load(QUrl("qrc:/qml/Main.qml"));
    if (engine.rootObjects().isEmpty()) return 1;
    auto *window = qobject_cast<QQuickWindow *>(engine.rootObjects().first());
    if (!window) return 1;
    window->show();
    if (!qEnvironmentVariableIsEmpty("DOCK_TEST_SNAPSHOT")) {
        QTimer::singleShot(100, window, [window] {
            window->grabWindow().save(qEnvironmentVariable("DOCK_TEST_SNAPSHOT"));
        });
    }
    QTimer::singleShot(150, &fixture, &Fixture::empty);
    QTimer::singleShot(300, &app, [&app, &fixture, &warnings] {
        app.exit(warnings == 0 && fixture.maskCalls > 0 ? 0 : 1);
    });
    return app.exec();
}

#include "qml-smoke.moc"
